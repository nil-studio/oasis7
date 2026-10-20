//! Generic engineering execution closure. Existing ScheduleRecipe capture is unchanged.
use super::{
    ExecutionBridgeRecord, ExecutionExternalEffectMaterialization, controlled_capture as capture,
    execution_hash::ExecutionHashPayload, to_cbor,
};
use oasis7_distfs::controlled_authority::replicated_protocol::{
    ArtifactObject, ArtifactRole, ClosedRecord, FixedTrust,
};
use oasis7_node::{NodeLocalExecutionContinuation, compute_consensus_action_root};
use oasis7_runtime::{Journal, ReleaseSecurityPolicy, Snapshot, World, blake3_hex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const SCOPE: &str = "controlled_local_execution_prerequisite_v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LiveManifest {
    pub schema_version: u32,
    pub scope: String,
    pub trust: FixedTrust,
    pub continuation: NodeLocalExecutionContinuation,
    pub predecessor_execution_block_hash: String,
    pub before_snapshot_ref: String,
    pub before_journal_ref: String,
    pub before_tick: u64,
    pub after_tick: u64,
    pub release_security_policy: ReleaseSecurityPolicy,
    pub effect_ref: String,
    pub artifact_roots: BTreeMap<ArtifactRole, String>,
}
fn insert(
    objects: &mut BTreeMap<String, ArtifactObject>,
    bytes: Vec<u8>,
    mut references: Vec<String>,
) -> String {
    references.sort();
    references.dedup();
    let content_hash = blake3_hex(&bytes);
    objects
        .entry(content_hash.clone())
        .or_insert_with(|| ArtifactObject {
            content_hash: content_hash.clone(),
            bytes,
            references,
        });
    content_hash
}

#[expect(
    clippy::too_many_arguments,
    reason = "Exact original and candidate bytes remain explicit at the publication boundary"
)]
pub(super) fn build(
    trust: &FixedTrust,
    continuation: &NodeLocalExecutionContinuation,
    predecessor_execution_block_hash: &str,
    before_snapshot_bytes: &[u8],
    before_journal_bytes: &[u8],
    after_snapshot_bytes: &[u8],
    after_journal_bytes: &[u8],
    record: &ExecutionBridgeRecord,
    effect: &ExecutionExternalEffectMaterialization,
    release_policy: &ReleaseSecurityPolicy,
) -> Result<ClosedRecord, String> {
    trust.validate().map_err(|e| e.to_string())?;
    let (before, after) = validate_transition(
        &trust.world_id,
        &trust.chain_id,
        &trust.genesis_digest,
        ActualTransition {
            continuation,
            predecessor_execution_block_hash,
            before_snapshot_bytes,
            before_journal_bytes,
            after_snapshot_bytes,
            after_journal_bytes,
            record,
            effect,
            release_policy,
        },
    )?;
    let c = &continuation.context;
    let state_root = blake3_hex(after_snapshot_bytes);
    let mut objects = BTreeMap::new();
    let before_snapshot_ref = insert(&mut objects, before_snapshot_bytes.to_vec(), vec![]);
    let before_journal_ref = insert(&mut objects, before_journal_bytes.to_vec(), vec![]);
    let input = insert(&mut objects, to_cbor(&c.committed_actions)?, vec![]);
    let snapshot = insert(
        &mut objects,
        after_snapshot_bytes.to_vec(),
        vec![before_snapshot_ref.clone()],
    );
    let journal = insert(
        &mut objects,
        after_journal_bytes.to_vec(),
        vec![before_journal_ref.clone()],
    );
    let effect_ref = insert(&mut objects, to_cbor(effect)?, vec![]);
    if record.external_effect_ref.as_deref() != Some(effect_ref.as_str()) {
        return Err("live effect reference mismatch".into());
    }
    let result = insert(
        &mut objects,
        to_cbor(record)?,
        vec![snapshot.clone(), journal.clone(), effect_ref.clone()],
    );
    // This version covers concrete existing capability, signed gameplay and
    // WorldService result ledgers. Other nonce domains remain in Snapshot.
    let nonce = insert(
        &mut objects,
        to_cbor((
            SCOPE,
            &after.capability_nonce_records,
            &after.capability_revocation_state.world_service_results,
            &after.state.authenticated_collect_data_last_nonces,
            &after.state.agent_intent_ledger,
        ))?,
        vec![input.clone(), result.clone()],
    );
    let outbox = insert(
        &mut objects,
        to_cbor((&after.pending_effects, &after.inflight_effects))?,
        vec![effect_ref.clone()],
    );
    let rules = insert(
        &mut objects,
        to_cbor((
            &before.manifest,
            &after.manifest,
            &before.policies,
            &after.policies,
            release_policy,
        ))?,
        vec![],
    );
    let mut wasm_refs = BTreeMap::new();
    for (hash, bytes) in before
        .module_artifact_bytes
        .iter()
        .chain(after.module_artifact_bytes.iter())
    {
        if let Some(previous) =
            wasm_refs.insert(hash.clone(), insert(&mut objects, bytes.clone(), vec![]))
            && previous != blake3_hex(bytes)
        {
            return Err("conflicting module bytes".into());
        }
    }
    let wasm = insert(
        &mut objects,
        to_cbor((&before.module_registry, &after.module_registry, &wasm_refs))?,
        wasm_refs.values().cloned().collect(),
    );
    let mut roots = BTreeMap::from([
        (ArtifactRole::Input, input.clone()),
        (ArtifactRole::Result, result),
        (ArtifactRole::Snapshot, snapshot),
        (ArtifactRole::Journal, journal),
        (ArtifactRole::NonceIndex, nonce),
        (ArtifactRole::EffectOutbox, outbox),
        (ArtifactRole::Wasm, wasm),
        (ArtifactRole::Rules, rules),
    ]);
    let manifest = LiveManifest {
        schema_version: 1,
        scope: SCOPE.into(),
        trust: trust.clone(),
        continuation: continuation.clone(),
        predecessor_execution_block_hash: predecessor_execution_block_hash.into(),
        before_snapshot_ref,
        before_journal_ref,
        before_tick: before.state.time,
        after_tick: after.state.time,
        release_security_policy: release_policy.clone(),
        effect_ref,
        artifact_roots: roots.clone(),
    };
    let manifest_ref = insert(
        &mut objects,
        to_cbor(&manifest)?,
        roots
            .values()
            .cloned()
            .chain([
                manifest.before_snapshot_ref.clone(),
                manifest.before_journal_ref.clone(),
            ])
            .collect(),
    );
    roots.insert(ArtifactRole::ExecutionManifest, manifest_ref);
    let package = ClosedRecord {
        payload_digest: input,
        before_state_root: blake3_hex(before_snapshot_bytes),
        after_state_root: state_root,
        roots,
        objects: objects.into_values().collect(),
    };
    package.validate().map_err(|e| e.to_string())?;
    capture::validate_package_budget(&to_cbor(&package)?)?;
    Ok(package)
}

pub(super) fn verify_package(
    package: &ClosedRecord,
    trust: &FixedTrust,
    expected: &NodeLocalExecutionContinuation,
    previous_block: &str,
    policy: &ReleaseSecurityPolicy,
) -> Result<(), String> {
    package.validate().map_err(|e| e.to_string())?;
    capture::validate_package_budget(&to_cbor(package)?)?;
    let manifest: LiveManifest = capture::decode_role(package, ArtifactRole::ExecutionManifest)?;
    if manifest.schema_version != 1
        || manifest.scope != SCOPE
        || &manifest.trust != trust
        || &manifest.continuation != expected
        || manifest.predecessor_execution_block_hash != previous_block
        || &manifest.release_security_policy != policy
    {
        return Err("live manifest original policy/context mismatch".into());
    }
    let record: ExecutionBridgeRecord = capture::decode_role(package, ArtifactRole::Result)?;
    let effect: ExecutionExternalEffectMaterialization =
        capture::decode_generic(capture::object(package, &manifest.effect_ref)?)?;
    let rebuilt = build(
        trust,
        expected,
        previous_block,
        capture::object(package, &manifest.before_snapshot_ref)?,
        capture::object(package, &manifest.before_journal_ref)?,
        capture::object(package, &package.roots[&ArtifactRole::Snapshot])?,
        capture::object(package, &package.roots[&ArtifactRole::Journal])?,
        &record,
        &effect,
        policy,
    )?;
    if &rebuilt != package {
        return Err("live exact typed nine-role closure mismatch".into());
    }
    Ok(())
}

/// Shared typed execution checks; identity strings grant no write authority.
/// Formal and engineering constructors use this without exchanging proofs.
pub(super) struct ActualTransition<'a> {
    pub continuation: &'a NodeLocalExecutionContinuation,
    pub predecessor_execution_block_hash: &'a str,
    pub before_snapshot_bytes: &'a [u8],
    pub before_journal_bytes: &'a [u8],
    pub after_snapshot_bytes: &'a [u8],
    pub after_journal_bytes: &'a [u8],
    pub record: &'a ExecutionBridgeRecord,
    pub effect: &'a ExecutionExternalEffectMaterialization,
    pub release_policy: &'a ReleaseSecurityPolicy,
}
pub(super) fn validate_transition(
    world_id: &str,
    chain_id: &str,
    genesis_digest: &str,
    transition: ActualTransition<'_>,
) -> Result<(Snapshot, Snapshot), String> {
    validate_transition_inner(world_id, chain_id, genesis_digest, transition, None, None)
}

/// Formal execution supplies the exact parent World and actual candidate World,
/// including their authenticated retained-history reader, before pruning can
/// make ordinary Snapshot reconstruction impossible. Their serialized forms
/// must exactly match the signed transition bytes below.
pub(super) fn validate_formal_transition(
    world_id: &str,
    chain_id: &str,
    genesis_digest: &str,
    transition: ActualTransition<'_>,
    unpruned: &Journal,
    actual_before: &World,
    actual_after: &World,
) -> Result<(Snapshot, Snapshot), String> {
    validate_transition_inner(
        world_id,
        chain_id,
        genesis_digest,
        transition,
        Some(unpruned),
        Some((actual_before, actual_after)),
    )
}

fn validate_transition_inner(
    world_id: &str,
    chain_id: &str,
    genesis_digest: &str,
    transition: ActualTransition<'_>,
    unpruned: Option<&Journal>,
    formal_worlds: Option<(&World, &World)>,
) -> Result<(Snapshot, Snapshot), String> {
    if unpruned.is_some() != formal_worlds.is_some() {
        return Err("formal validation requires authenticated before/after Worlds".into());
    }
    let ActualTransition {
        continuation,
        predecessor_execution_block_hash,
        before_snapshot_bytes,
        before_journal_bytes,
        after_snapshot_bytes,
        after_journal_bytes,
        record,
        effect,
        release_policy,
    } = transition;
    let c = &continuation.context;
    if continuation.schema_version != 1
        || c.world_id != world_id
        || c.node_id != c.proposer_id
        || c.height
            != continuation
                .predecessor_committed_height
                .checked_add(1)
                .ok_or("height exhausted")?
        || continuation.predecessor_execution_height != continuation.predecessor_committed_height
        || continuation.predecessor_execution_block_hash.as_deref()
            != Some(predecessor_execution_block_hash)
        || continuation.reserved_action_bytes
            != c.committed_actions
                .iter()
                .try_fold(0usize, |sum, a| sum.checked_add(a.payload_cbor.len()))
                .ok_or("live original action bytes overflow")?
        || compute_consensus_action_root(&c.committed_actions).map_err(|e| format!("{e:?}"))?
            != c.action_root
    {
        return Err("live original local decision binding mismatch".into());
    }
    for action in &c.committed_actions {
        action.validate().map_err(|e| format!("{e:?}"))?;
    }
    let (_, simulator, bootstrap, _) = super::driver_replicated_input::decode_committed_actions(c)?;
    if !simulator.is_empty() || bootstrap.is_some() {
        return Err("guarded execution rejects simulator/bootstrap mutations".into());
    }
    let before: Snapshot = capture::decode_snapshot(before_snapshot_bytes)?;
    let after: Snapshot = capture::decode_snapshot(after_snapshot_bytes)?;
    let before_journal: Journal = capture::decode_generic(before_journal_bytes)?;
    let after_journal: Journal = capture::decode_generic(after_journal_bytes)?;
    let restored_worlds = if let Some((actual_before, actual_after)) = formal_worlds {
        if to_cbor(actual_before.snapshot())? != before_snapshot_bytes
            || to_cbor(actual_before.journal())? != before_journal_bytes
            || to_cbor(actual_after.snapshot())? != after_snapshot_bytes
            || to_cbor(actual_after.journal())? != after_journal_bytes
        {
            return Err("formal actual Worlds differ from serialized transition bytes".into());
        }
        None
    } else {
        Some((
            oasis7::runtime_host::world::from_snapshot(before.clone(), before_journal.clone())
                .map_err(|e| format!("live predecessor restore: {e:?}"))?
                .with_release_security_policy(release_policy.clone()),
            oasis7::runtime_host::world::from_snapshot(after.clone(), after_journal.clone())
                .map_err(|e| format!("live candidate restore: {e:?}"))?,
        ))
    };
    let (before_world, _after_world) =
        if let Some((before_world, after_world)) = restored_worlds.as_ref() {
            (before_world, after_world)
        } else {
            formal_worlds.ok_or("formal validation lost actual authenticated Worlds")?
        };
    let full_journal = unpruned.unwrap_or(&after_journal);
    if let Some(full) = unpruned
        && (before.state.retained_history_root.is_none()
            || after.state.retained_history_root.is_none()
            || after_journal.events.len() > full.events.len()
            || full.events[full.events.len() - after_journal.events.len()..]
                != after_journal.events)
    {
        return Err("formal hot journal is not the exact executed suffix".into());
    }
    if to_cbor(before_world.snapshot())? != before_snapshot_bytes
        || continuation.predecessor_execution_state_root.as_deref()
            != Some(blake3_hex(before_snapshot_bytes).as_str())
        || full_journal.events.len() < before_journal.events.len()
        || full_journal.events[..before_journal.events.len()] != before_journal.events
        || after.state.time < before.state.time
        || after.state.time > before.state.time.saturating_add(1)
    {
        return Err("live typed predecessor/journal/tick mismatch".into());
    }
    for snapshot in [&before, &after] {
        let identity = &snapshot.chain_resource_manifest;
        if identity.world_id != world_id
            || identity.chain_id != chain_id
            || identity.genesis_ref.as_deref() != Some(genesis_digest)
        {
            return Err("live certificate-bound diagnostic identity mismatch".into());
        }
        let modules = snapshot
            .module_registry
            .records
            .values()
            .map(|r| r.manifest.wasm_hash.clone())
            .collect::<BTreeSet<_>>();
        if snapshot
            .module_artifact_bytes
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != modules
            || snapshot.module_artifacts != modules
        {
            return Err("live module closure has missing or extra bytes".into());
        }
    }
    let state_root = blake3_hex(after_snapshot_bytes);
    let journal_ref = blake3_hex(after_journal_bytes);
    if record.schema_version != 3
        || record.world_id != world_id
        || record.height != c.height
        || record.execution_state_root != state_root
        || record.snapshot_ref.as_deref() != Some(state_root.as_str())
        || record.latest_state_ref != record.snapshot_ref
        || record.journal_ref.as_deref() != Some(journal_ref.as_str())
        || record.journal_len != after_journal.len()
        || record.node_block_hash.as_deref() != Some(c.node_block_hash.as_str())
        || record.proposer_id.as_deref() != Some(c.proposer_id.as_str())
        || record.action_root.as_deref() != Some(c.action_root.as_str())
        || record.timestamp_ms != c.committed_at_unix_ms
        || record.simulator_mirror.is_some()
        || record.controlled_capture_ref.is_some()
        || record.commit_log_ref.is_some()
        || record.checkpoint_ref.is_some()
        || record.world_head_proof_ref.is_some()
        || record.world_head_proof_hash.is_some()
        || record.execution_block_hash
            != blake3_hex(&to_cbor(ExecutionHashPayload {
                world_id,
                height: c.height,
                prev_execution_block_hash: predecessor_execution_block_hash,
                execution_state_root: &state_root,
                journal_len: after_journal.len(),
            })?)
    {
        return Err("live candidate checkpoint/result mismatch".into());
    }
    super::external_effect::validate_execution_external_effect_for_context(effect, c)?;
    if super::external_effect::build_execution_external_effect_materialization_with_pre_step_root(
        before_world,
        c,
        None,
    )? != *effect
    {
        return Err("live predecessor module/effect binding mismatch".into());
    }
    Ok((before, after))
}
