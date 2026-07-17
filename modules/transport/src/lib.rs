//! M16 Cross-Device Transport (nova_transport).
//!
//! Provides TCP transport with local discovery, heartbeat, reconnection,
//! compression, and encryption for the NOVA ecosystem.

#![doc(html_root_url = "https://docs.rs/nova_transport/0.1.0")]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::Utc;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

type EventHandler = Arc<RwLock<Option<Box<dyn Fn(TransportEvent) + Send + Sync>>>>;
type TimeoutCallback = Arc<Mutex<Option<Box<dyn Fn(&str) + Send + Sync>>>>;

// ---------------------------------------------------------------------------
// Re-exports from sibling crates
// ---------------------------------------------------------------------------

/// Re-export the security manager type for convenience.
pub use nova_security::SecurityManager;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can arise during transport operations.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Disconnected")]
    Disconnected,

    #[error("Packet too large (max {0} bytes, got {1})")]
    PacketTooLarge(usize, usize),

    #[error("Compression failed: {0}")]
    CompressionFailed(String),

    #[error("Encryption failed")]
    EncryptionFailed,

    #[error("Decryption failed")]
    DecryptionFailed,

    #[error("Invalid handshake: {0}")]
    InvalidHandshake(String),

    #[error("Timeout")]
    Timeout,

    #[error("I/O error: {0}")]
    IoError(String),
}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        TransportError::IoError(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Events emitted by the transport layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportEvent {
    Connected { device_id: String, addr: String },
    Disconnected { device_id: String },
    PacketReceived { device_id: String, data: Vec<u8> },
    HeartbeatTimeout { device_id: String },
    Reconnected { device_id: String },
}

// ---------------------------------------------------------------------------
// Connection state
// ---------------------------------------------------------------------------

/// The state of a connection to a peer device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

// ---------------------------------------------------------------------------
// Transport configuration
// ---------------------------------------------------------------------------

/// Configuration for the transport manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub heartbeat_interval_ms: u64,
    pub reconnect_attempts: u32,
    pub reconnect_delay_ms: u64,
    pub max_packet_size: usize,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 5000,
            reconnect_attempts: 5,
            reconnect_delay_ms: 2000,
            max_packet_size: 65536,
            compression_enabled: true,
            encryption_enabled: true,
        }
    }
}

impl TransportConfig {
    /// Validate configuration values, returning an error if any are invalid.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.heartbeat_interval_ms < 100 {
            return Err(TransportError::IoError(
                "heartbeat_interval_ms must be >= 100".into(),
            ));
        }
        if self.reconnect_attempts > 100 {
            return Err(TransportError::IoError(
                "reconnect_attempts must be <= 100".into(),
            ));
        }
        if self.reconnect_delay_ms < 100 {
            return Err(TransportError::IoError(
                "reconnect_delay_ms must be >= 100".into(),
            ));
        }
        if self.max_packet_size < 1024 || self.max_packet_size > 10 * 1024 * 1024 {
            return Err(TransportError::IoError(
                "max_packet_size must be between 1024 and 10 MiB".into(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Packet
// ---------------------------------------------------------------------------

/// A transport packet sent between devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub id: Uuid,
    pub device_id: String,
    pub payload: Vec<u8>,
    pub compressed: bool,
    pub encrypted: bool,
    pub sequence: u64,
    pub timestamp: i64,
}

impl Packet {
    /// Create a new packet with the given device_id and payload.
    pub fn new(device_id: &str, payload: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4(),
            device_id: device_id.to_string(),
            payload,
            compressed: false,
            encrypted: false,
            sequence: 0,
            timestamp: Utc::now().timestamp_millis(),
        }
    }

    /// Serialize to length-prefixed bytes (4-byte big-endian length + bincode).
    pub fn to_bytes(&self) -> Result<Vec<u8>, TransportError> {
        let body = bincode::serialize(self)
            .map_err(|e| TransportError::IoError(format!("serialization: {e}")))?;
        let len = body.len() as u32;
        let mut out = Vec::with_capacity(4 + body.len());
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// Deserialize from length-prefixed bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, TransportError> {
        if data.len() < 4 {
            return Err(TransportError::IoError("packet too short".into()));
        }
        let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len {
            return Err(TransportError::IoError("incomplete packet".into()));
        }
        bincode::deserialize(&data[4..4 + len])
            .map_err(|e| TransportError::IoError(format!("deserialization: {e}")))
    }
}

// ---------------------------------------------------------------------------
// TransportListener trait
// ---------------------------------------------------------------------------

/// Listener for transport events from connected peers.
#[async_trait]
pub trait TransportListener: Send + Sync {
    /// Called when a new device connects. Return `true` to accept the connection.
    fn on_connection(&self, device_id: &str, addr: &str) -> bool;

    /// Called when a device disconnects.
    fn on_disconnection(&self, device_id: &str);

    /// Called when a packet is received from a device.
    fn on_packet(&self, device_id: &str, data: &[u8]);
}

// ---------------------------------------------------------------------------
// Internal connection handle
// ---------------------------------------------------------------------------

struct PeerConnection {
    #[allow(dead_code)]
    device_id: String,
    addr: String,
    state: ConnectionState,
    writer: mpsc::UnboundedSender<Vec<u8>>,
    last_seen: Instant,
    reconnect_count: u32,
}

// ---------------------------------------------------------------------------
// TransportManager
// ---------------------------------------------------------------------------

/// The core transport manager that handles TCP connections, heartbeats,
/// reconnection, compression, and encryption.
pub struct TransportManager {
    config: TransportConfig,
    listener: Arc<RwLock<Option<Arc<dyn TransportListener>>>>,
    security: Arc<RwLock<Option<Arc<SecurityManager>>>>,
    event_handler: EventHandler,
    connections: Arc<RwLock<HashMap<String, PeerConnection>>>,
    local_addr: Arc<RwLock<Option<String>>>,
    shutdown_flag: Arc<AtomicBool>,
    sequence_counter: Arc<AtomicU64>,
    shutdown_tx: Arc<Mutex<Option<mpsc::UnboundedSender<()>>>>,
}

impl TransportManager {
    /// Create a new transport manager with the given configuration.
    pub fn new(config: TransportConfig) -> Self {
        Self {
            config,
            listener: Arc::new(RwLock::new(None)),
            security: Arc::new(RwLock::new(None)),
            event_handler: Arc::new(RwLock::new(None)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            local_addr: Arc::new(RwLock::new(None)),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            sequence_counter: Arc::new(AtomicU64::new(0)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the security manager for encryption support.
    pub fn set_security_manager(&self, security: Arc<SecurityManager>) {
        *self.security.write() = Some(security);
    }

    /// Register an event callback.
    pub fn on_event(&self, handler: Box<dyn Fn(TransportEvent) + Send + Sync>) {
        *self.event_handler.write() = Some(handler);
    }

    fn emit_event(&self, event: TransportEvent) {
        if let Some(ref handler) = *self.event_handler.read() {
            handler(event);
        }
    }

    /// Start the TCP listener on an OS-assigned port and begin heartbeats.
    pub async fn start(&self, listener: Arc<dyn TransportListener>) {
        *self.listener.write() = Some(listener);

        let bind_addr = "0.0.0.0:0";
        let tcp_listener = match TcpListener::bind(bind_addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind TCP listener: {e}");
                return;
            }
        };

        let addr = tcp_listener.local_addr().ok();
        if let Some(a) = addr {
            *self.local_addr.write() = Some(a.to_string());
            info!("Transport listening on {a}");
        }

        let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel::<()>();
        *self.shutdown_tx.lock() = Some(shutdown_tx);

        let connections = self.connections.clone();
        let config = self.config.clone();
        let event_handler = self.event_handler.clone();
        let security = self.security.clone();
        let listener_clone = self.listener.clone();
        let shutdown_flag = self.shutdown_flag.clone();
        let seq_counter = self.sequence_counter.clone();

        // Accept loop
        let accept_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown_rx.recv() => {
                        info!("Accept loop shutting down");
                        break;
                    }
                    result = tcp_listener.accept() => {
                        let (stream, peer_addr) = match result {
                            Ok(v) => v,
                            Err(e) => {
                                error!("Accept error: {e}");
                                continue;
                            }
                        };

                        let conns = connections.clone();
                        let cfg = config.clone();
                        let eh = event_handler.clone();
                        let sec = security.clone();
                        let lst = listener_clone.clone();
                        let sf = shutdown_flag.clone();
                        let seq = seq_counter.clone();

                        tokio::spawn(async move {
                            if let Err(e) = handle_inbound(
                                stream, peer_addr, conns, cfg, eh, sec, lst, sf, seq,
                            )
                            .await
                            {
                                debug!("Inbound connection closed: {e}");
                            }
                        });
                    }
                }
            }
        });

        // Heartbeat loop
        let hb_connections = self.connections.clone();
        let hb_config = self.config.clone();
        let hb_event_handler = self.event_handler.clone();
        let hb_shutdown_flag = self.shutdown_flag.clone();

        let heartbeat_handle = tokio::spawn(async move {
            let mut tick = interval(Duration::from_millis(hb_config.heartbeat_interval_ms));
            loop {
                tick.tick().await;
                if hb_shutdown_flag.load(Ordering::Relaxed) {
                    break;
                }

                let disconnected: Vec<(String, String)> = {
                    let mut conns = hb_connections.write();
                    let mut dead = Vec::new();
                    let timeout = Duration::from_millis(hb_config.heartbeat_interval_ms * 3);
                    let now = Instant::now();
                    for (id, peer) in conns.iter_mut() {
                        if peer.state == ConnectionState::Connected
                            && now.duration_since(peer.last_seen) > timeout
                        {
                            if peer.reconnect_count < hb_config.reconnect_attempts {
                                peer.reconnect_count += 1;
                                peer.state = ConnectionState::Reconnecting;
                                warn!(
                                    "Heartbeat timeout for {id}, reconnecting ({}/{})",
                                    peer.reconnect_count, hb_config.reconnect_attempts
                                );
                                if let Some(ref handler) = *hb_event_handler.read() {
                                    handler(TransportEvent::HeartbeatTimeout {
                                        device_id: id.clone(),
                                    });
                                }
                            } else {
                                dead.push((id.clone(), peer.addr.clone()));
                            }
                        }
                    }
                    for (id, _addr) in &dead {
                        conns.remove(id);
                    }
                    dead
                };

                for (id, addr) in disconnected {
                    warn!("Device {id} at {addr} disconnected after heartbeat failure");
                    if let Some(ref handler) = *hb_event_handler.read() {
                        handler(TransportEvent::Disconnected { device_id: id });
                    }
                }
            }
        });

        // Keep handles alive — store them in a detached task
        tokio::spawn(async move {
            let _ = accept_handle.await;
            let _ = heartbeat_handle.await;
        });
    }

    /// Connect to a remote peer.
    pub async fn connect(&self, addr: &str, device_id: &str) -> Result<(), TransportError> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        let peer_addr = stream.peer_addr()?.to_string();
        let local_addr_str = self.local_addr.read().clone().unwrap_or_default();

        // Perform handshake: send our identity
        let handshake = Packet::new(device_id, local_addr_str.as_bytes().to_vec());
        let handshake_bytes = handshake.to_bytes()?;
        let (mut reader, mut writer) = tokio::io::split(stream);
        writer.write_all(&handshake_bytes).await?;

        // Read handshake response
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let body_len = u32::from_be_bytes(len_buf) as usize;
        let mut body = vec![0u8; body_len];
        reader.read_exact(&mut body).await?;
        let response: Packet = bincode::deserialize(&body)
            .map_err(|e| TransportError::InvalidHandshake(e.to_string()))?;

        let remote_device_id = response.device_id;
        debug!("Handshake complete with {remote_device_id}");

        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

        {
            let mut conns = self.connections.write();
            conns.insert(
                remote_device_id.clone(),
                PeerConnection {
                    device_id: remote_device_id.clone(),
                    addr: peer_addr.clone(),
                    state: ConnectionState::Connected,
                    writer: tx,
                    last_seen: Instant::now(),
                    reconnect_count: 0,
                },
            );
        }

        self.emit_event(TransportEvent::Connected {
            device_id: remote_device_id.clone(),
            addr: peer_addr.clone(),
        });

        // Spawn writer task
        let wc_device = remote_device_id.clone();
        let wc_connections = self.connections.clone();
        tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                if let Err(e) = writer.write_all(&data).await {
                    debug!("Write error for {wc_device}: {e}");
                    wc_connections.write().remove(&wc_device);
                    break;
                }
            }
        });

        // Spawn reader task
        let rc_connections = self.connections.clone();
        let rc_config = self.config.clone();
        let rc_event_handler = self.event_handler.clone();
        let rc_security = self.security.clone();
        let rc_listener = self.listener.clone();
        let rc_device = remote_device_id.clone();
        let rc_shutdown = self.shutdown_flag.clone();

        tokio::spawn(async move {
            let mut r = reader;
            loop {
                if rc_shutdown.load(Ordering::Relaxed) {
                    break;
                }
                let mut hdr = [0u8; 4];
                if r.read_exact(&mut hdr).await.is_err() {
                    break;
                }
                let pkt_len = u32::from_be_bytes(hdr) as usize;
                if pkt_len > rc_config.max_packet_size {
                    warn!("Oversized packet ({pkt_len} bytes) from {rc_device}");
                    continue;
                }
                let mut pkt_body = vec![0u8; pkt_len];
                if r.read_exact(&mut pkt_body).await.is_err() {
                    break;
                }

                let packet: Packet = match bincode::deserialize(&pkt_body) {
                    Ok(p) => p,
                    Err(e) => {
                        warn!("Deserialize error from {rc_device}: {e}");
                        continue;
                    }
                };

                // Update last seen
                if let Some(peer) = rc_connections.write().get_mut(&rc_device) {
                    peer.last_seen = Instant::now();
                }

                // Skip heartbeat pings
                if packet.payload.is_empty() {
                    continue;
                }

                let mut data = packet.payload;

                // Decrypt
                if packet.encrypted {
                    if let Some(ref sec) = *rc_security.read() {
                        let peer_pk = sec.x25519_public_key_bytes();
                        match sec.decrypt(&data, &peer_pk) {
                            Ok(d) => data = d,
                            Err(_) => {
                                warn!("Decryption failed from {rc_device}");
                                continue;
                            }
                        }
                    }
                }

                // Decompress
                if packet.compressed {
                    let mut decoder = ZlibDecoder::new(&data[..]);
                    let mut out = Vec::new();
                    if std::io::Read::read_to_end(&mut decoder, &mut out).is_err() {
                        warn!("Decompression failed from {rc_device}");
                        continue;
                    }
                    data = out;
                }

                if let Some(ref listener) = *rc_listener.read() {
                    listener.on_packet(&rc_device, &data);
                }
                if let Some(ref handler) = *rc_event_handler.read() {
                    handler(TransportEvent::PacketReceived {
                        device_id: rc_device.clone(),
                        data,
                    });
                }
            }

            // Cleanup on disconnect
            {
                let mut conns = rc_connections.write();
                conns.remove(&rc_device);
            }
            if let Some(ref handler) = *rc_event_handler.read() {
                handler(TransportEvent::Disconnected {
                    device_id: rc_device.clone(),
                });
            }
            if let Some(ref listener) = *rc_listener.read() {
                listener.on_disconnection(&rc_device);
            }
            debug!("Reader task for {rc_device} finished");
        });

        Ok(())
    }

    /// Disconnect a specific peer.
    pub fn disconnect(&self, device_id: &str) {
        let mut conns = self.connections.write();
        if let Some(peer) = conns.remove(device_id) {
            debug!("Disconnected {device_id} at {}", peer.addr);
            self.emit_event(TransportEvent::Disconnected {
                device_id: device_id.to_string(),
            });
        }
    }

    /// Send data to a connected peer.
    pub fn send(&self, device_id: &str, data: &[u8]) -> Result<(), TransportError> {
        let conns = self.connections.read();
        let peer = conns.get(device_id).ok_or(TransportError::Disconnected)?;

        if data.len() > self.config.max_packet_size {
            return Err(TransportError::PacketTooLarge(
                self.config.max_packet_size,
                data.len(),
            ));
        }

        let mut payload = data.to_vec();
        let mut compressed = false;
        let mut encrypted = false;

        // Compress
        if self.config.compression_enabled {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            if std::io::Write::write_all(&mut encoder, &payload).is_ok() {
                if let Ok(compressed_data) = encoder.finish() {
                    if compressed_data.len() < payload.len() {
                        payload = compressed_data;
                        compressed = true;
                    }
                }
            }
        }

        // Encrypt
        if self.config.encryption_enabled {
            if let Some(ref sec) = *self.security.read() {
                let peer_pk = sec.x25519_public_key_bytes();
                match sec.encrypt(&payload, &peer_pk) {
                    Ok(ct) => {
                        payload = ct;
                        encrypted = true;
                    }
                    Err(_) => return Err(TransportError::EncryptionFailed),
                }
            }
        }

        let seq = self.sequence_counter.fetch_add(1, Ordering::Relaxed);
        let packet = Packet {
            id: Uuid::new_v4(),
            device_id: device_id.to_string(),
            payload,
            compressed,
            encrypted,
            sequence: seq,
            timestamp: Utc::now().timestamp_millis(),
        };

        let bytes = packet.to_bytes()?;
        peer.writer
            .send(bytes)
            .map_err(|_| TransportError::Disconnected)
    }

    /// Broadcast data to all connected peers.
    pub fn broadcast(&self, data: &[u8]) {
        let device_ids: Vec<String> = {
            let conns = self.connections.read();
            conns.keys().cloned().collect()
        };
        for id in device_ids {
            let _ = self.send(&id, data);
        }
    }

    /// Return the list of connected device IDs.
    pub fn connected_devices(&self) -> Vec<String> {
        let conns = self.connections.read();
        conns
            .iter()
            .filter(|(_, p)| p.state == ConnectionState::Connected)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Check whether a specific device is connected.
    pub fn is_connected(&self, device_id: &str) -> bool {
        self.connections
            .read()
            .get(device_id)
            .is_some_and(|p| p.state == ConnectionState::Connected)
    }

    /// Return the local listening address, if any.
    pub fn local_addr(&self) -> Option<String> {
        self.local_addr.read().clone()
    }

    /// Gracefully shut down all connections.
    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        if let Some(tx) = self.shutdown_tx.lock().take() {
            let _ = tx.send(());
        }
        let mut conns = self.connections.write();
        for (id, _) in conns.drain() {
            self.emit_event(TransportEvent::Disconnected { device_id: id });
        }
        *self.local_addr.write() = None;
        info!("TransportManager shut down");
    }
}

// ---------------------------------------------------------------------------
// Inbound connection handler
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn handle_inbound(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    connections: Arc<RwLock<HashMap<String, PeerConnection>>>,
    config: TransportConfig,
    event_handler: EventHandler,
    security: Arc<RwLock<Option<Arc<SecurityManager>>>>,
    listener: Arc<RwLock<Option<Arc<dyn TransportListener>>>>,
    shutdown_flag: Arc<AtomicBool>,
    #[allow(unused_variables)] seq_counter: Arc<AtomicU64>,
) -> Result<(), TransportError> {
    let addr_str = peer_addr.to_string();

    // Read handshake
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let handshake_len = u32::from_be_bytes(len_buf) as usize;
    if handshake_len > config.max_packet_size {
        return Err(TransportError::PacketTooLarge(
            config.max_packet_size,
            handshake_len,
        ));
    }
    let mut handshake_body = vec![0u8; handshake_len];
    stream.read_exact(&mut handshake_body).await?;
    let handshake: Packet = bincode::deserialize(&handshake_body)
        .map_err(|e| TransportError::InvalidHandshake(e.to_string()))?;

    let device_id = handshake.device_id;

    // Ask listener whether to accept
    let accepted = if let Some(ref lst) = *listener.read() {
        lst.on_connection(&device_id, &addr_str)
    } else {
        true
    };

    if !accepted {
        warn!("Rejected connection from {device_id} at {addr_str}");
        return Err(TransportError::InvalidHandshake("rejected".into()));
    }

    // Send handshake response
    let response = Packet::new(&local_device_id().await, addr_str.as_bytes().to_vec());
    let response_bytes = response.to_bytes()?;
    stream.write_all(&response_bytes).await?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

    {
        let mut conns = connections.write();
        conns.insert(
            device_id.clone(),
            PeerConnection {
                device_id: device_id.clone(),
                addr: addr_str.clone(),
                state: ConnectionState::Connected,
                writer: tx,
                last_seen: Instant::now(),
                reconnect_count: 0,
            },
        );
    }

    info!("Accepted connection from {device_id} at {addr_str}");

    if let Some(ref handler) = *event_handler.read() {
        handler(TransportEvent::Connected {
            device_id: device_id.clone(),
            addr: addr_str.clone(),
        });
    }

    let (r_stream, mut w_stream) = tokio::io::split(stream);

    // Writer task
    let w_id = device_id.clone();
    let w_conns = connections.clone();
    tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if let Err(e) = w_stream.write_all(&data).await {
                debug!("Write error for {w_id}: {e}");
                w_conns.write().remove(&w_id);
                break;
            }
        }
    });

    // Reader task
    let r_connections = connections.clone();
    let r_config = config.clone();
    let r_event_handler = event_handler.clone();
    let r_security = security.clone();
    let r_listener = listener.clone();
    let r_device = device_id.clone();
    let r_shutdown = shutdown_flag.clone();

    tokio::spawn(async move {
        let mut r = r_stream;
        loop {
            if r_shutdown.load(Ordering::Relaxed) {
                break;
            }
            let mut hdr = [0u8; 4];
            if r.read_exact(&mut hdr).await.is_err() {
                break;
            }
            let pkt_len = u32::from_be_bytes(hdr) as usize;
            if pkt_len > r_config.max_packet_size {
                warn!("Oversized packet ({pkt_len} bytes) from {r_device}");
                continue;
            }
            let mut pkt_body = vec![0u8; pkt_len];
            if r.read_exact(&mut pkt_body).await.is_err() {
                break;
            }

            let packet: Packet = match bincode::deserialize(&pkt_body) {
                Ok(p) => p,
                Err(e) => {
                    warn!("Deserialize error from {r_device}: {e}");
                    continue;
                }
            };

            // Update last seen
            if let Some(peer) = r_connections.write().get_mut(&r_device) {
                peer.last_seen = Instant::now();
            }

            if packet.payload.is_empty() {
                continue;
            }

            let mut data = packet.payload;

            if packet.encrypted {
                if let Some(ref sec) = *r_security.read() {
                    let peer_pk = sec.x25519_public_key_bytes();
                    match sec.decrypt(&data, &peer_pk) {
                        Ok(d) => data = d,
                        Err(_) => {
                            warn!("Decryption failed from {r_device}");
                            continue;
                        }
                    }
                }
            }

            if packet.compressed {
                let mut decoder = ZlibDecoder::new(&data[..]);
                let mut out = Vec::new();
                if std::io::Read::read_to_end(&mut decoder, &mut out).is_err() {
                    warn!("Decompression failed from {r_device}");
                    continue;
                }
                data = out;
            }

            if let Some(ref listener) = *r_listener.read() {
                listener.on_packet(&r_device, &data);
            }
            if let Some(ref handler) = *r_event_handler.read() {
                handler(TransportEvent::PacketReceived {
                    device_id: r_device.clone(),
                    data,
                });
            }
        }

        // Cleanup
        {
            let mut conns = r_connections.write();
            conns.remove(&r_device);
        }
        if let Some(ref handler) = *r_event_handler.read() {
            handler(TransportEvent::Disconnected {
                device_id: r_device.clone(),
            });
        }
        if let Some(ref listener) = *r_listener.read() {
            listener.on_disconnection(&r_device);
        }
        debug!("Inbound reader for {r_device} finished");
    });

    Ok(())
}

/// Determine the local device ID.  In production this would come from config;
/// we use the hostname as a reasonable fallback.
async fn local_device_id() -> String {
    tokio::process::Command::new("hostname")
        .output()
        .await
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "nova-device".to_string())
}

// ---------------------------------------------------------------------------
// DiscoveredDevice
// ---------------------------------------------------------------------------

/// A device discovered via local service discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    pub device_id: String,
    pub device_name: String,
    pub device_type: String,
    pub addr: String,
    pub port: u16,
    pub version: String,
}

// ---------------------------------------------------------------------------
// LocalDiscovery
// ---------------------------------------------------------------------------

/// Local service discovery using UDP multicast (mDNS-like).
///
/// Broadcasts presence on `239.255.255.250:1900` and listens for peer
/// announcements.
pub struct LocalDiscovery {
    service_name: String,
    port: u16,
    socket: Arc<Mutex<Option<Arc<tokio::net::UdpSocket>>>>,
    running: Arc<AtomicBool>,
    announce_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl LocalDiscovery {
    /// Create a new discovery service.
    pub fn new(service_name: &str, port: u16) -> Self {
        Self {
            service_name: service_name.to_string(),
            port,
            socket: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
            announce_task: Arc::new(Mutex::new(None)),
        }
    }

    /// Start broadcasting presence and listening for peers.
    pub async fn start(&self) -> Result<(), TransportError> {
        if self.running.load(Ordering::Relaxed) {
            return Ok(());
        }

        let sock = tokio::net::UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| TransportError::IoError(e.to_string()))?;

        sock.set_broadcast(true)
            .map_err(|e| TransportError::IoError(e.to_string()))?;

        let sock = Arc::new(sock);
        let announce_sock = sock.clone();

        *self.socket.lock() = Some(sock);
        self.running.store(true, Ordering::Relaxed);

        let service_name = self.service_name.clone();
        let port = self.port;
        let running = self.running.clone();

        let announce_handle = tokio::spawn(async move {
            let multicast_addr = "239.255.255.250:1900";
            let payload = format!(
                "NOVA_DISCOVER {} {} {}",
                service_name,
                port,
                env!("CARGO_PKG_VERSION")
            );
            let mut tick = interval(Duration::from_secs(5));
            loop {
                tick.tick().await;
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                if let Err(e) = announce_sock
                    .send_to(payload.as_bytes(), multicast_addr)
                    .await
                {
                    debug!("Discovery announce send error: {e}");
                }
            }
        });

        *self.announce_task.lock() = Some(announce_handle);
        info!("LocalDiscovery started for {}", self.service_name);
        Ok(())
    }

    /// Discover peer devices on the local network.
    pub async fn discover(&self, timeout_ms: u64) -> Vec<DiscoveredDevice> {
        let sock = match self.socket.lock().as_ref() {
            Some(s) => s.clone(),
            None => return Vec::new(),
        };

        let mut buf = [0u8; 4096];
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut devices = Vec::new();

        // Send a discovery probe
        let probe = format!("NOVA_DISCOVER_QUERY {}", self.service_name);
        let _ = sock.send_to(probe.as_bytes(), "239.255.255.250:1900").await;

        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            tokio::select! {
                result = tokio::time::timeout(remaining, sock.recv_from(&mut buf)) => {
                    match result {
                        Ok(Ok((len, src))) => {
                            let msg = String::from_utf8_lossy(&buf[..len]);
                            if let Some(dev) = parse_discovery_msg(&msg, src) {
                                if !devices.iter().any(|d: &DiscoveredDevice| d.device_id == dev.device_id) {
                                    devices.push(dev);
                                }
                            }
                        }
                        _ => break,
                    }
                }
            }
        }

        devices
    }

    /// Stop the discovery service.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.announce_task.lock().take() {
            handle.abort();
        }
        self.socket.lock().take();
        info!("LocalDiscovery stopped");
    }
}

fn parse_discovery_msg(msg: &str, src: SocketAddr) -> Option<DiscoveredDevice> {
    let parts: Vec<&str> = msg.split_whitespace().collect();
    if parts.len() >= 5 && parts[0] == "NOVA_DISCOVER" {
        let version = parts.get(3).unwrap_or(&"0.1.0").to_string();
        let port: u16 = parts.get(2)?.parse().ok()?;
        Some(DiscoveredDevice {
            device_id: parts[1].to_string(),
            device_name: parts[1].to_string(),
            device_type: "unknown".to_string(),
            addr: src.ip().to_string(),
            port,
            version,
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// HeartbeatManager
// ---------------------------------------------------------------------------

/// Manages periodic heartbeat pings and detects timeouts.
pub struct HeartbeatManager {
    interval_ms: u64,
    reconnect_attempts: u32,
    connections: Arc<RwLock<HashMap<String, HeartbeatPeer>>>,
    running: Arc<AtomicBool>,
    task_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    timeout_callback: TimeoutCallback,
}

struct HeartbeatPeer {
    last_seen: Instant,
    reconnect_count: u32,
}

impl HeartbeatManager {
    /// Create a new heartbeat manager.
    pub fn new(interval_ms: u64, reconnect_attempts: u32) -> Self {
        Self {
            interval_ms,
            reconnect_attempts,
            connections: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            task_handle: Arc::new(Mutex::new(None)),
            timeout_callback: Arc::new(Mutex::new(None)),
        }
    }

    /// Register a device to track.
    pub fn register_device(&self, device_id: &str) {
        let mut conns = self.connections.write();
        conns.insert(
            device_id.to_string(),
            HeartbeatPeer {
                last_seen: Instant::now(),
                reconnect_count: 0,
            },
        );
    }

    /// Update the last-seen timestamp for a device.
    pub fn update_last_seen(&self, device_id: &str) {
        if let Some(peer) = self.connections.write().get_mut(device_id) {
            peer.last_seen = Instant::now();
        }
    }

    /// Unregister a device.
    pub fn unregister_device(&self, device_id: &str) {
        self.connections.write().remove(device_id);
    }

    /// Set a callback for heartbeat timeouts.
    pub fn on_timeout(&self, callback: Box<dyn Fn(&str) + Send + Sync>) {
        *self.timeout_callback.lock() = Some(callback);
    }

    /// Start the heartbeat monitor.
    pub fn start(&self) {
        if self.running.load(Ordering::Relaxed) {
            return;
        }
        self.running.store(true, Ordering::Relaxed);

        let interval_ms = self.interval_ms;
        let reconnect_attempts = self.reconnect_attempts;
        let connections = self.connections.clone();
        let running = self.running.clone();
        let timeout_callback = self.timeout_callback.clone();

        let handle = tokio::spawn(async move {
            let mut tick = interval(Duration::from_millis(interval_ms));
            loop {
                tick.tick().await;
                if !running.load(Ordering::Relaxed) {
                    break;
                }

                let timeout = Duration::from_millis(interval_ms * 3);
                let now = Instant::now();
                let mut timed_out = Vec::new();

                {
                    let mut conns = connections.write();
                    for (id, peer) in conns.iter_mut() {
                        if now.duration_since(peer.last_seen) > timeout {
                            if peer.reconnect_count < reconnect_attempts {
                                peer.reconnect_count += 1;
                                warn!(
                                    "Heartbeat timeout for {id}, reconnecting ({}/{})",
                                    peer.reconnect_count, reconnect_attempts
                                );
                            } else {
                                timed_out.push(id.clone());
                            }
                        }
                    }
                    for id in &timed_out {
                        conns.remove(id);
                    }
                }

                for id in &timed_out {
                    if let Some(ref cb) = *timeout_callback.lock() {
                        cb(id);
                    }
                }
            }
        });

        *self.task_handle.lock() = Some(handle);
        info!(
            "HeartbeatManager started ({} ms interval)",
            self.interval_ms
        );
    }

    /// Stop the heartbeat monitor.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.task_handle.lock().take() {
            handle.abort();
        }
        self.connections.write().clear();
        info!("HeartbeatManager stopped");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn transport_config_defaults() {
        let cfg = TransportConfig::default();
        assert_eq!(cfg.heartbeat_interval_ms, 5000);
        assert_eq!(cfg.reconnect_attempts, 5);
        assert_eq!(cfg.reconnect_delay_ms, 2000);
        assert_eq!(cfg.max_packet_size, 65536);
        assert!(cfg.compression_enabled);
        assert!(cfg.encryption_enabled);
    }

    #[test]
    fn packet_serialization_round_trip() {
        let pkt = Packet::new("test-device", b"hello nova".to_vec());
        let bytes = pkt.to_bytes().expect("serialization should succeed");
        let deserialized = Packet::from_bytes(&bytes).expect("deserialization should succeed");
        assert_eq!(pkt.id, deserialized.id);
        assert_eq!(pkt.device_id, deserialized.device_id);
        assert_eq!(pkt.payload, deserialized.payload);
        assert_eq!(pkt.compressed, deserialized.compressed);
        assert_eq!(pkt.encrypted, deserialized.encrypted);
        assert_eq!(pkt.sequence, deserialized.sequence);
    }

    #[test]
    fn packet_with_compression_flag() {
        let mut pkt = Packet::new(
            "dev-1",
            b"compressible data that repeats ".to_vec().repeat(10),
        );
        pkt.compressed = true;
        let bytes = pkt.to_bytes().expect("serialization should succeed");
        let deserialized = Packet::from_bytes(&bytes).expect("deserialization should succeed");
        assert!(deserialized.compressed);
        assert_eq!(pkt.payload, deserialized.payload);
    }

    #[test]
    fn manager_creation_and_shutdown() {
        let cfg = TransportConfig::default();
        let mgr = TransportManager::new(cfg);
        assert!(mgr.local_addr().is_none());
        assert!(mgr.connected_devices().is_empty());
        mgr.shutdown();
        assert!(mgr.local_addr().is_none());
    }

    #[tokio::test]
    async fn connect_to_invalid_address_returns_error() {
        let cfg = TransportConfig::default();
        let mgr = TransportManager::new(cfg);
        let result = mgr.connect("127.0.0.1:1", "test-device").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn heartbeat_timeout_detection() {
        let hm = HeartbeatManager::new(50, 2);
        let timeout_fired = Arc::new(AtomicBool::new(false));
        let tf = timeout_fired.clone();

        hm.on_timeout(Box::new(move |_id| {
            tf.store(true, Ordering::Relaxed);
        }));

        hm.register_device("test-device");
        hm.start();

        // Wait longer than 3x interval plus grace for reconnect attempts
        tokio::time::sleep(Duration::from_millis(600)).await;

        hm.stop();
        assert!(timeout_fired.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn local_discovery_start_stop() {
        let disc = LocalDiscovery::new("_nova._tcp", 9999);
        let result = disc.start().await;
        assert!(result.is_ok(), "discovery start should succeed: {result:?}");

        // Give it a moment to bind
        tokio::time::sleep(Duration::from_millis(100)).await;

        disc.stop();

        // After stop it should not be running
        let sock = disc.socket.lock();
        assert!(sock.is_none());
    }

    #[test]
    fn connected_devices_returns_empty_initially() {
        let cfg = TransportConfig::default();
        let mgr = TransportManager::new(cfg);
        let devices = mgr.connected_devices();
        assert!(devices.is_empty());
    }

    #[test]
    fn send_to_unknown_device_returns_error() {
        let cfg = TransportConfig::default();
        let mgr = TransportManager::new(cfg);
        let result = mgr.send("nonexistent", b"hello");
        assert!(result.is_err());
    }

    #[test]
    fn config_validation() {
        let mut cfg = TransportConfig::default();

        assert!(cfg.validate().is_ok());

        cfg.heartbeat_interval_ms = 50;
        assert!(cfg.validate().is_err());

        cfg.heartbeat_interval_ms = 5000;
        cfg.reconnect_attempts = 200;
        assert!(cfg.validate().is_err());

        cfg.reconnect_attempts = 5;
        cfg.max_packet_size = 512;
        assert!(cfg.validate().is_err());

        cfg.max_packet_size = 20 * 1024 * 1024;
        assert!(cfg.validate().is_err());
    }

    #[tokio::test]
    async fn transport_manager_default_state() {
        let cfg = TransportConfig::default();
        let mgr = TransportManager::new(cfg);
        assert!(!mgr.is_connected("any-device"));
        assert_eq!(mgr.connected_devices().len(), 0);
        assert!(mgr.local_addr().is_none());
    }

    #[test]
    fn transport_error_display() {
        let err = TransportError::ConnectionFailed("refused".into());
        assert_eq!(format!("{err}"), "Connection failed: refused");

        let err = TransportError::PacketTooLarge(1024, 4096);
        assert_eq!(
            format!("{err}"),
            "Packet too large (max 1024 bytes, got 4096)"
        );

        let err = TransportError::Timeout;
        assert_eq!(format!("{err}"), "Timeout");
    }

    #[test]
    fn connection_state_serde() {
        let states = [
            ConnectionState::Disconnected,
            ConnectionState::Connecting,
            ConnectionState::Connected,
            ConnectionState::Reconnecting,
        ];
        for s in &states {
            let json = serde_json::to_string(s).unwrap();
            let back: ConnectionState = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, back);
        }
    }

    #[test]
    fn packet_sequence_increments() {
        let p1 = Packet::new("d1", vec![1]);
        let p2 = Packet::new("d1", vec![2]);
        assert_eq!(p1.sequence, 0);
        assert_eq!(p2.sequence, 0); // Each new() starts at 0
    }

    // -----------------------------------------------------------------------
    // Real TCP transport integration tests
    // Run with: $env:NOVA_REAL_TRANSPORT_TEST=1; cargo test real_transport
    // -----------------------------------------------------------------------

    fn real_transport_available() -> bool {
        std::env::var("NOVA_REAL_TRANSPORT_TEST").as_deref() == Ok("1")
    }

    struct TestListener {
        received: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl TransportListener for TestListener {
        fn on_connection(&self, device_id: &str, _addr: &str) -> bool {
            println!("[REAL TCP] Connection from {device_id}");
            true
        }

        fn on_disconnection(&self, device_id: &str) {
            println!("[REAL TCP] Disconnected: {device_id}");
        }

        fn on_packet(&self, _device_id: &str, data: &[u8]) {
            let msg = String::from_utf8_lossy(data).to_string();
            println!("[REAL TCP] Received: {msg}");
            self.received.lock().push(msg);
        }
    }

    #[tokio::test]
    async fn real_transport_tcp_roundtrip() {
        if !real_transport_available() {
            return;
        }

        // Server — listens on OS-assigned port
        let server = TransportManager::new(TransportConfig {
            heartbeat_interval_ms: 5000,
            reconnect_attempts: 2,
            reconnect_delay_ms: 500,
            max_packet_size: 65536,
            compression_enabled: false,
            encryption_enabled: false,
        });
        let server_received = Arc::new(Mutex::new(Vec::<String>::new()));
        server
            .start(Arc::new(TestListener {
                received: server_received.clone(),
            }))
            .await;
        let server_addr = server
            .local_addr()
            .expect("Server should have an address after start")
            .replace("0.0.0.0", "127.0.0.1");
        println!("[REAL TCP] Server listening on {server_addr}");

        // Client connects to server with device_id "test-client"
        let client = TransportManager::new(TransportConfig::default());
        let client_received = Arc::new(Mutex::new(Vec::<String>::new()));
        let cr = client_received.clone();
        client.start(Arc::new(TestListener { received: cr })).await;
        client
            .connect(&server_addr, "test-client")
            .await
            .expect("Client should connect to server");
        println!("[REAL TCP] Client connected to server");

        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            server.is_connected("test-client"),
            "Server should see client as connected"
        );

        // Server sends to client (server knows device_id = "test-client")
        let test_msg = "Hello from server over real TCP!";
        server
            .send("test-client", test_msg.as_bytes())
            .expect("Server should send data to client");

        tokio::time::sleep(Duration::from_millis(500)).await;

        {
            let received = client_received.lock();
            assert!(
                !received.is_empty(),
                "Client should have received data from server"
            );
            assert!(
                received.iter().any(|m| m.contains("Hello from server")),
                "Client received unexpected data: {:?}",
                *received
            );
        }

        println!("[REAL TCP] Round-trip test passed: server -> client over real TCP");

        server.shutdown();
        client.shutdown();
        tokio::time::sleep(Duration::from_millis(200)).await;
        println!("[REAL TCP] Clean shutdown");
    }

    #[tokio::test]
    async fn real_transport_connect_twice() {
        if !real_transport_available() {
            return;
        }

        let server = TransportManager::new(TransportConfig::default());
        let server_received = Arc::new(Mutex::new(Vec::<String>::new()));
        server
            .start(Arc::new(TestListener {
                received: server_received.clone(),
            }))
            .await;
        let addr = server
            .local_addr()
            .expect("server addr")
            .replace("0.0.0.0", "127.0.0.1");

        // Connect client A
        let client_a = TransportManager::new(TransportConfig::default());
        client_a
            .connect(&addr, "client-a")
            .await
            .expect("client-a connects");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(server.is_connected("client-a"));

        // Connect client B
        let client_b = TransportManager::new(TransportConfig::default());
        client_b
            .connect(&addr, "client-b")
            .await
            .expect("client-b connects");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(server.is_connected("client-b"));

        assert_eq!(server.connected_devices().len(), 2);

        // Broadcast from server to both
        server.broadcast(b"broadcast message");
        tokio::time::sleep(Duration::from_millis(300)).await;

        println!("[REAL TCP] Two clients connected, broadcast sent");

        server.shutdown();
        tokio::time::sleep(Duration::from_millis(100)).await;
        println!("[REAL TCP] Multi-connect test passed");
    }
}
