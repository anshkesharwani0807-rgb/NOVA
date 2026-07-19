use std::collections::HashMap;

use parking_lot::RwLock;

use crate::agent_runtime::AgentSession;

/// Persistent storage for agent sessions.
pub trait SessionStore: Send + Sync {
    fn save_session(&self, session: &AgentSession) -> Result<(), String>;
    fn load_session(&self, session_id: &str) -> Result<AgentSession, String>;
    fn delete_session(&self, session_id: &str) -> Result<(), String>;
    fn list_sessions(&self) -> Result<Vec<AgentSession>, String>;
    fn session_count(&self) -> Result<usize, String>;
}

/// In-memory session store backed by a `HashMap`.
pub struct InMemorySessionStore {
    sessions: RwLock<HashMap<String, AgentSession>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore for InMemorySessionStore {
    fn save_session(&self, session: &AgentSession) -> Result<(), String> {
        self.sessions
            .write()
            .insert(session.session_id.clone(), session.clone());
        Ok(())
    }

    fn load_session(&self, session_id: &str) -> Result<AgentSession, String> {
        self.sessions
            .read()
            .get(session_id)
            .cloned()
            .ok_or_else(|| format!("session '{}' not found", session_id))
    }

    fn delete_session(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.write();
        if sessions.remove(session_id).is_none() {
            return Err(format!("session '{}' not found for deletion", session_id));
        }
        Ok(())
    }

    fn list_sessions(&self) -> Result<Vec<AgentSession>, String> {
        Ok(self.sessions.read().values().cloned().collect())
    }

    fn session_count(&self) -> Result<usize, String> {
        Ok(self.sessions.read().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::Goal;

    fn make_session(id: &str, desc: &str) -> AgentSession {
        AgentSession::new(id, Goal::new(desc))
    }

    #[test]
    fn test_store_save_load() {
        let store = InMemorySessionStore::new();
        let session = make_session("s1", "open chrome");
        store.save_session(&session).unwrap();
        let loaded = store.load_session("s1").unwrap();
        assert_eq!(loaded.session_id, "s1");
        assert_eq!(loaded.goal.description, "open chrome");
    }

    #[test]
    fn test_store_load_nonexistent() {
        let store = InMemorySessionStore::new();
        assert!(store.load_session("nope").is_err());
    }

    #[test]
    fn test_store_delete() {
        let store = InMemorySessionStore::new();
        let session = make_session("s1", "test");
        store.save_session(&session).unwrap();
        store.delete_session("s1").unwrap();
        assert!(store.load_session("s1").is_err());
    }

    #[test]
    fn test_store_delete_nonexistent() {
        let store = InMemorySessionStore::new();
        assert!(store.delete_session("nope").is_err());
    }

    #[test]
    fn test_store_list() {
        let store = InMemorySessionStore::new();
        store.save_session(&make_session("s1", "g1")).unwrap();
        store.save_session(&make_session("s2", "g2")).unwrap();
        let list = store.list_sessions().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_store_list_empty() {
        let store = InMemorySessionStore::new();
        assert_eq!(store.list_sessions().unwrap().len(), 0);
    }

    #[test]
    fn test_store_save_overwrites() {
        let store = InMemorySessionStore::new();
        let s1 = make_session("s1", "original");
        store.save_session(&s1).unwrap();
        let s2 = AgentSession::new("s1", Goal::new("updated"));
        store.save_session(&s2).unwrap();
        let loaded = store.load_session("s1").unwrap();
        assert_eq!(loaded.goal.description, "updated");
    }

    #[test]
    fn test_store_session_count() {
        let store = InMemorySessionStore::new();
        assert_eq!(store.session_count().unwrap(), 0);
        store.save_session(&make_session("s1", "g1")).unwrap();
        store.save_session(&make_session("s2", "g2")).unwrap();
        assert_eq!(store.session_count().unwrap(), 2);
    }

    #[test]
    fn test_store_roundtrip_preserves_all_fields() {
        let store = InMemorySessionStore::new();
        let mut session = make_session("s1", "complex goal");
        session.current_step_index = 3;
        session.retry_count = 2;
        session.recovery_count = 1;
        session.replan_count = 0;
        session.metrics.completed_steps = 3;
        session.metrics.failed_steps = 1;
        store.save_session(&session).unwrap();
        let loaded = store.load_session("s1").unwrap();
        assert_eq!(loaded.current_step_index, 3);
        assert_eq!(loaded.retry_count, 2);
        assert_eq!(loaded.recovery_count, 1);
        assert_eq!(loaded.metrics.completed_steps, 3);
        assert_eq!(loaded.metrics.failed_steps, 1);
    }
}
