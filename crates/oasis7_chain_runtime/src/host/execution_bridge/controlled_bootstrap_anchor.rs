//! Read-only, certificate-bound bootstrap validation. No startup or readiness consumer.

use super::execution_hash::ExecutionHashPayload;
use super::{ExecutionBridgeRecord, controlled_capture as capture, to_cbor};
use oasis7_distfs::controlled_authority::{
    activation::VerifiedInitialActivation,
    replicated_protocol::{ArtifactObject, ArtifactRole, ClosedRecord},
};
use oasis7_runtime::{
    ChainResourceDerivationContext, Journal, ReleaseSecurityPolicy, Snapshot, blake3_hex,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const SCOPE: &str = "certificate_bound_no_state_change_bootstrap_v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivationArtifactManifestV1 {
    pub schema_version: u32,
    pub scope: String,
    pub world_id: String,
    pub chain_id: String,
    pub genesis_digest: String,
    /// Chain execution checkpoint height, NOT the simulation tick.
    pub execution_height: u64,
    pub snapshot_tick: u64,
    pub previous_execution_block_hash: String,
    /// Seven roles: excludes Input and this ExecutionManifest itself.
    pub artifact_roots: BTreeMap<ArtifactRole, String>,
}
/// Validated checkpoint identity comes from the independently authorized issuer
/// certificate. Diagnostic resource annotations are consistency checks only.
#[derive(Debug, Clone)]
pub(crate) struct VerifiedBootstrapAnchor {
    activation: VerifiedInitialActivation,
    snapshot: Snapshot,
    journal: Journal,
    execution_height: u64,
    simulation_tick: u64,
}
impl VerifiedBootstrapAnchor {
    pub(crate) fn activation(&self) -> &VerifiedInitialActivation {
        &self.activation
    }
    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub(crate) fn journal(&self) -> &Journal {
        &self.journal
    }
    pub(crate) fn execution_height(&self) -> u64 {
        self.execution_height
    }
    pub(crate) fn simulation_tick(&self) -> u64 {
        self.simulation_tick
    }
}
fn object(bytes: Vec<u8>, mut references: Vec<String>) -> ArtifactObject {
    references.sort();
    references.dedup();
    ArtifactObject {
        content_hash: blake3_hex(&bytes),
        bytes,
        references,
    }
}
fn insert(
    objects: &mut BTreeMap<String, ArtifactObject>,
    bytes: Vec<u8>,
    references: Vec<String>,
) -> String {
    let o = object(bytes, references);
    let hash = o.content_hash.clone();
    objects.insert(hash.clone(), o);
    hash
}

pub(crate) fn verify_bootstrap_anchor(
    activation: &VerifiedInitialActivation,
    record: &ClosedRecord,
    release_policy: &ReleaseSecurityPolicy,
) -> Result<VerifiedBootstrapAnchor, String> {
    record.validate().map_err(|e| e.to_string())?;
    capture::validate_package_budget(&to_cbor(record)?)?;
    let b = activation.body();
    let expected_roots = b
        .artifact_roots
        .iter()
        .map(|(r, h)| (*r, h.clone()))
        .chain([(
            ArtifactRole::Input,
            activation.envelope_digest().to_string(),
        )])
        .collect::<BTreeMap<_, _>>();
    if record.roots != expected_roots
        || record.payload_digest != activation.envelope_digest()
        || record.before_state_root != b.before_state_root
        || record.after_state_root != b.after_state_root
        || record.before_state_root != record.after_state_root
        || record.roots.values().collect::<BTreeSet<_>>().len() != 9
    {
        return Err("bootstrap token/original record binding or no-state-change mismatch".into());
    }
    let manifest: ActivationArtifactManifestV1 =
        capture::decode_role(record, ArtifactRole::ExecutionManifest)?;
    let manifest_roots = record
        .roots
        .iter()
        .filter(|(r, _)| !matches!(r, ArtifactRole::Input | ArtifactRole::ExecutionManifest))
        .map(|(r, h)| (*r, h.clone()))
        .collect::<BTreeMap<_, _>>();
    if manifest.schema_version != 1
        || manifest.scope != SCOPE
        || manifest.world_id != b.trust.world_id
        || manifest.chain_id != b.trust.chain_id
        || manifest.genesis_digest != b.trust.genesis_digest
        || manifest.execution_height != b.activation_height
        || manifest.artifact_roots != manifest_roots
    {
        return Err("bootstrap manifest certificate identity/roles mismatch".into());
    }
    let snapshot_bytes = capture::object(record, &record.roots[&ArtifactRole::Snapshot])?;
    let journal_bytes = capture::object(record, &record.roots[&ArtifactRole::Journal])?;
    let snapshot = capture::decode_snapshot(snapshot_bytes)?;
    let journal: Journal = capture::decode_generic(journal_bytes)?;
    let checkpoint: ExecutionBridgeRecord = capture::decode_role(record, ArtifactRole::Result)?;
    if blake3_hex(snapshot_bytes) != record.after_state_root
        || snapshot.state.time != manifest.snapshot_tick
        || snapshot.journal_len != journal.len()
        || snapshot.journal_commitment.is_empty()
        || checkpoint.schema_version != 3
        || checkpoint.world_id != b.trust.world_id
        || checkpoint.height != manifest.execution_height
        || checkpoint.execution_state_root != record.after_state_root
        || checkpoint.snapshot_ref.as_deref() != Some(record.after_state_root.as_str())
        || checkpoint.latest_state_ref != checkpoint.snapshot_ref
        || checkpoint.journal_ref.as_deref() != Some(record.roots[&ArtifactRole::Journal].as_str())
        || checkpoint.journal_len != journal.len()
        || checkpoint.execution_block_hash.is_empty()
        || checkpoint
            .node_block_hash
            .as_deref()
            .is_none_or(str::is_empty)
        || checkpoint.action_root.as_deref().is_none_or(str::is_empty)
        || checkpoint.proposer_id.as_deref().is_none_or(str::is_empty)
        || checkpoint.simulator_mirror.is_some()
        || checkpoint.controlled_capture_ref.is_some()
        || checkpoint.commit_log_ref.is_some()
        || checkpoint.checkpoint_ref.is_some()
        || checkpoint.external_effect_ref.is_some()
        || checkpoint.world_head_proof_ref.is_some()
        || checkpoint.world_head_proof_hash.is_some()
    {
        return Err("bootstrap snapshot/journal/checkpoint binding mismatch".into());
    }
    if manifest.previous_execution_block_hash.is_empty()
        || checkpoint.execution_block_hash
            != blake3_hex(&to_cbor(ExecutionHashPayload {
                world_id: &manifest.world_id,
                height: manifest.execution_height,
                prev_execution_block_hash: &manifest.previous_execution_block_hash,
                execution_state_root: &record.after_state_root,
                journal_len: journal.len(),
            })?)
    {
        return Err("bootstrap checkpoint execution block hash mismatch".into());
    }
    let identity = &snapshot.chain_resource_manifest;
    if identity.world_id != b.trust.world_id
        || identity.chain_id != b.trust.chain_id
        || identity.genesis_ref.as_deref() != Some(b.trust.genesis_digest.as_str())
    {
        return Err(
            "bootstrap diagnostic identity is absent or inconsistent with certificate".into(),
        );
    }
    // Restore validates actual journal prefix and artifact SHA256; it has no path,
    // dispatcher, module executor, signer or configured persistence destination.
    let world = oasis7::runtime_host::world::from_snapshot(snapshot.clone(), journal.clone())
        .map_err(|e| format!("bootstrap world restore: {e:?}"))?;
    if to_cbor(world.snapshot())? != snapshot_bytes || to_cbor(world.journal())? != journal_bytes {
        return Err("bootstrap restored bytes differ from original checkpoint".into());
    }
    if identity.manifest_height != checkpoint.height
        || identity.created_at_height > checkpoint.height
    {
        return Err("bootstrap resource annotation height mismatch".into());
    }
    let regenerated = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: &identity.world_id,
            chain_id: &identity.chain_id,
            genesis_ref: identity.genesis_ref.as_deref(),
            created_at_height: identity.created_at_height,
            manifest_height: identity.manifest_height,
            commit_block_hash: identity.created_at_block_hash.as_deref(),
            tick: snapshot.state.time,
        },
        identity.world_config_hash.clone(),
        identity.generation_algorithm_hash.clone(),
    );
    if regenerated.chain_resource_manifest != snapshot.chain_resource_manifest
        || regenerated.latest_chain_resource_delta != snapshot.latest_chain_resource_delta
    {
        return Err(
            "bootstrap diagnostic resource pair is inconsistent with restored state".into(),
        );
    }
    let declared_modules = snapshot
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
        != declared_modules
        || snapshot.module_artifacts != declared_modules
    {
        return Err("bootstrap missing or extra module bytes".into());
    }
    let mut objects = BTreeMap::new();
    let input_bytes = capture::object(record, activation.envelope_digest())?.to_vec();
    insert(&mut objects, input_bytes, vec![]);
    insert(&mut objects, snapshot_bytes.to_vec(), vec![]);
    insert(&mut objects, journal_bytes.to_vec(), vec![]);
    insert(&mut objects, to_cbor(&checkpoint)?, vec![]);
    // Version 1 intentionally covers this existing capability nonce map only;
    // it does not assert universal player/request nonce recovery.
    let nonce = insert(
        &mut objects,
        to_cbor(&snapshot.capability_nonce_records)?,
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
        to_cbor((
            &snapshot.module_registry,
            &snapshot.module_registry,
            &wasm_refs,
        ))?,
        wasm_refs.values().cloned().collect(),
    );
    let manifest_hash = insert(
        &mut objects,
        to_cbor(&manifest)?,
        manifest.artifact_roots.values().cloned().collect(),
    );
    if nonce != record.roots[&ArtifactRole::NonceIndex]
        || outbox != record.roots[&ArtifactRole::EffectOutbox]
        || rules != record.roots[&ArtifactRole::Rules]
        || wasm != record.roots[&ArtifactRole::Wasm]
        || manifest_hash != record.roots[&ArtifactRole::ExecutionManifest]
        || objects.into_values().collect::<Vec<_>>() != record.objects
    {
        return Err("bootstrap concrete typed artifact closure mismatch".into());
    }
    Ok(VerifiedBootstrapAnchor {
        activation: activation.clone(),
        execution_height: checkpoint.height,
        simulation_tick: snapshot.state.time,
        snapshot,
        journal,
    })
}

#[cfg(test)]
#[path = "tests/controlled_bootstrap_anchor.rs"]
pub(crate) mod tests;
