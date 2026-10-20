//! Offline capture for the bounded origin-bearing ScheduleRecipe path.
//! This index records execution outcomes; it is not the ingress nonce ledger.
use super::{ExecutionBridgeRecord, ExecutionExternalEffectMaterialization, to_cbor};
use oasis7_distfs::controlled_authority::replicated_protocol::{
    ArtifactObject, ArtifactRole, ClosedRecord,
};
use oasis7_node::{NodeExecutionCommitContext, compute_consensus_action_root};
use oasis7_runtime::blake3_hex;
use oasis7_runtime::{BlobStore, Journal, LocalCasStore, Snapshot, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(super) const CAPTURE_SCHEMA: u32 = 1;
pub(super) const CAPTURE_SCOPE: &str = "schedule_recipe_execution_history_prerequisite_v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CapturePreparation {
    pub schema_version: u32,
    pub scope: String,
    pub context: NodeExecutionCommitContext,
    pub release_security_policy: oasis7_runtime::ReleaseSecurityPolicy,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes"
    )]
    pub before_snapshot: Vec<u8>,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes"
    )]
    pub before_journal: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparationBinding {
    schema_version: u32,
    scope: String,
    context: NodeExecutionCommitContext,
    release_security_policy: oasis7_runtime::ReleaseSecurityPolicy,
    before_snapshot_ref: String,
    before_journal_ref: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutcomeEntry {
    pub submission: oasis7_runtime::GameplaySubmissionOrigin,
    pub input_hash: String,
    pub execution_height: u64,
    pub execution_block_hash: String,
    pub result_state_root: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutcomeIndex {
    pub scope: String,
    pub initial_height: u64,
    pub initial_state_root: String,
    pub entries: Vec<OutcomeEntry>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CaptureManifest {
    pub schema_version: u32,
    pub scope: String,
    pub context: NodeExecutionCommitContext,
    pub record: ExecutionBridgeRecord,
    pub before_snapshot_ref: String,
    pub before_journal_ref: String,
    pub preparation_ref: String,
    pub previous_capture_ref: Option<String>,
    pub outcome_index_ref: String,
    pub effect_ref: String,
    pub wasm_refs: BTreeMap<String, String>,
    pub rules_ref: String,
}

pub(super) fn supported_origin(
    context: &NodeExecutionCommitContext,
) -> Result<oasis7_runtime::CommittedRecipeOrigin, String> {
    if context.committed_actions.len() != 1 {
        return Err("capture supports exactly one origin-bearing ScheduleRecipe".into());
    }
    let input = &context.committed_actions[0];
    if blake3_hex(&input.payload_cbor) != input.payload_hash {
        return Err("capture payload hash mismatch".into());
    }
    if compute_consensus_action_root(&context.committed_actions).map_err(|e| format!("{e:?}"))?
        != context.action_root
    {
        return Err("capture action root mismatch".into());
    }
    let (actions, simulator, bootstrap, service_intents) =
        super::driver_replicated_input::decode_committed_actions(context)?;
    if actions.len() != 1
        || !simulator.is_empty()
        || bootstrap.is_some()
        || !service_intents.is_empty()
    {
        return Err("unsupported capture operation".into());
    }
    actions
        .into_iter()
        .next()
        .and_then(|(_, origin)| origin)
        .ok_or_else(|| "legacy request has no verified ingress origin".into())
}

/// Preserve every state byte while accepting only the existing snapshot cache,
/// reopen, and default resource-annotation transitions. These annotations are
/// diagnostic runtime contexts, not trusted genesis identities.
pub(super) fn validate_continuity(
    package: &ClosedRecord,
    manifest: &CaptureManifest,
    next: &CapturePreparation,
) -> Result<(), String> {
    let previous_pre = decode_snapshot(object(package, &manifest.before_snapshot_ref)?)?;
    let post = decode_snapshot(object(
        package,
        manifest
            .record
            .snapshot_ref
            .as_deref()
            .ok_or("missing post snapshot")?,
    )?)?;
    let journal: Journal = decode_generic(object(
        package,
        manifest
            .record
            .journal_ref
            .as_deref()
            .ok_or("missing post journal")?,
    )?)?;
    validate_snapshot_continuity(&post, journal, Some(&previous_pre), next)
}

pub(super) fn validate_snapshot_continuity(
    post: &Snapshot,
    journal: Journal,
    previous_pre: Option<&Snapshot>,
    next: &CapturePreparation,
) -> Result<(), String> {
    use oasis7_runtime::ChainResourceDerivationContext;
    use sha2::{Digest, Sha256};
    let world = oasis7::runtime_host::world::from_snapshot(post.clone(), journal)
        .map_err(|e| format!("prefix restore: {e:?}"))?;
    let incoming = decode_snapshot(&next.before_snapshot)?;
    let manifest_hash = hex::encode(Sha256::digest(
        serde_json::to_vec(&post.manifest).map_err(|e| e.to_string())?,
    ));
    let fallback = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: "runtime-world",
            chain_id: "runtime-chain",
            genesis_ref: None,
            created_at_height: world.journal().len() as u64,
            manifest_height: world.journal().len() as u64,
            commit_block_hash: None,
            tick: post.state.time,
        },
        manifest_hash.clone(),
        manifest_hash,
    );
    let pair_matches = previous_pre
        .into_iter()
        .chain([post, &fallback])
        .any(|candidate| {
            candidate.chain_resource_manifest == incoming.chain_resource_manifest
                && candidate.latest_chain_resource_delta == incoming.latest_chain_resource_delta
        });
    if !pair_matches {
        return Err("unrecognized resource annotation transition".into());
    }
    let resource = &incoming.chain_resource_manifest;
    let expected_delta = oasis7_runtime::chain_resource::latest_from_runtime_manifest(
        ChainResourceDerivationContext {
            world_id: &resource.world_id,
            chain_id: &resource.chain_id,
            genesis_ref: resource.genesis_ref.as_deref(),
            created_at_height: resource.created_at_height,
            manifest_height: resource.manifest_height,
            commit_block_hash: resource.created_at_block_hash.as_deref(),
            tick: incoming
                .latest_chain_resource_delta
                .as_ref()
                .ok_or("missing resource delta")?
                .tick,
        },
        resource,
    );
    if incoming.latest_chain_resource_delta.as_ref() != Some(&expected_delta) {
        return Err("resource delta/manifest binding mismatch".into());
    }
    let mut expected = world.snapshot();
    expected.chain_resource_manifest = incoming.chain_resource_manifest;
    expected.latest_chain_resource_delta = incoming.latest_chain_resource_delta;
    if to_cbor(expected)? != next.before_snapshot
        || to_cbor(world.journal())? != next.before_journal
    {
        return Err("history typed world/journal prefix mismatch".into());
    }
    Ok(())
}

pub(super) fn prepare(
    world: &World,
    context: &NodeExecutionCommitContext,
) -> Result<CapturePreparation, String> {
    supported_origin(context)?;
    Ok(CapturePreparation {
        schema_version: CAPTURE_SCHEMA,
        scope: CAPTURE_SCOPE.into(),
        context: context.clone(),
        release_security_policy: world.release_security_policy().clone(),
        before_snapshot: to_cbor(world.snapshot())?,
        before_journal: to_cbor(world.journal())?,
    })
}

fn add(
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

/// Build from real captured material, never from the mutable ingress ledger.
/// The caller supplies the preceding captured outcome prefix; a historical
/// reader verifies that prefix against all preceding records.
pub(super) fn build(
    preparation: &CapturePreparation,
    record: &ExecutionBridgeRecord,
    snapshot_bytes: &[u8],
    journal_bytes: &[u8],
    effect: &ExecutionExternalEffectMaterialization,
    preceding: Option<(&ClosedRecord, &CaptureManifest)>,
) -> Result<ClosedRecord, String> {
    if preparation.schema_version != CAPTURE_SCHEMA
        || preparation.scope != CAPTURE_SCOPE
        || record.schema_version != 3
        || record.controlled_capture_ref.is_some()
    {
        return Err("unsupported capture schema or recursive record".into());
    }
    let origin = supported_origin(&preparation.context)?;
    super::external_effect::validate_execution_external_effect_for_context(
        effect,
        &preparation.context,
    )?;
    let before: Snapshot = decode_snapshot(&preparation.before_snapshot)?;
    let after: Snapshot = decode_snapshot(snapshot_bytes)?;
    // Validate both historical byte sets using the existing restore contract,
    // including inline module SHA256 and journal-prefix commitments.
    oasis7::runtime_host::world::from_snapshot(
        after.clone(),
        decode_generic::<Journal>(journal_bytes)?,
    )
    .map_err(|e| format!("invalid post-execution world: {e:?}"))?;
    if before.state.time.saturating_add(1) != preparation.context.height
        || after.state.time != preparation.context.height
        || record.height != preparation.context.height
        || record.world_id != preparation.context.world_id
        || record.execution_state_root != blake3_hex(snapshot_bytes)
        || effect.pre_step_execution_state_root != blake3_hex(&preparation.before_snapshot)
    {
        return Err("capture state/context binding mismatch".into());
    }
    let before_world = oasis7::runtime_host::world::from_snapshot(
        before.clone(),
        decode_generic::<Journal>(&preparation.before_journal)?,
    )
    .map_err(|e| format!("pre-world restore: {e:?}"))?
    .with_release_security_policy(preparation.release_security_policy.clone());
    let expected_effect =
        super::external_effect::build_execution_external_effect_materialization_with_pre_step_root(
            &before_world,
            &preparation.context,
            None,
        )?;
    if &expected_effect != effect {
        return Err("capture pre-world manifest/module/effect binding mismatch".into());
    }
    for snapshot in [&before, &after] {
        for module in snapshot.module_registry.records.values() {
            if !snapshot
                .module_artifact_bytes
                .contains_key(&module.manifest.wasm_hash)
            {
                return Err("capture missing declared module bytes".into());
            }
        }
    }
    if record.node_block_hash.as_deref() != Some(preparation.context.node_block_hash.as_str())
        || record.proposer_id.as_deref() != Some(preparation.context.proposer_id.as_str())
        || record.action_root.as_deref() != Some(preparation.context.action_root.as_str())
        || record.timestamp_ms != preparation.context.committed_at_unix_ms
        || record.snapshot_ref.as_deref() != Some(blake3_hex(snapshot_bytes).as_str())
        || record.journal_ref.as_deref() != Some(blake3_hex(journal_bytes).as_str())
        || record.external_effect_ref.as_deref() != Some(blake3_hex(&to_cbor(effect)?).as_str())
        || record.simulator_mirror.is_some()
    {
        return Err("capture record/context/artifact binding mismatch".into());
    }
    if record.latest_state_ref != record.snapshot_ref
        || record.checkpoint_ref.is_some()
        || record.commit_log_ref.is_some()
        || record.world_head_proof_ref.is_some()
        || record.world_head_proof_hash.is_some()
    {
        return Err("capture requires undecorated execution record refs".into());
    }
    let mut index = if let Some((package, manifest)) = preceding {
        validate_continuity(package, manifest, preparation)?;
        decode_role::<OutcomeIndex>(package, ArtifactRole::NonceIndex).and_then(|index| {
            if manifest.record.height + 1 != record.height
                || package.after_state_root != manifest.record.execution_state_root
            {
                Err("capture prefix is not contiguous".into())
            } else {
                Ok(index)
            }
        })?
    } else {
        OutcomeIndex {
            scope: CAPTURE_SCOPE.into(),
            initial_height: before.state.time,
            initial_state_root: effect.pre_step_execution_state_root.clone(),
            entries: vec![],
        }
    };
    if index.entries.iter().any(|e| {
        e.submission.verified_player_id == origin.submission.verified_player_id
            && e.submission.public_key == origin.submission.public_key
            && e.submission.auth_nonce == origin.submission.auth_nonce
            && e.submission.hosted_registration_nonce == origin.submission.hosted_registration_nonce
    }) {
        return Err("duplicate captured execution request identity".into());
    }
    index.entries.push(OutcomeEntry {
        submission: origin.submission,
        input_hash: preparation.context.committed_actions[0]
            .payload_hash
            .clone(),
        execution_height: record.height,
        execution_block_hash: record.execution_block_hash.clone(),
        result_state_root: record.execution_state_root.clone(),
    });
    let mut objects = BTreeMap::new();
    let before_snapshot_ref = add(&mut objects, preparation.before_snapshot.clone(), vec![]);
    let before_journal_ref = add(&mut objects, preparation.before_journal.clone(), vec![]);
    let preparation_ref = add(
        &mut objects,
        to_cbor(PreparationBinding {
            schema_version: CAPTURE_SCHEMA,
            scope: CAPTURE_SCOPE.into(),
            context: preparation.context.clone(),
            release_security_policy: preparation.release_security_policy.clone(),
            before_snapshot_ref: before_snapshot_ref.clone(),
            before_journal_ref: before_journal_ref.clone(),
        })?,
        vec![before_snapshot_ref.clone(), before_journal_ref.clone()],
    );
    let input_ref = add(
        &mut objects,
        preparation.context.committed_actions[0]
            .payload_cbor
            .clone(),
        vec![],
    );
    let snapshot_ref = add(
        &mut objects,
        snapshot_bytes.to_vec(),
        vec![before_snapshot_ref.clone()],
    );
    let journal_ref = add(
        &mut objects,
        journal_bytes.to_vec(),
        vec![before_journal_ref.clone()],
    );
    let result_ref = add(
        &mut objects,
        to_cbor(record)?,
        vec![snapshot_ref.clone(), journal_ref.clone()],
    );
    let effect_ref = add(&mut objects, to_cbor(effect)?, vec![]);
    let index_ref = add(
        &mut objects,
        to_cbor(&index)?,
        vec![input_ref.clone(), result_ref.clone()],
    );
    let outbox_ref = add(
        &mut objects,
        to_cbor((&after.pending_effects, &after.inflight_effects))?,
        vec![effect_ref.clone()],
    );
    let rules_ref = add(
        &mut objects,
        to_cbor((&before.manifest, &after.manifest))?,
        vec![],
    );
    let mut wasm_refs = BTreeMap::new();
    for (hash, bytes) in before
        .module_artifact_bytes
        .iter()
        .chain(after.module_artifact_bytes.iter())
    {
        wasm_refs.insert(hash.clone(), add(&mut objects, bytes.clone(), vec![]));
    }
    // A typed module registry manifest is the WASM root, including all modules,
    // even when an execution has no installed modules. It is not an empty blob.
    let wasm_ref = add(
        &mut objects,
        to_cbor((&before.module_registry, &after.module_registry, &wasm_refs))?,
        wasm_refs.values().cloned().collect(),
    );
    let manifest = CaptureManifest {
        schema_version: CAPTURE_SCHEMA,
        scope: CAPTURE_SCOPE.into(),
        context: preparation.context.clone(),
        record: record.clone(),
        before_snapshot_ref,
        before_journal_ref,
        preparation_ref,
        previous_capture_ref: preceding
            .map(|(p, _)| to_cbor(p).map(|bytes| blake3_hex(&bytes)))
            .transpose()?,
        outcome_index_ref: index_ref.clone(),
        effect_ref,
        wasm_refs,
        rules_ref: rules_ref.clone(),
    };
    let manifest_ref = add(
        &mut objects,
        to_cbor(&manifest)?,
        vec![
            preparation_ref_of(&manifest),
            input_ref.clone(),
            snapshot_ref.clone(),
            journal_ref.clone(),
            result_ref.clone(),
            index_ref.clone(),
            outbox_ref.clone(),
            rules_ref.clone(),
            wasm_ref.clone(),
        ],
    );
    let package = ClosedRecord {
        payload_digest: input_ref.clone(),
        before_state_root: preceding
            .map(|(p, _)| p.after_state_root.clone())
            .unwrap_or_else(|| effect.pre_step_execution_state_root.clone()),
        after_state_root: record.execution_state_root.clone(),
        roots: BTreeMap::from([
            (ArtifactRole::Input, input_ref),
            (ArtifactRole::Result, result_ref),
            (ArtifactRole::Snapshot, snapshot_ref),
            (ArtifactRole::Journal, journal_ref),
            (ArtifactRole::NonceIndex, index_ref),
            (ArtifactRole::EffectOutbox, outbox_ref),
            (ArtifactRole::ExecutionManifest, manifest_ref),
            (ArtifactRole::Wasm, wasm_ref),
            (ArtifactRole::Rules, rules_ref),
        ]),
        objects: objects.into_values().collect(),
    };
    package.validate().map_err(|e| e.to_string())?;
    Ok(package)
}
fn preparation_ref_of(manifest: &CaptureManifest) -> String {
    manifest.preparation_ref.clone()
}
pub(super) fn object<'a>(package: &'a ClosedRecord, reference: &str) -> Result<&'a [u8], String> {
    package
        .objects
        .iter()
        .find(|o| o.content_hash == reference)
        .map(|o| o.bytes.as_slice())
        .ok_or_else(|| "capture object unavailable".into())
}
pub(super) fn decode_role<T: serde::de::DeserializeOwned>(
    package: &ClosedRecord,
    role: ArtifactRole,
) -> Result<T, String> {
    if role == ArtifactRole::Snapshot {
        return decode_generic(&super::formal_snapshot::bytes(package)?);
    }
    let reference = package.roots.get(&role).ok_or("capture role unavailable")?;
    decode_generic(object(package, reference)?)
}

/// Only the new capture publication path requires this stronger local CAS
/// boundary; it does not alter legacy CAS semantics or establish finality.
pub(super) fn persist_capture_bytes(store: &LocalCasStore, bytes: &[u8]) -> Result<String, String> {
    if !cfg!(unix) {
        return Err("durable history capture requires Unix directory synchronization".into());
    }
    super::durable_transaction::ensure_dir_all_durable(store.blobs_dir())?;
    let reference = store
        .put_bytes(bytes)
        .map_err(|e| format!("capture CAS write: {e:?}"))?;
    let actual = store
        .get(&reference)
        .map_err(|e| format!("capture CAS read: {e:?}"))?;
    if actual != bytes || blake3_hex(&actual) != reference {
        return Err("capture CAS content mismatch".into());
    }
    super::durable_transaction::sync_existing_file_durable(
        &store.blobs_dir().join(format!("{reference}.blob")),
    )?;
    Ok(reference)
}

pub(super) fn persist_preparation(
    store: &LocalCasStore,
    preparation: &CapturePreparation,
) -> Result<String, String> {
    persist_capture_bytes(store, &to_cbor(preparation)?)
}
pub(super) fn load_preparation(
    store: &LocalCasStore,
    reference: &str,
    context: &NodeExecutionCommitContext,
) -> Result<CapturePreparation, String> {
    let bytes = store
        .get(reference)
        .map_err(|e| format!("capture preparation load: {e:?}"))?;
    if bytes.len() > 8 * 1024 * 1024 || blake3_hex(&bytes) != reference {
        return Err("invalid preparation size or hash".into());
    }
    let preparation: CapturePreparation = decode_generic(&bytes)?;
    if &preparation.context != context
        || preparation.schema_version != CAPTURE_SCHEMA
        || preparation.scope != CAPTURE_SCOPE
    {
        return Err("original capture preparation context mismatch".into());
    }
    supported_origin(context)?;
    Ok(preparation)
}
pub(super) fn load_package(store: &LocalCasStore, reference: &str) -> Result<ClosedRecord, String> {
    let bytes = store
        .get(reference)
        .map_err(|e| format!("capture package load: {e:?}"))?;
    if bytes.len() > 16 * 1024 * 1024 || blake3_hex(&bytes) != reference {
        return Err("invalid capture size or hash".into());
    }
    validate_package_budget(&bytes)?;
    let package: ClosedRecord = serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
    package.validate().map_err(|e| e.to_string())?;
    Ok(package)
}

pub(super) fn validate_package_budget(bytes: &[u8]) -> Result<(), String> {
    // ClosedRecord contains at most 8MiB aggregate byte closure and 64
    // objects with bounded refs. Byte nodes plus 64KiB typed metadata remain
    // finite; ordinary Snapshot and Journal keep their original budgets.
    let limits = oasis7_runtime::ObserverReadLimits {
        max_file_bytes: 16 * 1024 * 1024,
        max_single_allocation_bytes: 8 * 1024 * 1024,
        max_elements: 8 * 1024 * 1024 + 64 * 1024,
        ..oasis7_runtime::ObserverReadLimits::default()
    };
    World::validate_offline_closed_record_cbor_budget(bytes, limits)
        .map_err(|e| format!("closed capture budget: {e:?}"))?;
    Ok(())
}

fn serialize_bytes<S: serde::Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(value)
}
fn deserialize_bytes<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Vec<u8>, D::Error> {
    struct Bytes;
    impl<'de> serde::de::Visitor<'de> for Bytes {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded CBOR byte buffer")
        }
        fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Vec<u8>, E> {
            if bytes.len() > 8 * 1024 * 1024 {
                Err(E::custom("capture byte capacity"))
            } else {
                Ok(bytes.to_vec())
            }
        }
        fn visit_borrowed_bytes<E: serde::de::Error>(self, bytes: &'de [u8]) -> Result<Vec<u8>, E> {
            self.visit_bytes(bytes)
        }
    }
    decoder.deserialize_bytes(Bytes)
}
pub(super) fn decode_snapshot(bytes: &[u8]) -> Result<Snapshot, String> {
    World::validate_offline_snapshot_cbor_budget(
        bytes,
        oasis7_runtime::ObserverReadLimits::default(),
    )
    .map_err(|e| format!("snapshot budget: {e:?}"))?;
    serde_cbor::from_slice(bytes).map_err(|e| e.to_string())
}
pub(super) fn decode_generic<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    World::validate_offline_cbor_budget(bytes, oasis7_runtime::ObserverReadLimits::default())
        .map_err(|e| format!("typed capture budget: {e:?}"))?;
    serde_cbor::from_slice(bytes).map_err(|e| e.to_string())
}

pub(super) fn materialize_preparation(
    package: &ClosedRecord,
    manifest: &CaptureManifest,
) -> Result<CapturePreparation, String> {
    let binding: PreparationBinding = decode_generic(object(package, &manifest.preparation_ref)?)?;
    if binding.schema_version != CAPTURE_SCHEMA
        || binding.scope != CAPTURE_SCOPE
        || binding.context != manifest.context
        || binding.before_snapshot_ref != manifest.before_snapshot_ref
        || binding.before_journal_ref != manifest.before_journal_ref
    {
        return Err("typed preparation binding mismatch".into());
    }
    Ok(CapturePreparation {
        schema_version: binding.schema_version,
        scope: binding.scope,
        context: binding.context,
        release_security_policy: binding.release_security_policy,
        before_snapshot: object(package, &binding.before_snapshot_ref)?.to_vec(),
        before_journal: object(package, &binding.before_journal_ref)?.to_vec(),
    })
}
