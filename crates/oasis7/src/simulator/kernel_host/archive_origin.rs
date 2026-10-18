use super::*;

#[derive(Debug, Clone)]
pub(super) struct ArchiveOrigin {
    pub extensions: WorldSnapshotExtensions,
    pub external_chain_evidence: bool,
}
impl ArchiveOrigin {
    pub fn new(
        native: &KernelSnapshot,
        extensions: WorldSnapshotExtensions,
        events: &[oasis7_simulator::KernelEvent],
    ) -> Result<Self, PersistError> {
        let prefix = events
            .get(..native.journal_len)
            .ok_or(PersistError::SnapshotMismatch {
                expected: native.journal_len,
                actual: events.len(),
            })?;
        // Classify against the INPUT committed cut, before replay changes the model.
        let context = ChainResourceDerivationContext::simulator_default(native.time);
        let manifest = oasis7_simulator::from_simulator_state(
            context,
            &native.model,
            &native.config,
            &native.chunk_runtime,
            prefix,
        );
        let delta = oasis7_simulator::latest_from_simulator_journal(context, &manifest, prefix);
        let external_chain_evidence = extensions.chain_resource_manifest != manifest
            || extensions.latest_chain_resource_delta != delta;
        Ok(Self {
            extensions,
            external_chain_evidence,
        })
    }
}
