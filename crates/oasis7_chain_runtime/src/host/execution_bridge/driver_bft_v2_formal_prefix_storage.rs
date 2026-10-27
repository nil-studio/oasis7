//! A separate H-anchored multiheight format. Neither an engineering genesis
//! nor the old fixed H+1 ledger is accepted as this ledger's origin.
use super::super::super::storage as common_storage;
use super::formal_prefix_files as files;
use super::*;
use oasis7_formal_protocol::bft::{
    BFT_FORMAL_PREFIX_MAX_HEIGHTS_V1, FormalBftCertifiedEntryV1, FormalBftCertifiedPrefixV1,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub(super) const DIRECTORY: &str = "formal-bft-prefix-v1";
const MARKER: &str = "formal-bft-prefix-required-v1";
const MARKER_BYTES: &[u8] = b"oasis7.formal.bft.prefix.required/v1\n";
const LEGACY_MARKER: &str = "formal-bft-read-required-v1";
const DOWNGRADE_FENCE: &[u8] = common_storage::FORMAL_PREFIX_DOWNGRADE_FENCE;
const SCOPE: &str = "formal-bft-prefix-v1";
const MAX_RECORD: usize = 8 * 1024 * 1024;
const MAX_MATERIAL: usize = 512 * 1024 * 1024;
#[path = "driver_bft_v2_formal_prefix_init.rs"]
mod initialization;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginPin {
    schema_version: u32,
    domain: String,
    ledger_id: String,
    material_pin_digest: String,
    first_height: u64,
    format_lock: common_storage::LockCarrierIdentity,
    writer_lock: common_storage::LockCarrierIdentity,
}
impl OriginPin {
    fn validate(&self) -> Result<(), String> {
        hash(&self.ledger_id)?;
        hash(&self.material_pin_digest)?;
        hash(&self.format_lock.token)?;
        hash(&self.writer_lock.token)?;
        if self.schema_version != 1
            || self.domain != "oasis7.formal.bft.prefix.origin.v1"
            || self.format_lock.inode == 0
            || self.writer_lock.inode == 0
            || self.first_height == 0
            || self.first_height.checked_add(63).is_none()
        {
            return Err("formal prefix origin schema or height capacity".into());
        }
        Ok(())
    }
    fn digest(&self) -> Result<String, String> {
        self.validate()?;
        digest(self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    schema_version: u32,
    origin_digest: String,
    pub(super) height: u64,
    parent_record_digest: String,
    pub(super) entry: FormalBftCertifiedEntryV1,
    snapshot_ref: String,
    journal_ref: String,
    business_index_refs: Vec<String>,
    new_results: Vec<CanonicalIntentResultV1>,
}
impl Record {
    pub(super) fn digest(&self) -> Result<String, String> {
        digest(self)
    }
    fn references(&self) -> BTreeSet<String> {
        let mut refs: BTreeSet<_> = self.business_index_refs.iter().cloned().collect();
        refs.insert(self.snapshot_ref.clone());
        refs.insert(self.journal_ref.clone());
        refs
    }
    fn validate(&self, origin: &OriginPin) -> Result<(), String> {
        for value in [
            &self.origin_digest,
            &self.parent_record_digest,
            &self.snapshot_ref,
            &self.journal_ref,
        ] {
            hash(value)?;
        }
        self.entry.certificate.validate_wire_bounds()?;
        self.entry.commitment.validate()?;
        if self.schema_version != 1
            || self.origin_digest != origin.digest()?
            || self.height != self.entry.certificate.proposal.block.height
            || self.height < origin.first_height
            || self.height - origin.first_height >= 64
            || self.new_results.len() > 1024
            || serde_cbor::to_vec(&self.new_results)
                .map_err(|e| e.to_string())?
                .len()
                > 4 * 1024 * 1024
            || self.business_index_refs.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES
            || !self
                .business_index_refs
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        {
            return Err("formal prefix record header or dependency/result capacity".into());
        }
        for reference in &self.business_index_refs {
            hash(reference)?;
        }
        self.entry.commitment.verify_block(
            &self.entry.certificate.proposal.block,
            self.entry.commitment.activation_digest(),
        )?;
        Ok(())
    }
}

#[derive(Clone)]
struct Slot {
    record: Record,
    committed: bool,
    intent: bool,
    reserved: bool,
}

#[derive(Default)]
struct Capacity {
    bytes: usize,
    objects: BTreeSet<String>,
    records: BTreeSet<u64>,
}
impl Capacity {
    fn charge(&mut self, size: usize) -> Result<(), String> {
        self.bytes = self
            .bytes
            .checked_add(size)
            .filter(|bytes| *bytes <= MAX_MATERIAL)
            .ok_or("StorageCapacityExceeded: formal prefix cumulative materials exceed 512 MiB")?;
        Ok(())
    }
    fn object(&mut self, reference: &str, size: usize) -> Result<(), String> {
        if !self.objects.contains(reference) {
            self.charge(size)?;
            self.objects.insert(reference.to_owned());
        }
        Ok(())
    }
    fn read_bound(&self, reference: &str, per_blob: usize) -> Result<usize, String> {
        if self.objects.contains(reference) {
            return Ok(per_blob);
        }
        let remaining = MAX_MATERIAL
            .checked_sub(self.bytes)
            .filter(|n| *n > 0)
            .ok_or("StorageCapacityExceeded: formal prefix has no remaining material bytes")?;
        Ok(per_blob.min(remaining))
    }
    fn record(&mut self, record: &Record) -> Result<(), String> {
        let bytes = serde_cbor::to_vec(record).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_RECORD {
            return Err("formal prefix record exceeds byte capacity".into());
        }
        if !self.records.contains(&record.height) {
            self.charge(bytes.len())?;
            self.records.insert(record.height);
        }
        Ok(())
    }
}

pub(super) struct Ledger {
    root: PathBuf,
    dir: PathBuf,
    _lock: File,
    _format_lock: File,
    pin: Option<OriginPin>,
    slots: BTreeMap<u64, Slot>,
    capacity: Capacity,
    initialize: bool,
    init_pending: bool,
    proof_bytes: usize,
    origin_bytes: usize,
    #[cfg(test)]
    fault: Option<(u64, PublicationFault)>,
    #[cfg(test)]
    kill_on_fault: bool,
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PublicationFault {
    PinsPublished,
    IntentPublished,
    RecordLinked,
}

impl Ledger {
    #[cfg(test)]
    pub(super) fn inject_publication_failure(&mut self, height: u64, phase: PublicationFault) {
        self.fault = Some((height, phase));
    }

    #[cfg(test)]
    pub(super) fn inject_process_kill(&mut self, height: u64, phase: PublicationFault) {
        self.fault = Some((height, phase));
        self.kill_on_fault = true;
    }

    pub(super) fn exists(root: &Path) -> Result<bool, String> {
        let directory = match fs::symlink_metadata(root.join(DIRECTORY)) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
                true
            }
            Ok(_) => return Err("formal prefix ledger path is not a real directory".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.to_string()),
        };
        let fence = common_storage::read_optional_bytes(&root.join(LEGACY_MARKER), 128)?;
        if fence.as_deref() == Some(DOWNGRADE_FENCE) && !directory {
            return Err("formal prefix downgrade fence exists without original ledger".into());
        }
        let marker = common_storage::read_optional_bytes(&root.join(MARKER), MARKER_BYTES.len())?;
        if let Some(bytes) = marker.as_ref() {
            if bytes != MARKER_BYTES || !directory {
                return Err("formal prefix required marker has no valid ledger".into());
            }
            crate::controlled_live_config::require_private_owned(&root.join(MARKER), false)?;
        }
        Ok(directory || marker.is_some())
    }

    pub(super) fn open(root: &Path, initialize: bool, origin_bytes: usize) -> Result<Self, String> {
        if !initialize {
            crate::controlled_live_config::require_private_owned(root, true)?;
        } else {
            super::super::super::super::super::controlled_live::private_directory(root)?;
        }
        if Self::exists(root)? && fs::symlink_metadata(root.join("bft-format.lock")).is_err() {
            return Err(
                "formal prefix stable format lock is missing; refusing to recreate it".into(),
            );
        }
        let format_lock = common_storage::lock_format(root)?;
        let fence = common_storage::read_optional_bytes(&root.join(LEGACY_MARKER), 128)?;
        let legacy_directory = match fs::symlink_metadata(root.join("formal-bft-read-v1")) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.to_string()),
        };
        if legacy_directory || (fence.is_some() && fence.as_deref() != Some(DOWNGRADE_FENCE)) {
            return Err(
                "formal prefix format refuses the old H+1 ledger; use a separate record directory"
                    .into(),
            );
        }
        if super::super::super::storage::BftExecutionLedger::required_material_exists(root)?
            || super::super::storage::BftV2ExecutionLedger::required_material_exists(root)?
        {
            return Err("formal prefix format refuses an engineering ledger".into());
        }
        Self::exists(root)?;
        if !initialize {
            crate::controlled_live_config::require_private_owned(root, true)?;
        } else {
            super::super::super::super::super::controlled_live::private_directory(root)?;
        }
        let dir = root.join(DIRECTORY);
        let lock = files::lock(&dir, initialize)?;
        let (pin, init_pending) = Self::load_origin(&dir, root, initialize)?;
        let mut ledger = Self {
            root: root.into(),
            dir,
            _lock: lock,
            _format_lock: format_lock,
            pin,
            slots: BTreeMap::new(),
            capacity: Capacity::default(),
            initialize,
            init_pending,
            proof_bytes: 0,
            origin_bytes,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            kill_on_fault: false,
        };
        ledger.verify_carriers()?;
        ledger.capacity.charge(origin_bytes)?;
        ledger.reload()?;
        Ok(ledger)
    }

    pub(super) fn reload(&mut self) -> Result<(), String> {
        crate::controlled_live_config::require_private_owned(&self.root, true)?;
        crate::controlled_live_config::require_private_owned(&self.dir, true)?;
        self.verify_carriers()?;
        let (disk_pin, pending) = Self::load_origin(&self.dir, &self.root, self.initialize)?;
        if disk_pin != self.pin || pending != self.init_pending {
            return Err(
                "formal prefix original origin changed or disappeared during durable reread".into(),
            );
        }

        let mut heights = BTreeSet::new();
        for (count, entry) in fs::read_dir(&self.dir)
            .map_err(|e| e.to_string())?
            .enumerate()
        {
            if count > 3 * BFT_FORMAL_PREFIX_MAX_HEIGHTS_V1 + 4 {
                return Err("formal prefix directory capacity exceeded".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "formal prefix non-UTF8 filename")?;
            if name == "writer.lock" || name == "origin.cbor" || name == "init-intent.cbor" {
                continue;
            }
            let target = files::stage_target(&name).unwrap_or(&name);
            if target == "init-intent.cbor" {
                continue;
            }
            let height = parse(target, "record-")
                .or_else(|| parse(target, "intent-"))
                .or_else(|| parse(target, "reservation-"))
                .ok_or("formal prefix directory contains an unknown file; preserved")?;
            if files::stage_target(&name).is_some()
                && parse(target, "reservation-").is_none()
                && parse(target, "intent-").is_none()
            {
                return Err("formal prefix unsupported staging target; preserved".into());
            }
            heights.insert(height);
        }
        if heights.len() > BFT_FORMAL_PREFIX_MAX_HEIGHTS_V1 {
            return Err("formal prefix height capacity exceeded".into());
        }
        if (self.pin.is_none() || self.init_pending) && !heights.is_empty() {
            return Err("formal prefix records exist without original origin".into());
        }
        let mut metadata_bytes = self.origin_bytes;
        let mut slots = BTreeMap::new();
        let mut previous = self.pin.as_ref().map(OriginPin::digest).transpose()?;
        for height in heights {
            let record_path = self.record_path(height);
            let intent_path = self.intent_path(height);
            let reservation_path = self.reservation_path(height);
            let paths = [&record_path, &intent_path, &reservation_path];
            let mut all_paths = paths.iter().map(|p| (*p).clone()).collect::<Vec<_>>();
            for path in paths {
                all_paths.extend(files::stages(path)?);
            }
            let size = all_paths
                .iter()
                .filter_map(|path| match fs::symlink_metadata(path) {
                    Ok(metadata) => Some(Ok(metadata.len())),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => Some(Err(error.to_string())),
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .max()
                .unwrap_or_default();
            let maximum = (MAX_MATERIAL
                .checked_sub(metadata_bytes)
                .ok_or("StorageCapacityExceeded: formal prefix cumulative record metadata")?)
            .min(MAX_RECORD);
            if size > maximum as u64 {
                return Err(
                    "StorageCapacityExceeded: formal prefix cumulative record metadata".into(),
                );
            }
            metadata_bytes = metadata_bytes
                .checked_add(size as usize)
                .ok_or("formal prefix metadata byte overflow")?;
            let record: Option<Record> =
                common_storage::read_optional_canonical(&record_path, maximum)?;
            let intent: Option<Record> =
                common_storage::read_optional_canonical(&intent_path, maximum)?;
            let staged_intent: Option<Record> = files::staged_value(&intent_path, maximum)?;
            let canonical_reservation: Option<Record> =
                common_storage::read_optional_canonical(&reservation_path, maximum)?;
            let staged_reservation: Option<Record> =
                files::staged_value(&reservation_path, maximum)?;
            let reservation = canonical_reservation
                .as_ref()
                .or(staged_reservation.as_ref());
            if canonical_reservation
                .as_ref()
                .zip(staged_reservation.as_ref())
                .is_some_and(|(a, b)| a != b)
                || intent
                    .as_ref()
                    .zip(staged_intent.as_ref())
                    .is_some_and(|(a, b)| a != b)
                || (reservation.is_some()
                    && (record.is_some() || intent.is_some() || staged_intent.is_some()))
            {
                return Err(
                    "formal prefix original reservation/intent identities conflict; preserved"
                        .into(),
                );
            }
            if canonical_reservation.is_some() {
                files::single_link(&reservation_path)?;
            }
            let pending_intent = intent.as_ref().or(staged_intent.as_ref());
            let value = match (&record, &intent) {
                (Some(a), Some(b)) => {
                    files::same_pair(&record_path, &intent_path)?;
                    if a != b {
                        return Err("formal prefix original record/intent bytes conflict".into());
                    }
                    a
                }
                (Some(a), None) => {
                    files::single_link(&record_path)?;
                    a
                }
                (None, Some(a)) => {
                    files::single_link(&intent_path)?;
                    a
                }
                _ => pending_intent
                    .or(reservation)
                    .ok_or("formal prefix slot disappeared during locked load")?,
            };
            let pin = self.pin.as_ref().ok_or("formal prefix origin missing")?;
            value.validate(pin)?;
            let expected_height = pin
                .first_height
                .checked_add(slots.len() as u64)
                .ok_or("formal prefix height overflow")?;
            if height != expected_height
                || value.height != height
                || previous.as_deref() != Some(&value.parent_record_digest)
            {
                return Err(
                    "formal prefix immutable history has a gap or different parent digest".into(),
                );
            }
            if slots
                .values()
                .any(|slot: &Slot| slot.intent || slot.reserved || !slot.committed)
            {
                return Err(
                    "formal prefix contains records after an unfinished original intent".into(),
                );
            }
            previous = Some(value.digest()?);
            slots.insert(
                height,
                Slot {
                    record: value.clone(),
                    committed: record.is_some(),
                    intent: pending_intent.is_some(),
                    reserved: reservation.is_some(),
                },
            );
        }
        if !slots.is_empty() {
            if common_storage::read_optional_bytes(&self.root.join(MARKER), MARKER_BYTES.len())?
                .as_deref()
                != Some(MARKER_BYTES)
                || common_storage::read_optional_bytes(&self.root.join(LEGACY_MARKER), 128)?
                    .as_deref()
                    != Some(DOWNGRADE_FENCE)
            {
                return Err("formal prefix original profile/downgrade marker disappeared".into());
            }
            files::single_link(&self.root.join(MARKER))?;
            files::single_link(&self.root.join(LEGACY_MARKER))?;
        }
        self.slots = slots;
        Ok(())
    }

    pub(super) fn bind(
        &mut self,
        material_pin_digest: &str,
        first_height: u64,
    ) -> Result<(), String> {
        self.bind_origin(material_pin_digest, first_height)
    }

    pub(super) fn preload(
        &mut self,
        store: &LocalCasStore,
        through: Option<u64>,
    ) -> Result<Vec<(String, Vec<u8>)>, String> {
        if self.pin.is_none() {
            return Ok(Vec::new());
        }
        let prefix = format!("origin-{}-height-", self.origin_digest()?);
        let known = self
            .slots
            .keys()
            .map(|height| self.shard(*height))
            .collect::<Result<BTreeSet<_>, _>>()?;
        for shard in store
            .list_pin_scope_shards(SCOPE)
            .map_err(|e| format!("list formal prefix pins: {e:?}"))?
        {
            if shard.starts_with(&prefix) && !known.contains(&shard) {
                return Err(
                    "formal prefix retention shard has no original QC identity; preserved".into(),
                );
            }
        }
        let mut references = BTreeSet::new();
        let mut visible = BTreeSet::new();
        for slot in self.slots.values() {
            self.capacity.record(&slot.record)?;
            let record = &slot.record;
            if slot.reserved
                && !store
                    .list_pin_scope_shards(SCOPE)
                    .map_err(|e| format!("list prefix pins: {e:?}"))?
                    .contains(&self.shard(record.height)?)
            {
                continue;
            }
            self.verify_pins(record, store)?;
            if through.is_none_or(|height| record.height <= height) {
                visible.extend(record.business_index_refs.iter().cloned());
            }
            for (reference, limit) in [
                (&record.snapshot_ref, common_storage::MAX_BFT_SNAPSHOT_BYTES),
                (&record.journal_ref, common_storage::MAX_BFT_JOURNAL_BYTES),
            ] {
                let maximum = self.capacity.read_bound(reference, limit)?;
                let bytes = common_storage::read_cas_bounded(store, reference, maximum)?;
                self.capacity.object(reference, bytes.len())?;
            }
            references.extend(record.business_index_refs.iter().cloned());
            if references.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES {
                return Err("formal prefix cumulative index node capacity exceeded".into());
            }
        }
        let nodes = formal_storage::read_business_index_nodes_bounded(
            &references,
            formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES,
            |reference, limit| {
                let maximum = self.capacity.read_bound(reference, limit)?;
                let bytes = common_storage::read_cas_bounded(store, reference, maximum)?;
                self.capacity.object(reference, bytes.len())?;
                Ok(bytes)
            },
        )?;
        Ok(nodes
            .into_iter()
            .filter(|(reference, _)| visible.contains(reference))
            .collect())
    }

    pub(super) fn prefix_through(&self, height: u64) -> Result<FormalBftCertifiedPrefixV1, String> {
        if !self.slots.contains_key(&height) {
            return Err("formal prefix exact target has not been installed; original prefix input is required".into());
        }
        Ok(FormalBftCertifiedPrefixV1 {
            schema_version: 1,
            entries: self
                .slots
                .range(..=height)
                .map(|(_, slot)| slot.record.entry.clone())
                .collect(),
        })
    }
    pub(super) fn check_input(&self, prefix: &FormalBftCertifiedPrefixV1) -> Result<(), String> {
        for (index, slot) in self.slots.values().enumerate() {
            if prefix.entries.get(index) != Some(&slot.record.entry) {
                return Err(
                    "formal prefix input conflicts with installed history or original pending QC"
                        .into(),
                );
            }
        }
        Ok(())
    }
    pub(super) fn charge_proof(&mut self, bytes: usize) -> Result<(), String> {
        self.capacity
            .charge(bytes.saturating_sub(self.proof_bytes))?;
        self.proof_bytes = self.proof_bytes.max(bytes);
        Ok(())
    }
    pub(super) fn empty(&self) -> bool {
        self.slots.is_empty()
    }
    pub(super) fn material_pin_digest(&self) -> Result<&str, String> {
        Ok(&self
            .pin
            .as_ref()
            .ok_or("formal prefix origin missing")?
            .material_pin_digest)
    }
    pub(super) fn origin_digest(&self) -> Result<String, String> {
        self.pin
            .as_ref()
            .ok_or("formal prefix origin missing")?
            .digest()
    }

    pub(super) fn accept(
        &mut self,
        entry: &FormalBftCertifiedEntryV1,
        executed: &ExecutedFormalCandidateV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        previous: &str,
        store: &LocalCasStore,
        append: bool,
    ) -> Result<String, String> {
        self.verify_carriers()?;
        if entry.certificate.proposal.block != executed.block
            || entry.commitment != executed.commitment
        {
            return Err("formal prefix candidate was not independently reproduced".into());
        }
        formal_dependencies::validate_nodes(nodes)?;
        let height = executed.block.height;
        let existing = self.slots.get(&height).cloned();
        if existing.is_none() && !append {
            return Err("formal prefix verification cannot append a new height".into());
        }
        let observed: BTreeSet<_> = nodes.keys().cloned().collect();
        let refs = if let Some(slot) = &existing {
            let retained: BTreeSet<_> = slot.record.business_index_refs.iter().cloned().collect();
            if !observed.is_subset(&retained) {
                return Err("formal prefix replay consumed an unretained dependency".into());
            }
            slot.record.business_index_refs.clone()
        } else {
            nodes.keys().cloned().collect()
        };
        let record = Record {
            schema_version: 1,
            origin_digest: self.origin_digest()?,
            height,
            parent_record_digest: previous.into(),
            entry: entry.clone(),
            snapshot_ref: blake3_hex(&executed.snapshot_bytes),
            journal_ref: blake3_hex(&executed.journal_bytes),
            business_index_refs: refs,
            new_results: executed.new_results.clone(),
        };
        record.validate(self.pin.as_ref().ok_or("formal prefix origin absent")?)?;
        self.capacity.record(&record)?;
        self.capacity
            .object(&record.snapshot_ref, executed.snapshot_bytes.len())?;
        self.capacity
            .object(&record.journal_ref, executed.journal_bytes.len())?;
        for (reference, bytes) in nodes {
            self.capacity.object(reference, bytes.len())?;
        }
        let bytes = serde_cbor::to_vec(&record).map_err(|e| e.to_string())?;
        if existing.as_ref().is_some_and(|slot| slot.record != record) {
            return Err(
                "formal prefix durable record differs from actual replay or original QC".into(),
            );
        }
        let reserved = existing.as_ref().is_none_or(|slot| slot.reserved);
        if reserved {
            if existing.is_none() && self.slots.len() >= 64 {
                return Err("formal prefix height capacity exceeded".into());
            }
            // Fix the original full QC before any externally retained material.
            files::publish(&self.reservation_path(height), &bytes)?;
            let shard = self.shard(height)?;
            let has_pins = store
                .list_pin_scope_shards(SCOPE)
                .map_err(|e| format!("list prefix pins: {e:?}"))?
                .contains(&shard);
            if has_pins {
                self.verify_pins(&record, store)?;
                self.verify_material(&record, executed, nodes, store)?;
            } else {
                common_storage::put_cas_durable(
                    store,
                    &executed.snapshot_bytes,
                    common_storage::MAX_BFT_SNAPSHOT_BYTES,
                )?;
                common_storage::put_cas_durable(
                    store,
                    &executed.journal_bytes,
                    common_storage::MAX_BFT_JOURNAL_BYTES,
                )?;
                for bytes in nodes.values() {
                    common_storage::put_cas_durable(
                        store,
                        bytes,
                        formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
                    )?;
                }
                self.verify_carriers()?;
                store
                    .replace_pin_scope_shard(SCOPE, &shard, &record.references())
                    .map_err(|e| format!("publish formal prefix pins: {e:?}"))?;
                self.verify_pins(&record, store)?;
            }
            #[cfg(test)]
            self.publication_fault(height, PublicationFault::PinsPublished)?;
            self.verify_carriers()?;
            files::promote(
                &self.reservation_path(height),
                &self.intent_path(height),
                &bytes,
            )?;
        } else {
            self.verify_pins(&record, store)?;
            self.verify_material(&record, executed, nodes, store)?;
        }
        if existing
            .as_ref()
            .is_none_or(|slot| !slot.committed || slot.intent)
        {
            // This also promotes a proven equivalent complete pre-rename intent.
            if !existing
                .as_ref()
                .is_some_and(|slot| slot.committed && slot.intent)
            {
                files::publish(&self.intent_path(height), &bytes)?;
            }
            #[cfg(test)]
            self.publication_fault(height, PublicationFault::IntentPublished)?;
            self.verify_carriers()?;
            files::link_record(&self.record_path(height), &self.intent_path(height), &bytes)?;
            #[cfg(test)]
            self.publication_fault(height, PublicationFault::RecordLinked)?;
            self.verify_carriers()?;
            files::finish(&self.record_path(height), &self.intent_path(height), &bytes)?;
        }
        self.verify_carriers()?;
        files::clean_equivalent_stages(&self.reservation_path(height), &bytes)?;
        files::clean_equivalent_stages(&self.intent_path(height), &bytes)?;
        self.slots.insert(
            height,
            Slot {
                record: record.clone(),
                committed: true,
                intent: false,
                reserved: false,
            },
        );
        record.digest()
    }
    fn verify_carriers(&self) -> Result<(), String> {
        common_storage::verify_lock_carrier(
            &self._format_lock,
            &self.root.join("bft-format.lock"),
        )?;
        common_storage::verify_lock_carrier(&self._lock, &self.dir.join("writer.lock"))?;
        if let Some(pin) = &self.pin
            && (pin.format_lock != common_storage::lock_carrier_identity(&self._format_lock)?
                || pin.writer_lock != common_storage::lock_carrier_identity(&self._lock)?)
        {
            return Err("formal prefix original lock carrier identity was replaced".into());
        }

        Ok(())
    }
    fn verify_material(
        &self,
        record: &Record,
        executed: &ExecutedFormalCandidateV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        store: &LocalCasStore,
    ) -> Result<(), String> {
        if common_storage::read_cas_bounded(
            store,
            &record.snapshot_ref,
            common_storage::MAX_BFT_SNAPSHOT_BYTES,
        )? != executed.snapshot_bytes
            || common_storage::read_cas_bounded(
                store,
                &record.journal_ref,
                common_storage::MAX_BFT_JOURNAL_BYTES,
            )? != executed.journal_bytes
        {
            return Err("formal prefix stored Snapshot/Journal differs from actual replay".into());
        }
        for (reference, expected) in nodes {
            if common_storage::read_cas_bounded(
                store,
                reference,
                formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            )? != *expected
            {
                return Err("formal prefix stored dependency differs from actual replay".into());
            }
        }
        Ok(())
    }
    #[cfg(test)]
    fn publication_fault(&self, height: u64, phase: PublicationFault) -> Result<(), String> {
        if self.fault != Some((height, phase)) {
            return Ok(());
        }
        if self.kill_on_fault {
            use std::io::Read;
            fs::write(
                self.root.join("prefix-test-publication-ready"),
                b"original publication ready",
            )
            .map_err(|e| e.to_string())?;
            let _ = std::io::stdin().read_exact(&mut [0u8]);
            // SAFETY: the dedicated test child terminates only itself.
            unsafe {
                libc::_exit(92);
            }
        }
        Err("injected original prefix publication interruption".into())
    }
    fn verify_pins(&self, record: &Record, store: &LocalCasStore) -> Result<(), String> {
        store
            .verify_pin_scope_shard(SCOPE, &self.shard(record.height)?, &record.references())
            .map_err(|e| {
                format!(
                    "formal prefix retention receipt missing or invalid at {}: {e:?}",
                    record.height
                )
            })
    }
    fn shard(&self, height: u64) -> Result<String, String> {
        Ok(format!(
            "origin-{}-height-{height:020}",
            self.origin_digest()?
        ))
    }
    fn record_path(&self, height: u64) -> PathBuf {
        self.dir.join(format!("record-{height:020}.cbor"))
    }
    fn reservation_path(&self, height: u64) -> PathBuf {
        self.dir.join(format!("reservation-{height:020}.cbor"))
    }
    fn intent_path(&self, height: u64) -> PathBuf {
        self.dir.join(format!("intent-{height:020}.cbor"))
    }
}

fn parse(name: &str, prefix: &str) -> Option<u64> {
    let number = name.strip_prefix(prefix)?.strip_suffix(".cbor")?;
    if number.len() != 20 || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}
fn digest(value: &impl Serialize) -> Result<String, String> {
    Ok(blake3_hex(
        &serde_cbor::to_vec(value).map_err(|e| e.to_string())?,
    ))
}
fn hash(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("formal prefix digest is not canonical lowercase BLAKE3 hex".into());
    }
    Ok(())
}

#[cfg(test)]
mod capacity_tests {
    use super::*;
    #[test]
    fn completed_origin_rejects_replaced_os_lock_carriers_on_fresh_open() {
        use std::os::unix::fs::OpenOptionsExt;
        struct Root(PathBuf);
        impl Drop for Root {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).unwrap();
        let root = Root(
            std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("oasis7-prefix-carrier-{}", hex::encode(random))),
        );
        let mut ledger = Ledger::open(&root.0, true, 0).unwrap();
        ledger.bind(&"a".repeat(64), 3).unwrap();
        drop(ledger);
        for relative in ["bft-format.lock", "formal-bft-prefix-v1/writer.lock"] {
            let carrier = root.0.join(relative);
            let token = fs::read(&carrier).unwrap();
            for changed in [vec![0x7a; 32], Vec::new()] {
                fs::write(&carrier, &changed).unwrap();
                assert!(
                    Ledger::open(&root.0, true, 0).is_err(),
                    "fresh open accepted changed/empty token on same inode: {relative}"
                );
                assert_eq!(
                    fs::read(&carrier).unwrap(),
                    changed,
                    "failed reopen must preserve original corrupted carrier bytes"
                );
                fs::write(&carrier, &token).unwrap();
                drop(Ledger::open(&root.0, false, 0).unwrap());
            }
            let original = carrier.with_extension("preserved-test-lock");
            fs::rename(&carrier, &original).unwrap();
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&carrier)
                .unwrap();
            assert!(
                Ledger::open(&root.0, true, 0).is_err(),
                "fresh open accepted replaced {relative}"
            );
            fs::remove_file(&carrier).unwrap();
            fs::rename(&original, &carrier).unwrap();
            drop(Ledger::open(&root.0, false, 0).unwrap());
        }
        // Even if this fence is the only remaining durable prefix identity,
        // a current engineering-mode attempt must preserve the damaged lock.
        fs::remove_file(root.0.join(DIRECTORY).join("origin.cbor")).unwrap();
        fs::remove_file(root.0.join(MARKER)).unwrap();
        assert_eq!(
            fs::read(root.0.join(LEGACY_MARKER)).unwrap(),
            DOWNGRADE_FENCE
        );
        let carrier = root.0.join("bft-format.lock");
        fs::write(&carrier, []).unwrap();
        assert!(common_storage::lock_format(&root.0).is_err());
        assert!(fs::read(&carrier).unwrap().is_empty());
        assert!(Ledger::open(&root.0, true, 0).is_err());
        assert!(fs::read(&carrier).unwrap().is_empty());
    }
    #[test]
    fn failed_material_reservation_cannot_make_a_retry_skip_the_limit() {
        let mut capacity = Capacity::default();
        capacity.charge(MAX_MATERIAL).unwrap();
        assert!(capacity.object("not-retained", 1).is_err());
        assert!(capacity.object("not-retained", 1).is_err());
        assert!(!capacity.objects.contains("not-retained"));
    }
}
