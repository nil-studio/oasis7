//! Parsed untrusted proofs keyed by the complete bytes read from a publication.
//! No path, material, authority, qualification, or failed parse is cached.
use super::FormalQualifiedCommitProofV1;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
};

const MAX_ENTRIES: usize = 256;
const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(super) struct Key {
    digest: [u8; 32],
    length: usize,
}

pub(super) fn key(bytes: &[u8]) -> Key {
    let mut hash = blake3::Hasher::new_derive_key("oasis7.formal.published-proof.parse-memo.v1");
    hash.update(bytes);
    Key {
        digest: *hash.finalize().as_bytes(),
        length: bytes.len(),
    }
}

#[derive(Default)]
struct Memo {
    proofs: HashMap<Key, (Arc<FormalQualifiedCommitProofV1>, usize)>,
    order: VecDeque<Key>,
    bytes: usize,
}

impl Memo {
    fn get(&self, key: &Key) -> Option<Arc<FormalQualifiedCommitProofV1>> {
        self.proofs.get(key).map(|(proof, _)| proof.clone())
    }

    fn insert(&mut self, key: Key, proof: Arc<FormalQualifiedCommitProofV1>, weight: usize) {
        if weight > MAX_BYTES || self.proofs.contains_key(&key) {
            return;
        }
        while self.proofs.len() >= MAX_ENTRIES || self.bytes > MAX_BYTES - weight {
            let Some(oldest) = self.order.pop_front() else {
                // An inconsistent optimization must never affect the decoder.
                self.proofs.clear();
                self.bytes = 0;
                return;
            };
            let Some((_, removed)) = self.proofs.remove(&oldest) else {
                self.proofs.clear();
                self.order.clear();
                self.bytes = 0;
                return;
            };
            self.bytes = self.bytes.saturating_sub(removed);
        }
        self.bytes += weight;
        self.order.push_back(key);
        self.proofs.insert(key, (proof, weight));
    }
}

fn memo() -> &'static Mutex<Memo> {
    static MEMO: OnceLock<Mutex<Memo>> = OnceLock::new();
    MEMO.get_or_init(|| Mutex::new(Memo::default()))
}

pub(super) fn get(key: &Key) -> Option<FormalQualifiedCommitProofV1> {
    // Poisoning disables this optimization instead of trusting partial state.
    let proof = cached(memo(), key)?;
    Some((*proof).clone())
}

fn cached(cache: &Mutex<Memo>, key: &Key) -> Option<Arc<FormalQualifiedCommitProofV1>> {
    cache.lock().ok()?.get(key)
}

pub(super) fn insert(key: Key, proof: &FormalQualifiedCommitProofV1) {
    let Ok(bytes) = serde_cbor::to_vec(proof) else {
        return;
    };
    if bytes.len() > MAX_BYTES {
        return;
    }
    let proof = Arc::new(proof.clone());
    if let Ok(mut memo) = memo().lock() {
        memo.insert(key, proof, bytes.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsed_proof_memo_is_byte_bound_fifo_and_weight_bounded() {
        let (_, _, _, proof) = crate::formal_read_fixtures::actual_world_initial_fixture();
        let mut memo = Memo::default();
        let first = key(b"first");
        memo.insert(first, Arc::new(proof.clone()), 1);
        for index in 0..MAX_ENTRIES {
            memo.insert(key(&index.to_le_bytes()), Arc::new(proof.clone()), 1);
        }
        assert!(memo.get(&first).is_none());
        assert_eq!(memo.proofs.len(), MAX_ENTRIES);
        assert_eq!(memo.bytes, MAX_ENTRIES);

        let full = key(b"full");
        memo.insert(full, Arc::new(proof.clone()), MAX_BYTES);
        assert_eq!(memo.proofs.len(), 1);
        assert_eq!(memo.bytes, MAX_BYTES);
        assert_eq!(memo.get(&full).as_deref(), Some(&proof));
        let oversized = key(b"oversized");
        memo.insert(oversized, Arc::new(proof.clone()), MAX_BYTES + 1);
        assert!(memo.get(&oversized).is_none());
        assert_eq!(memo.get(&full).as_deref(), Some(&proof));
        assert_ne!(key(b"same"), key(b"changed"));
    }

    #[test]
    fn poisoned_local_memo_is_a_miss_without_affecting_global_decoder() {
        let (_, _, _, proof) = crate::formal_read_fixtures::actual_world_initial_fixture();
        let local = Arc::new(Mutex::new(Memo::default()));
        let key = key(b"local poison");
        local.lock().unwrap().insert(key, Arc::new(proof), 1);
        let poison = local.clone();
        assert!(
            std::thread::spawn(move || {
                let _guard = poison.lock().unwrap();
                panic!("poison only this test's independent memo");
            })
            .join()
            .is_err()
        );
        assert!(cached(&local, &key).is_none());
    }
}
