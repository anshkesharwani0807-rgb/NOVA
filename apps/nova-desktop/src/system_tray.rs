// System tray integration for NOVA Desktop.
// Uses tray-icon crate for Windows notification area icon.
// Minimizes to tray on close; left-click restores window.
// TODO: wire into eframe app lifecycle (minimize-to-tray on window close).

#![allow(dead_code)]

use std::sync::mpsc;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub enum TrayMessage {
    Show,
    Hide,
    Quit,
}

pub struct NovaTray {
    tray_icon: TrayIcon,
    rx: mpsc::Receiver<TrayMessage>,
    tx: mpsc::Sender<TrayMessage>,
}

impl NovaTray {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();

        let tray_icon = TrayIconBuilder::new()
            .with_tooltip("NOVA — Personal AI Assistant")
            .with_icon(Icon::from_resource(101, None).unwrap_or_else(|_| {
                Icon::from_rgba(vec![0u8; 64 * 64 * 4], 64, 64)
                    .expect("Failed to create fallback icon")
            }))
            .build()
            .expect("Failed to build tray icon");

        Self { tray_icon, rx, tx }
    }

    pub fn receiver(&self) -> &mpsc::Receiver<TrayMessage> {
        &self.rx
    }

    pub fn sender(&self) -> mpsc::Sender<TrayMessage> {
        self.tx.clone()
    }
}

impl Default for NovaTray {
    fn default() -> Self {
        Self::new()
    }
}
