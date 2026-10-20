use std::sync::Mutex;

use sha2::{Digest, Sha256};

use super::explorer_p0_store::{EXPLORER_STORE, ExplorerStore};

pub(super) fn build_tx_hash(
    action_id: u64,
    from_account_id: &str,
    to_account_id: &str,
    amount: u64,
    nonce: u64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(action_id.to_be_bytes());
    hasher.update(from_account_id.as_bytes());
    hasher.update([0]);
    hasher.update(to_account_id.as_bytes());
    hasher.update([0]);
    hasher.update(amount.to_be_bytes());
    hasher.update(nonce.to_be_bytes());
    format!("0x{}", hex::encode(hasher.finalize()))
}

pub(super) fn lock_store() -> std::sync::MutexGuard<'static, ExplorerStore> {
    EXPLORER_STORE
        .get_or_init(|| Mutex::new(ExplorerStore::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
pub(super) fn reset_store_for_tests() {
    let mut store = lock_store();
    store.persistence_path = None;
    store.loaded = false;
    store.blocks_by_height.clear();
    store.txs_by_hash.clear();
    store.tx_hash_by_action_id.clear();
}
