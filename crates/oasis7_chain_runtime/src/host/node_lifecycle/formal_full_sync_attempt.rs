//! Per-`sync_latest()` attempt state and shared cut/material helpers. The first
//! independently verified target stays fixed for later providers, and material
//! bytes already charged by a failed provider remain charged for this call.

use super::cas_lifecycle::CasBudget;
use super::FormalFullSyncConfigV1;
use oasis7::{
    runtime::LocalCasStore, world_service::verified_view::VerifiedWorldView,
};
use oasis7_client_api::world_service::WorldIdentity;
use oasis7_client_api::world_service_full_sync::{
    FormalFullSyncArtifactRoleV1 as WireArtifactRole, FormalFullSyncCutV1,
};
use oasis7_distfs::controlled_authority::{
    formal::VerifiedFormalReadAnchorV1,
    replicated_protocol::{ArtifactRole, ClosedRecord},
};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct FullSyncAttemptV1 {
    pub(super) budget: CasBudget,
    pub(super) target: Option<FullSyncTargetV1>,
}

pub(super) struct FullSyncTargetV1 {
    pub(super) anchor: VerifiedFormalReadAnchorV1,
    pub(super) cut: FormalFullSyncCutV1,
}

impl FullSyncAttemptV1 {
    pub(super) fn new(max_bytes: u64) -> Self {
        Self {
            budget: CasBudget::new(max_bytes),
            target: None,
        }
    }

    pub(super) fn pin_target(
        &mut self,
        anchor: VerifiedFormalReadAnchorV1,
        cut: FormalFullSyncCutV1,
    ) {
        self.target = Some(FullSyncTargetV1 { anchor, cut });
    }
}

pub(super) fn verified_anchor(
    view: &VerifiedWorldView,
    config: &FormalFullSyncConfigV1,
) -> Result<VerifiedFormalReadAnchorV1, String> {
    let commit = view
        .formal_commit()
        .ok_or("full-sync source read has no independently verified commit")?;
    let anchor = view
        .formal_read_anchor()
        .ok_or("full-sync source read has no independently verified read anchor")?;
    if commit.record() != anchor.head().record()
        || commit.finalizing_authority_id() != anchor.head().finalizing_authority_id()
        || anchor.authority().definition().profile_version != 2
        || anchor.head().record().world.world_id != config.world_id
        || anchor.head().record().world.chain_id != config.chain_id
        || anchor.head().record().world.genesis_digest != config.genesis_digest
    {
        return Err("full-sync source proof differs from configured World or exact cut".into());
    }
    Ok(anchor.clone())
}

pub(super) fn cut_from_anchor(
    anchor: &VerifiedFormalReadAnchorV1,
) -> Result<FormalFullSyncCutV1, String> {
    let record = anchor.head().record();
    Ok(FormalFullSyncCutV1 {
        world: WorldIdentity {
            world_id: record.world.world_id.clone(),
            genesis_digest: record.world.genesis_digest.clone(),
        },
        chain_id: record.world.chain_id.clone(),
        position: record.position,
        record_id: record.record_id().map_err(|error| error.to_string())?,
        after_state_root: record.after_state_root.clone(),
        active_authority_id: anchor
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?,
        finalizing_authority_id: anchor.head().finalizing_authority_id().to_owned(),
        authority_epoch: anchor.authority().definition().epoch,
    })
}

pub(super) fn native_role(role: WireArtifactRole) -> ArtifactRole {
    match role {
        WireArtifactRole::Input => ArtifactRole::Input,
        WireArtifactRole::Result => ArtifactRole::Result,
        WireArtifactRole::Snapshot => ArtifactRole::Snapshot,
        WireArtifactRole::Journal => ArtifactRole::Journal,
        WireArtifactRole::NonceIndex => ArtifactRole::NonceIndex,
        WireArtifactRole::EffectOutbox => ArtifactRole::EffectOutbox,
        WireArtifactRole::ExecutionManifest => ArtifactRole::ExecutionManifest,
        WireArtifactRole::Wasm => ArtifactRole::Wasm,
        WireArtifactRole::Rules => ArtifactRole::Rules,
        WireArtifactRole::NativeExecutionEnvironment => ArtifactRole::NativeExecutionEnvironment,
    }
}

pub(super) fn object_bytes<'a>(
    material: &'a ClosedRecord,
    digest: &str,
) -> Result<&'a [u8], String> {
    material
        .objects
        .iter()
        .find(|object| object.content_hash == digest)
        .map(|object| object.bytes.as_slice())
        .ok_or_else(|| "full-sync artifact object missing from checked closure".into())
}

pub(super) fn verified_local_object(
    store: &LocalCasStore,
    digest: &str,
) -> Result<Option<Vec<u8>>, String> {
    match store.get_verified(digest) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(oasis7_proto::world_error::WorldError::BlobNotFound { .. }) => Ok(None),
        Err(error) => Err(format!(
            "local content-addressed object is invalid: {error:?}"
        )),
    }
}

pub(super) fn nondecreasing_cut(
    previous: &FormalFullSyncCutV1,
    current: &FormalFullSyncCutV1,
) -> bool {
    previous.world == current.world
        && previous.chain_id == current.chain_id
        && current.position >= previous.position
        && (current.position != previous.position || current == previous)
}

pub(super) fn valid_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes * 2
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn now_unix_ms() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| u64::try_from(time.as_millis()).ok())
}
