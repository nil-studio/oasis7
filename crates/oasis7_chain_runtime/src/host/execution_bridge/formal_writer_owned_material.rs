//! Actual installed writer publication bytes; storage capture never grants authority.
use super::*;
use std::sync::Arc;
#[derive(Clone, Copy, Default)]
pub(crate) enum PublicationMaterialCaptureV1 {
    #[default]
    Disabled,
    LatestVerified,
}
impl PublicationMaterialCaptureV1 {
    pub(crate) fn for_storage_capture(enabled: bool) -> Self {
        if enabled {
            Self::LatestVerified
        } else {
            Self::Disabled
        }
    }

    pub(super) fn enabled(self) -> bool {
        matches!(self, Self::LatestVerified)
    }
}
/// This token has no wire constructor, Deserialize, Clone or caller trust flag.
/// A single adjacent publication does not establish complete cold lineage.
pub(crate) struct OwnedVerifiedPublicationMaterialV1 {
    bytes: Arc<Vec<u8>>,
    before: Option<VerifiedFormalReadAnchorV1>,
    after: VerifiedFormalReadAnchorV1,
}
pub(super) fn installed(
    before: Option<VerifiedFormalReadAnchorV1>,
    after: VerifiedFormalReadAnchorV1,
    bytes: Vec<u8>,
) -> Arc<OwnedVerifiedPublicationMaterialV1> {
    Arc::new(OwnedVerifiedPublicationMaterialV1 {
        bytes: Arc::new(bytes),
        before,
        after,
    })
}
impl OwnedVerifiedPublicationMaterialV1 {
    pub(crate) fn original_bytes(&self) -> &Arc<Vec<u8>> {
        &self.bytes
    }
    pub(crate) fn before(&self) -> Option<&VerifiedFormalReadAnchorV1> {
        self.before.as_ref()
    }
    pub(crate) fn after(&self) -> &VerifiedFormalReadAnchorV1 {
        &self.after
    }
    /// Read-only exact execution evidence from the actual installed bytes.
    /// This does not construct a Current, dispatch or archive-lineage grant.
    pub(crate) fn exact_singleton_execution(
        &self,
        raw: &[u8],
        action_id: u64,
    ) -> Result<oasis7_node::NodeExecutionCommitContext, String> {
        let before = self
            .before()
            .ok_or("publication has no captured adjacent predecessor")?;
        let stored: Published = published::decode(self.original_bytes().as_slice())?;
        if stored.schema_version != 1
            || &stored.candidate.candidate.record != self.after.head().record()
            || stored.proof.decision.record != stored.candidate.candidate.record
            || self.after.head().record().kind != FormalRecordKindV1::Execution
        {
            return Err("installed publication does not contain exact Execution proof".into());
        }
        let frozen = stored
            .candidate
            .frozen
            .as_ref()
            .ok_or("Execution Frozen certificate missing")?;
        let material = stored
            .candidate
            .frozen_material
            .as_ref()
            .ok_or("Execution Frozen material missing")?;
        verify_frozen_material(&frozen.reservation.body, material)
            .map_err(|error| error.to_string())?;
        let manifest: super::super::formal_package::FormalExecutionManifestV1 =
            capture::decode_role(material, ArtifactRole::ExecutionManifest)?;
        if manifest.parent != before.head().head()
            || manifest.world != self.after.head().record().world
            || manifest.continuation.context.world_id != self.after.head().record().world.world_id
        {
            return Err("Execution manifest differs from installed adjacent lineage".into());
        }
        let context = manifest.continuation.context;
        let [action] = context.committed_actions.as_slice() else {
            return Err("qualified mutation requires actual singleton action".into());
        };
        action.validate().map_err(|error| error.to_string())?;
        if action.action_id != action_id
            || action.payload_cbor != raw
            || oasis7_node::compute_consensus_action_root(&context.committed_actions)
                .map_err(|error| error.to_string())?
                != context.action_root
            || context.committed_at_unix_ms <= 0
        {
            return Err("qualified mutation is not the exact committed singleton".into());
        }
        Ok(context)
    }
}
