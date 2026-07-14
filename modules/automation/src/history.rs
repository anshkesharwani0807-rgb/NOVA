use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    Completed,
    Failed,
    Partial,
    Cancelled,
    Running,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub execution_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: ExecutionStatus,
    pub started_at: i64,
    pub duration_ms: i64,
    pub steps_succeeded: usize,
    pub steps_failed: usize,
    pub steps_total: usize,
    pub error: Option<String>,
}

pub trait HistoryStore: Send + Sync {
    fn store(&self, record: ExecutionRecord);
    fn recent(&self, limit: usize) -> Vec<ExecutionRecord>;
    fn by_workflow(&self, workflow_id: &str, limit: usize) -> Vec<ExecutionRecord>;
    fn by_status(&self, status: &ExecutionStatus, limit: usize) -> Vec<ExecutionRecord>;
    fn clear(&self);
    fn count(&self) -> usize;
}

pub struct InMemoryHistory {
    records: RwLock<VecDeque<ExecutionRecord>>,
    max_entries: usize,
}

impl InMemoryHistory {
    pub fn new() -> Self {
        Self::with_max(500)
    }

    pub fn with_max(max_entries: usize) -> Self {
        Self {
            records: RwLock::new(VecDeque::with_capacity(max_entries)),
            max_entries,
        }
    }
}

impl Default for InMemoryHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryStore for InMemoryHistory {
    fn store(&self, record: ExecutionRecord) {
        let mut records = self.records.write();
        if records.len() >= self.max_entries {
            records.pop_front();
        }
        records.push_back(record);
    }

    fn recent(&self, limit: usize) -> Vec<ExecutionRecord> {
        let records = self.records.read();
        records.iter().rev().take(limit).cloned().collect()
    }

    fn by_workflow(&self, workflow_id: &str, limit: usize) -> Vec<ExecutionRecord> {
        let records = self.records.read();
        records
            .iter()
            .rev()
            .filter(|r| r.workflow_id == workflow_id)
            .take(limit)
            .cloned()
            .collect()
    }

    fn by_status(&self, status: &ExecutionStatus, limit: usize) -> Vec<ExecutionRecord> {
        let records = self.records.read();
        records
            .iter()
            .rev()
            .filter(|r| r.status == *status)
            .take(limit)
            .cloned()
            .collect()
    }

    fn clear(&self) {
        self.records.write().clear();
    }

    fn count(&self) -> usize {
        self.records.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(
        execution_id: &str,
        workflow_id: &str,
        status: ExecutionStatus,
    ) -> ExecutionRecord {
        ExecutionRecord {
            execution_id: execution_id.into(),
            workflow_id: workflow_id.into(),
            workflow_name: workflow_id.into(),
            status,
            started_at: chrono::Utc::now().timestamp_millis(),
            duration_ms: 0,
            steps_succeeded: 0,
            steps_failed: 0,
            steps_total: 0,
            error: None,
        }
    }

    #[test]
    fn test_record_creation() {
        let r = make_record("exec1", "wf1", ExecutionStatus::Running);
        assert_eq!(r.execution_id, "exec1");
        assert_eq!(r.status, ExecutionStatus::Running);
    }

    #[test]
    fn test_in_memory_store_and_retrieve() {
        let history = InMemoryHistory::new();
        history.store(make_record("exec1", "wf1", ExecutionStatus::Running));
        history.store(make_record("exec2", "wf1", ExecutionStatus::Completed));
        let records = history.by_workflow("wf1", 10);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].status, ExecutionStatus::Completed);
    }

    #[test]
    fn test_recent() {
        let history = InMemoryHistory::new();
        for i in 0..3 {
            history.store(make_record(
                &format!("e{}", i),
                "w1",
                ExecutionStatus::Running,
            ));
        }
        let recent = history.recent(2);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_count() {
        let history = InMemoryHistory::new();
        assert_eq!(history.count(), 0);
        history.store(make_record("e1", "w1", ExecutionStatus::Running));
        assert_eq!(history.count(), 1);
    }

    #[test]
    fn test_max_entries() {
        let history = InMemoryHistory::with_max(2);
        for i in 0..5 {
            history.store(make_record(
                &format!("e{}", i),
                "w1",
                ExecutionStatus::Running,
            ));
        }
        assert_eq!(history.count(), 2);
    }

    #[test]
    fn test_by_workflow_nonexistent() {
        let history = InMemoryHistory::new();
        let records = history.by_workflow("nonexistent", 10);
        assert!(records.is_empty());
    }

    #[test]
    fn test_record_serialization() {
        let r = make_record("e1", "w1", ExecutionStatus::Running);
        let json = serde_json::to_string(&r).unwrap();
        let deserialized: ExecutionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.execution_id, "e1");
        assert_eq!(deserialized.status, ExecutionStatus::Running);
    }
}
