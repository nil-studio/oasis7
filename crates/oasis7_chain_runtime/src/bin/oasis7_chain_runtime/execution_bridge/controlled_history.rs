//! Read-only, real in-memory re-execution; no node, driver, dispatcher or persistence.
use super::controlled_capture::{
    self as capture, CAPTURE_SCOPE, CaptureManifest, CapturePreparation, OutcomeIndex,
};
use super::execution_hash::{
    ExecutionHashPayload, execution_resource_commit_hash, execution_resource_context_hash,
    execution_resource_created_at_height,
};
use super::{ExecutionExternalEffectMaterialization, to_cbor};
use oasis7_distfs::controlled_authority::{
    LocalRequestIdentity,
    replicated_protocol::{
        ArtifactRole, ClosedRecord, DurabilityEvidence, FixedTrust, HeadAnchor, verify_evidence,
    },
};
use oasis7_runtime::{
    ChainResourceDerivationContext, Journal, RuntimeCommittedTickContext, Snapshot, blake3_hex,
};
use oasis7_wasm_executor::{WasmExecutor, WasmExecutorConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrustedHistoryConfiguration {
    pub trust: FixedTrust,
    pub minimum_head: HeadAnchor,
    pub release_security_policy: oasis7_runtime::ReleaseSecurityPolicy,
    pub initial_execution_height: u64,
    pub initial_execution_state_root: String,
    pub initial_execution_block_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_initial_anchor: Option<PublishedInitialAnchor>,
}

/// Operator-pinned original published files, saved before the recipe input.
/// Hashes come from external observation, never from the capture being checked.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublishedInitialAnchor {
    pub record_path: std::path::PathBuf,
    pub record_hash: String,
    pub snapshot_path: std::path::PathBuf,
    pub snapshot_hash: String,
    pub journal_path: std::path::PathBuf,
    pub journal_hash: String,
    /// Independently saved before input; authenticates only the complete cached
    /// diagnostic pair, never the current published state or genesis authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_resource_snapshot: Option<PinnedResourceSnapshot>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PinnedResourceSnapshot {
    pub snapshot_path: std::path::PathBuf,
    pub snapshot_hash: String,
}
#[derive(Debug, Serialize)]
pub(crate) struct HistoryVerification {
    pub scope: &'static str,
    pub claim: &'static str,
    pub records_reexecuted: usize,
    pub head: HeadAnchor,
    pub execution_height: u64,
    pub execution_state_root: String,
}

pub(super) fn request_identity(
    origin: &oasis7_runtime::GameplaySubmissionOrigin,
) -> Result<LocalRequestIdentity, String> {
    // Full identity tuple is unambiguous and preserves the existing session distinction.
    Ok(LocalRequestIdentity {
        verified_subject: blake3_hex(&to_cbor((
            &origin.verified_player_id,
            &origin.public_key,
            &origin.hosted_registration_nonce,
        ))?),
        operation_domain: "schedule_recipe".into(),
        nonce_scope: "gameplay_player_public_key_nonce_v1".into(),
        request_id: origin.auth_nonce.to_string(),
    })
}

pub(crate) fn verify_history(
    config: &TrustedHistoryConfiguration,
    evidence: &[DurabilityEvidence],
) -> Result<HistoryVerification, String> {
    config.trust.validate().map_err(|e| e.to_string())?;
    if evidence.is_empty()
        || evidence.len() > 64
        || (config.minimum_head.position > 0 && !config.minimum_head.qualified)
        || config.initial_execution_state_root.len() != 64
        || config.initial_execution_block_hash.is_empty()
    {
        return Err("invalid external history anchor or history length".into());
    }
    let mut parent = config.trust.genesis_anchor().map_err(|e| e.to_string())?;
    let mut minimum_seen = parent == config.minimum_head;
    let mut packages = Vec::with_capacity(evidence.len());
    for e in evidence {
        let next = verify_evidence(e, &config.trust, &parent).map_err(|e| e.to_string())?;
        let package = &e.proposal.body.record;
        let manifest: CaptureManifest =
            capture::decode_role(package, ArtifactRole::ExecutionManifest)?;
        let origin = capture::supported_origin(&manifest.context)?;
        if e.proposal.body.request != request_identity(&origin.submission)? {
            return Err("signed history request binding mismatch".into());
        }
        packages.push(package);
        minimum_seen |= next == config.minimum_head;
        parent = next;
    }
    let (previous_height, final_root) = validate_capture_prefix(config, &packages)?;
    if !minimum_seen {
        return Err("external minimum head not established by history".into());
    }
    Ok(HistoryVerification {
        scope: CAPTURE_SCOPE,
        claim: "verified_local_durability_and_reexecution_prerequisite_not_activation_or_formal_commit",
        records_reexecuted: evidence.len(),
        head: parent,
        execution_height: previous_height,
        execution_state_root: final_root,
    })
}

/// Shared unsigned preflight: complete typed closure and real reexecution before
/// a diagnostic operator probe asks the public storage protocol to sign anything.
pub(super) fn validate_capture_prefix(
    config: &TrustedHistoryConfiguration,
    packages: &[&ClosedRecord],
) -> Result<(u64, String), String> {
    config.trust.validate().map_err(|e| e.to_string())?;
    if packages.is_empty() || packages.len() > 64 {
        return Err("invalid capture prefix length".into());
    }
    let mut preceding: Option<(&ClosedRecord, CaptureManifest)> = None;
    let mut previous_block = config.initial_execution_block_hash.clone();
    let mut previous_height = config.initial_execution_height;
    for (position, package) in packages.iter().copied().enumerate() {
        package.validate().map_err(|e| e.to_string())?;
        capture::validate_package_budget(&to_cbor(package)?)?;
        let manifest: CaptureManifest =
            capture::decode_role(package, ArtifactRole::ExecutionManifest)?;
        let preparation = capture::materialize_preparation(package, &manifest)?;
        if preparation.release_security_policy != config.release_security_policy {
            return Err("external execution security policy mismatch".into());
        }
        capture::supported_origin(&manifest.context)?;
        if manifest.context.world_id != config.trust.world_id
            || manifest.context.height
                != previous_height
                    .checked_add(1)
                    .ok_or("execution height overflow")?
            || manifest.context != preparation.context
        {
            return Err("history request/world/context binding mismatch".into());
        }
        let before: Snapshot = capture::decode_snapshot(&preparation.before_snapshot)?;
        if let Some((previous_package, previous_manifest)) = &preceding {
            capture::validate_continuity(previous_package, previous_manifest, &preparation)?;
        } else if let Some(anchor) = &config.published_initial_anchor {
            validate_published_initial_anchor(config, anchor, &preparation)?;
        } else if package.before_state_root != config.initial_execution_state_root
            || blake3_hex(&preparation.before_snapshot) != config.initial_execution_state_root
            || before.state.time != config.initial_execution_height
        {
            return Err("initial external execution anchor mismatch".into());
        }
        let snapshot_bytes = capture::object(package, &package.roots[&ArtifactRole::Snapshot])?;
        let journal_bytes = capture::object(package, &package.roots[&ArtifactRole::Journal])?;
        let effect: ExecutionExternalEffectMaterialization =
            capture::decode_generic(capture::object(package, &manifest.effect_ref)?)?;
        let rebuilt = capture::build(
            &preparation,
            &manifest.record,
            snapshot_bytes,
            journal_bytes,
            &effect,
            preceding.as_ref().map(|(p, m)| (*p, m)),
        )?;
        if &rebuilt != package {
            return Err("typed capture closure or outcome prefix mismatch".into());
        }
        let index: OutcomeIndex = capture::decode_role(package, ArtifactRole::NonceIndex)?;
        if index.entries.len() != (position + 1)
            || index.initial_height != config.initial_execution_height
            || index.initial_state_root != packages[0].before_state_root
        {
            return Err("outcome index does not match complete history".into());
        }
        reexecute(
            &preparation,
            &manifest,
            snapshot_bytes,
            journal_bytes,
            &previous_block,
        )?;
        previous_block = manifest.record.execution_block_hash.clone();
        previous_height = manifest.record.height;
        preceding = Some((package, manifest));
    }
    Ok((
        previous_height,
        preceding
            .ok_or("empty capture prefix")?
            .0
            .after_state_root
            .clone(),
    ))
}

fn validate_published_initial_anchor(
    config: &TrustedHistoryConfiguration,
    anchor: &PublishedInitialAnchor,
    preparation: &CapturePreparation,
) -> Result<(), String> {
    fn pinned(path: &std::path::Path, hash: &str, maximum: u64) -> Result<Vec<u8>, String> {
        if !path.is_absolute() {
            return Err("external anchor path must be absolute".into());
        }
        let bytes = crate::controlled_history_cli::read_bounded(path, maximum)?;
        if hash.len() != 64 || blake3_hex(&bytes) != hash {
            return Err("external published anchor content hash mismatch".into());
        }
        Ok(bytes)
    }
    let record: super::ExecutionBridgeRecord = serde_json::from_slice(&pinned(
        &anchor.record_path,
        &anchor.record_hash,
        64 * 1024,
    )?)
    .map_err(|e| format!("external published record: {e}"))?;
    let snapshot_bytes = pinned(
        &anchor.snapshot_path,
        &anchor.snapshot_hash,
        64 * 1024 * 1024,
    )?;
    let journal_bytes = pinned(&anchor.journal_path, &anchor.journal_hash, 64 * 1024 * 1024)?;
    let snapshot = capture::decode_snapshot(&snapshot_bytes)?;
    let journal: Journal = capture::decode_generic(&journal_bytes)?;
    if record.schema_version != 3
        || record.world_id != config.trust.world_id
        || record.height != config.initial_execution_height
        || record.execution_block_hash != config.initial_execution_block_hash
        || record.execution_state_root != config.initial_execution_state_root
        || record.execution_state_root != anchor.snapshot_hash
        || record.snapshot_ref.as_deref() != Some(&anchor.snapshot_hash)
        || record.latest_state_ref.as_deref() != Some(&anchor.snapshot_hash)
        || record.journal_ref.as_deref() != Some(&anchor.journal_hash)
        || record.journal_len != journal.len()
        || snapshot.state.time != record.height
        || record.node_block_hash.is_none()
        || record.action_root.is_none()
        || record.proposer_id.is_none()
        || record.simulator_mirror.is_some()
    {
        return Err("external published execution anchor binding mismatch".into());
    }
    // Validate the published diagnostic context exactly as the existing driver
    // constructed it; it is never interpreted as a formal genesis identity.
    let world = oasis7::runtime_host::world::from_snapshot(snapshot.clone(), journal.clone())
        .map_err(|e| format!("published anchor restore: {e:?}"))?;
    let commit = execution_resource_commit_hash(&record.world_id, record.height);
    let resource_hash = execution_resource_context_hash(&record.world_id);
    let expected = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: &record.world_id,
            chain_id: &record.world_id,
            genesis_ref: None,
            created_at_height: execution_resource_created_at_height(record.height),
            manifest_height: record.height,
            commit_block_hash: Some(&commit),
            tick: snapshot.state.time,
        },
        resource_hash.clone(),
        resource_hash,
    );
    if expected.chain_resource_manifest != snapshot.chain_resource_manifest
        || expected.latest_chain_resource_delta != snapshot.latest_chain_resource_delta
    {
        return Err("published anchor diagnostic resource context mismatch".into());
    }
    let cached = anchor
        .cached_resource_snapshot
        .as_ref()
        .map(|pin| -> Result<Snapshot, String> {
            let bytes = pinned(&pin.snapshot_path, &pin.snapshot_hash, 64 * 1024 * 1024)?;
            let cached = capture::decode_snapshot(&bytes)?;
            if cached.chain_resource_manifest.world_id != record.world_id
                || cached.state.time > record.height
                || cached.chain_resource_manifest.manifest_height > record.height
            {
                return Err("external cached resource snapshot context mismatch".into());
            }
            Ok(cached)
        })
        .transpose()?;
    capture::validate_snapshot_continuity(&snapshot, journal, cached.as_ref(), preparation)
}

fn reexecute(
    preparation: &CapturePreparation,
    manifest: &CaptureManifest,
    snapshot_bytes: &[u8],
    journal_bytes: &[u8],
    previous_block: &str,
) -> Result<(), String> {
    if !cfg!(feature = "wasmtime") {
        return Err("real reexecution requires the wasmtime feature".into());
    }
    let before: Snapshot = capture::decode_snapshot(&preparation.before_snapshot)?;
    let journal: Journal = capture::decode_generic(&preparation.before_journal)?;
    let mut world = oasis7::runtime_host::world::from_snapshot(before, journal)
        .map_err(|e| format!("world restore: {e:?}"))?
        .with_release_security_policy(preparation.release_security_policy.clone());
    let context = &preparation.context;
    let (actions, simulator, bootstrap, service_intents) =
        super::driver_replicated_input::decode_committed_actions(context)?;
    if !simulator.is_empty() || bootstrap.is_some() || !service_intents.is_empty() {
        return Err("unsupported replay operation".into());
    }
    for (action, origin) in actions {
        world
            .submit_recipe_action_with_origin(action, origin.ok_or("missing replay origin")?)
            .map_err(|e| format!("replay submit: {e:?}"))?;
    }
    // The real executor uses an empty Linker and no disk compiled cache. The
    // restored World has no configured persistence path or effect dispatcher.
    let mut sandbox =
        WasmExecutor::new(WasmExecutorConfig::default()).map_err(|e| e.to_string())?;
    world
        .step_with_modules_for_committed_context(
            &mut sandbox,
            &RuntimeCommittedTickContext {
                height: context.height,
                slot: context.slot,
                epoch: context.epoch,
                node_block_hash: String::new(),
                action_root: context.action_root.clone(),
                authority_node_id: context.node_id.clone(),
                committed_at_unix_ms: context.committed_at_unix_ms,
            },
        )
        .map_err(|e| format!("real replay step: {e:?}"))?;
    let resource_commit = execution_resource_commit_hash(&context.world_id, context.height);
    let resource_context = ChainResourceDerivationContext {
        world_id: &context.world_id,
        chain_id: &context.world_id,
        genesis_ref: None,
        created_at_height: execution_resource_created_at_height(context.height),
        manifest_height: context.height,
        commit_block_hash: Some(&resource_commit),
        tick: world.state().time,
    };
    let resource_hash = execution_resource_context_hash(&context.world_id);
    let actual_snapshot = to_cbor(world.snapshot_with_chain_resource_context(
        resource_context,
        resource_hash.clone(),
        resource_hash,
    ))?;
    let actual_journal = to_cbor(world.journal())?;
    if actual_snapshot != snapshot_bytes || actual_journal != journal_bytes {
        return Err("real reexecution snapshot/journal/result mismatch".into());
    }
    let root = blake3_hex(&actual_snapshot);
    let block = blake3_hex(&to_cbor(ExecutionHashPayload {
        world_id: &context.world_id,
        height: context.height,
        prev_execution_block_hash: previous_block,
        execution_state_root: &root,
        journal_len: world.journal().len(),
    })?);
    if root != manifest.record.execution_state_root || block != manifest.record.execution_block_hash
    {
        return Err("real reexecution block/state root mismatch".into());
    }
    Ok(())
}
