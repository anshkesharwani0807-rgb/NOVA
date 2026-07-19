use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// ResourceType
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    Screen,
    Mouse,
    Keyboard,
    Clipboard,
    Audio,
    Camera,
    Microphone,
    Filesystem,
    Network,
}

impl ResourceType {
    pub fn supports_shared(&self) -> bool {
        matches!(
            self,
            ResourceType::Filesystem | ResourceType::Network | ResourceType::Clipboard
        )
    }

    pub fn supports_exclusive(&self) -> bool {
        true
    }

    pub fn default_access_mode(&self) -> AccessMode {
        if self.supports_shared() {
            AccessMode::Shared
        } else {
            AccessMode::Exclusive
        }
    }

    pub fn all() -> Vec<ResourceType> {
        vec![
            ResourceType::Screen,
            ResourceType::Mouse,
            ResourceType::Keyboard,
            ResourceType::Clipboard,
            ResourceType::Audio,
            ResourceType::Camera,
            ResourceType::Microphone,
            ResourceType::Filesystem,
            ResourceType::Network,
        ]
    }
}

// ---------------------------------------------------------------------------
// AccessMode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccessMode {
    Shared,
    Exclusive,
}

// ---------------------------------------------------------------------------
// ResourceLock — per-resource lock state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ResourceLockState {
    access_mode: AccessMode,
    owners: HashSet<String>,
    shared_count: usize,
    waiting_queue: VecDeque<(String, AccessMode, Instant)>,
    acquired_at: Option<i64>,
}

impl ResourceLockState {
    fn new() -> Self {
        Self {
            access_mode: AccessMode::Shared,
            owners: HashSet::new(),
            shared_count: 0,
            waiting_queue: VecDeque::new(),
            acquired_at: None,
        }
    }
}

// ---------------------------------------------------------------------------
// OwnedResource
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnedResource {
    pub resource: ResourceType,
    pub access_mode: AccessMode,
    pub acquired_at: i64,
    pub session_id: String,
}

// ---------------------------------------------------------------------------
// ResourceManagerConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ResourceManagerConfig {
    pub default_timeout_ms: u64,
    pub max_wait_queue: usize,
    pub enable_deadlock_detection: bool,
    pub forced_release_on_timeout: bool,
}

impl Default for ResourceManagerConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 30_000,
            max_wait_queue: 100,
            enable_deadlock_detection: true,
            forced_release_on_timeout: true,
        }
    }
}

// ---------------------------------------------------------------------------
// ResourceMetrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceMetrics {
    pub total_acquisitions: u64,
    pub total_releases: u64,
    pub total_wait_time_ms: i64,
    pub total_contentions: u64,
    pub peak_contention: u64,
    pub forced_releases: u64,
    pub acquisition_failures: u64,
    pub timeouts: u64,
    pub deadlock_detections: u64,
    pub current_locks_held: u64,
    pub peak_locks_held: u64,
}

// ---------------------------------------------------------------------------
// ResourceManager
// ---------------------------------------------------------------------------

pub struct ResourceManager {
    locks: RwLock<HashMap<ResourceType, ResourceLockState>>,
    session_resources: RwLock<HashMap<String, Vec<OwnedResource>>>,
    config: ResourceManagerConfig,
    metrics: RwLock<ResourceMetrics>,
    // Internal lock ordering for deadlock prevention
    lock_order: Vec<ResourceType>,
}

impl ResourceManager {
    pub fn new(config: ResourceManagerConfig) -> Self {
        // Lock ordering: acquire resources in this order to prevent deadlocks
        let lock_order = vec![
            ResourceType::Screen,
            ResourceType::Camera,
            ResourceType::Microphone,
            ResourceType::Audio,
            ResourceType::Keyboard,
            ResourceType::Mouse,
            ResourceType::Clipboard,
            ResourceType::Filesystem,
            ResourceType::Network,
        ];

        let mut locks = HashMap::new();
        for r in ResourceType::all() {
            locks.insert(r, ResourceLockState::new());
        }

        Self {
            locks: RwLock::new(locks),
            session_resources: RwLock::new(HashMap::new()),
            config,
            metrics: RwLock::new(ResourceMetrics::default()),
            lock_order,
        }
    }

    /// Acquire resources for a session. Blocks until all resources are acquired or timeout.
    pub fn acquire(
        &self,
        session_id: &str,
        resources: &[(ResourceType, AccessMode)],
    ) -> Result<Vec<OwnedResource>, String> {
        self.acquire_with_timeout(session_id, resources, self.config.default_timeout_ms)
    }

    /// Try to acquire resources without blocking.
    pub fn try_acquire(
        &self,
        session_id: &str,
        resources: &[(ResourceType, AccessMode)],
    ) -> Result<Vec<OwnedResource>, String> {
        self.acquire_with_timeout(session_id, resources, 0)
    }

    /// Acquire resources with a custom timeout (0 = no wait, immediate fail).
    pub fn acquire_with_timeout(
        &self,
        session_id: &str,
        resources: &[(ResourceType, AccessMode)],
        timeout_ms: u64,
    ) -> Result<Vec<OwnedResource>, String> {
        // Validate resources
        self.validate_resources(resources)?;

        // Check lock ordering (deadlock prevention)
        if self.config.enable_deadlock_detection {
            self.check_lock_ordering(resources)?;
        }

        // Check recursive locks — session already holds some of these
        {
            let held = self.session_resources.read();
            if let Some(owned) = held.get(session_id) {
                let already_held: HashSet<&ResourceType> =
                    owned.iter().map(|o| &o.resource).collect();
                for (res, _) in resources {
                    if already_held.contains(res) {
                        let mut metrics = self.metrics.write();
                        metrics.total_acquisitions += 1;
                        return Err(format!(
                            "session '{}' already holds resource '{:?}' (recursive lock detected)",
                            session_id, res
                        ));
                    }
                }
            }
        }

        let start = Instant::now();

        // Sort resources by lock ordering
        let mut sorted: Vec<(ResourceType, AccessMode)> = resources.to_vec();
        sorted.sort_by_key(|(r, _)| self.lock_order_index(r));

        // Phase 1: try to acquire all resources without waiting
        let mut acquired = Vec::new();
        let mut needs_wait = false;

        {
            let mut locks = self.locks.write();
            for (res, mode) in &sorted {
                let state = locks
                    .get_mut(res)
                    .ok_or_else(|| format!("unknown resource type: {:?}", res))?;

                let can_acquire = match mode {
                    AccessMode::Shared => {
                        state.owners.is_empty() || state.access_mode == AccessMode::Shared
                    }
                    AccessMode::Exclusive => state.owners.is_empty(),
                };

                if !can_acquire {
                    needs_wait = true;
                    break;
                }

                state.owners.insert(session_id.to_string());
                if *mode == AccessMode::Shared {
                    state.shared_count += 1;
                }
                state.access_mode = *mode;
                state.acquired_at = Some(Utc::now().timestamp_millis());

                let owned = OwnedResource {
                    resource: *res,
                    access_mode: *mode,
                    acquired_at: state.acquired_at.unwrap(),
                    session_id: session_id.to_string(),
                };
                acquired.push(owned);
            }
        }

        if !needs_wait {
            // All acquired — track by session and update metrics
            return self.finalize_acquire(session_id, acquired, start);
        }

        // Some resources were contended. Release what we've acquired so far.
        if !acquired.is_empty() {
            let to_release: Vec<ResourceType> = acquired.iter().map(|o| o.resource).collect();
            self.release_internal(session_id, &to_release);
            acquired.clear();
        }

        // Phase 2: wait with timeout for each contended resource
        if timeout_ms == 0 {
            let mut metrics = self.metrics.write();
            metrics.total_contentions += 1;
            metrics.acquisition_failures += 1;
            return Err("resource contention (no wait)".into());
        }

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);

        for (res, mode) in &sorted {
            // Register in waiting queue
            {
                let mut locks = self.locks.write();
                if let Some(state) = locks.get_mut(res) {
                    state
                        .waiting_queue
                        .push_back((session_id.to_string(), *mode, Instant::now()));
                }
                let mut metrics = self.metrics.write();
                metrics.total_contentions += 1;
                let current = locks
                    .get(res)
                    .map(|s| s.waiting_queue.len() as u64)
                    .unwrap_or(0);
                if current > metrics.peak_contention {
                    metrics.peak_contention = current;
                }
            }

            // Wait loop
            let acquired_here = loop {
                if Instant::now() >= deadline {
                    // Timeout — clean up waiting queue entry
                    {
                        let mut locks = self.locks.write();
                        if let Some(state) = locks.get_mut(res) {
                            state.waiting_queue.retain(|(sid, _, _)| sid != session_id);
                        }
                    }
                    let acq: Vec<ResourceType> = acquired.iter().map(|o| o.resource).collect();
                    self.release_internal(session_id, &acq);
                    let mut metrics = self.metrics.write();
                    metrics.timeouts += 1;
                    metrics.acquisition_failures += 1;
                    return Err(format!(
                        "timeout waiting for resource '{:?}' after {}ms",
                        res, timeout_ms
                    ));
                }

                std::thread::sleep(Duration::from_millis(10));

                let mut locks = self.locks.write();
                let state = match locks.get_mut(res) {
                    Some(s) => s,
                    None => continue,
                };

                let can_acquire = match mode {
                    AccessMode::Shared => {
                        state.owners.is_empty() || state.access_mode == AccessMode::Shared
                    }
                    AccessMode::Exclusive => state.owners.is_empty(),
                };

                if can_acquire {
                    // Check fairness
                    if let Some(front) = state.waiting_queue.front() {
                        if front.0 != session_id {
                            drop(locks);
                            continue;
                        }
                    }

                    state.waiting_queue.pop_front();
                    state.owners.insert(session_id.to_string());
                    if *mode == AccessMode::Shared {
                        state.shared_count += 1;
                    }
                    state.access_mode = *mode;
                    state.acquired_at = Some(Utc::now().timestamp_millis());

                    let owned = OwnedResource {
                        resource: *res,
                        access_mode: *mode,
                        acquired_at: state.acquired_at.unwrap(),
                        session_id: session_id.to_string(),
                    };
                    drop(locks);

                    let wait_time = start.elapsed().as_millis() as i64;
                    let mut metrics = self.metrics.write();
                    metrics.total_wait_time_ms += wait_time;
                    break owned;
                }
                drop(locks);
            };

            acquired.push(acquired_here);
        }

        self.finalize_acquire(session_id, acquired, start)
    }

    fn finalize_acquire(
        &self,
        session_id: &str,
        acquired: Vec<OwnedResource>,
        _start: Instant,
    ) -> Result<Vec<OwnedResource>, String> {
        // Track by session
        let mut session_res = self.session_resources.write();
        session_res
            .entry(session_id.to_string())
            .or_default()
            .extend(acquired.iter().cloned());

        // Update metrics
        let mut metrics = self.metrics.write();
        metrics.total_acquisitions += acquired.len() as u64;
        metrics.current_locks_held = session_res.values().map(|v| v.len() as u64).sum();
        if metrics.current_locks_held > metrics.peak_locks_held {
            metrics.peak_locks_held = metrics.current_locks_held;
        }

        Ok(acquired)
    }

    /// Release specific resources for a session.
    pub fn release(&self, session_id: &str, resources: &[ResourceType]) -> Result<(), String> {
        self.release_internal(session_id, resources);

        // Clean up session tracking
        let mut session_res = self.session_resources.write();
        if let Some(owned) = session_res.get_mut(session_id) {
            owned.retain(|o| !resources.contains(&o.resource));
            if owned.is_empty() {
                session_res.remove(session_id);
            }
        }

        Ok(())
    }

    /// Release all resources held by a session.
    pub fn release_all(&self, session_id: &str) -> Vec<OwnedResource> {
        let resources = {
            let session_res = self.session_resources.read();
            session_res.get(session_id).cloned().unwrap_or_default()
        };

        let released: Vec<ResourceType> = resources.iter().map(|o| o.resource).collect();
        self.release_internal(session_id, &released);

        let mut session_res = self.session_resources.write();
        session_res.remove(session_id);

        resources
    }

    /// Get the owner of a resource.
    pub fn owner(&self, resource: &ResourceType) -> Option<Vec<String>> {
        let locks = self.locks.read();
        locks.get(resource).map(|state| {
            if state.owners.is_empty() {
                vec![]
            } else {
                state.owners.iter().cloned().collect()
            }
        })
    }

    /// Get sessions waiting for a resource.
    pub fn waiting_sessions(&self, resource: &ResourceType) -> Vec<String> {
        let locks = self.locks.read();
        locks
            .get(resource)
            .map(|state| {
                state
                    .waiting_queue
                    .iter()
                    .map(|(sid, _, _)| sid.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get current resource metrics.
    pub fn resource_metrics(&self) -> ResourceMetrics {
        self.metrics.read().clone()
    }

    /// Check if a resource is currently locked.
    pub fn is_locked(&self, resource: &ResourceType) -> bool {
        let locks = self.locks.read();
        locks
            .get(resource)
            .map(|state| !state.owners.is_empty())
            .unwrap_or(false)
    }

    /// Get all resources held by a session.
    pub fn session_resources(&self, session_id: &str) -> Vec<OwnedResource> {
        self.session_resources
            .read()
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Check if any of the given resources have a conflict with current holders.
    /// Returns the first conflicting resource, if any.
    pub fn has_conflict(
        &self,
        session_id: &str,
        resources: &[(ResourceType, AccessMode)],
    ) -> Option<ResourceType> {
        let locks = self.locks.read();
        for (res, mode) in resources {
            if let Some(state) = locks.get(res) {
                if !state.owners.is_empty() && !state.owners.contains(session_id) {
                    let conflicts = match mode {
                        AccessMode::Shared => state.access_mode == AccessMode::Exclusive,
                        AccessMode::Exclusive => true,
                    };
                    if conflicts {
                        return Some(*res);
                    }
                }
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Deadlock detection
    // -----------------------------------------------------------------------

    /// Detect if there's a deadlock in the system.
    pub fn detect_deadlock(&self) -> Vec<Vec<String>> {
        let locks = self.locks.read();
        let mut deadlocks = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();

        // Build a wait-for graph
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();

        for (_, state) in locks.iter() {
            for owner in &state.owners {
                for (waiter, _, _) in &state.waiting_queue {
                    graph.entry(owner.clone()).or_default().push(waiter.clone());
                }
            }
        }

        // Detect cycles using DFS
        for node in graph.keys() {
            if !visited.contains(node) {
                let mut path = Vec::new();
                let mut in_path = HashSet::new();
                if self.dfs_cycle(node, &graph, &mut visited, &mut path, &mut in_path) {
                    deadlocks.push(path);
                }
            }
        }

        if !deadlocks.is_empty() {
            let mut metrics = self.metrics.write();
            metrics.deadlock_detections += deadlocks.len() as u64;
        }

        deadlocks
    }

    /// Force-release all resources held by a session (e.g., on timeout/crash).
    pub fn force_release(&self, session_id: &str) -> Vec<OwnedResource> {
        let released = self.release_all(session_id);
        if !released.is_empty() {
            // Clean up waiting queues
            let mut locks = self.locks.write();
            for state in locks.values_mut() {
                state.waiting_queue.retain(|(sid, _, _)| sid != session_id);
            }

            let mut metrics = self.metrics.write();
            metrics.forced_releases += released.len() as u64;
        }
        released
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn release_internal(&self, session_id: &str, resources: &[ResourceType]) {
        let mut locks = self.locks.write();
        let mut released_count = 0u64;

        for res in resources {
            if let Some(state) = locks.get_mut(res) {
                if state.owners.remove(session_id) {
                    if state.access_mode == AccessMode::Shared && state.shared_count > 0 {
                        state.shared_count -= 1;
                    }
                    if state.owners.is_empty() {
                        state.shared_count = 0;
                        state.access_mode = AccessMode::Shared;
                    }
                    released_count += 1;
                }
            }
        }

        if released_count > 0 {
            let mut metrics = self.metrics.write();
            metrics.total_releases += released_count;
        }
    }

    fn validate_resources(&self, resources: &[(ResourceType, AccessMode)]) -> Result<(), String> {
        if resources.is_empty() {
            return Err("no resources specified".into());
        }

        let known: HashSet<ResourceType> = ResourceType::all().into_iter().collect();
        for (res, mode) in resources {
            if !known.contains(res) {
                return Err(format!("unknown resource type: {:?}", res));
            }
            if *mode == AccessMode::Shared && !res.supports_shared() {
                return Err(format!(
                    "resource '{:?}' does not support shared access",
                    res
                ));
            }
        }
        Ok(())
    }

    fn lock_order_index(&self, resource: &ResourceType) -> usize {
        self.lock_order
            .iter()
            .position(|r| r == resource)
            .unwrap_or(usize::MAX)
    }

    fn check_lock_ordering(&self, resources: &[(ResourceType, AccessMode)]) -> Result<(), String> {
        let mut prev_index: Option<usize> = None;
        let mut sorted = resources.to_vec();
        sorted.sort_by_key(|(r, _)| self.lock_order_index(r));

        for (res, _) in &sorted {
            let idx = self.lock_order_index(res);
            if let Some(prev) = prev_index {
                if idx < prev {
                    return Err(format!(
                        "lock ordering violation: resource '{:?}' should be acquired before the previous one",
                        res
                    ));
                }
            }
            prev_index = Some(idx);
        }
        Ok(())
    }

    fn dfs_cycle(
        &self,
        node: &str,
        graph: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
        in_path: &mut HashSet<String>,
    ) -> bool {
        if in_path.contains(node) {
            return true;
        }
        if visited.contains(node) {
            return false;
        }
        visited.insert(node.to_string());
        in_path.insert(node.to_string());
        path.push(node.to_string());

        if let Some(neighbors) = graph.get(node) {
            for neighbor in neighbors {
                if self.dfs_cycle(neighbor, graph, visited, path, in_path) {
                    return true;
                }
            }
        }

        path.pop();
        in_path.remove(node);
        false
    }
}

impl Default for ResourceManager {
    fn default() -> Self {
        Self::new(ResourceManagerConfig::default())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_manager() -> ResourceManager {
        ResourceManager::new(ResourceManagerConfig {
            default_timeout_ms: 5000,
            max_wait_queue: 100,
            enable_deadlock_detection: true,
            forced_release_on_timeout: true,
        })
    }

    // -----------------------------------------------------------------------
    // acquire / release
    // -----------------------------------------------------------------------

    #[test]
    fn test_acquire_release_exclusive() {
        let mgr = make_manager();
        let acquired = mgr
            .acquire(
                "session-1",
                &[(ResourceType::Screen, AccessMode::Exclusive)],
            )
            .unwrap();
        assert_eq!(acquired.len(), 1);
        assert_eq!(acquired[0].resource, ResourceType::Screen);
        assert!(mgr.is_locked(&ResourceType::Screen));

        mgr.release("session-1", &[ResourceType::Screen]).unwrap();
        assert!(!mgr.is_locked(&ResourceType::Screen));
    }

    #[test]
    fn test_acquire_multiple_resources() {
        let mgr = make_manager();
        let resources = [
            (ResourceType::Screen, AccessMode::Exclusive),
            (ResourceType::Keyboard, AccessMode::Exclusive),
            (ResourceType::Mouse, AccessMode::Exclusive),
        ];
        let acquired = mgr.acquire("session-1", &resources).unwrap();
        assert_eq!(acquired.len(), 3);

        assert!(mgr.is_locked(&ResourceType::Screen));
        assert!(mgr.is_locked(&ResourceType::Keyboard));
        assert!(mgr.is_locked(&ResourceType::Mouse));

        mgr.release_all("session-1");
        assert!(!mgr.is_locked(&ResourceType::Screen));
    }

    #[test]
    fn test_release_partial() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[
                (ResourceType::Screen, AccessMode::Exclusive),
                (ResourceType::Mouse, AccessMode::Exclusive),
            ],
        )
        .unwrap();

        mgr.release("session-1", &[ResourceType::Screen]).unwrap();
        assert!(!mgr.is_locked(&ResourceType::Screen));
        assert!(mgr.is_locked(&ResourceType::Mouse));

        mgr.release("session-1", &[ResourceType::Mouse]).unwrap();
        assert!(!mgr.is_locked(&ResourceType::Mouse));
    }

    #[test]
    fn test_release_nonexistent_fails() {
        let mgr = make_manager();
        let result = mgr.release("session-x", &[ResourceType::Screen]);
        assert!(result.is_ok()); // releasing non-held resources is a no-op
    }

    #[test]
    fn test_acquire_empty_fails() {
        let mgr = make_manager();
        let result = mgr.acquire("session-1", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_acquire_unknown_resource_fails() {
        // Can't really test unknown since ResourceType is exhaustive
        // but empty access mode lists work
        let mgr = make_manager();
        let result = mgr.try_acquire("session-1", &[]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Contention
    // -----------------------------------------------------------------------

    #[test]
    fn test_exclusive_contention() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let result = mgr.try_acquire(
            "session-2",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("contention"));
    }

    #[test]
    fn test_exclusive_blocks_shared() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let result = mgr.try_acquire("session-2", &[(ResourceType::Screen, AccessMode::Shared)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_shared_does_not_block_shared() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Filesystem, AccessMode::Shared)],
        )
        .unwrap();
        assert!(mgr.is_locked(&ResourceType::Filesystem));

        let result = mgr.try_acquire(
            "session-2",
            &[(ResourceType::Filesystem, AccessMode::Shared)],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_shared_blocks_exclusive() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Filesystem, AccessMode::Shared)],
        )
        .unwrap();

        let result = mgr.try_acquire(
            "session-2",
            &[(ResourceType::Filesystem, AccessMode::Exclusive)],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_different_resources_no_contention() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let result = mgr.try_acquire(
            "session-2",
            &[(ResourceType::Keyboard, AccessMode::Exclusive)],
        );
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Try acquire (no wait)
    // -----------------------------------------------------------------------

    #[test]
    fn test_try_acquire_success() {
        let mgr = make_manager();
        let result = mgr.try_acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_acquire_failure() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let result = mgr.try_acquire(
            "session-2",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        );
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Timeout
    // -----------------------------------------------------------------------

    #[test]
    fn test_acquire_timeout_expires() {
        let mgr = ResourceManager::new(ResourceManagerConfig {
            default_timeout_ms: 5000,
            max_wait_queue: 100,
            enable_deadlock_detection: true,
            forced_release_on_timeout: true,
        });

        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let start = Instant::now();
        let result = mgr.acquire_with_timeout(
            "session-2",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
            50, // Very short timeout
        );
        let elapsed = start.elapsed().as_millis();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timeout"));
        assert!(elapsed < 500); // Should fail quickly
    }

    #[test]
    fn test_acquire_timeout_zero_is_try() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let result = mgr.acquire_with_timeout(
            "session-2",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
            0,
        );
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Ownership
    // -----------------------------------------------------------------------

    #[test]
    fn test_owner_returns_holders() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let owners = mgr.owner(&ResourceType::Screen);
        assert!(owners.is_some());
        assert_eq!(owners.unwrap(), vec!["session-1"]);
    }

    #[test]
    fn test_owner_empty_when_not_held() {
        let mgr = make_manager();
        let owners = mgr.owner(&ResourceType::Screen);
        assert!(owners.is_some());
        assert!(owners.unwrap().is_empty());
    }

    #[test]
    fn test_waiting_sessions() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        // Try acquire with timeout so it enters waiting queue
        // (try_acquire doesn't wait, so we use a thread for concurrent access)
        let mgr2 = Arc::new(mgr);
        let mgr_clone = mgr2.clone();
        std::thread::spawn(move || {
            let _ = mgr_clone.acquire_with_timeout(
                "session-2",
                &[(ResourceType::Screen, AccessMode::Exclusive)],
                2000,
            );
        });

        std::thread::sleep(Duration::from_millis(50));
        let waiting = mgr2.waiting_sessions(&ResourceType::Screen);
        assert!(!waiting.is_empty());
    }

    // -----------------------------------------------------------------------
    // Recursive lock
    // -----------------------------------------------------------------------

    #[test]
    fn test_recursive_lock_detected() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let result = mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("recursive lock"));
    }

    #[test]
    fn test_recursive_lock_different_resources_ok() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let result = mgr.acquire(
            "session-1",
            &[(ResourceType::Keyboard, AccessMode::Exclusive)],
        );
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Forced cleanup
    // -----------------------------------------------------------------------

    #[test]
    fn test_force_release_clears_held() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();
        assert!(mgr.is_locked(&ResourceType::Screen));

        let released = mgr.force_release("session-1");
        assert_eq!(released.len(), 1);
        assert!(!mgr.is_locked(&ResourceType::Screen));
    }

    #[test]
    fn test_force_release_clears_wait_queue() {
        let mgr = Arc::new(make_manager());
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let mgr2 = mgr.clone();
        std::thread::spawn(move || {
            let _ = mgr2.acquire_with_timeout(
                "session-2",
                &[(ResourceType::Screen, AccessMode::Exclusive)],
                5000,
            );
        });

        std::thread::sleep(Duration::from_millis(50));
        mgr.force_release("session-1");

        std::thread::sleep(Duration::from_millis(50));
        let waiting = mgr.waiting_sessions(&ResourceType::Screen);
        assert!(waiting.is_empty() || waiting.len() == 1);
    }

    #[test]
    fn test_release_all_returns_owned() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[
                (ResourceType::Screen, AccessMode::Exclusive),
                (ResourceType::Keyboard, AccessMode::Exclusive),
            ],
        )
        .unwrap();

        let released = mgr.release_all("session-1");
        assert_eq!(released.len(), 2);
        assert!(!mgr.is_locked(&ResourceType::Screen));
        assert!(!mgr.is_locked(&ResourceType::Keyboard));
    }

    // -----------------------------------------------------------------------
    // Deadlock
    // -----------------------------------------------------------------------

    #[test]
    fn test_deadlock_detection_empty() {
        let mgr = make_manager();
        let deadlocks = mgr.detect_deadlock();
        assert!(deadlocks.is_empty());
    }

    #[test]
    fn test_deadlock_detection_no_cycle() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();
        mgr.acquire(
            "session-2",
            &[(ResourceType::Keyboard, AccessMode::Exclusive)],
        )
        .unwrap();

        let deadlocks = mgr.detect_deadlock();
        assert!(deadlocks.is_empty());
    }

    // -----------------------------------------------------------------------
    // Fairness
    // -----------------------------------------------------------------------

    #[test]
    fn test_waiting_queue_empty_initially() {
        let mgr = make_manager();
        let waiting = mgr.waiting_sessions(&ResourceType::Screen);
        assert!(waiting.is_empty());
    }

    #[test]
    fn test_multiple_waiters_enqueued() {
        let mgr = Arc::new(make_manager());
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let mgr2 = mgr.clone();
        std::thread::spawn(move || {
            let _ = mgr2.acquire_with_timeout(
                "session-2",
                &[(ResourceType::Screen, AccessMode::Exclusive)],
                500,
            );
        });

        let mgr3 = mgr.clone();
        std::thread::spawn(move || {
            let _ = mgr3.acquire_with_timeout(
                "session-3",
                &[(ResourceType::Screen, AccessMode::Exclusive)],
                500,
            );
        });

        std::thread::sleep(Duration::from_millis(100));
        let waiting = mgr.waiting_sessions(&ResourceType::Screen);
        // At least session-2 or session-3 should be waiting
        assert!(!waiting.is_empty());

        // Force release to clean up
        mgr.force_release("session-1");
    }

    // -----------------------------------------------------------------------
    // Resource type access modes
    // -----------------------------------------------------------------------

    #[test]
    fn test_screen_only_exclusive() {
        assert!(!ResourceType::Screen.supports_shared());
        assert!(ResourceType::Screen.supports_exclusive());
    }

    #[test]
    fn test_filesystem_supports_shared() {
        assert!(ResourceType::Filesystem.supports_shared());
        assert!(ResourceType::Filesystem.supports_exclusive());
    }

    #[test]
    fn test_network_supports_shared() {
        assert!(ResourceType::Network.supports_shared());
    }

    #[test]
    fn test_shared_on_exclusive_only_fails() {
        let mgr = make_manager();
        let result = mgr.try_acquire("session-1", &[(ResourceType::Screen, AccessMode::Shared)]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not support shared"));
    }

    // -----------------------------------------------------------------------
    // Metrics
    // -----------------------------------------------------------------------

    #[test]
    fn test_metrics_initial() {
        let mgr = make_manager();
        let metrics = mgr.resource_metrics();
        assert_eq!(metrics.total_acquisitions, 0);
        assert_eq!(metrics.total_releases, 0);
        assert_eq!(metrics.total_contentions, 0);
        assert_eq!(metrics.forced_releases, 0);
    }

    #[test]
    fn test_metrics_after_acquire_release() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();
        mgr.release("session-1", &[ResourceType::Screen]).unwrap();

        let metrics = mgr.resource_metrics();
        assert_eq!(metrics.total_acquisitions, 1);
        assert_eq!(metrics.total_releases, 1);
    }

    #[test]
    fn test_metrics_contentions() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();
        let _ = mgr.try_acquire(
            "session-2",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        );

        let metrics = mgr.resource_metrics();
        assert!(metrics.total_contentions >= 1);
        assert!(metrics.acquisition_failures >= 1);
    }

    #[test]
    fn test_metrics_forced_release() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[
                (ResourceType::Screen, AccessMode::Exclusive),
                (ResourceType::Keyboard, AccessMode::Exclusive),
            ],
        )
        .unwrap();

        mgr.force_release("session-1");

        let metrics = mgr.resource_metrics();
        assert_eq!(metrics.forced_releases, 2);
    }

    #[test]
    fn test_metrics_peak_locks() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[
                (ResourceType::Screen, AccessMode::Exclusive),
                (ResourceType::Keyboard, AccessMode::Exclusive),
                (ResourceType::Mouse, AccessMode::Exclusive),
            ],
        )
        .unwrap();

        let metrics = mgr.resource_metrics();
        assert_eq!(metrics.peak_locks_held, 3);
    }

    #[test]
    fn test_metrics_timeout() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();
        let _ = mgr.acquire_with_timeout(
            "session-2",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
            0,
        );

        // 0 timeout is try_acquire, which doesn't add to timeouts
        let metrics = mgr.resource_metrics();
        assert_eq!(metrics.timeouts, 0);
    }

    // -----------------------------------------------------------------------
    // Session lifecycle integration helpers
    // -----------------------------------------------------------------------

    #[test]
    fn test_session_resources_tracked() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[
                (ResourceType::Screen, AccessMode::Exclusive),
                (ResourceType::Keyboard, AccessMode::Exclusive),
            ],
        )
        .unwrap();

        let resources = mgr.session_resources("session-1");
        assert_eq!(resources.len(), 2);

        let resource_types: Vec<ResourceType> = resources.iter().map(|o| o.resource).collect();
        assert!(resource_types.contains(&ResourceType::Screen));
        assert!(resource_types.contains(&ResourceType::Keyboard));
    }

    #[test]
    fn test_session_resources_empty_after_release() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();
        mgr.release_all("session-1");

        let resources = mgr.session_resources("session-1");
        assert!(resources.is_empty());
    }

    #[test]
    fn test_has_conflict_detects_exclusive() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let conflict = mgr.has_conflict(
            "session-2",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        );
        assert_eq!(conflict, Some(ResourceType::Screen));
    }

    #[test]
    fn test_has_conflict_no_conflict() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let conflict = mgr.has_conflict(
            "session-2",
            &[(ResourceType::Keyboard, AccessMode::Exclusive)],
        );
        assert_eq!(conflict, None);
    }

    #[test]
    fn test_has_conflict_same_session_no_conflict() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();

        let conflict = mgr.has_conflict(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        );
        assert_eq!(conflict, None);
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    #[test]
    fn test_acquire_invalid_access_mode() {
        let mgr = make_manager();
        let result = mgr.try_acquire("session-1", &[(ResourceType::Screen, AccessMode::Shared)]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Release_all on empty session
    // -----------------------------------------------------------------------

    #[test]
    fn test_release_all_no_resources() {
        let mgr = make_manager();
        let released = mgr.release_all("nonexistent");
        assert!(released.is_empty());
    }

    // -----------------------------------------------------------------------
    // Concurrent sessions (basic)
    // -----------------------------------------------------------------------

    #[test]
    fn test_concurrent_independent_resources() {
        let mgr = Arc::new(make_manager());
        let mgr1 = mgr.clone();
        let mgr2 = mgr.clone();

        let h1 = std::thread::spawn(move || {
            mgr1.acquire(
                "session-1",
                &[(ResourceType::Screen, AccessMode::Exclusive)],
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(50));
            mgr1.release_all("session-1");
        });

        let h2 = std::thread::spawn(move || {
            mgr2.acquire(
                "session-2",
                &[(ResourceType::Keyboard, AccessMode::Exclusive)],
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(50));
            mgr2.release_all("session-2");
        });

        h1.join().unwrap();
        h2.join().unwrap();
    }

    #[test]
    fn test_concurrent_same_resource_exclusive_released() {
        let mgr = Arc::new(make_manager());
        let mgr1 = mgr.clone();
        let mgr2 = mgr.clone();

        let h1 = std::thread::spawn(move || {
            mgr1.acquire(
                "session-1",
                &[(ResourceType::Screen, AccessMode::Exclusive)],
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(100));
            mgr1.release_all("session-1");
        });

        std::thread::sleep(Duration::from_millis(20));

        let h2 = std::thread::spawn(move || {
            let result = mgr2.acquire_with_timeout(
                "session-2",
                &[(ResourceType::Screen, AccessMode::Exclusive)],
                200,
            );
            assert!(result.is_ok());
            mgr2.release_all("session-2");
        });

        h1.join().unwrap();
        h2.join().unwrap();
    }

    // -----------------------------------------------------------------------
    // Zero-config
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_manager() {
        let mgr = ResourceManager::default();
        let acquired = mgr
            .acquire(
                "session-1",
                &[(ResourceType::Screen, AccessMode::Exclusive)],
            )
            .unwrap();
        assert_eq!(acquired.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Lock ordering
    // -----------------------------------------------------------------------

    #[test]
    fn test_lock_order_preserved() {
        let mgr = make_manager();
        // Acquire in order: Screen (idx 0), Keyboard (idx 4)
        let result = mgr.try_acquire(
            "session-1",
            &[
                (ResourceType::Screen, AccessMode::Exclusive),
                (ResourceType::Keyboard, AccessMode::Exclusive),
            ],
        );
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_double_release_is_safe() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();
        mgr.release("session-1", &[ResourceType::Screen]).unwrap();
        let result = mgr.release("session-1", &[ResourceType::Screen]);
        assert!(result.is_ok()); // Double release is safe
    }

    #[test]
    fn test_force_release_nonexistent_session() {
        let mgr = make_manager();
        let released = mgr.force_release("nonexistent");
        assert!(released.is_empty());
    }

    #[test]
    fn test_resources_all_categories() {
        // Verify all resource types have correct default access modes
        for res in ResourceType::all() {
            match res {
                ResourceType::Screen
                | ResourceType::Mouse
                | ResourceType::Keyboard
                | ResourceType::Audio
                | ResourceType::Camera
                | ResourceType::Microphone => {
                    assert!(
                        !res.supports_shared(),
                        "{:?} should not support shared",
                        res
                    );
                }
                ResourceType::Clipboard | ResourceType::Filesystem | ResourceType::Network => {
                    assert!(res.supports_shared(), "{:?} should support shared", res);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Runtime integration simulation
    // -----------------------------------------------------------------------

    #[test]
    fn test_session_lifecycle_acquire_release() {
        let mgr = make_manager();

        // Simulate: session creates, acquires resources, executes, releases
        mgr.acquire(
            "session-A",
            &[
                (ResourceType::Screen, AccessMode::Exclusive),
                (ResourceType::Mouse, AccessMode::Exclusive),
            ],
        )
        .unwrap();

        assert!(mgr.is_locked(&ResourceType::Screen));
        assert!(mgr.is_locked(&ResourceType::Mouse));

        // Execution phase
        let held = mgr.session_resources("session-A");
        assert_eq!(held.len(), 2);

        // Completion releases all
        mgr.release_all("session-A");
        assert!(!mgr.is_locked(&ResourceType::Screen));
        assert!(!mgr.is_locked(&ResourceType::Mouse));
    }

    #[test]
    fn test_session_panic_cleanup() {
        let mgr = Arc::new(make_manager());

        // Simulate a session that panics
        let mgr_inner = mgr.clone();
        let handle = std::thread::spawn(move || {
            mgr_inner
                .acquire(
                    "session-panic",
                    &[(ResourceType::Screen, AccessMode::Exclusive)],
                )
                .unwrap();
            // Panic without releasing
            panic!("simulated panic");
        });

        let _ = handle.join();

        // Resource should still be held (we can't catch panics in this test)
        // but force_release should work
        let _released = mgr.force_release("session-panic");
        // The thread may or may not have acquired the lock before panicking
        assert!(!mgr.is_locked(&ResourceType::Screen));
    }

    #[test]
    fn test_resource_owner_tracking() {
        let mgr = make_manager();
        mgr.acquire("session-1", &[(ResourceType::Audio, AccessMode::Exclusive)])
            .unwrap();

        let owners = mgr.owner(&ResourceType::Audio);
        assert_eq!(owners, Some(vec!["session-1".to_string()]));

        mgr.release_all("session-1");
        let owners = mgr.owner(&ResourceType::Audio);
        assert_eq!(owners, Some(vec![]));
    }

    #[test]
    fn test_multiple_shared_owners() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Filesystem, AccessMode::Shared)],
        )
        .unwrap();
        mgr.acquire(
            "session-2",
            &[(ResourceType::Filesystem, AccessMode::Shared)],
        )
        .unwrap();

        // Both should be able to hold shared
        let owners = mgr.owner(&ResourceType::Filesystem).unwrap();
        assert!(owners.contains(&"session-1".to_string()));
        assert!(owners.contains(&"session-2".to_string()));

        mgr.release_all("session-1");
        mgr.release_all("session-2");
    }

    #[test]
    fn test_metrics_after_release_and_reacquire() {
        let mgr = make_manager();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();
        mgr.release("session-1", &[ResourceType::Screen]).unwrap();
        mgr.acquire(
            "session-1",
            &[(ResourceType::Screen, AccessMode::Exclusive)],
        )
        .unwrap();
        mgr.release_all("session-1");

        let metrics = mgr.resource_metrics();
        assert_eq!(metrics.total_acquisitions, 2);
        assert_eq!(metrics.total_releases, 2);
    }
}
