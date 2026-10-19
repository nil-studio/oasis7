//! Distributed storage coordination built on the local CAS and pure proof layers.
use oasis7_proto::world_error::WorldError;

pub use oasis7_cas::{
    BlobReadReceipt, BlobStore, FileIndexAuditReport, FileIndexManifest, FileIndexManifestRef,
    FileMetadata, FileStore, HashAlgorithm, LocalCasStore, blake3_hex, decode_local_blob_into,
};
pub use oasis7_cas::{
    NodeStorageChallengeStats, STORAGE_CHALLENGE_DEFAULT_ALLOWED_CLOCK_SKEW_MS,
    STORAGE_CHALLENGE_DEFAULT_MAX_SAMPLE_BYTES, STORAGE_CHALLENGE_DEFAULT_TTL_MS,
    STORAGE_CHALLENGE_DEFAULTS_PER_TICK, STORAGE_CHALLENGE_PROOF_KIND_CHUNK_HASH_V1,
    STORAGE_CHALLENGE_VERSION, StorageChallenge, StorageChallengeAdaptivePolicy,
    StorageChallengeProbeConfig, StorageChallengeProbeCursorState, StorageChallengeProbeReport,
    StorageChallengeReceipt, StorageChallengeRequest, storage_challenge_receipt_to_proof_semantics,
    summarize_node_storage_challenge_stats, verify_storage_challenge_receipt,
};
use oasis7_cas::{to_canonical_cbor, validate_hash};

pub mod controlled_authority;
mod feedback;
mod feedback_p2p;
mod replication;
pub mod storage_inventory;

pub use feedback::*;
pub use feedback_p2p::*;
pub use oasis7_cas::{assemble_journal, assemble_snapshot, segment_journal, segment_snapshot};
pub use replication::*;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
mod test_process;
