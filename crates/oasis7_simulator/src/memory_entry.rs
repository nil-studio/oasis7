//! Persisted memory values shared by the kernel and host memory stores.
use super::types::WorldTime;
use serde::{Deserialize, Serialize};

/// Long-term memory entry: a summarized or indexed piece of knowledge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LongTermMemoryEntry {
    /// Unique identifier for this entry.
    pub id: String,
    /// The world time when this was stored.
    pub created_at: WorldTime,
    /// Last accessed time.
    pub last_accessed: WorldTime,
    /// Access count (for LRU-like eviction).
    pub access_count: u64,
    /// The content of the memory.
    pub content: String,
    /// Tags for categorization and retrieval.
    pub tags: Vec<String>,
    /// Importance score.
    pub importance: f64,
}

impl LongTermMemoryEntry {
    /// Create a new long-term memory entry.
    pub fn new(id: impl Into<String>, created_at: WorldTime, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            created_at,
            last_accessed: created_at,
            access_count: 0,
            content: content.into(),
            tags: Vec::new(),
            importance: 0.5,
        }
    }

    /// Add a tag to this entry.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set the importance score.
    pub fn with_importance(mut self, importance: f64) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    /// Mark this entry as accessed.
    pub fn mark_accessed(&mut self, time: WorldTime) {
        self.last_accessed = time;
        self.access_count += 1;
    }
}
