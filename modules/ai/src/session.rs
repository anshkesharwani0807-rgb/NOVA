//! Conversation context / session management for the AI Runtime (Milestone 6).
//!
//! A [`Session`] holds the rolling message history for one conversation; the
//! [`ConversationManager`] owns many sessions keyed by id. History is bounded so context
//! stays memory-efficient over long conversations.

use crate::provider::Message;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// A single conversation's bounded message history.
pub struct Session {
    pub id: String,
    history: RwLock<Vec<Message>>,
    max_history: usize,
}

impl Session {
    pub fn new(id: impl Into<String>, max_history: usize) -> Self {
        Self {
            id: id.into(),
            history: RwLock::new(Vec::new()),
            max_history: max_history.max(1),
        }
    }

    /// Append a message, trimming oldest entries beyond `max_history`.
    pub fn push(&self, message: Message) {
        let mut h = self.history.write();
        h.push(message);
        let len = h.len();
        if len > self.max_history {
            h.drain(0..len - self.max_history);
        }
    }

    pub fn history(&self) -> Vec<Message> {
        self.history.read().clone()
    }

    pub fn len(&self) -> usize {
        self.history.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.history.read().is_empty()
    }

    pub fn clear(&self) {
        self.history.write().clear();
    }
}

/// Owns and looks up conversation sessions.
pub struct ConversationManager {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
    max_history: usize,
}

impl Default for ConversationManager {
    fn default() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            max_history: 32,
        }
    }
}

impl ConversationManager {
    pub fn new(max_history: usize) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            max_history: max_history.max(1),
        }
    }

    /// Create a new session with a generated id.
    pub fn create(&self) -> Arc<Session> {
        let id = Uuid::new_v4().to_string();
        self.get_or_create(&id)
    }

    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.read().get(id).cloned()
    }

    /// Return the session for `id`, creating it if absent.
    pub fn get_or_create(&self, id: &str) -> Arc<Session> {
        if let Some(s) = self.sessions.read().get(id) {
            return s.clone();
        }
        let mut sessions = self.sessions.write();
        sessions
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Session::new(id, self.max_history)))
            .clone()
    }

    pub fn remove(&self, id: &str) -> bool {
        self.sessions.write().remove(id).is_some()
    }

    pub fn count(&self) -> usize {
        self.sessions.read().len()
    }
}
