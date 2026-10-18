use oasis7_distfs::controlled_authority::formal::business_index::{
    BusinessIndexNodeReader, BusinessIndexRootV1,
};
use std::sync::Arc;

#[cfg(test)]
use crate::modules::ModuleSubscription;
#[cfg(test)]
use oasis7_wasm_router::PreparedSubscription;

#[derive(Clone)]
pub(crate) struct SharedFormalHistoryReader {
    reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
}

impl SharedFormalHistoryReader {
    pub(crate) fn new(reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>) -> Self {
        Self { reader }
    }
}

impl std::fmt::Debug for SharedFormalHistoryReader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedFormalHistoryReader(attached)")
    }
}

impl BusinessIndexNodeReader for SharedFormalHistoryReader {
    fn prefetch_path(&self, root: &BusinessIndexRootV1, key: &str) -> Result<(), String> {
        self.reader.prefetch_path(root, key)
    }

    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.reader.read_cached_node(digest)
    }

    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        self.reader.cache_content_node(digest, bytes)
    }

    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.reader.read_node(digest)
    }
}

#[derive(Debug, Clone)]
pub(super) struct PreparedSubscriptionCacheEntry {
    #[cfg(test)]
    pub(super) subscriptions: Vec<ModuleSubscription>,
    pub(super) _subscription_fingerprint: String,
    #[cfg(test)]
    pub(super) prepared: Arc<[PreparedSubscription]>,
}

#[cfg(any(test, feature = "test_tier_required", feature = "test_tier_full"))]
pub(super) fn test_module_signer_public_key_hex() -> String {
    use ed25519_dalek::SigningKey;

    let seed = crate::util::sha256_hex(b"oasis7-test-module-artifact-signer-v1");
    let seed_bytes = hex::decode(seed).expect("decode test module signing seed");
    let private_key_bytes: [u8; 32] = seed_bytes
        .as_slice()
        .try_into()
        .expect("test module signing seed is 32 bytes");
    let signing_key = SigningKey::from_bytes(&private_key_bytes);
    hex::encode(signing_key.verifying_key().to_bytes())
}
