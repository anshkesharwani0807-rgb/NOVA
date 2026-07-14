use chrono::{DateTime, Local};
use nova_kernel::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use uuid::Uuid;

use crate::engine::VisionEngine;
use crate::hashing::ImageHash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Serialize, Deserialize)]
pub enum AnalysisPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisJob {
    pub id: String,
    pub image_bytes: Vec<u8>,
    pub priority: AnalysisPriority,
    pub created_at: DateTime<Local>,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingJob {
    pub id: String,
    pub priority: AnalysisPriority,
    pub created_at: DateTime<Local>,
    pub wait_time_ms: u64,
}

pub struct VisionManager {
    engine: Arc<VisionEngine>,
    queue: RwLock<VecDeque<AnalysisJob>>,
    processed_hashes: RwLock<Vec<(ImageHash, DateTime<Local>)>>,
    dedup_window_secs: u64,
    processing: RwLock<bool>,
}

impl VisionManager {
    pub fn new(engine: Arc<VisionEngine>) -> Self {
        Self {
            engine,
            queue: RwLock::new(VecDeque::new()),
            processed_hashes: RwLock::new(Vec::new()),
            dedup_window_secs: 300,
            processing: RwLock::new(false),
        }
    }

    pub fn submit(
        &self,
        bytes: Vec<u8>,
        priority: AnalysisPriority,
        correlation_id: Uuid,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let job = AnalysisJob {
            id: id.clone(),
            image_bytes: bytes,
            priority,
            created_at: Local::now(),
            correlation_id,
        };
        let mut queue = self.queue.write();
        match priority {
            AnalysisPriority::Critical => queue.push_front(job),
            AnalysisPriority::High => {
                let insert_at = queue
                    .iter()
                    .position(|j| j.priority < AnalysisPriority::High)
                    .unwrap_or(queue.len());
                queue.insert(insert_at, job);
            }
            _ => queue.push_back(job),
        }
        id
    }

    pub fn pending_count(&self) -> usize {
        self.queue.read().len()
    }

    pub fn is_processing(&self) -> bool {
        *self.processing.read()
    }

    pub fn pending_jobs(&self) -> Vec<PendingJob> {
        let now = Local::now();
        self.queue
            .read()
            .iter()
            .map(|j| {
                let ms = (now - j.created_at).num_milliseconds() as u64;
                PendingJob {
                    id: j.id.clone(),
                    priority: j.priority,
                    created_at: j.created_at,
                    wait_time_ms: ms,
                }
            })
            .collect()
    }

    pub fn is_duplicate(&self, hash: &ImageHash) -> bool {
        let cutoff = Local::now() - chrono::Duration::seconds(self.dedup_window_secs as i64);
        self.processed_hashes
            .read()
            .iter()
            .any(|(h, t)| *t > cutoff && h.is_similar(hash))
    }

    pub fn mark_processed(&self, hash: ImageHash) {
        let mut hashes = self.processed_hashes.write();
        hashes.push((hash, Local::now()));
        let cutoff = Local::now() - chrono::Duration::seconds(self.dedup_window_secs as i64 * 2);
        hashes.retain(|(_, t)| *t > cutoff);
    }

    pub async fn process_next(&self) -> Result<Option<AnalysisJob>> {
        let job = self.queue.write().pop_front();
        match job {
            Some(job) => {
                *self.processing.write() = true;
                let hash = self.engine.hash_image(&job.image_bytes).await;
                if let Ok(ref h) = hash {
                    self.mark_processed(*h);
                }
                *self.processing.write() = false;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    pub fn clear_queue(&self) {
        self.queue.write().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock::MockVisionProvider;

    fn setup() -> Arc<VisionManager> {
        let provider =
            Arc::new(MockVisionProvider::new()) as Arc<dyn crate::providers::VisionProvider>;
        let engine = Arc::new(VisionEngine::new(provider));
        Arc::new(VisionManager::new(engine))
    }

    #[test]
    fn test_submit_and_pending() {
        let mgr = setup();
        let cid = Uuid::new_v4();
        let id = mgr.submit(vec![0u8; 16], AnalysisPriority::Normal, cid);
        assert_eq!(mgr.pending_count(), 1);
        assert!(!id.is_empty());
    }

    #[test]
    fn test_priority_ordering() {
        let mgr = setup();
        let cid = Uuid::new_v4();
        mgr.submit(vec![0u8; 16], AnalysisPriority::Low, cid);
        mgr.submit(vec![0u8; 16], AnalysisPriority::Critical, cid);
        let jobs = mgr.pending_jobs();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].priority, AnalysisPriority::Critical);
    }

    #[tokio::test]
    async fn test_process_next() {
        let mgr = setup();
        let cid = Uuid::new_v4();
        mgr.submit(vec![0u8; 16], AnalysisPriority::Normal, cid);
        let result = mgr.process_next().await.unwrap();
        assert!(result.is_some());
        assert_eq!(mgr.pending_count(), 0);
    }

    #[tokio::test]
    async fn test_process_next_empty() {
        let mgr = setup();
        let result = mgr.process_next().await.unwrap();
        assert!(result.is_none());
    }
}
