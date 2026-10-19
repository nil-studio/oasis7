//! Work bounds; the soft deadline never interrupts a filesystem operation.
use std::time::{Duration, Instant};
#[derive(Clone, Copy)]
pub(crate) struct GenerationBudget {
    pub(super) objects: usize,
    pub(super) bytes: usize,
    deadline: Option<Instant>,
}
impl GenerationBudget {
    pub(crate) fn explicit() -> Self {
        Self {
            objects: 1024,
            bytes: 8 * 1024 * 1024,
            deadline: None,
        }
    }
    pub(crate) fn automatic() -> Self {
        Self {
            objects: 16,
            bytes: 64 * 1024,
            deadline: Some(Instant::now() + Duration::from_millis(10)),
        }
    }
    pub(super) fn expired(self) -> bool {
        self.deadline.is_some_and(|end| Instant::now() >= end)
    }
}
