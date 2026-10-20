//! Formal execution input closure. No engineering evidence grants authority.
use super::{
    ExecutionBridgeRecord, ExecutionExternalEffectMaterialization, controlled_capture as capture,
    to_cbor,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use oasis7_distfs::controlled_authority::{
    formal::{coordinator::ExecutableFormalFrozenV1, endpoint::FormalCandidateMaterialV1, *},
    replicated_protocol::{ArtifactObject, ArtifactRole, ClosedRecord},
};
use oasis7_node::{NodeLocalExecutionContinuation, compute_consensus_action_root};
use oasis7_runtime::{Journal, ReleaseSecurityPolicy, Snapshot, World, blake3_hex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[path = "formal_frozen_snapshot.rs"]
mod frozen_snapshot;
#[cfg(test)]
#[path = "formal_package_tests.rs"]
mod tests;
pub(super) use frozen_snapshot::VerifiedFrozenSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalExecutionManifestV1 {
    pub schema_version: u32,
    pub domain: String,
    pub world: FormalWorldIdentityV1,
    pub authority_id: String,
    pub activation_ref: String,
    pub parent: FormalHeadRefV1,
    pub continuation: NodeLocalExecutionContinuation,
    pub release_security_policy: ReleaseSecurityPolicy,
    pub executable_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_environment_ref: Option<String>,
    pub execution_policy_cbor: Vec<u8>,
    pub execution_policy_digest: String,
    /// Only current input dependencies; never recursive parent proof closures.
    pub artifact_roots: BTreeMap<ArtifactRole, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_tree: Option<oasis7::world_service::formal_result::FormalBusinessResultTreeV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_qualified: Option<FormalQualifiedCommitProofV1>,
    /// A one-shot, old-writer-signed healthy durable-set transition. Omission
    /// preserves the historical Frozen input bytes exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durable_set_transition: Option<SignedNormalDurableSetTransitionV1>,
}

pub(super) fn insert(
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

/// A complete current prestate and the already reserved actual Node decision.
/// This builder does not query cognition, execute actions or mint activation.
pub(super) fn freeze_material(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    continuation: &NodeLocalExecutionContinuation,
    world: &World,
    release_policy: &ReleaseSecurityPolicy,
) -> Result<(FormalFrozenBodyV1, ClosedRecord), String> {
    freeze_material_inner(
        authority,
        parent,
        continuation,
        world,
        release_policy,
        None,
        None,
        None,
        None,
        None,
    )
}
pub(super) fn freeze_material_with_durable_set_transition(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    continuation: &NodeLocalExecutionContinuation,
    world: &World,
    release_policy: &ReleaseSecurityPolicy,
    transition: &SignedNormalDurableSetTransitionV1,
) -> Result<(FormalFrozenBodyV1, ClosedRecord), String> {
    freeze_material_inner(
        authority,
        parent,
        continuation,
        world,
        release_policy,
        None,
        None,
        Some(transition.clone()),
        None,
        None,
    )
}
/// Fresh profile-2 writers reuse the qualified predecessor's exact bounded
/// Snapshot artifact chain, avoiding a full prestate checkpoint every commit.
#[expect(
    clippy::too_many_arguments,
    reason = "Distinct authenticated inputs remain explicit."
)]
pub(super) fn freeze_material_from_parent(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    continuation: &NodeLocalExecutionContinuation,
    world: &World,
    release_policy: &ReleaseSecurityPolicy,
    business_tree: oasis7::world_service::formal_result::FormalBusinessResultTreeV1,
    parent_qualified: FormalQualifiedCommitProofV1,
    transition: Option<SignedNormalDurableSetTransitionV1>,
    parent_material: &ClosedRecord,
) -> Result<(FormalFrozenBodyV1, ClosedRecord), String> {
    business_tree.validate_world_boundary(world, parent.record())?;
    verify_record_material(parent.record(), parent_material).map_err(|e| e.to_string())?;
    freeze_material_inner(
        authority,
        parent,
        continuation,
        world,
        release_policy,
        Some(business_tree),
        Some(parent_qualified),
        transition,
        Some(parent_material),
        None,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "This private encoder assembles the exact signed Frozen closure from distinct typed inputs."
)]
fn freeze_material_inner(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    continuation: &NodeLocalExecutionContinuation,
    world: &World,
    release_policy: &ReleaseSecurityPolicy,
    business_tree: Option<oasis7::world_service::formal_result::FormalBusinessResultTreeV1>,
    parent_qualified: Option<FormalQualifiedCommitProofV1>,
    durable_set_transition: Option<SignedNormalDurableSetTransitionV1>,
    parent_material: Option<&ClosedRecord>,
    verified_snapshot: Option<&VerifiedFrozenSnapshot<'_>>,
) -> Result<(FormalFrozenBodyV1, ClosedRecord), String> {
    if authority.definition().profile_version == 2
        && (business_tree.is_none() || parent_qualified.is_none())
    {
        return Err("formal profile 2 requires an authenticated predecessor business tree".into());
    }
    if parent_qualified
        .as_ref()
        .is_some_and(|proof| &proof.decision.record != parent.record())
    {
        return Err("formal frozen parent public proof mismatch".into());
    }
    let snapshot = world.snapshot();
    let snapshot_bytes = to_cbor(&snapshot)?;
    let pre_state_root = blake3_hex(&snapshot_bytes);
    let c = &continuation.context;
    let identity = &snapshot.chain_resource_manifest;
    let formal_world = &authority.definition().world;
    let next_position = parent
        .head()
        .position
        .checked_add(1)
        .ok_or("formal position exhausted")?;
    if let Some(transition) = &durable_set_transition {
        verify_durable_set_transition_arm(transition, authority, parent)?;
    }
    if continuation.schema_version != 1
        || c.world_id != formal_world.world_id
        || identity.world_id != formal_world.world_id
        || identity.chain_id != formal_world.chain_id
        || identity.genesis_ref.as_deref() != Some(formal_world.genesis_digest.as_str())
        || parent.record().world != *formal_world
        || parent.record().after_state_root != pre_state_root
        || continuation.predecessor_execution_state_root.as_deref() != Some(pre_state_root.as_str())
        || c.node_id != c.proposer_id
        || continuation.predecessor_execution_height != continuation.predecessor_committed_height
        || continuation.predecessor_committed_height.checked_add(1) != Some(c.height)
        || continuation.reserved_action_bytes
            != c.committed_actions
                .iter()
                .try_fold(0usize, |n, a| n.checked_add(a.payload_cbor.len()))
                .ok_or("formal action byte overflow")?
        || compute_consensus_action_root(&c.committed_actions).map_err(|e| format!("{e:?}"))?
            != c.action_root
    {
        return Err("formal actual decision/prestate/world binding mismatch".into());
    }
    for action in &c.committed_actions {
        action.validate().map_err(|e| format!("{e:?}"))?;
    }
    let (_, simulator, bootstrap, _) = super::driver_replicated_input::decode_committed_actions(c)?;
    if !simulator.is_empty() || bootstrap.is_some() {
        return Err("formal execution does not support simulator/bootstrap mutation".into());
    }
    validate_modules(&snapshot)?;
    let journal: Journal = world.journal().clone();
    let restored = if authority.definition().profile_version == 2 {
        world.restore_snapshot_with_attached_formal_history(snapshot.clone(), journal.clone())
    } else {
        oasis7::runtime_host::world::from_snapshot(snapshot.clone(), journal.clone())
    }
    .map_err(|e| format!("{e:?}"))?
    .with_release_security_policy(release_policy.clone());
    if to_cbor(restored.snapshot())? != snapshot_bytes {
        return Err("formal typed prestate restoration changed bytes".into());
    }
    let (policy_cbor, executable_digest, execution_policy_digest) =
        execution_policy_material(release_policy)?;
    if execution_policy_digest != parent.record().execution_policy_digest {
        return Err("formal execution policy change requires authorized history transition".into());
    }
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    let governing_manifest_digest = binding
        .runtime_manifest_hash
        .to_string()
        .strip_prefix("blake3:")
        .ok_or("formal governing manifest encoding")?
        .to_owned();
    if governing_manifest_digest != parent.record().governing_manifest_digest {
        return Err("formal prestate governing manifest differs from committed parent".into());
    }
    let mut objects = BTreeMap::new();
    let input = insert(&mut objects, to_cbor(&c.committed_actions)?, vec![]);
    let snapshot_ref = if let Some(parent_material) = parent_material {
        if let Some(verified) = verified_snapshot {
            super::formal_snapshot::inherit_verified(
                &mut objects,
                parent_material,
                &snapshot_bytes,
                verified,
            )?
        } else {
            super::formal_snapshot::inherit(&mut objects, parent_material, &snapshot_bytes)?
        }
    } else {
        insert(&mut objects, snapshot_bytes, vec![])
    };
    let journal_ref = insert(&mut objects, to_cbor(&journal)?, vec![]);
    let result = insert(
        &mut objects,
        to_cbor(("oasis7.formal.frozen_result.v1", &parent.head()))?,
        vec![],
    );
    let nonce = insert(
        &mut objects,
        to_cbor((
            "oasis7.formal.nonce_state.v1",
            &snapshot.capability_nonce_records,
            &snapshot.capability_revocation_state.world_service_results,
            &snapshot.state.authenticated_collect_data_last_nonces,
            &snapshot.state.agent_intent_ledger,
        ))?,
        vec![],
    );
    let outbox = insert(
        &mut objects,
        to_cbor((&snapshot.pending_effects, &snapshot.inflight_effects))?,
        vec![],
    );
    let rules = insert(
        &mut objects,
        to_cbor((&snapshot.manifest, &snapshot.policies, release_policy))?,
        vec![],
    );
    let mut wasm_refs = BTreeMap::new();
    for (hash, bytes) in &snapshot.module_artifact_bytes {
        wasm_refs.insert(hash.clone(), insert(&mut objects, bytes.clone(), vec![]));
    }
    let wasm = insert(
        &mut objects,
        to_cbor((&snapshot.module_registry, &wasm_refs))?,
        wasm_refs.values().cloned().collect(),
    );
    let mut roots = BTreeMap::from([
        (ArtifactRole::Input, input.clone()),
        (ArtifactRole::Snapshot, snapshot_ref),
        (ArtifactRole::Journal, journal_ref),
        (ArtifactRole::Result, result),
        (ArtifactRole::NonceIndex, nonce),
        (ArtifactRole::EffectOutbox, outbox),
        (ArtifactRole::Rules, rules),
        (ArtifactRole::Wasm, wasm),
    ]);
    let manifest = FormalExecutionManifestV1 {
        schema_version: 1,
        domain: "oasis7.formal.execution_input.v1".into(),
        world: formal_world.clone(),
        authority_id: authority.authority_id().map_err(|e| e.to_string())?,
        activation_ref: authority.activation_ref().into(),
        parent: parent.head(),
        continuation: continuation.clone(),
        release_security_policy: release_policy.clone(),
        executable_digest,
        native_environment_ref: None,
        execution_policy_cbor: policy_cbor,
        execution_policy_digest: execution_policy_digest.clone(),
        artifact_roots: roots.clone(),
        business_tree,
        parent_qualified,
        durable_set_transition,
    };
    let manifest_ref = insert(
        &mut objects,
        to_cbor(&manifest)?,
        roots.values().cloned().collect(),
    );
    roots.insert(ArtifactRole::ExecutionManifest, manifest_ref.clone());
    let material = ClosedRecord {
        payload_digest: input.clone(),
        before_state_root: pre_state_root.clone(),
        after_state_root: pre_state_root.clone(),
        roots,
        objects: objects.into_values().collect(),
    };
    material.validate().map_err(|e| e.to_string())?;
    capture::validate_package_budget(&to_cbor(&material)?)?;
    let body = FormalFrozenBodyV1 {
        schema_version: if parent_material.is_some() { 2 } else { 1 },
        world: formal_world.clone(),
        authority_id: manifest.authority_id,
        epoch: authority.definition().epoch,
        activation_ref: manifest.activation_ref,
        parent: parent.head(),
        next_position,
        request_digest: blake3_hex(&to_cbor((
            "oasis7.formal.node_request.v1",
            formal_world,
            continuation,
        ))?),
        payload_digest: input,
        runtime_manifest_digest: manifest_ref,
        governing_manifest_digest,
        execution_policy_digest,
        pre_state_root,
        frozen_material_digest: frozen_material_digest(&material).map_err(|e| e.to_string())?,
        handoff: None,
    };
    // Only restoration with a fully verified immutable original may defer this
    // last check. Its caller must compare the complete rebuilt body/material to
    // that token before returning; ordinary Frozen builders still verify here.
    if verified_snapshot.is_none() {
        verify_frozen_material(&body, &material).map_err(|e| e.to_string())?;
    }
    Ok((body, material))
}

/// Validate the arm before it becomes part of a Frozen input. This proves the
/// old writer authorized exactly the next canonical position; the later
/// Qualified proof is still required before an activation can be verified.
pub(super) fn verify_durable_set_transition_arm(
    transition: &SignedNormalDurableSetTransitionV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
) -> Result<String, String> {
    transition
        .body
        .validate(authority)
        .map_err(|error| error.to_string())?;
    let expected_authority = authority
        .authority_id()
        .map_err(|error| error.to_string())?;
    if parent.record().durable_set_transition_digest.is_some()
        && authority.durable_set_activation_digest().is_none()
        && parent.record().original_authority_id == expected_authority
    {
        return Err(
            "old formal authority cannot arm or execute after an unactivated durable-set boundary"
                .into(),
        );
    }
    let expected_position = parent
        .head()
        .position
        .checked_add(1)
        .ok_or("formal durable-set boundary position exhausted")?;
    if transition.body.predecessor != parent.head()
        || transition.body.predecessor_authority_id != expected_authority
        || parent.finalizing_authority_id() != expected_authority
        || transition.body.boundary_position != expected_position
        || parent.record().world != transition.body.world
    {
        return Err(
            "formal durable-set arm differs from exact qualified predecessor/next position".into(),
        );
    }
    let writer_public_key = hex::decode(&authority.definition().writer_public_key)
        .map_err(|_| "formal durable-set old writer key is not hex")?;
    let writer_public_key: [u8; 32] = writer_public_key
        .try_into()
        .map_err(|_| "formal durable-set old writer key width")?;
    if hex::encode(writer_public_key) != authority.definition().writer_public_key {
        return Err("formal durable-set old writer key is not canonical hex".into());
    }
    let verifying_key = VerifyingKey::from_bytes(&writer_public_key)
        .map_err(|_| "formal durable-set old writer key is invalid")?;
    let signature_bytes = hex::decode(&transition.writer_signature_hex)
        .map_err(|_| "formal durable-set transition signature is not hex")?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| "formal durable-set transition signature width")?;
    if hex::encode(signature.to_bytes()) != transition.writer_signature_hex {
        return Err("formal durable-set transition signature is not canonical hex".into());
    }
    let signing_bytes = normal_durable_set_transition_signing_bytes(&transition.body)
        .map_err(|error| error.to_string())?;
    verifying_key
        .verify_strict(&signing_bytes, &signature)
        .map_err(|_| "formal durable-set transition lacks the old writer signature")?;
    transition.body.digest().map_err(|error| error.to_string())
}

pub(super) fn validate_modules(snapshot: &Snapshot) -> Result<(), String> {
    let modules = snapshot
        .module_registry
        .records
        .values()
        .map(|r| r.manifest.wasm_hash.clone())
        .collect::<BTreeSet<_>>();
    if snapshot.module_artifacts != modules
        || snapshot
            .module_artifact_bytes
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != modules
    {
        return Err("formal prestate has missing or extra module bytes".into());
    }
    Ok(())
}

/// Only the coordinator's dual durable Frozen token can authorize execution.
/// Replay decodes the old captured decision, never asks a planner for new input.
fn checked_execution_manifest(
    frozen: &ExecutableFormalFrozenV1,
    original_authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
) -> Result<FormalExecutionManifestV1, String> {
    let material = frozen.material();
    if frozen.certificate().reservation != *frozen.input() {
        return Err("formal Frozen certificate differs from original signed input".into());
    }
    frozen
        .certificate()
        .verify(
            original_authority.definition(),
            original_authority.activation_ref(),
        )
        .map_err(|e| e.to_string())?;
    let manifest: FormalExecutionManifestV1 =
        capture::decode_role(material, ArtifactRole::ExecutionManifest)?;
    if !matches!(
        (manifest.schema_version, manifest.domain.as_str()),
        (1, "oasis7.formal.execution_input.v1") | (2, "oasis7.formal.execution_input.v2")
    ) || manifest.parent != parent.head()
        || manifest.authority_id
            != original_authority
                .authority_id()
                .map_err(|e| e.to_string())?
        || manifest.activation_ref != original_authority.activation_ref()
        || manifest.world != original_authority.definition().world
        || manifest.executable_digest != crate::controlled_execution_policy::executable_digest()?
    {
        return Err("formal frozen original execution artifact/context mismatch".into());
    }
    Ok(manifest)
}

pub(super) fn restore_executable(
    frozen: &ExecutableFormalFrozenV1,
    original_authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    history_source: Option<&World>,
    read_anchor: Option<&VerifiedFormalReadAnchorV1>,
    business_reader: Option<&super::formal_business_package::SharedBusinessReader>,
    verified_snapshot: &VerifiedFrozenSnapshot<'_>,
) -> Result<(World, NodeLocalExecutionContinuation), String> {
    verified_snapshot.check_binding(frozen, original_authority, parent)?;
    #[cfg(test)]
    verified_snapshot.record_restore();
    let material = frozen.material();
    let manifest = checked_execution_manifest(frozen, original_authority, parent)?;
    super::formal_native_environment::verify_current(material, &manifest)?;
    if let Some(transition) = &manifest.durable_set_transition {
        verify_durable_set_transition_arm(transition, original_authority, parent)?;
    }
    let snapshot: Snapshot = capture::decode_generic(verified_snapshot.bytes_for(material)?)?;
    let journal: Journal = capture::decode_role(material, ArtifactRole::Journal)?;
    let world = if original_authority.definition().profile_version == 2 {
        if let Some(source) = history_source {
            source
                .restore_snapshot_with_attached_formal_history(snapshot, journal)
                .map_err(|e| format!("{e:?}"))?
        } else {
            let anchor = read_anchor.ok_or("formal frozen restore lacks qualified read anchor")?;
            if anchor.head().record() != parent.record() {
                return Err("formal frozen restore read anchor differs from exact parent".into());
            }
            let reader = business_reader.ok_or("formal frozen restore lacks CAS reader")?;
            let bytes = to_cbor(&snapshot)?;
            let retained =
                oasis7_runtime::VerifiedFormalRetainedHistoryParentV1::verify(anchor, &bytes)?;
            oasis7::runtime_host::world::from_verified_formal_snapshot(
                snapshot,
                journal,
                retained,
                reader.0.clone(),
            )
            .map_err(|e| format!("{e:?}"))?
        }
    } else {
        oasis7::runtime_host::world::from_snapshot(snapshot, journal)
            .map_err(|e| format!("{e:?}"))?
    }
    .with_release_security_policy(manifest.release_security_policy.clone());
    if let Some(tree) = &manifest.business_tree {
        tree.validate_world_boundary(&world, parent.record())?;
    }
    let (mut expected, mut rebuilt) = freeze_material_inner(
        original_authority,
        parent,
        &manifest.continuation,
        &world,
        &manifest.release_security_policy,
        manifest.business_tree.clone(),
        manifest.parent_qualified.clone(),
        manifest.durable_set_transition.clone(),
        (frozen.input().body.schema_version >= 2).then_some(material),
        Some(verified_snapshot),
    )?;
    if let Some(descriptor) = super::formal_native_environment::descriptor(material)? {
        super::formal_native_environment::attach_frozen_for_exact_restore(
            &mut expected,
            &mut rebuilt,
            &descriptor,
            verified_snapshot,
        )?;
    }
    verified_snapshot.verify_exact_rebuild(&expected, &rebuilt)?;
    Ok((world, manifest.continuation))
}

/// Encode an actual executed result under the original formal reservation.
/// Canonical output identity excludes all receipts and finalizer envelopes.
pub(super) struct FormalRetainedExecutionOutputs {
    pub new_results: Vec<oasis7::world_service::CanonicalIntentResultV1>,
    pub event_root: String,
    pub unpruned_journal: oasis7_runtime::Journal,
    pub journal_cursors: Vec<(u64, u64)>,
    pub delta:
        Option<oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexDeltaV1>,
}
#[expect(
    clippy::too_many_arguments,
    reason = "Candidate construction receives exact execution outputs and authenticated inputs from separate verified stages."
)]
pub(super) fn candidate_with_business_reader(
    frozen: &ExecutableFormalFrozenV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    after_world: &World,
    record: &ExecutionBridgeRecord,
    effect: &ExecutionExternalEffectMaterialization,
    writer: &SigningKey,
    business_reader: &impl oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeReader,
    retained_outputs: Option<&FormalRetainedExecutionOutputs>,
    recovery: Option<&VerifiedRecovery>,
    verified_snapshot: &VerifiedFrozenSnapshot<'_>,
) -> Result<FormalCandidateMaterialV1, String> {
    if authority.definition().profile_version == 2 && retained_outputs.is_none() {
        return Err("formal profile 2 requires pre-prune actual execution outputs".into());
    }
    let signing_authority = if let Some(recovery) = recovery {
        if recovery.original_authority().definition() != authority.definition()
            || recovery.original_authority().policy() != authority.policy()
            || recovery.original_authority().activation_ref() != authority.activation_ref()
            || recovery.body().sealed_cut != parent.head()
        {
            return Err("formal replay original authority/cut mismatch".into());
        }
        recovery.successor()
    } else {
        authority
    };
    if hex::encode(writer.verifying_key().as_bytes())
        != signing_authority.definition().writer_public_key
    {
        return Err("formal candidate original writer key mismatch".into());
    }
    let (before_world, continuation) = restore_executable(
        frozen,
        authority,
        parent,
        Some(after_world),
        None,
        None,
        verified_snapshot,
    )?;
    let manifest: FormalExecutionManifestV1 =
        capture::decode_role(frozen.material(), ArtifactRole::ExecutionManifest)?;
    if recovery.is_some() && manifest.durable_set_transition.is_some() {
        return Err(
            "healthy durable-set transition cannot be combined with recovery replay".into(),
        );
    }
    let transition_digest = manifest
        .durable_set_transition
        .as_ref()
        .map(|transition| verify_durable_set_transition_arm(transition, authority, parent))
        .transpose()?;
    let before_bytes = to_cbor(before_world.snapshot())?;
    let before_journal_bytes = to_cbor(before_world.journal())?;
    let after_bytes = to_cbor(after_world.snapshot())?;
    let after_journal_bytes = to_cbor(after_world.journal())?;
    let previous_block = continuation
        .predecessor_execution_block_hash
        .as_deref()
        .ok_or("formal original business execution predecessor missing")?;
    let world = &authority.definition().world;
    let transition = super::controlled_live_package::ActualTransition {
        continuation: &continuation,
        predecessor_execution_block_hash: previous_block,
        before_snapshot_bytes: &before_bytes,
        before_journal_bytes: &before_journal_bytes,
        after_snapshot_bytes: &after_bytes,
        after_journal_bytes: &after_journal_bytes,
        record,
        effect,
        release_policy: &manifest.release_security_policy,
    };
    let (before, after) = if let Some(outputs) = retained_outputs {
        let full = &outputs.unpruned_journal.events;
        let hot = &after_world.journal().events;
        if full.len() != outputs.journal_cursors.len()
            || hot.len() > full.len()
            || full[full.len() - hot.len()..] != *hot
        {
            return Err("formal pruned journal differs from actual execution outputs".into());
        }
        let removed = full.len() - hot.len();
        let prefix = outputs.journal_cursors[..removed]
            .iter()
            .copied()
            .zip(full[..removed].iter().cloned())
            .collect::<Vec<_>>();
        after_world
            .verify_formal_archived_journal_prefix(&prefix)
            .map_err(|error| error.to_string())?;
        let predecessor: Journal = capture::decode_generic(&before_journal_bytes)?;
        if full.len() < predecessor.events.len()
            || full[..predecessor.events.len()] != predecessor.events
            || outputs.event_root != blake3_hex(&to_cbor(&full[predecessor.events.len()..])?)
        {
            return Err("formal event root differs from actual pre-prune journal suffix".into());
        }
        super::controlled_live_package::validate_formal_transition(
            &world.world_id,
            &world.chain_id,
            &world.genesis_digest,
            transition,
            &outputs.unpruned_journal,
            &before_world,
            after_world,
        )?
    } else {
        super::controlled_live_package::validate_transition(
            &world.world_id,
            &world.chain_id,
            &world.genesis_digest,
            transition,
        )?
    };
    let mut objects = frozen
        .material()
        .objects
        .iter()
        .map(|object| (object.content_hash.clone(), object.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut roots = frozen.material().roots.clone();
    let state_root = blake3_hex(&after_bytes);
    let snapshot = super::formal_snapshot::append(
        &mut objects,
        frozen.material(),
        &before_world.snapshot(),
        &after_world.snapshot(),
        after_bytes,
    )?;
    let journal = insert(&mut objects, after_journal_bytes, vec![]);
    let effect_ref = insert(&mut objects, to_cbor(effect)?, vec![]);
    let result = insert(
        &mut objects,
        to_cbor(record)?,
        vec![snapshot.clone(), journal.clone(), effect_ref.clone()],
    );
    let nonce = insert(
        &mut objects,
        to_cbor((
            "oasis7.formal.nonce_state.v1",
            &after.capability_nonce_records,
            &after.capability_revocation_state.world_service_results,
            &after.state.authenticated_collect_data_last_nonces,
            &after.state.agent_intent_ledger,
        ))?,
        vec![],
    );
    let outbox = insert(
        &mut objects,
        to_cbor((&after.pending_effects, &after.inflight_effects))?,
        vec![effect_ref],
    );
    let rules = insert(
        &mut objects,
        to_cbor((
            &before.manifest,
            &after.manifest,
            &before.policies,
            &after.policies,
            &manifest.release_security_policy,
        ))?,
        vec![],
    );
    let mut wasm_refs = BTreeMap::new();
    for (hash, bytes) in before
        .module_artifact_bytes
        .iter()
        .chain(after.module_artifact_bytes.iter())
    {
        wasm_refs.insert(hash.clone(), insert(&mut objects, bytes.clone(), vec![]));
    }
    let wasm = insert(
        &mut objects,
        to_cbor((&before.module_registry, &after.module_registry, &wasm_refs))?,
        wasm_refs.values().cloned().collect(),
    );
    roots.extend([
        (ArtifactRole::Snapshot, snapshot.clone()),
        (ArtifactRole::Journal, journal),
        (ArtifactRole::Result, result.clone()),
        (ArtifactRole::NonceIndex, nonce),
        (ArtifactRole::EffectOutbox, outbox),
        (ArtifactRole::Rules, rules),
        (ArtifactRole::Wasm, wasm),
    ]);
    let mut material = ClosedRecord {
        payload_digest: frozen.material().payload_digest.clone(),
        before_state_root: frozen.input().body.pre_state_root.clone(),
        after_state_root: state_root.clone(),
        roots: roots.clone(),
        objects: objects.into_values().collect(),
    };
    material.validate().map_err(|e| e.to_string())?;
    capture::validate_package_budget(&to_cbor(&material)?)?;
    let governing = after_world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?
        .runtime_manifest_hash
        .to_string()
        .strip_prefix("blake3:")
        .ok_or("formal output governing manifest encoding")?
        .to_owned();
    let body = &frozen.input().body;
    let mut canonical = CanonicalRecordV1 {
        schema_version: match (
            manifest.native_environment_ref.is_some(),
            transition_digest.is_some(),
        ) {
            (true, false) => 5,
            (true, true) => 6,
            (false, false) => 3,
            (false, true) => 4,
        },
        durable_set_transition_digest: transition_digest,
        kind: FormalRecordKindV1::Execution,
        world: body.world.clone(),
        position: body.next_position,
        parent_record_id: body.parent.record_id.clone(),
        original_authority_id: body.authority_id.clone(),
        original_epoch: body.epoch,
        activation_ref: body.activation_ref.clone(),
        request_digest: body.request_digest.clone(),
        payload_digest: body.payload_digest.clone(),
        frozen_reservation_id: Some(body.reservation_id().map_err(|e| e.to_string())?),
        runtime_manifest_digest: body.runtime_manifest_digest.clone(),
        governing_manifest_digest: governing,
        execution_policy_digest: body.execution_policy_digest.clone(),
        before_state_root: body.pre_state_root.clone(),
        after_state_root: state_root,
        action_root: continuation.context.action_root.clone(),
        event_root: match retained_outputs {
            Some(outputs) => outputs.event_root.clone(),
            None => blake3_hex(&to_cbor(
                &after_world.journal().events[before_world.journal().events.len()..],
            )?),
        },
        result_root: result,
        projection_root: oasis7::world_service::formal_projection::commitments_for_world(
            after_world,
        )?
        .root()?,
        artifact_roots: roots,
        handoff: body.handoff.clone(),
    };
    if let Some(previous) = &manifest.business_tree {
        use oasis7::world_service::{
            CanonicalIntentResultV1, formal_result::FormalBusinessResultTreeV1,
        };
        use oasis7_distfs::controlled_authority::formal::business_material::{
            FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1, FormalBusinessResultEnvelopeV1,
        };
        let new_results = if let Some(outputs) = retained_outputs {
            outputs.new_results.clone()
        } else {
            after
                .capability_revocation_state
                .world_service_results
                .iter()
                .filter(|(key, value)| {
                    before
                        .capability_revocation_state
                        .world_service_results
                        .get(*key)
                        != Some(*value)
                })
                .map(|(_, value)| {
                    serde_json::from_value::<CanonicalIntentResultV1>(value.clone())
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let update = FormalBusinessResultTreeV1::extend_with_parent_proof(
            after_world,
            &canonical,
            previous,
            manifest
                .parent_qualified
                .as_ref()
                .ok_or("formal original parent proof missing")?,
            &new_results,
            business_reader,
        )?;
        canonical.result_root = update.tree.root()?;
        let mut references = vec![
            material.roots[&ArtifactRole::Snapshot].clone(),
            material.roots[&ArtifactRole::Journal].clone(),
            material.roots[&ArtifactRole::Result].clone(),
        ];
        for node in &update.delta.node_writes {
            if blake3_hex(&node.bytes) != node.digest {
                return Err("formal business node write hash mismatch".into());
            }
        }
        let envelope = FormalBusinessResultEnvelopeV1 {
            schema_version: 1,
            domain: FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1.into(),
            execution_result_cbor: to_cbor(record)?,
            tree_descriptor_cbor: update.tree.encode_cbor()?,
            delta: update.delta,
            retained_history_delta: retained_outputs.and_then(|outputs| outputs.delta.clone()),
        }
        .encode()
        .map_err(|e| e.to_string())?;
        references.sort();
        references.dedup();
        let digest = blake3_hex(&envelope);
        material.objects.push(ArtifactObject {
            content_hash: digest.clone(),
            bytes: envelope,
            references,
        });
        material
            .objects
            .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
        material.roots.insert(ArtifactRole::Result, digest);
        canonical.artifact_roots = material.roots.clone();
    } else if authority.definition().profile_version == 2 {
        return Err("formal profile 2 result lacks predecessor business tree".into());
    }
    verify_record_material(&canonical, &material).map_err(|e| e.to_string())?;
    frozen
        .certificate()
        .matches_record(&canonical)
        .map_err(|e| e.to_string())?;
    let signature = writer
        .sign(&canonical_candidate_signing_bytes(&canonical).map_err(|e| e.to_string())?)
        .to_bytes();
    let candidate = FormalCandidateMaterialV1 {
        candidate: SignedCanonicalCandidateV1 {
            record: canonical,
            writer_signature_hex: hex::encode(signature),
        },
        material,
        frozen: Some(frozen.certificate().clone()),
        frozen_material: Some(frozen.material().clone()),
        branch: if recovery.is_some() {
            FormalCompletionBranchV1::ReplayFrozen
        } else {
            FormalCompletionBranchV1::Normal
        },
    };
    verify_formal_candidate(
        &candidate.candidate,
        candidate.frozen.as_ref(),
        authority,
        Some(parent),
        recovery,
        candidate.branch,
    )
    .map_err(|error| error.to_string())?;
    Ok(candidate)
}

pub(crate) fn execution_policy_material(
    release_policy: &ReleaseSecurityPolicy,
) -> Result<(Vec<u8>, String, String), String> {
    let policy_cbor =
        to_cbor(crate::controlled_execution_policy::GuardedExecutionPolicy::current())?;
    let executable_digest = crate::controlled_execution_policy::executable_digest()?;
    let digest = blake3_hex(&to_cbor((
        "oasis7.formal.execution_policy.v1",
        &policy_cbor,
        &executable_digest,
        release_policy,
    ))?);
    Ok((policy_cbor, executable_digest, digest))
}
