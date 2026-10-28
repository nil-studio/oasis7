//! Pure cryptographic fact reuse within one explicit synchronous operation.
//! This stores no authority, time, nonce eligibility or validation outcome.
use std::cell::RefCell;
use zeroize::Zeroizing;
const MAX_TUPLES: usize = 128;
const MAX_BYTES: usize = 128 * 1024;
struct Entry {
    public: [u8; 32],
    signature: [u8; 64],
    canonical: Zeroizing<Vec<u8>>,
}
struct Batch {
    depth: usize,
    entries: Vec<Entry>,
    retained_bytes: usize,
}
impl Batch {
    fn new() -> Self {
        // Reserve the whole bounded tuple table and charge its metadata up front.
        Self {
            depth: 1,
            entries: Vec::with_capacity(MAX_TUPLES),
            retained_bytes: MAX_TUPLES * std::mem::size_of::<Entry>(),
        }
    }
}
thread_local! { static BATCH: RefCell<Option<Batch>> = const { RefCell::new(None) }; }
struct Guard;
impl Guard {
    fn enter() -> Self {
        BATCH.with(|state| {
            let mut state = state.borrow_mut();
            match state.as_mut() {
                Some(batch) => {
                    batch.depth = batch
                        .depth
                        .checked_add(1)
                        .expect("signature batch nesting overflow")
                }
                None => *state = Some(Batch::new()),
            }
        });
        Self
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        BATCH.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(batch) = state.as_mut() {
                batch.depth -= 1;
                if batch.depth == 0 {
                    *state = None;
                }
            }
        });
    }
}
/// Opt into bounded reuse of successful strict Ed25519 facts during this
/// synchronous closure only. Nested scopes share the outer table; returning
/// any value (including Err), or unwinding a panic, discards the outer table.
/// A returned future is not covered while subsequently polled. Callers must
/// still execute every structure, authority, replay and freshness check.
pub fn with_owner_signature_verification_batch<R>(operation: impl FnOnce() -> R) -> R {
    let _guard = Guard::enter();
    operation()
}
pub(super) fn contains(public: &[u8; 32], signature: &[u8; 64], canonical: &[u8]) -> bool {
    BATCH.with(|state| {
        state.borrow().as_ref().is_some_and(|batch| {
            batch.entries.iter().any(|entry| {
                &entry.public == public
                    && &entry.signature == signature
                    && entry.canonical.as_slice() == canonical
            })
        })
    })
}
pub(super) fn record_success(public: [u8; 32], signature: [u8; 64], canonical: &[u8]) {
    BATCH.with(|state| {
        let mut state = state.borrow_mut();
        let Some(batch) = state.as_mut() else {
            return;
        };
        let Some(bytes) = batch.retained_bytes.checked_add(canonical.len()) else {
            return;
        };
        if batch.entries.len() >= MAX_TUPLES || bytes > MAX_BYTES {
            return;
        }
        // No hash-only key and no result supplied by a caller. This function is
        // private and called only after the actual strict verifier succeeds.
        batch.entries.push(Entry {
            public,
            signature,
            canonical: Zeroizing::new(canonical.to_vec()),
        });
        batch.retained_bytes = bytes;
    });
}
#[cfg(test)]
#[path = "signature_memo_tests.rs"]
mod tests;
