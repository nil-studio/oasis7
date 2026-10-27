//! Private crash-safe publication mechanics, shared by distinct typed ledger formats.
//! This layer does not verify consensus authority or grant a publication capability.
use super::super::super::storage as common_storage;
use super::formal_prefix_files as files;
use super::*;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub(super) const MAX_RECORD: usize = 8 * 1024 * 1024;
pub(super) const MAX_MATERIAL: usize = 512 * 1024 * 1024;
const LEGACY_MARKER: &str = "formal-bft-read-required-v1";
#[path = "driver_bft_v2_formal_prefix_init.rs"]
mod initialization;

pub(super) trait OriginContract: Clone + PartialEq + Serialize + DeserializeOwned {
    fn validate(&self) -> Result<(), String>;
    fn digest(&self) -> Result<String, String>;
    fn material_pin_digest(&self) -> &str;
    fn ledger_id(&self) -> &str;
    fn first_height(&self) -> u64;
    fn format_lock(&self) -> &common_storage::LockCarrierIdentity;
    fn writer_lock(&self) -> &common_storage::LockCarrierIdentity;
}
pub(super) trait RecordContract<O>:
    Clone + PartialEq + Serialize + DeserializeOwned
{
    fn validate(&self, origin: &O) -> Result<(), String>;
    fn digest(&self) -> Result<String, String>;
    fn height(&self) -> u64;
    fn origin_digest(&self) -> &str;
    fn parent_record_digest(&self) -> &str;
    fn snapshot_ref(&self) -> &str;
    fn journal_ref(&self) -> &str;
    fn business_index_refs(&self) -> &[String];
    fn references(&self) -> BTreeSet<String> {
        let mut refs: BTreeSet<_> = self.business_index_refs().iter().cloned().collect();
        refs.insert(self.snapshot_ref().into());
        refs.insert(self.journal_ref().into());
        refs
    }
}
pub(super) trait Format {
    type Origin: OriginContract;
    type Record: RecordContract<Self::Origin>;
    const MAX_HEIGHTS: usize = 64;
    const DIRECTORY: &'static str;
    const MARKER: &'static str;
    const MARKER_BYTES: &'static [u8];
    const DOWNGRADE_FENCE: &'static [u8];
    const SCOPE: &'static str;
    const OTHER_DIRECTORY: &'static str;
    const OTHER_MARKER: &'static str;
    fn new_origin(
        material_pin_digest: &str,
        first_height: u64,
        ledger_id: String,
        format_lock: common_storage::LockCarrierIdentity,
        writer_lock: common_storage::LockCarrierIdentity,
    ) -> Self::Origin;
}

#[derive(Clone)]
struct Slot<R> {
    record: R,
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
    fn record<O, R: RecordContract<O>>(&mut self, record: &R) -> Result<(), String> {
        let bytes = serde_cbor::to_vec(record).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_RECORD {
            return Err("formal prefix record exceeds byte capacity".into());
        }
        if !self.records.contains(&record.height()) {
            self.charge(bytes.len())?;
            self.records.insert(record.height());
        }
        Ok(())
    }
}

pub(super) struct GenericLedger<F: Format> {
    root: PathBuf,
    dir: PathBuf,
    _lock: File,
    _format_lock: File,
    pin: Option<F::Origin>,
    slots: BTreeMap<u64, Slot<F::Record>>,
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

impl<F: Format> GenericLedger<F> {
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
        let directory = match fs::symlink_metadata(root.join(F::DIRECTORY)) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
                true
            }
            Ok(_) => return Err("formal prefix ledger path is not a real directory".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.to_string()),
        };
        let fence = common_storage::read_optional_bytes(&root.join(LEGACY_MARKER), 128)?;
        if fence.as_deref() == Some(F::DOWNGRADE_FENCE) && !directory {
            return Err("formal prefix downgrade fence exists without original ledger".into());
        }
        let marker =
            common_storage::read_optional_bytes(&root.join(F::MARKER), F::MARKER_BYTES.len())?;
        if let Some(bytes) = marker.as_ref() {
            if bytes != F::MARKER_BYTES || !directory {
                return Err("formal prefix required marker has no valid ledger".into());
            }
            crate::controlled_live_config::require_private_owned(&root.join(F::MARKER), false)?;
        }
        Ok(directory || marker.is_some())
    }

    pub(super) fn open(root: &Path, initialize: bool, origin_bytes: usize) -> Result<Self, String> {
        if F::MAX_HEIGHTS == 0 || F::MAX_HEIGHTS > 64 {
            return Err("formal prefix unsupported height capacity".into());
        }
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
        for name in [F::OTHER_DIRECTORY, F::OTHER_MARKER] {
            match fs::symlink_metadata(root.join(name)) {
                Ok(_) => {
                    return Err(
                        "formal prefix format refuses another typed ledger namespace".into(),
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        let format_lock = common_storage::lock_format(root)?;
        for name in [F::OTHER_DIRECTORY, F::OTHER_MARKER] {
            match fs::symlink_metadata(root.join(name)) {
                Ok(_) => {
                    return Err(
                        "formal prefix format refuses another typed ledger namespace".into(),
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }

        let fence = common_storage::read_optional_bytes(&root.join(LEGACY_MARKER), 128)?;
        let legacy_directory = match fs::symlink_metadata(root.join("formal-bft-read-v1")) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.to_string()),
        };
        if legacy_directory || (fence.is_some() && fence.as_deref() != Some(F::DOWNGRADE_FENCE)) {
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
        let dir = root.join(F::DIRECTORY);
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
            if count > 3 * F::MAX_HEIGHTS + 4 {
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
        if heights.len() > F::MAX_HEIGHTS {
            return Err("formal prefix height capacity exceeded".into());
        }
        if (self.pin.is_none() || self.init_pending) && !heights.is_empty() {
            return Err("formal prefix records exist without original origin".into());
        }
        let mut metadata_bytes = self.origin_bytes;
        let mut slots = BTreeMap::new();
        let mut previous = self.pin.as_ref().map(OriginContract::digest).transpose()?;
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
            let record: Option<F::Record> =
                common_storage::read_optional_canonical(&record_path, maximum)?;
            let intent: Option<F::Record> =
                common_storage::read_optional_canonical(&intent_path, maximum)?;
            let staged_intent: Option<F::Record> = files::staged_value(&intent_path, maximum)?;
            let canonical_reservation: Option<F::Record> =
                common_storage::read_optional_canonical(&reservation_path, maximum)?;
            let staged_reservation: Option<F::Record> =
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
            if value.origin_digest() != pin.digest()? {
                return Err("formal prefix record origin identity mismatch".into());
            }
            let expected_height = pin
                .first_height()
                .checked_add(slots.len() as u64)
                .ok_or("formal prefix height overflow")?;
            if height != expected_height
                || value.height() != height
                || previous.as_deref() != Some(value.parent_record_digest())
            {
                return Err(
                    "formal prefix immutable history has a gap or different parent digest".into(),
                );
            }
            if slots
                .values()
                .any(|slot: &Slot<F::Record>| slot.intent || slot.reserved || !slot.committed)
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
            if common_storage::read_optional_bytes(
                &self.root.join(F::MARKER),
                F::MARKER_BYTES.len(),
            )?
            .as_deref()
                != Some(F::MARKER_BYTES)
                || common_storage::read_optional_bytes(&self.root.join(LEGACY_MARKER), 128)?
                    .as_deref()
                    != Some(F::DOWNGRADE_FENCE)
            {
                return Err("formal prefix original profile/downgrade marker disappeared".into());
            }
            files::single_link(&self.root.join(F::MARKER))?;
            files::single_link(&self.root.join(LEGACY_MARKER))?;
        }
        self.slots = slots;
        Ok(())
    }

    pub(super) fn bind_retained(
        &mut self,
        material_pin_digest: &str,
        first_height: u64,
        retain: impl FnOnce(&F::Origin, bool) -> Result<(), String>,
    ) -> Result<(), String> {
        self.bind_origin(material_pin_digest, first_height, retain)
    }

    pub(super) fn bind(
        &mut self,
        material_pin_digest: &str,
        first_height: u64,
    ) -> Result<(), String> {
        self.bind_origin(material_pin_digest, first_height, |_, _| Ok(()))
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
            .list_pin_scope_shards(F::SCOPE)
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
            self.capacity.record::<F::Origin, F::Record>(&slot.record)?;
            let record = &slot.record;
            if slot.reserved
                && !store
                    .list_pin_scope_shards(F::SCOPE)
                    .map_err(|e| format!("list prefix pins: {e:?}"))?
                    .contains(&self.shard(record.height())?)
            {
                continue;
            }
            self.verify_pins(record, store)?;
            if through.is_none_or(|height| record.height() <= height) {
                visible.extend(record.business_index_refs().iter().cloned());
            }
            for (reference, limit) in [
                (
                    record.snapshot_ref(),
                    common_storage::MAX_BFT_SNAPSHOT_BYTES,
                ),
                (record.journal_ref(), common_storage::MAX_BFT_JOURNAL_BYTES),
            ] {
                let maximum = self.capacity.read_bound(reference, limit)?;
                let bytes = common_storage::read_cas_bounded(store, reference, maximum)?;
                self.capacity.object(reference, bytes.len())?;
            }
            references.extend(record.business_index_refs().iter().cloned());
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

    pub(super) fn origin(&self) -> Result<&F::Origin, String> {
        self.pin
            .as_ref()
            .ok_or_else(|| "formal prefix origin missing".into())
    }
    pub(super) fn record_at(&self, height: u64) -> Option<&F::Record> {
        self.slots.get(&height).map(|slot| &slot.record)
    }
    pub(super) fn records(&self) -> impl Iterator<Item = &F::Record> {
        self.slots.values().map(|slot| &slot.record)
    }
    pub(super) fn records_through(
        &self,
        height: u64,
    ) -> Result<impl Iterator<Item = &F::Record>, String> {
        if !self.slots.contains_key(&height) {
            return Err("formal prefix exact target has not been installed; original prefix input is required".into());
        }
        Ok(self.slots.range(..=height).map(|(_, slot)| &slot.record))
    }
    pub(super) fn retained_business_refs(
        &self,
        height: u64,
        nodes: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Vec<String>, String> {
        let observed: BTreeSet<_> = nodes.keys().cloned().collect();
        if let Some(slot) = self.slots.get(&height) {
            let retained: BTreeSet<_> = slot.record.business_index_refs().iter().cloned().collect();
            if !observed.is_subset(&retained) {
                return Err("formal prefix replay consumed an unretained dependency".into());
            }
            Ok(slot.record.business_index_refs().to_vec())
        } else {
            Ok(nodes.keys().cloned().collect())
        }
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
        Ok(self
            .pin
            .as_ref()
            .ok_or("formal prefix origin missing")?
            .material_pin_digest())
    }
    pub(super) fn origin_digest(&self) -> Result<String, String> {
        self.origin()?.digest()
    }

    pub(super) fn accept_prepared(
        &mut self,
        record: F::Record,
        snapshot_bytes: &[u8],
        journal_bytes: &[u8],
        nodes: &BTreeMap<String, Vec<u8>>,
        store: &LocalCasStore,
        append: bool,
    ) -> Result<String, String> {
        self.verify_carriers()?;
        formal_dependencies::validate_nodes(nodes)?;
        let height = record.height();
        let existing = self.slots.get(&height).cloned();
        if existing.is_none() && !append {
            return Err("formal prefix verification cannot append a new height".into());
        }
        if record.snapshot_ref() != blake3_hex(snapshot_bytes)
            || record.journal_ref() != blake3_hex(journal_bytes)
        {
            return Err("formal prefix prepared material digest mismatch".into());
        }
        if record.business_index_refs() != self.retained_business_refs(height, nodes)? {
            return Err("formal prefix prepared dependency identity mismatch".into());
        }
        record.validate(self.origin()?)?;
        if record.origin_digest() != self.origin_digest()? {
            return Err("formal prefix prepared record origin identity mismatch".into());
        }
        self.capacity.record::<F::Origin, F::Record>(&record)?;
        self.capacity
            .object(record.snapshot_ref(), snapshot_bytes.len())?;
        self.capacity
            .object(record.journal_ref(), journal_bytes.len())?;
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
            if existing.is_none() && self.slots.len() >= F::MAX_HEIGHTS {
                return Err("formal prefix height capacity exceeded".into());
            }
            // Fix the original full QC before any externally retained material.
            files::publish(&self.reservation_path(height), &bytes)?;
            let shard = self.shard(height)?;
            let has_pins = store
                .list_pin_scope_shards(F::SCOPE)
                .map_err(|e| format!("list prefix pins: {e:?}"))?
                .contains(&shard);
            if has_pins {
                self.verify_pins(&record, store)?;
                self.verify_material(&record, snapshot_bytes, journal_bytes, nodes, store)?;
            } else {
                common_storage::put_cas_durable(
                    store,
                    snapshot_bytes,
                    common_storage::MAX_BFT_SNAPSHOT_BYTES,
                )?;
                common_storage::put_cas_durable(
                    store,
                    journal_bytes,
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
                    .replace_pin_scope_shard(F::SCOPE, &shard, &record.references())
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
            self.verify_material(&record, snapshot_bytes, journal_bytes, nodes, store)?;
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
    pub(super) fn verify_carriers(&self) -> Result<(), String> {
        common_storage::verify_lock_carrier(
            &self._format_lock,
            &self.root.join("bft-format.lock"),
        )?;
        common_storage::verify_lock_carrier(&self._lock, &self.dir.join("writer.lock"))?;
        if let Some(pin) = &self.pin {
            canonical_hash(pin.ledger_id())?;
        }
        if let Some(pin) = &self.pin
            && (pin.format_lock() != &common_storage::lock_carrier_identity(&self._format_lock)?
                || pin.writer_lock() != &common_storage::lock_carrier_identity(&self._lock)?)
        {
            return Err("formal prefix original lock carrier identity was replaced".into());
        }

        Ok(())
    }
    fn verify_material(
        &self,
        record: &F::Record,
        snapshot_bytes: &[u8],
        journal_bytes: &[u8],
        nodes: &BTreeMap<String, Vec<u8>>,
        store: &LocalCasStore,
    ) -> Result<(), String> {
        if common_storage::read_cas_bounded(
            store,
            record.snapshot_ref(),
            common_storage::MAX_BFT_SNAPSHOT_BYTES,
        )? != snapshot_bytes
            || common_storage::read_cas_bounded(
                store,
                record.journal_ref(),
                common_storage::MAX_BFT_JOURNAL_BYTES,
            )? != journal_bytes
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
    fn verify_pins(&self, record: &F::Record, store: &LocalCasStore) -> Result<(), String> {
        store
            .verify_pin_scope_shard(
                F::SCOPE,
                &self.shard(record.height())?,
                &record.references(),
            )
            .map_err(|e| {
                format!(
                    "formal prefix retention receipt missing or invalid at {}: {e:?}",
                    record.height()
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

fn canonical_hash(value: &str) -> Result<(), String> {
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
    fn failed_material_reservation_cannot_make_a_retry_skip_the_limit() {
        let mut capacity = Capacity::default();
        capacity.charge(MAX_MATERIAL).unwrap();
        assert!(capacity.object("not-retained", 1).is_err());
        assert!(capacity.object("not-retained", 1).is_err());
        assert!(!capacity.objects.contains("not-retained"));
    }
}
