use chrono::{DateTime, Local};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T: Clone> {
    pub value: T,
    pub created_at: DateTime<Local>,
    pub accessed_count: u64,
    pub size_bytes: u64,
}

impl<T: Clone> CacheEntry<T> {
    pub fn new(value: T, size_bytes: u64) -> Self {
        Self {
            value,
            created_at: Local::now(),
            accessed_count: 0,
            size_bytes,
        }
    }
}

pub struct TypedCache<T: Clone + Send + Sync> {
    inner: RwLock<HashMap<String, CacheEntry<T>>>,
    max_entries: usize,
    ttl: Duration,
}

impl<T: Clone + Send + Sync> TypedCache<T> {
    pub fn new(max_entries: usize, ttl_secs: u64) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            max_entries,
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    pub fn get(&self, key: &str) -> Option<T> {
        let mut map = self.inner.write();
        if let Some(entry) = map.get_mut(key) {
            if entry.created_at + self.ttl < Local::now() {
                map.remove(key);
                return None;
            }
            entry.accessed_count += 1;
            Some(entry.value.clone())
        } else {
            None
        }
    }

    pub fn insert(&self, key: String, value: T, size_bytes: u64) {
        let mut map = self.inner.write();
        if map.len() >= self.max_entries {
            let lru = map
                .iter()
                .min_by_key(|(_, v)| v.accessed_count)
                .map(|(k, _)| k.clone());
            if let Some(k) = lru {
                map.remove(&k);
            }
        }
        map.insert(key, CacheEntry::new(value, size_bytes));
    }

    pub fn remove(&self, key: &str) {
        self.inner.write().remove(key);
    }

    pub fn clear(&self) {
        self.inner.write().clear();
    }

    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub thumbnails: usize,
    pub embeddings: usize,
    pub ocr_results: usize,
    pub captions: usize,
    pub total_estimated_bytes: u64,
}

pub struct VisionCache {
    pub thumbnails: TypedCache<Vec<u8>>,
    pub embeddings: TypedCache<Vec<f32>>,
    pub ocr_results: TypedCache<String>,
    pub captions: TypedCache<String>,
    memory_budget: RwLock<u64>,
}

impl VisionCache {
    pub fn new(max_entries: usize, ttl_secs: u64, memory_budget: u64) -> Self {
        Self {
            thumbnails: TypedCache::new(max_entries, ttl_secs),
            embeddings: TypedCache::new(max_entries / 2, ttl_secs),
            ocr_results: TypedCache::new(max_entries, ttl_secs),
            captions: TypedCache::new(max_entries, ttl_secs),
            memory_budget: RwLock::new(memory_budget),
        }
    }

    pub fn stats(&self) -> CacheStats {
        let t = self.thumbnails.len() as u64 * 256 * 256 * 4;
        let e = self.embeddings.len() as u64 * 384 * 4;
        let o = self.ocr_results.len() as u64 * 512;
        let c = self.captions.len() as u64 * 256;
        CacheStats {
            thumbnails: self.thumbnails.len(),
            embeddings: self.embeddings.len(),
            ocr_results: self.ocr_results.len(),
            captions: self.captions.len(),
            total_estimated_bytes: t + e + o + c,
        }
    }

    pub fn clear(&self) {
        self.thumbnails.clear();
        self.embeddings.clear();
        self.ocr_results.clear();
        self.captions.clear();
    }

    pub fn memory_budget(&self) -> u64 {
        *self.memory_budget.read()
    }

    pub fn set_memory_budget(&self, budget: u64) {
        *self.memory_budget.write() = budget;
    }
}
