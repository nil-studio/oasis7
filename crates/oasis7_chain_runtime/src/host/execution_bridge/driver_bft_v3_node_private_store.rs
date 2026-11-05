//! Original private Node candidates, retained before any remote signing request.
//! The canonical ledger's OS claim protects this subordinate directory.
use super::*;
#[path = "driver_bft_v3_node_private_publication.rs"]
mod publication;
#[path = "driver_bft_v3_source_private_activation.rs"]
mod source_activation;
#[path = "driver_bft_v3_source_archive_activation.rs"]
mod source_archive_activation;
pub(in super::super) use source_archive_activation::{
    VerifiedSourceArchiveActivationV1, verify_source_archive_activation,
};
#[path = "driver_bft_v3_source_private_input.rs"]
pub(super) mod source_input;
pub(super) use self::source_activation::OriginalSourcePublicationV1;
use self::source_activation::SourceActivationBindingV1;
pub(super) use source_input::{
    ReservedSourceInputV1, SourceSlotKindV1, active_admission_slot_kind, source_admission_slot_kind,
};
pub(in super::super) use source_input::{
    VerifiedActiveAdmissionInputV1, active_admission_candidate, read_source_origin,
    source_admission_archive_reservation_bytes, source_admission_candidate,
    source_admission_historical_reservation_bytes, source_admission_reservation_bytes,
    source_selection_identity, validate_source_admission_input,
    validate_source_admission_slot_kind, verify_active_admission_original,
    verify_active_admission_ready, verify_active_selection_current, verify_source_admission_ready,
    verify_source_input_context_role, verify_source_input_historical_role,
    verify_source_seal_context_role, verify_source_selection_current,
};
#[path = "driver_bft_v3_node_private_publication_archive.rs"]
mod publication_archive;
pub(in super::super) use publication_archive::{
    CarryCopyDescriptorV1, PublicationArchiveMaterialV1, PublicationArchiveOwnerV1,
    VerifiedPublicationOriginalV1, carry_origin_usage, dependency_identity,
};

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    super::super::super::super::super::super::super::super::controlled_live::private_directory(path)
}

const MAX_RECORD: usize = 4 * 1024 * 1024;
const SCOPE: &str = "formal-bft-header-node-private-v3";
const MARKER: &[u8] = b"oasis7.formal.bft.node_private.required/v3\n";
const COVERED_SOURCE_ORIGIN_DOMAIN: &str = "oasis7.formal.bft.node_private_covered_origin.v1";
const PROTECTED_INPUT_DOMAIN: &str = "oasis7.formal.bft.node_private_protected_input.v1";
const PROTECTED_RECEIVED_INPUT_DOMAIN: &str =
    "oasis7.formal.bft.node_private_protected_received_input.v1";
const ACTIVE_LOCAL_INPUT_DOMAIN: &str = "oasis7.formal.bft.node_private_active_input.v11";
const ACTIVE_RECEIVED_INPUT_DOMAIN: &str =
    "oasis7.formal.bft.node_private_active_received_input.v12";
const ACTIVE_SELECTION_DOMAIN: &str = "oasis7.formal.bft.node_private_active_selection.v13";
const DOMAIN: &str = "oasis7.formal.bft.node_private.v3";
const MIGRATED_DOMAIN: &str = "oasis7.formal.bft.node_private_migrated.v1";
const MULTI_DOMAIN: &str = "oasis7.formal.bft.node_private_candidate.v4";
const SELECTION_DOMAIN: &str = "oasis7.formal.bft.node_private_selection.v5";
const MAX_CANDIDATES_PER_HEIGHT: usize = 16;
const MAX_CANDIDATES: usize = 256;
#[path = "driver_bft_v3_node_private_multivalue.rs"]
mod multivalue;
use super::super::super::publication_reservation::{
    PublicationCandidateIdentityV1, PublicationReservationV1,
};

#[path = "driver_bft_v3_node_private_data.rs"]
mod data_codec;
use data_codec::Data;

#[derive(Clone)]
struct Slot {
    data: Data,
    complete: bool,
}

pub(super) struct NodePrivateStore {
    instance_id: String,
    auxiliary_root: Option<PathBuf>,
    dir: PathBuf,
    store: LocalCasStore,
    origin: Option<Slot>,
    candidates: BTreeMap<u64, Slot>,
    values: BTreeMap<(u64, String), Slot>,
    selections: BTreeMap<u64, Slot>,
    publication_ancestors: BTreeMap<String, VerifiedPublicationOriginalV1>,
}

struct SeedReader {
    nodes: BTreeMap<String, Vec<u8>>,
    pending_references: Option<BTreeSet<String>>,
    fallback: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
}
impl BusinessIndexNodeReader for SeedReader {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        if let Some(bytes) = self.nodes.get(digest) {
            return Ok(Some(bytes.clone()));
        }
        if self
            .pending_references
            .as_ref()
            .is_none_or(|refs| refs.contains(digest))
        {
            return self.fallback.read_node(digest);
        }
        Ok(None)
    }
}

impl NodePrivateStore {
    /// A completed H record supplies only its original verified CAS nodes.
    /// Fallback is allowed solely before original Node initialization completes.
    pub(super) fn open(
        records: &Path,
        store: LocalCasStore,
        fallback: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<(Self, Arc<dyn BusinessIndexNodeReader + Send + Sync>), String> {
        let dir = records
            .join(storage::DIRECTORY)
            .join(storage::NODE_PRIVATE_DIRECTORY);
        let (mut owner, reader) = Self::open_directory(dir, store, fallback, None)?;
        owner.auxiliary_root = Some(records.into());
        Ok((owner, reader))
    }
    pub(super) fn open_window(
        dir: PathBuf,
        store: LocalCasStore,
        fallback: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        original_instance: &str,
    ) -> Result<(Self, Arc<dyn BusinessIndexNodeReader + Send + Sync>), String> {
        if !canonical_hash(original_instance) {
            return Err("window original private instance encoding".into());
        }
        Self::open_directory(dir, store, fallback, Some(original_instance))
    }
    fn open_directory(
        dir: PathBuf,
        store: LocalCasStore,
        fallback: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        original_instance: Option<&str>,
    ) -> Result<(Self, Arc<dyn BusinessIndexNodeReader + Send + Sync>), String> {
        if fs::symlink_metadata(&dir).is_ok() {
            crate::controlled_live_config::require_private_owned(&dir, true)?;
        }
        let origin = read_slot(&dir.join("origin.cbor"))?;
        let instance_id = match &origin {
            Some(slot) => slot.data.instance_id.clone(),
            None if original_instance.is_some() => original_instance
                .ok_or("window instance unavailable")?
                .to_owned(),
            None => {
                let mut nonce = [0u8; 32];
                getrandom::fill(&mut nonce)
                    .map_err(|_| "generate private Node instance identity")?;
                hex::encode(nonce)
            }
        };
        if original_instance.is_some_and(|expected| expected != instance_id) {
            return Err("window private instance differs from original Node".into());
        }
        let mut nodes = BTreeMap::new();
        let pending_references = match &origin {
            Some(slot) => {
                if let Some(activation) = &slot.data.source_activation {
                    activation.verify_directory(&dir)?;
                }
                if !slot.data.origin {
                    return Err("Node origin has candidate schema".into());
                }
                let marker = common_storage::read_optional_bytes(&dir.join("ready"), MARKER.len())?;
                if marker.is_some()
                    && common_storage::read_optional_bytes(&dir.join("origin.cbor"), MAX_RECORD)?
                        .is_none()
                {
                    return Err("Node ready marker lost its original origin record".into());
                }
                if slot.complete && marker.as_deref() != Some(MARKER) {
                    return Err("completed Node origin lost its original marker".into());
                }
                let retention_required = retention_required(&dir.join("origin.cbor"))?;
                if retention_required {
                    verify_material(&store, &slot.data)?;
                }
                let mut retained_bytes = 0usize;
                for reference in &slot.data.references {
                    match common_storage::read_cas_bounded(
                        &store,
                        reference,
                        formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                    ) {
                        Ok(bytes) => {
                            retained_bytes = retained_bytes
                                .checked_add(bytes.len())
                                .ok_or("Node H dependency byte capacity")?;
                            if retained_bytes > formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES
                            {
                                return Err("Node H dependency byte capacity".into());
                            }
                            nodes.insert(reference.clone(), bytes);
                        }
                        Err(_) if !retention_required => {}
                        Err(_) => return Err("original Node H dependency is missing".into()),
                    }
                }
                if slot.complete {
                    Some(BTreeSet::new())
                } else {
                    Some(slot.data.references.iter().cloned().collect())
                }
            }
            None => {
                match fs::read_dir(&dir) {
                    Ok(mut entries) => {
                        if entries.next().is_some() {
                            return Err("Node private files lost their original origin".into());
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err("inspect original Node private directory".into()),
                }
                None
            }
        };
        let source = Arc::new(SeedReader {
            nodes,
            pending_references,
            fallback,
        });
        Ok((
            Self {
                instance_id,
                auxiliary_root: None,
                dir,
                store,
                origin,
                candidates: BTreeMap::new(),
                values: BTreeMap::new(),
                selections: BTreeMap::new(),
                publication_ancestors: BTreeMap::new(),
            },
            source,
        ))
    }

    pub(super) fn require_completed_origin(&self) -> Result<(), String> {
        let slot = self
            .origin
            .as_ref()
            .ok_or("original private identity absent")?;
        if !slot.complete || !slot.data.origin {
            return Err("original private identity WAL unresolved".into());
        }
        verify_original_slot(&self.store, &self.dir.join("origin.cbor"), slot)
    }

    pub(super) fn verify_original_binding(
        &self,
        ledger: &str,
        material: &str,
        height: u64,
    ) -> Result<(), String> {
        self.require_completed_origin()?;
        let origin = &self
            .origin
            .as_ref()
            .ok_or("original private identity absent")?
            .data;
        if origin.ledger_origin_digest != ledger
            || origin.material_identity != material
            || origin.height != height
        {
            return Err(
                "read-only original private identity differs from independently verified H".into(),
            );
        }
        Ok(())
    }

    pub(super) fn require_completed_inventory(&self) -> Result<(), String> {
        self.require_completed_origin()?;
        if self
            .candidates
            .values()
            .chain(self.values.values())
            .chain(self.selections.values())
            .any(|s| !s.complete)
        {
            return Err("active private candidate WAL remains unresolved before Adopt".into());
        }
        Ok(())
    }
    /// Current backend context may authenticate later Active WAL originals;
    /// it cannot admit unrelated files beside the immutable C0 carry set.
    pub(super) fn verify_current_active_inventory(
        &self,
        base: &super::super::successor_verifier::VerifiedPrivateSuccessorBaseV1,
        namespace: &str,
        current: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
    ) -> Result<(), String> {
        if base.original_retention_attestor()? != current.attestor() {
            return Err("Active WAL current capability differs from original C0".into());
        }
        #[cfg(test)]
        let mut active_originals = Vec::new();
        let mut carry_seen = BTreeSet::new();
        for (kind, slot) in self
            .candidates
            .values()
            .map(|slot| (SourceSlotKindV1::Local, slot))
            .chain(
                self.values
                    .values()
                    .map(|slot| (SourceSlotKindV1::Received, slot)),
            )
        {
            let active = self.verify_current_active_slot(
                base,
                namespace,
                current,
                slot,
                kind,
                &mut carry_seen,
            )?;
            #[cfg(test)]
            if active {
                active_originals.push(slot);
            }
            let _ = active;
        }
        if carry_seen.len() != base.pending.len() {
            return Err("active preparation lacks an exact unique carry slot".into());
        }
        #[cfg(test)]
        if !active_originals.is_empty() {
            current_inventory_negative_controls::verify(
                self,
                base,
                namespace,
                current,
                &active_originals,
            );
        }
        Ok(())
    }

    fn verify_current_active_slot(
        &self,
        base: &super::super::successor_verifier::VerifiedPrivateSuccessorBaseV1,
        namespace: &str,
        current: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        slot: &Slot,
        kind: SourceSlotKindV1,
        carry_seen: &mut BTreeSet<usize>,
    ) -> Result<bool, String> {
        if SourceSlotKindV1::from_data(&slot.data)? != kind
            || slot.data.ledger_origin_digest != namespace
        {
            return Err("active preparation contains unproved private candidates".into());
        }
        for (index, (roles, package)) in base.pending.iter().enumerate() {
            let expected =
                if roles.contains(&super::super::successor_archive::PendingRoleV1::RetainedLocal) {
                    SourceSlotKindV1::Local
                } else {
                    SourceSlotKindV1::Received
                };
            if kind == expected
                && slot.data.height == package.candidate.public_value.execution_commitment.height
                && slot.data.value.as_ref() == Some(&package.candidate.public_value)
                && slot.data.actions_cbor == package.candidate.actions_cbor
            {
                if !carry_seen.insert(index) {
                    return Err("active preparation contains duplicate carry slot".into());
                }
                return Ok(false);
            }
        }
        if !matches!(slot.data.schema_version, 11 | 12)
            || slot.data.ledger_origin_digest != namespace
            || !slot.complete
        {
            return Err("active preparation contains unproved private candidates".into());
        }
        let value = slot
            .data
            .value
            .as_ref()
            .ok_or("Active WAL candidate absent")?;
        let header = value.block_id()?;
        let mut originals = current.original_inputs().filter(|input| {
            input.key.ledger_origin_digest == namespace
                && input.key.height == slot.data.height
                && input.key.header_id == header
        });
        let input = originals
            .next()
            .ok_or("Active WAL lacks authenticated current original")?;
        if originals.next().is_some() {
            return Err("Active WAL current original is ambiguous".into());
        }
        let entry = current
            .original_entry(&input.key)
            .map_err(|e| e.to_string())?
            .ok_or("Active WAL current entry absent")?;
        let reference =
            oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputRefV1::from_entry(
                &current.attestor().source,
                entry,
            )
            .map_err(|e| e.to_string())?;
        reference.validate_input(input).map_err(|e| e.to_string())?;
        let candidate = source_input::active_admission_candidate(input)?;
        let oasis7_distfs::protected_bft_v3::BftPrivateRetentionFactV1::Some {
            original_data_ref,
            ..
        } = &entry.fact
        else {
            return Err("Active WAL current fact is not Some".into());
        };
        if source_input::active_admission_slot_kind(input)? != kind
            || slot.data.digest()? != *original_data_ref
        {
            return Err(
                "Active WAL kind or bytes differ from authenticated current original".into(),
            );
        }
        self.verify_ready_active_candidate(&candidate, &reference, &entry.fact)?;
        Ok(true)
    }

    pub(super) fn inventory_count(&self) -> usize {
        self.candidates.len() + self.values.len()
    }
    pub(super) fn accounting_store(&self) -> LocalCasStore {
        self.store.clone()
    }
    pub(super) fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub(super) fn bind_origin(
        &mut self,
        ledger_origin: &str,
        material: &str,
        h: u64,
        nodes: &BTreeMap<String, Vec<u8>>,
        reserve: &mut impl FnMut(
            usize,
            super::super::super::super::formal_ledger::PublicationPromiseInventoryV1,
        ) -> Result<(), String>,
    ) -> Result<(), String> {
        let data = Data {
            schema_version: 3,
            domain: DOMAIN.into(),
            origin: true,
            ledger_origin_digest: ledger_origin.into(),
            material_identity: material.into(),
            instance_id: self.instance_id.clone(),
            height: h,
            value: None,
            actions_cbor: Vec::new(),
            references: nodes.keys().cloned().collect(),
            publication_reservation_ref: None,
            publication_original_data_ref: None,
            source_activation: None,
            protected_source_origin_ref: None,
        };
        if self.origin.as_ref().is_some_and(|slot| slot.data != data) {
            return Err("Node private origin differs from its exact qualified H".into());
        }
        if self.origin.is_none() {
            let prefix = format!("ledger-{}-", data.ledger_origin_digest);
            if self
                .store
                .list_pin_scope_shards(SCOPE)
                .map_err(|_| "inspect original Node H receipt")?
                .iter()
                .any(|shard| shard.starts_with(&prefix))
            {
                return Err(
                    "original Node H metadata is missing; receipt cannot initialize it".into(),
                );
            }
        }
        super::super::super::super::super::super::super::super::controlled_live::private_directory(
            &self.dir,
        )?;
        self.origin.get_or_insert(Slot {
            data: data.clone(),
            complete: false,
        });
        self.load_inventory(ledger_origin, material, h)?;
        reserve(self.usage_with(None, nodes)?, self.publication_promises()?)?;
        publish(
            &self.store,
            &self.dir.join("origin.cbor"),
            &data,
            nodes,
            self.origin.as_ref().unwrap().complete,
            true,
        )?;
        self.origin = Some(Slot {
            data,
            complete: true,
        });
        self.verify_inventory_pins()?;
        Ok(())
    }

    pub(super) fn load_inventory(
        &mut self,
        origin: &str,
        material: &str,
        h: u64,
    ) -> Result<(), String> {
        let mut heights = BTreeSet::new();
        let mut multi = BTreeSet::new();
        let mut selections = BTreeSet::new();
        let mut inventory_bytes = 0u64;
        for (count, entry) in fs::read_dir(&self.dir)
            .map_err(|_| "read private Node inventory")?
            .enumerate()
        {
            if count > 8 * (MAX_CANDIDATES + 64) + 8 {
                return Err("private Node directory capacity".into());
            }
            let entry = entry.map_err(|_| "inspect private Node inventory")?;
            if !entry
                .file_type()
                .map_err(|_| "inspect Node inventory file type")?
                .is_file()
            {
                return Err("private Node inventory contains a nonregular file".into());
            }
            inventory_bytes = inventory_bytes
                .checked_add(
                    entry
                        .metadata()
                        .map_err(|_| "inspect Node inventory bytes")?
                        .len(),
                )
                .filter(|bytes| *bytes <= 512 * 1024 * 1024)
                .ok_or("private Node inventory cumulative byte capacity")?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF8 private Node file")?;
            let target = files::stage_target(&name).unwrap_or(&name);
            if [
                "origin.cbor",
                "origin.intent.cbor",
                "origin.reservation.cbor",
                "ready",
            ]
            .contains(&target)
            {
                continue;
            }
            if let Some((height, id)) = multivalue::parse_target(target)? {
                if height <= h || height - h > 64 {
                    return Err("private Node height capacity".into());
                }
                if let Some(id) = id {
                    multi.insert((height, id));
                } else {
                    selections.insert(height);
                }
                continue;
            }
            let rest = target
                .strip_prefix("candidate-")
                .ok_or("unknown private Node file; preserved")?;
            let digits = rest.get(..20).ok_or("invalid private Node filename")?;
            let suffix = rest.get(20..).ok_or("invalid private Node filename")?;
            if ![".cbor", ".intent.cbor", ".reservation.cbor"].contains(&suffix)
                || !digits.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err("invalid private Node candidate filename".into());
            }
            let height: u64 = digits.parse().map_err(|_| "private Node height overflow")?;
            if height <= h || height - h > 64 {
                return Err("private Node height capacity".into());
            }
            heights.insert(height);
        }
        if heights.len() + multi.len() > MAX_CANDIDATES || selections.len() > 64 {
            return Err("Node candidate inventory count capacity".into());
        }
        let mut per_height = BTreeMap::<u64, usize>::new();
        for height in heights
            .iter()
            .copied()
            .chain(multi.iter().map(|(height, _)| *height))
        {
            let count = per_height.entry(height).or_default();
            *count += 1;
            if *count > MAX_CANDIDATES_PER_HEIGHT {
                return Err("Node per-height candidate inventory capacity".into());
            }
        }
        for height in heights {
            let slot = read_slot(&self.candidate_path(height))?
                .ok_or("private Node candidate identity is missing")?;
            if !matches!(slot.data.schema_version, 3 | 6 | 9 | 11)
                || slot.data.origin
                || slot.data.height != height
                || slot.data.ledger_origin_digest != origin
                || slot.data.material_identity != material
                || slot.data.instance_id != self.instance_id
            {
                return Err("private Node candidate differs from original H identity".into());
            }
            if retention_required(&self.candidate_path(height))? {
                verify_material(&self.store, &slot.data)?;
            }
            self.candidates.insert(height, slot);
        }
        self.load_multivalue_inventory(origin, material, &multi, &selections)?;
        for slot in self.candidates.values().chain(self.values.values()) {
            let path = self.source_slot_path(&slot.data)?;
            if matches!(slot.data.schema_version, 9 | 10 | 11 | 12) && !retention_required(&path)? {
                continue;
            }
            self.validate_publication_lineage(&slot.data)?;
            read_publication(&self.store, &slot.data)?;
        }
        self.check_counts(None)?;
        Ok(())
    }
    pub(super) fn verify_inventory_pins(&self) -> Result<(), String> {
        let origin = self.origin.as_ref().ok_or("Node origin unavailable")?;
        let prefix = format!("ledger-{}-", origin.data.ledger_origin_digest);
        let known: BTreeSet<_> = std::iter::once(origin.data.shard())
            .chain(self.candidates.values().map(|slot| slot.data.shard()))
            .chain(self.values.values().map(|slot| slot.data.shard()))
            .chain(self.selections.values().map(|slot| slot.data.shard()))
            .collect();
        for shard in self
            .store
            .list_pin_scope_shards(SCOPE)
            .map_err(|_| "read Node retention inventory")?
        {
            if shard.starts_with(&prefix) && !known.contains(&shard) {
                return Err("Node retention receipt lost its original private metadata".into());
            }
        }
        Ok(())
    }
    pub(super) fn origin_nodes(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        let slot = self.origin.as_ref().ok_or("Node origin unavailable")?;
        verify_original_slot(&self.store, &self.dir.join("origin.cbor"), slot)?;
        if common_storage::read_optional_bytes(&self.dir.join("ready"), MARKER.len())?.as_deref()
            != Some(MARKER)
        {
            return Err("original Node ready marker is missing or changed".into());
        }
        let origin = &slot.data;
        origin
            .references
            .iter()
            .map(|reference| {
                common_storage::read_cas_bounded(
                    &self.store,
                    reference,
                    formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                )
                .map(|bytes| (reference.clone(), bytes))
            })
            .collect()
    }
    pub(super) fn candidate(
        &self,
        height: u64,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        let Some(slot) = self.candidates.get(&height) else {
            return self.selected_candidate(height);
        };
        verify_original_slot(&self.store, &self.candidate_path(height), slot)?;
        Ok(Some(NodeBftPrivateCandidateV3 {
            public_value: slot
                .data
                .value
                .clone()
                .ok_or("private Node candidate value missing")?,
            actions_cbor: slot.data.actions_cbor.clone(),
        }))
    }
    pub(super) fn retain(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        nodes: &BTreeMap<String, Vec<u8>>,
        reserve: &mut impl FnMut(usize) -> Result<(), String>,
    ) -> Result<String, String> {
        let height = candidate.public_value.execution_commitment.height;
        if self.selections.contains_key(&height)
            || self
                .values
                .contains_key(&(height, candidate.public_value.block_id()?))
        {
            return self.select_received(candidate, nodes, reserve);
        }
        self.check_counts(Some((height, candidate.public_value.block_id()?)))?;
        let origin = &self
            .origin
            .as_ref()
            .ok_or("private Node origin unavailable")?
            .data;
        if height <= origin.height || height - origin.height > 64 {
            return Err("private Node height capacity".into());
        }
        let mut data = Data {
            schema_version: 3,
            domain: DOMAIN.into(),
            origin: false,
            ledger_origin_digest: origin.ledger_origin_digest.clone(),
            material_identity: origin.material_identity.clone(),
            instance_id: self.instance_id.clone(),
            height,
            value: Some(candidate.public_value.clone()),
            actions_cbor: candidate.actions_cbor.clone(),
            references: nodes.keys().cloned().collect(),
            publication_reservation_ref: None,
            publication_original_data_ref: None,
            source_activation: None,
            protected_source_origin_ref: None,
        };
        // Retries preserve the original Data bytes, including a new-format
        // reservation commitment; they never retrofit a legacy candidate.
        if let Some(old) = self.candidates.get(&height) {
            data.publication_reservation_ref = old.data.publication_reservation_ref.clone();
            data.publication_original_data_ref = old.data.publication_original_data_ref.clone();
            data.schema_version = old.data.schema_version;
            data.domain = old.data.domain.clone();
        }
        data.validate()?;
        if self
            .candidates
            .get(&height)
            .is_some_and(|slot| slot.data != data)
        {
            return Err(
                "Node height already retains a different original private candidate".into(),
            );
        }
        if let Some(slot) = self.candidates.get(&height) {
            verify_original_slot(&self.store, &self.candidate_path(height), slot)?;
        }
        let complete = self
            .candidates
            .get(&height)
            .is_some_and(|slot| slot.complete);
        reserve(self.usage_with(Some(&data), nodes)?)?;
        self.candidates.entry(height).or_insert(Slot {
            data: data.clone(),
            complete: false,
        });
        publish(
            &self.store,
            &self.candidate_path(height),
            &data,
            nodes,
            complete,
            false,
        )?;
        self.candidates.insert(
            height,
            Slot {
                data: data.clone(),
                complete: true,
            },
        );
        data.digest()
    }
    pub(super) fn set_auxiliary_root(&mut self, records: &Path) {
        self.auxiliary_root = Some(records.into());
    }
    pub(super) fn usage(&self) -> Result<usize, String> {
        self.usage_with(None, &BTreeMap::new())
    }
    pub(super) fn usage_without_auxiliary(&self) -> Result<usize, String> {
        self.usage_with_auxiliary(None, &BTreeMap::new(), false)
    }
    fn usage_with(
        &self,
        additional: Option<&Data>,
        nodes: &BTreeMap<String, Vec<u8>>,
    ) -> Result<usize, String> {
        self.usage_with_auxiliary(additional, nodes, true)
    }
    fn usage_with_auxiliary(
        &self,
        additional: Option<&Data>,
        nodes: &BTreeMap<String, Vec<u8>>,
        include_auxiliary: bool,
    ) -> Result<usize, String> {
        let material = self
            .origin
            .as_ref()
            .map(|s| s.data.material_identity.as_str())
            .or_else(|| additional.map(|d| d.material_identity.as_str()));
        let mut bytes = match (&self.auxiliary_root, material, include_auxiliary) {
            (Some(root), Some(material), true) => {
                super::auto_prepare::retained_usage(root, &self.store, material, &self.instance_id)?
            }
            _ => 0,
        };
        let mut refs = BTreeSet::new();
        for data in self
            .origin
            .iter()
            .map(|slot| &slot.data)
            .chain(
                self.candidates
                    .values()
                    .chain(self.values.values())
                    .chain(self.selections.values())
                    .filter(|slot| additional.is_none_or(|a| a.shard() != slot.data.shard()))
                    .map(|slot| &slot.data),
            )
            .chain(additional)
        {
            if self.carry_selection_held(data)? {
                continue;
            }
            if matches!(data.schema_version, 5 | 13) {
                let value = data.value.as_ref().ok_or("source selection value absent")?;
                if let Some(original) = self.values.get(&(data.height, value.block_id()?))
                    && matches!(original.data.schema_version, 10 | 12)
                {
                    if source_input::source_selection_data(&original.data)? != *data {
                        return Err("protected received selection original differs".into());
                    }
                    // Schema10 already holds the complete exact selection cost.
                    continue;
                }
            }
            bytes = bytes
                .checked_add(
                    data.bytes()?
                        .len()
                        .checked_mul(2)
                        .ok_or("Node capacity overflow")?,
                )
                .filter(|sum| *sum <= 512 * 1024 * 1024)
                .ok_or("Node cumulative material capacity")?;
            if matches!(data.schema_version, 9 | 10 | 11 | 12) {
                bytes = bytes
                    .checked_add(source_input::private_pin_bound(data)?)
                    .and_then(|n| n.checked_add(source_input::source_selection_bound(data).ok()?))
                    .filter(|n| *n <= 512 * 1024 * 1024)
                    .ok_or("protected source pin material capacity")?;
            }
            if let Some(activation) = &data.source_activation {
                bytes = bytes
                    .checked_add(activation.accounted_bytes()?)
                    .filter(|n| *n <= 512 * 1024 * 1024)
                    .ok_or("source activation cumulative material capacity")?;
            }
            if let Some(publication) = read_publication(&self.store, data)? {
                bytes = bytes
                    .checked_add(publication.accounted_bytes()?)
                    .filter(|sum| *sum <= 512 * 1024 * 1024)
                    .ok_or("Node cumulative publication reservation capacity")?;
            }
            if let Some(reference) = &data.publication_original_data_ref {
                bytes = bytes
                    .checked_add(
                        publication_archive::carry_initialization::original_and_carry_hold(
                            &self.store,
                            data,
                            reference,
                        )?,
                    )
                    .filter(|n| *n <= 512 * 1024 * 1024)
                    .ok_or("original publication Data cumulative capacity")?;
            }
            if !matches!(data.schema_version, 5 | 13) {
                refs.extend(data.references.iter().cloned());
            }
        }
        if refs.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES {
            return Err("Node retained index count capacity".into());
        }
        let mut dependency_bytes = 0usize;
        for reference in refs {
            let size = if let Some(data) = nodes.get(&reference) {
                data.len()
            } else {
                common_storage::read_cas_bounded(
                    &self.store,
                    &reference,
                    formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                )?
                .len()
            };
            dependency_bytes = dependency_bytes
                .checked_add(size)
                .filter(|sum| *sum <= formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES)
                .ok_or("Node cumulative dependency byte capacity")?;
            bytes = bytes
                .checked_add(size)
                .filter(|sum| *sum <= 512 * 1024 * 1024)
                .ok_or("Node cumulative material capacity")?;
        }
        Ok(bytes)
    }
    fn candidate_path(&self, height: u64) -> PathBuf {
        self.dir.join(format!("candidate-{height:020}.cbor"))
    }
}

fn read_slot(path: &Path) -> Result<Option<Slot>, String> {
    let parent = path.parent().ok_or("private Node record parent missing")?;
    match fs::symlink_metadata(parent) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("inspect private Node record directory".into()),
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err("invalid private Node record directory".into());
        }
        Ok(_) => {}
    }
    let intent = path.with_extension("intent.cbor");
    let reservation = path.with_extension("reservation.cbor");
    let record: Option<Data> = common_storage::read_optional_canonical(path, MAX_RECORD)
        .map_err(|_| "invalid original private Node record")?;
    let pending: Option<Data> = common_storage::read_optional_canonical(&intent, MAX_RECORD)
        .map_err(|_| "invalid original private Node intent")?;
    let reserved: Option<Data> = common_storage::read_optional_canonical(&reservation, MAX_RECORD)
        .map_err(|_| "invalid original private Node reservation")?;
    let staged_intent: Option<Data> = files::staged_value(&intent, MAX_RECORD)
        .map_err(|_| "invalid original private Node intent stage")?;
    let staged_reservation: Option<Data> = files::staged_value(&reservation, MAX_RECORD)
        .map_err(|_| "invalid original private Node reservation stage")?;
    if !files::stages(path)?.is_empty() {
        return Err("private Node record has unsupported direct staging".into());
    }
    let values = [
        record.as_ref(),
        pending.as_ref(),
        reserved.as_ref(),
        staged_intent.as_ref(),
        staged_reservation.as_ref(),
    ];
    let Some(first) = values.into_iter().flatten().next() else {
        return Ok(None);
    };
    first.validate()?;
    if values.into_iter().flatten().any(|data| data != first) {
        return Err("private Node original transaction bytes conflict".into());
    }
    match (record.is_some(), pending.is_some()) {
        (true, true) => files::same_pair(path, &intent)?,
        (true, false) => {
            files::single_link(path)?;
            if reserved.is_some() || staged_intent.is_some() || staged_reservation.is_some() {
                return Err("completed Node record has foreign pending material".into());
            }
        }
        (false, true) => files::single_link(&intent)?,
        _ => {}
    }
    if reserved.is_some() {
        files::single_link(&reservation)?;
    }
    Ok(Some(Slot {
        data: first.clone(),
        complete: record.is_some() && pending.is_none(),
    }))
}
fn verify_original_slot(store: &LocalCasStore, path: &Path, expected: &Slot) -> Result<(), String> {
    let actual = read_slot(path)?.ok_or("original private Node metadata is missing")?;
    if actual.data != expected.data || (expected.complete && !actual.complete) {
        return Err("original private Node metadata changed or regressed".into());
    }
    read_publication(store, &actual.data)?;
    if retention_required(path)? {
        verify_material(store, &actual.data)?;
    }
    Ok(())
}
fn publication_identity(data: &Data) -> Result<PublicationCandidateIdentityV1, String> {
    if data.origin || matches!(data.schema_version, 5 | 13) {
        return Err("publication requires original candidate Data".into());
    }
    Ok(PublicationCandidateIdentityV1 {
        origin: data.ledger_origin_digest.clone(),
        material: data.material_identity.clone(),
        instance: data.instance_id.clone(),
        height: data.height,
        header: data
            .value
            .as_ref()
            .ok_or("publication value missing")?
            .block_id()?,
        actions: blake3_hex(&data.actions_cbor),
        reservation_ref: data.publication_reservation_ref.clone(),
    })
}
fn read_publication(
    store: &LocalCasStore,
    data: &Data,
) -> Result<Option<PublicationReservationV1>, String> {
    let Some(reference) = &data.publication_reservation_ref else {
        return Ok(None);
    };
    let value = PublicationReservationV1::read(store, reference)?;
    let original = match &data.publication_original_data_ref {
        Some(reference) => Some(publication_archive::read_original_data(store, reference)?),
        None => None,
    };
    value.validate_identity(&publication_identity(original.as_ref().unwrap_or(data))?)?;
    Ok(Some(value))
}
fn verify_material(store: &LocalCasStore, data: &Data) -> Result<(), String> {
    store
        .verify_pin_scope_shard(SCOPE, &data.shard(), &data.retained()?)
        .map_err(|_| "original Node retention receipt missing or changed")?;
    if common_storage::read_cas_bounded(store, &data.digest()?, MAX_RECORD)? != data.bytes()? {
        return Err("original private Node CAS record changed".into());
    }
    read_publication(store, data)?;
    source_activation::verify_activation_material(store, data)?;
    let reference_limit = if matches!(data.schema_version, 5 | 13) {
        MAX_RECORD
    } else {
        formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES
    };
    for reference in &data.references {
        common_storage::read_cas_bounded(store, reference, reference_limit)?;
    }
    Ok(())
}
fn retention_required(path: &Path) -> Result<bool, String> {
    let intent = path.with_extension("intent.cbor");
    Ok(
        common_storage::read_optional_bytes(path, MAX_RECORD)?.is_some()
            || common_storage::read_optional_bytes(&intent, MAX_RECORD)?.is_some()
            || !files::stages(&intent)?.is_empty(),
    )
}
fn publish(
    store: &LocalCasStore,
    path: &Path,
    data: &Data,
    nodes: &BTreeMap<String, Vec<u8>>,
    complete: bool,
    origin: bool,
) -> Result<(), String> {
    publish_checked(store, path, data, nodes, complete, origin, &mut || Ok(()))
}
fn publish_checked(
    store: &LocalCasStore,
    path: &Path,
    data: &Data,
    nodes: &BTreeMap<String, Vec<u8>>,
    complete: bool,
    origin: bool,
    guard: &mut impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    guard()?;
    if complete {
        return verify_material(store, data);
    }
    // An intent or record can exist only after original retention succeeded.
    // Its loss cannot authorize a new pin publication, even before finish.
    let already_retained = retention_required(path)?;
    if already_retained {
        verify_material(store, data)?;
    }
    read_publication(store, data)?;
    let bytes = data.bytes()?;
    let intent = path.with_extension("intent.cbor");
    let reservation = path.with_extension("reservation.cbor");
    if common_storage::read_optional_bytes(&intent, MAX_RECORD)?.is_none()
        && common_storage::read_optional_bytes(path, MAX_RECORD)?.is_none()
    {
        guard()?;
        files::publish(&reservation, &bytes)?;
    }
    let has_pins = store
        .list_pin_scope_shards(SCOPE)
        .map_err(|_| "inspect Node original pins")?
        .contains(&data.shard());
    if has_pins {
        verify_material(store, data)?;
    } else {
        if already_retained {
            return Err("original Node retention receipt is missing".into());
        }
        guard()?;
        common_storage::put_cas_durable(store, &bytes, MAX_RECORD)?;
        guard()?;
        let reference_limit = if matches!(data.schema_version, 5 | 13) {
            MAX_RECORD
        } else {
            formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES
        };
        for reference in &data.references {
            if let Some(bytes) = nodes.get(reference) {
                if blake3_hex(bytes) != *reference {
                    return Err("Node retained dependency identity mismatch".into());
                }
                guard()?;
                common_storage::put_cas_durable(store, bytes, reference_limit)?;
            } else {
                common_storage::read_cas_bounded(store, reference, reference_limit)?;
            }
        }
        guard()?;
        store
            .replace_pin_scope_shard(SCOPE, &data.shard(), &data.retained()?)
            .map_err(|_| "publish original Node pins")?;
        verify_material(store, data)?;
    }
    if common_storage::read_optional_bytes(path, MAX_RECORD)?.is_some() {
        files::same_pair(path, &intent)?;
    } else {
        // Promotion already consumed the reservation in an intent-only crash.
        // Resume the retained original intent; never recreate its material.
        if common_storage::read_optional_bytes(&intent, MAX_RECORD)?.is_none() {
            guard()?;
            files::promote(&reservation, &intent, &bytes)?;
        }
        guard()?;
        files::link_record(path, &intent, &bytes)?;
    }
    if origin {
        guard()?;
        files::publish(
            &path
                .parent()
                .ok_or("Node origin parent missing")?
                .join("ready"),
            MARKER,
        )?;
    }
    guard()?;
    files::finish(path, &intent, &bytes)
}
fn canonical_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn bounded_references<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    struct Refs;
    impl<'de> serde::de::Visitor<'de> for Refs {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded Node references")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let max = super::super::super::super::formal_dependencies::MAX_RETAINED_INDEX_NODES;
            if seq.size_hint().is_some_and(|n| n > max) {
                return Err(serde::de::Error::custom("Node references exceed bound"));
            }
            let mut values = Vec::new();
            while values.len() < max {
                let Some(value) = seq.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("Node references exceed bound"));
            }
            Ok(values)
        }
    }
    d.deserialize_seq(Refs)
}

#[cfg(test)]
#[path = "driver_bft_v3_node_private_store_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "driver_bft_v3_current_inventory_negative_controls.rs"]
mod current_inventory_negative_controls;
