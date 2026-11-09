//! IWS is an explicit material format of the existing authority protocol.
//! Original record IDs, signatures, endpoint roles and qualification remain
//! unchanged. Storage CommitBody IDs are separate typed content identities.
use super::{crypto::*, *};
use crate::controlled_authority::replicated_protocol::ArtifactRole;
use std::collections::BTreeMap;

pub const IWS_AUTHORITY_PROFILE_VERSION: u32 = 3;
pub const IWS_ACTIVATION_SCHEMA: u32 = 3;
pub const IWS_RECORD_SCHEMA: u32 = 7;
pub const IWS_DURABLE_RECORD_SCHEMA: u32 = 8;
pub const IWS_FROZEN_SCHEMA: u32 = 4;

pub fn is_iws_record(schema: u32) -> bool {
    matches!(schema, IWS_RECORD_SCHEMA | IWS_DURABLE_RECORD_SCHEMA)
}
pub fn is_iws_role(role: ArtifactRole) -> bool {
    matches!(
        role,
        ArtifactRole::IncrementalGenesisConfig
            | ArtifactRole::IncrementalRootSet
            | ArtifactRole::IncrementalCommitBody
            | ArtifactRole::IncrementalHistoryPrefix
    )
}
pub(super) fn validate_roots(
    roots: &BTreeMap<ArtifactRole, String>,
    initial: bool,
    world: &FormalWorldIdentityV1,
    state: &str,
    manifest: &str,
) -> Result<(), FormalError> {
    const INITIAL: [ArtifactRole; 6] = [
        ArtifactRole::IncrementalGenesisConfig,
        ArtifactRole::IncrementalRootSet,
        ArtifactRole::ExecutionManifest,
        ArtifactRole::Wasm,
        ArtifactRole::Rules,
        ArtifactRole::NativeExecutionEnvironment,
    ];
    const RECORD: [ArtifactRole; 9] = [
        ArtifactRole::Input,
        ArtifactRole::IncrementalGenesisConfig,
        ArtifactRole::IncrementalRootSet,
        ArtifactRole::IncrementalCommitBody,
        ArtifactRole::IncrementalHistoryPrefix,
        ArtifactRole::ExecutionManifest,
        ArtifactRole::Wasm,
        ArtifactRole::Rules,
        ArtifactRole::NativeExecutionEnvironment,
    ];
    let required: &[ArtifactRole] = if initial { &INITIAL } else { &RECORD };
    if roots.len() != required.len()
        || required.iter().any(|role| !roots.contains_key(role))
        || roots
            .get(&ArtifactRole::IncrementalGenesisConfig)
            .map(String::as_str)
            != Some(world.genesis_digest.as_str())
        || roots
            .get(&ArtifactRole::IncrementalRootSet)
            .map(String::as_str)
            != Some(state)
        || roots
            .get(&ArtifactRole::ExecutionManifest)
            .map(String::as_str)
            != Some(manifest)
    {
        return Err(invalid("incremental authority typed root set"));
    }
    for value in roots.values() {
        hash_hex(value)?;
    }
    Ok(())
}
pub(super) fn check_record_profile(
    record: &CanonicalRecordV1,
    authority: &AuthorityDefinitionV1,
) -> Result<(), FormalError> {
    if is_iws_record(record.schema_version)
        != (authority.profile_version == IWS_AUTHORITY_PROFILE_VERSION)
    {
        return Err(invalid("authority/record incremental format mismatch"));
    }
    Ok(())
}

/// The original verified authorization window becomes the transaction fence.
/// This token is integrity context; it is not a commit or service-readiness grant.
pub fn authority_binding(
    authority: &VerifiedAuthorityV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> Result<crate::incremental_execution::AuthorityBinding, BindingError> {
    if authority.definition().profile_version != IWS_AUTHORITY_PROFILE_VERSION {
        return Err(invalid("incremental authority profile required").into());
    }
    let _workspace = memory.reserve(
        128 * 1024,
        oasis7_proto::memory_budget::MemoryClass::Ordinary,
    )?;
    let token = hex::decode(authority.context_digest()?)
        .map_err(|_| invalid("authority context digest"))?
        .try_into()
        .map_err(|_| invalid("authority context width"))?;
    Ok(crate::incremental_execution::AuthorityBinding {
        token,
        epoch: authority.definition().epoch,
    })
}

#[derive(Debug)]
pub enum BindingError {
    Protocol(FormalError),
    Memory(oasis7_proto::memory_budget::MemoryLimitExceeded),
}
impl From<FormalError> for BindingError {
    fn from(error: FormalError) -> Self {
        Self::Protocol(error)
    }
}
impl From<oasis7_proto::memory_budget::MemoryLimitExceeded> for BindingError {
    fn from(error: oasis7_proto::memory_budget::MemoryLimitExceeded) -> Self {
        Self::Memory(error)
    }
}
impl std::fmt::Display for BindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Protocol(error) => error.fmt(f),
            Self::Memory(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for BindingError {}

/// Borrowed typed IWS header material; no whole-world package.
pub struct CommitMaterialRefs<'a> {
    pub config: &'a crate::incremental_history::GenesisConfigV1,
    pub roots: &'a crate::incremental_execution::RootSetV1,
    pub body: &'a crate::incremental_history::CommitBodyV1,
    pub history: &'a crate::incremental_history::HistoryPrefixV1,
}

/// Bind typed IWS content to an original protocol record. Endpoint consumers
/// must additionally verify original object bytes, the state transition, full
/// execution/artifact closure and durable-domain conditions before receipts.
/// Returning () here intentionally issues no new Verified/accepted capability.
pub fn verify_commit_binding(
    record: &CanonicalRecordV1,
    authority: &VerifiedAuthorityV1,
    parent: Option<&VerifiedFormalCommitV1>,
    material: CommitMaterialRefs<'_>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> Result<(), BindingError> {
    let _workspace = memory.reserve(
        128 * 1024,
        oasis7_proto::memory_budget::MemoryClass::Ordinary,
    )?;
    let CommitMaterialRefs {
        config,
        roots,
        body,
        history,
    } = material;
    record.validate()?;
    check_record_profile(record, authority.definition())?;
    if !is_iws_record(record.schema_version) {
        return Err(invalid("incremental record required").into());
    }
    let bad = || invalid("incremental commit/original record binding");
    let scope = config.scope().map_err(|_| bad())?;
    roots.validate().map_err(|_| bad())?;
    body.validate().map_err(|_| bad())?;
    history.validate().map_err(|_| bad())?;
    let root_id = hex::encode(roots.digest().map_err(|_| bad())?);
    let body_id = body.digest().map_err(|_| bad())?;
    let history_id = history.digest().map_err(|_| bad())?;
    if record.world != authority.definition().world
        || record.original_authority_id != authority.authority_id()?
        || record.original_epoch != authority.definition().epoch
        || record.activation_ref != authority.activation_ref()
        || record.world.world_id != scope.world_id
        || record.world.genesis_digest != hex::encode(scope.genesis_config)
        || !scope.matches(roots)
        || body.scope != scope
        || history.scope != scope
        || body.authority != authority_binding(authority, memory)?
        || body.height != roots.height
        || roots.height.checked_add(1) != Some(record.position)
        || history.count != record.position
        || history_id != body.history
        || hex::encode(body.roots) != root_id
        || record.after_state_root != root_id
        || hex::encode(body.operation) != record.request_digest
        || record
            .artifact_roots
            .get(&ArtifactRole::IncrementalCommitBody)
            != Some(&hex::encode(body_id))
        || record
            .artifact_roots
            .get(&ArtifactRole::IncrementalHistoryPrefix)
            != Some(&hex::encode(history_id))
        || record
            .artifact_roots
            .get(&ArtifactRole::NativeExecutionEnvironment)
            != Some(&hex::encode(body.execution_environment))
    {
        return Err(bad().into());
    }
    match (record.kind.clone(), parent) {
        (FormalRecordKindV1::InitialActivation, None) => {
            if record.parent_record_id != record.world.genesis_head()?.record_id
                || body.execution_environment != config.execution_environment
                || roots.height != 0
                || body.parent_commit.is_some()
                || body.transition.is_some()
                || history.previous.is_some()
                || body.catalog_before.count != 0
                || record.before_state_root != record.after_state_root
            {
                return Err(bad().into());
            }
        }
        (FormalRecordKindV1::Execution, Some(parent)) => {
            let previous = parent.record();
            if !is_iws_record(previous.schema_version)
                || previous.world != record.world
                || previous.position.checked_add(1) != Some(record.position)
                || parent.head().record_id != record.parent_record_id
                || previous.after_state_root != record.before_state_root
                || body.parent_commit.map(hex::encode).as_ref()
                    != previous
                        .artifact_roots
                        .get(&ArtifactRole::IncrementalCommitBody)
                || history.previous.map(hex::encode).as_ref()
                    != previous
                        .artifact_roots
                        .get(&ArtifactRole::IncrementalHistoryPrefix)
                || body.catalog_before.last_commit != body.parent_commit
                || body.catalog_before.count != roots.height
                || body.transition.is_none()
            {
                return Err(bad().into());
            }
        }
        _ => return Err(bad().into()),
    }
    Ok(())
}

#[cfg(test)]
#[path = "incremental_tests.rs"]
mod tests;
