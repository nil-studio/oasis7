//! Test-only per-backend accounting at actual BLAKE3 input sites. These
//! counters never enter a wire message, material budget or authority decision.
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
#[derive(Default)]
pub(crate) struct Counts {
    object_payload_bytes: AtomicU64,
    canonical_hasher_feed_bytes: AtomicU64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub object_payload_bytes: u64,
    /// Both actual Hasher::update calls, including canonical metadata. This is
    /// deliberately not described as object payload size.
    pub canonical_hasher_feed_bytes: u64,
}
impl Counts {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            object_payload_bytes: self.object_payload_bytes.load(Ordering::Relaxed),
            canonical_hasher_feed_bytes: self.canonical_hasher_feed_bytes.load(Ordering::Relaxed),
        }
    }
}
thread_local! {
    static ACTIVE: RefCell<Option<Arc<Counts>>> = const { RefCell::new(None) };
    static INPUT_OBJECT: Cell<bool> = const { Cell::new(false) };
}
pub(crate) struct Scope {
    previous: Option<Arc<Counts>>,
    _same_thread: PhantomData<Rc<()>>,
}
impl Scope {
    pub fn enter(counts: Arc<Counts>) -> Self {
        let previous = ACTIVE.with(|active| active.replace(Some(counts)));
        Self {
            previous,
            _same_thread: PhantomData,
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            active.replace(self.previous.take());
        });
    }
}
pub(crate) struct ObjectHashScope {
    previous: bool,
    _same_thread: PhantomData<Rc<()>>,
}
impl ObjectHashScope {
    pub fn enter() -> Self {
        Self {
            previous: INPUT_OBJECT.with(|active| active.replace(true)),
            _same_thread: PhantomData,
        }
    }
}
impl Drop for ObjectHashScope {
    fn drop(&mut self) {
        INPUT_OBJECT.with(|active| active.set(self.previous));
    }
}
pub(crate) fn object_hash_completed(bytes: usize) {
    if INPUT_OBJECT.with(Cell::get) {
        ACTIVE.with(|active| {
            if let Some(counts) = active.borrow().as_ref() {
                counts
                    .object_payload_bytes
                    .fetch_add(bytes as u64, Ordering::Relaxed);
            }
        });
    }
}
pub(crate) fn canonical_hasher_updated(bytes: usize) {
    ACTIVE.with(|active| {
        if let Some(counts) = active.borrow().as_ref() {
            counts
                .canonical_hasher_feed_bytes
                .fetch_add(bytes as u64, Ordering::Relaxed);
        }
    });
}
