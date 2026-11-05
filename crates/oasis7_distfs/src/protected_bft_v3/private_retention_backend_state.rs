//! Transaction state shared by the Memory adapter and staged file commits.
//! Cloning preserves immutable original Arc owners; it never copies Input payloads.
use super::*;

#[derive(Clone)]
pub(in super::super) struct BackendState {
    pub(super) material_epoch: u64,
    pub(super) validated_inputs: ValidatedMemoryInputs,
    #[cfg(test)]
    pub(super) hash_probe: Arc<hash_probe::Counts>,
    pub(super) _preparation: Option<Arc<FreshPreparation>>,
    pub(super) creation: Option<BftPrivateRetentionCreationMarkerV1>,
    pub(super) signer: BftSignerBackendSnapshotV3,
    /// Exact original operation snapshot, distinct from the current lease/state.
    /// A subsequent retention operation is permitted only after its predecessor
    /// is read back. Older receipts/maps belong to the immutable archive chain.
    pub(super) original: Option<Arc<BftPrivateRetentionSnapshotV1>>,
    pub(super) original_observed: bool,
    pub(super) operations: std::collections::BTreeMap<String, Arc<BftPrivateRetentionSnapshotV1>>,
    pub(super) history: super::super::private_retention_history::BftPrivateRetentionHistoryV1,
    pub(super) seals: std::collections::BTreeMap<
        String,
        Arc<super::super::private_retention_seal::BftPrivateRetentionSealV1>,
    >,
    pub(super) sealed_inputs: BftPrivateRetentionInputInventoryV1,
}
