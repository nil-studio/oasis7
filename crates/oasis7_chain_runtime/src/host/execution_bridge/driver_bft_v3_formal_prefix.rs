//! Durable private header-prefix import and exact historical-cut recovery.
//! Publication requires a second, strict H-to-tip World replay of durable bytes.
use super::super::formal_ledger::{OriginContract, RecordContract};
use super::*;
use oasis7_formal_protocol::bft::{
    FormalBftCertifiedHeaderPrefixV3, VerifiedFormalBftHeaderPrefixV3,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MAX_H_BYTES: usize = 64 * 1024 * 1024;
const ORIGIN_PIN_SCOPE: &str = "formal-bft-header-prefix-origin-v1";

#[derive(Clone)]
struct Inputs {
    anchor: VerifiedFormalReadAnchorV1,
    target: BftProfileTargetV2,
    material: ClosedRecord,
    source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
}
pub(crate) struct FormalBftHeaderLedgerExecutionV3 {
    origin: FormalBftHeaderExecutionV3,
    ledger: storage::Ledger,
    store: LocalCasStore,
    inputs: Inputs,
}

/// Private constructor; a stored Snapshot or protocol-only prefix cannot mint
/// this capability. It grants neither current-read nor validator authority.
pub(crate) struct FormalBftHeaderPrefixCutV3 {
    prefix: FormalBftCertifiedHeaderPrefixV3,
    proof: VerifiedFormalBftHeaderPrefixV3,
    world: RuntimeWorld,
    result_tree: FormalBusinessResultTreeV1,
    record_digest: String,
}
impl FormalBftHeaderPrefixCutV3 {
    pub(crate) fn certified_prefix(&self) -> &FormalBftCertifiedHeaderPrefixV3 {
        &self.prefix
    }
    pub(crate) fn proof(&self) -> &VerifiedFormalBftHeaderPrefixV3 {
        &self.proof
    }
    pub(crate) fn world(&self) -> &RuntimeWorld {
        &self.world
    }
    pub(crate) fn result_tree(&self) -> &FormalBusinessResultTreeV1 {
        &self.result_tree
    }
    pub(crate) fn durable_record_digest(&self) -> &str {
        &self.record_digest
    }
}

impl FormalBftHeaderLedgerExecutionV3 {
    pub(crate) fn open_from_handoff(
        records: &Path,
        store: LocalCasStore,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        material: ClosedRecord,
        source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        initialize: bool,
    ) -> Result<Self, String> {
        validate_storage_paths(records, store.root())?;
        let handoff = verify_formal_bft_handoff_v2(&anchor, target.clone())?;
        validate_handoff_material_for_scope(&anchor, &target.scope, &material)?;
        let encoded_h = encoded_h_material(&material)?;
        let snapshot_h = material
            .snapshot_bytes()
            .map_err(|_| "wire 3 H Snapshot unavailable")?;
        let journal = material
            .roots
            .get(&ArtifactRole::Journal)
            .and_then(|reference| {
                material
                    .objects
                    .iter()
                    .find(|object| &object.content_hash == reference)
            })
            .ok_or("wire 3 H Journal unavailable")?;
        let h_bytes = encoded_h
            .len()
            .checked_add(snapshot_h.len())
            .and_then(|n| n.checked_add(journal.bytes.len()))
            .ok_or("wire 3 H material capacity overflow")?;
        let mut ledger = storage::Ledger::open(records, initialize, h_bytes)?;
        let seed = ledger.preload(&store, None)?;
        let origin = FormalBftHeaderExecutionV3::from_handoff(
            anchor.clone(),
            target.clone(),
            &material,
            source.clone(),
            seed,
            !initialize,
        )?;
        let pin = material_pin_digest(&origin, &material)?;
        ledger.bind_retained(&pin, handoff.activation_height()?, |origin, pending| {
            retain_origin_material(&store, &material, origin, pending)
        })?;
        let owner = Self {
            origin,
            ledger,
            store,
            inputs: Inputs {
                anchor,
                target,
                material,
                source,
            },
        };
        owner.ensure_origin_material()?;
        Ok(owner)
    }

    pub(crate) fn install_prefix(
        mut self,
        input: &PrivateHeaderPrefixV3,
        exact_height: u64,
        exact_header_id: &str,
    ) -> Result<FormalBftHeaderPrefixCutV3, String> {
        let public = input.public_prefix()?;
        let proof = public.verify(&self.origin.handoff)?;
        check_target(&proof, exact_height, exact_header_id)?;
        for (index, stored) in self.ledger.records().enumerate() {
            if input.entries().get(index) != Some(&stored.input()) {
                return Err(
                    "wire 3 prefix conflicts with original private history or pending QC".into(),
                );
            }
        }
        self.ledger.charge_proof(public.canonical_bytes()?.len())?;
        self.replay(input, true)?;
        // Throw away every candidate/cache and reread the exact durable bytes.
        self.verified_cut(exact_height, exact_header_id)
    }

    pub(crate) fn verified_cut(
        &mut self,
        exact_height: u64,
        exact_header_id: &str,
    ) -> Result<FormalBftHeaderPrefixCutV3, String> {
        self.ledger.reload()?;
        self.ensure_origin_material()?;
        let input = PrivateHeaderPrefixV3::from_entries(
            self.ledger
                .records_through(exact_height)?
                .map(storage::Record::input)
                .collect(),
        )?;
        let prefix = input.public_prefix()?;
        let proof = prefix.verify(&self.origin.handoff)?;
        check_target(&proof, exact_height, exact_header_id)?;
        let seed = self.ledger.preload(&self.store, Some(exact_height))?;
        self.origin = FormalBftHeaderExecutionV3::from_handoff(
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
            &self.inputs.material,
            self.inputs.source.clone(),
            seed,
            true,
        )?;
        if material_pin_digest(&self.origin, &self.inputs.material)?
            != self.ledger.material_pin_digest()?
        {
            return Err("wire 3 durable reread changed the independently restored H".into());
        }
        self.ledger.charge_proof(prefix.canonical_bytes()?.len())?;
        let record_digest = self.replay(&input, false)?;
        Ok(FormalBftHeaderPrefixCutV3 {
            prefix,
            proof,
            world: self.origin.parent_world.clone(),
            result_tree: self.origin.parent_result_tree.clone(),
            record_digest,
        })
    }

    fn replay(&mut self, input: &PrivateHeaderPrefixV3, append: bool) -> Result<String, String> {
        let mut previous = self.ledger.origin_digest()?;
        for entry in input.entries() {
            let executed = self
                .origin
                .replay_next(&entry.certificate, &entry.actions_cbor)?;
            let nodes = self.origin.retained_nodes.accessed_nodes()?;
            let references: Vec<_> = nodes.keys().cloned().collect();
            // New format has no legacy overcapture: every retained dependency
            // must be observed by this exact replay, including cold H reads.
            if let Some(stored) = self
                .ledger
                .record_at(entry.certificate.execution_commitment.height)
                && stored.business_index_refs() != references
            {
                return Err("wire 3 replay dependency set differs from original cut".into());
            }
            let record = storage::Record::from_actual(
                self.ledger.origin()?,
                &previous,
                entry,
                &executed,
                references,
            )?;
            previous = self.ledger.accept_prepared(
                record,
                &executed.snapshot_bytes,
                &executed.journal_bytes,
                &nodes,
                &self.store,
                append,
            )?;
        }
        Ok(previous)
    }

    fn ensure_origin_material(&self) -> Result<(), String> {
        self.ledger.verify_carriers()?;
        retain_origin_material(
            &self.store,
            &self.inputs.material,
            self.ledger.origin()?,
            false,
        )?;
        self.ledger.verify_carriers()
    }

    #[cfg(test)]
    pub(crate) fn inject_publication_failure(
        &mut self,
        height: u64,
        phase: &str,
        kill: bool,
    ) -> Result<(), String> {
        use super::super::formal_ledger::PublicationFault;
        let phase = match phase {
            "pins" => PublicationFault::PinsPublished,
            "intent" => PublicationFault::IntentPublished,
            "link" => PublicationFault::RecordLinked,
            _ => return Err("unknown wire 3 test publication phase".into()),
        };
        if kill {
            self.ledger.inject_process_kill(height, phase);
        } else {
            self.ledger.inject_publication_failure(height, phase);
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct MaterialPin<'a> {
    domain: &'static str,
    schema_version: u32,
    target: &'a BftProfileTargetV2,
    target_digest: &'a str,
    activation_digest: &'a str,
    handoff_head: &'a oasis7_formal_protocol::controlled_authority::formal::FormalHeadRefV1,
    raw_snapshot_root: &'a str,
    governing_manifest_digest: &'a str,
    parent_semantic_state_root: String,
    parent_manifest_hash: String,
    parent_journal_root: String,
    parent_result_root: String,
    parent_projection_root: String,
    closed_material_digest: String,
    handoff_material_ref: String,
}
fn material_pin_digest(
    origin: &FormalBftHeaderExecutionV3,
    material: &ClosedRecord,
) -> Result<String, String> {
    let world = &origin.parent_world;
    let pin =
        MaterialPin {
            domain: "oasis7.formal.bft.header_material_pin.v1",
            schema_version: 1,
            target: origin.handoff.target(),
            target_digest: origin.handoff.target_digest(),
            activation_digest: origin.handoff.activation_digest(),
            handoff_head: origin.handoff.handoff_head(),
            raw_snapshot_root: origin.handoff.raw_snapshot_root(),
            governing_manifest_digest: origin.handoff.governing_manifest_digest(),
            parent_semantic_state_root: world
                .execution_state_identity()
                .map_err(|_| "read wire 3 H semantic root")?
                .state_root_hash,
            parent_manifest_hash: world
                .current_manifest_hash()
                .map_err(|_| "read wire 3 H manifest")?,
            parent_journal_root: blake3_hex(
                &oasis7_world_state::codec::journal_cbor(world.journal())
                    .map_err(|_| "encode wire 3 H Journal")?,
            ),
            parent_result_root: origin.parent_result_tree.root()?,
            parent_projection_root:
                oasis7::world_service::formal_projection::commitments_for_world(world)?.root()?,
            closed_material_digest: super::super::closed_material_digest(
                &origin.parent_record,
                material,
            )?,
            handoff_material_ref: blake3_hex(&encoded_h_material(material)?),
        };
    Ok(blake3_hex(
        &serde_cbor::to_vec(&pin).map_err(|_| "encode wire 3 material pin")?,
    ))
}
fn encoded_h_material(material: &ClosedRecord) -> Result<Vec<u8>, String> {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(material)
        .map_err(|_| "encode private wire 3 H material")?;
    if bytes.len() > MAX_H_BYTES {
        return Err("wire 3 H material byte capacity".into());
    }
    Ok(bytes)
}
fn check_target(
    proof: &VerifiedFormalBftHeaderPrefixV3,
    height: u64,
    header_id: &str,
) -> Result<(), String> {
    if proof.commitment().height != height || proof.certificate().block_id() != header_id {
        return Err("wire 3 prefix differs from its independently pinned exact target".into());
    }
    Ok(())
}
fn canonical_storage_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("wire 3 storage paths must be absolute".into());
    }
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("wire 3 storage symlink path refused".into());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("inspect wire 3 storage path".into()),
        }
    }
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(path
            .parent()
            .ok_or("wire 3 storage parent missing")?
            .canonicalize()
            .map_err(|_| "wire 3 storage parent is unavailable")?
            .join(path.file_name().ok_or("wire 3 storage filename missing")?)),
        Err(_) => Err("canonicalize wire 3 storage path".into()),
    }
}
fn validate_storage_paths(records: &Path, cas: &Path) -> Result<(), String> {
    let records = canonical_storage_path(records)?;
    let cas = canonical_storage_path(cas)?;
    crate::controlled_live_config::require_private_owned(&cas, true)?;
    if records.starts_with(&cas) || cas.starts_with(&records) {
        return Err("wire 3 private CAS and ledger must be separate".into());
    }
    Ok(())
}

fn retain_origin_material(
    store: &LocalCasStore,
    material: &ClosedRecord,
    origin: &storage::Origin,
    pending: bool,
) -> Result<(), String> {
    use super::super::super::super::storage as common_storage;
    let bytes = encoded_h_material(material)?;
    let reference = blake3_hex(&bytes);
    let expected = BTreeSet::from([reference.clone()]);
    let shard = format!("origin-{}", origin.digest()?);
    let shards = store
        .list_pin_scope_shards(ORIGIN_PIN_SCOPE)
        .map_err(|_| "read wire 3 retained H receipt")?;
    if shards.contains(&shard) {
        store
            .verify_pin_scope_shard(ORIGIN_PIN_SCOPE, &shard, &expected)
            .map_err(|_| "wire 3 original H retention receipt changed")?;
        if common_storage::read_cas_bounded(store, &reference, MAX_H_BYTES)? != bytes {
            return Err("wire 3 original H material differs from its retained blob".into());
        }
    } else {
        if !pending {
            return Err("wire 3 original H retention receipt is missing".into());
        }
        common_storage::put_cas_durable(store, &bytes, MAX_H_BYTES)?;
        store
            .replace_pin_scope_shard(ORIGIN_PIN_SCOPE, &shard, &expected)
            .map_err(|_| "publish wire 3 original H retention receipt")?;
        store
            .verify_pin_scope_shard(ORIGIN_PIN_SCOPE, &shard, &expected)
            .map_err(|_| "verify wire 3 original H retention receipt")?;
        if common_storage::read_cas_bounded(store, &reference, MAX_H_BYTES)? != bytes {
            return Err("wire 3 retained H reread differs".into());
        }
    }
    Ok(())
}
