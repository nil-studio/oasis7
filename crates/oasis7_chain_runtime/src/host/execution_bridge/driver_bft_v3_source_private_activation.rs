//! Original covered H private namespace publication. The protected lease
//! supplies the original empty activation before any local WAL is written.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionActivationV1, BftPrivateRetentionAttestorV1,
    BftPrivateRetentionCreationMarkerV1, BftSignerCheckpointV3,
};
use std::fs::File;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::time::Instant;

const ACTIVATION_DOMAIN: &str = "oasis7.formal.bft.private-source.activation.v1";
pub(super) const MAX_ACTIVATION_BYTES: usize = 64 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryIdentity {
    device: u64,
    inode: u64,
}

impl DirectoryIdentity {
    fn from_descriptor(file: &File) -> Result<Self, String> {
        let metadata = file
            .metadata()
            .map_err(|_| "inspect private directory descriptor")?;
        if !metadata.is_dir() || metadata.ino() == 0 {
            return Err("covered private directory identity invalid".into());
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    fn open(path: &Path) -> Result<Self, String> {
        crate::controlled_live_config::require_private_owned(path, true)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)
            .map_err(|_| "open covered private source directory")?;
        Self::from_descriptor(&file)
    }
}

/// Exact original schema-8 activation carrier.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceActivationBindingV1 {
    pub(super) reference: String,
    pub(super) encoded_bytes: u64,
    directory: DirectoryIdentity,
}

impl SourceActivationBindingV1 {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.directory.inode == 0
            || !canonical_hash(&self.reference)
            || self.encoded_bytes == 0
            || self.encoded_bytes > MAX_ACTIVATION_BYTES as u64
        {
            return Err("covered private Origin activation carrier invalid".into());
        }
        Ok(())
    }

    fn accounted_bytes(&self) -> Result<usize, String> {
        let pin = serde_json::to_vec(&serde_json::json!({"pins": [&self.reference]}))
            .map_err(|_| "encode original activation pin bound")?
            .len();
        (self.encoded_bytes as usize)
            .checked_mul(4)
            .and_then(|n| n.checked_add(2 * pin))
            .ok_or("original activation publication bound overflow".into())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct SourceActivationRecordV1 {
    schema_version: u32,
    domain: String,
    marker: BftPrivateRetentionCreationMarkerV1,
    capability_digest: String,
    original_empty_coverage: BftPrivateRetentionActivationV1,
}

// The exact authenticated checkpoint prevents replaying a valid old activation
// after the source has already admitted a candidate.
impl SourceActivationRecordV1 {
    pub(in super::super) fn from_lease(
        binding: &BftSignerBindingV3,
        fixed_attestor: &BftPrivateRetentionAttestorV1,
        marker: &BftPrivateRetentionCreationMarkerV1,
        activation: BftPrivateRetentionActivationV1,
        checkpoint: &BftSignerCheckpointV3,
    ) -> Result<Self, String> {
        if checkpoint.provider_public_key != fixed_attestor.provider_public_key {
            return Err("covered source provider differs from independent attestor".into());
        }
        activation
            .verify(
                binding,
                fixed_attestor,
                marker,
                &checkpoint.state,
                checkpoint.lease_generation,
            )
            .map_err(|error| error.to_string())?;
        Ok(Self {
            schema_version: 1,
            domain: ACTIVATION_DOMAIN.into(),
            marker: marker.clone(),
            capability_digest: fixed_attestor.digest().map_err(|error| error.to_string())?,
            original_empty_coverage: activation,
        })
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        let bytes = serde_cbor::to_vec(self).map_err(|_| "encode original source activation")?;
        if bytes.len() > MAX_ACTIVATION_BYTES {
            return Err("original source activation byte capacity".into());
        }
        Ok(bytes)
    }
}

pub(in super::super) struct CoveredSourcePublication<'a> {
    pub(in super::super) store: &'a LocalCasStore,
    pub(in super::super) cursor_height: u64,
    pub(in super::super) nodes: &'a BTreeMap<String, Vec<u8>>,
    pub(in super::super) activation: &'a SourceActivationRecordV1,
    pub(in super::super) deadline: Instant,
}

impl CoveredSourcePublication<'_> {
    pub(in super::super) fn publish(self, ledger: &mut storage::Ledger) -> Result<String, String> {
        let dir = ledger.directory().join(storage::NODE_PRIVATE_DIRECTORY);
        let live = || -> Result<(), String> {
            if Instant::now() >= self.deadline {
                return Err("original source activation expired".into());
            }
            Ok(())
        };
        live()?;
        ledger.verify_carriers()?;
        let marker = &self.activation.marker;
        if self.cursor_height.checked_add(1) != Some(marker.initial_height) {
            return Err("covered source must initialize at original H".into());
        }
        let path = dir.join("origin.cbor");
        let existing = preflight(&dir, self.store, marker, None)?;
        ensure_private_directory(&dir)?;
        let directory = DirectoryIdentity::open(&dir)?;
        let guarded = || -> Result<(), String> {
            live()?;
            if DirectoryIdentity::open(&dir)? != directory {
                return Err("covered original private directory changed".into());
            }
            Ok(())
        };
        guarded()?;
        let bytes = self.activation.bytes()?;
        let activation = SourceActivationBindingV1 {
            reference: blake3_hex(&bytes),
            encoded_bytes: bytes.len() as u64,
            directory: directory.clone(),
        };
        let data = Data {
            schema_version: 8,
            domain: COVERED_SOURCE_ORIGIN_DOMAIN.into(),
            origin: true,
            ledger_origin_digest: marker.ledger_origin_digest.clone(),
            material_identity: marker.source.material_source_digest.clone(),
            instance_id: marker.creation_identity.clone(),
            height: self.cursor_height,
            value: None,
            actions_cbor: Vec::new(),
            references: self.nodes.keys().cloned().collect(),
            publication_reservation_ref: None,
            source_activation: Some(activation.clone()),
            protected_source_origin_ref: None,
        };
        if existing.as_ref().is_some_and(|slot| slot.data != data) {
            return Err("existing covered source differs from original H/receipt bytes".into());
        }
        let mut usage = data
            .bytes()?
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(activation.accounted_bytes().ok()?))
            .ok_or("covered source origin publication capacity")?;
        for bytes in self.nodes.values() {
            usage = usage
                .checked_add(bytes.len())
                .ok_or("covered source H inventory capacity")?;
        }
        ledger.verify_carriers()?;
        ledger.reserve_external_material(usage)?;
        let guarded = || -> Result<(), String> {
            guarded()?;
            ledger.verify_carriers()?;
            #[cfg(test)]
            if path.with_extension("reservation.cbor").is_file()
                && ABORT_AFTER_RESERVATION.with(|abort| abort.replace(false))
            {
                return Err("injected cut after covered original reservation".into());
            }
            Ok(())
        };
        guarded()?;
        let complete = existing.as_ref().is_some_and(|slot| slot.complete);
        let retained = retention_required(&path)?
            || self
                .store
                .list_pin_scope_shards(SCOPE)
                .map_err(|_| "inspect covered source activation pin")?
                .contains(&data.shard());
        if retained {
            verify_material(self.store, &data)?;
        } else {
            common_storage::put_cas_durable(self.store, &bytes, MAX_ACTIVATION_BYTES)?;
        }
        publish_checked(
            self.store, &path, &data, self.nodes, complete, true, &guarded,
        )?;
        verify_original_slot(
            self.store,
            &path,
            &Slot {
                data: data.clone(),
                complete: true,
            },
        )?;
        if common_storage::read_optional_bytes(&dir.join("ready"), MARKER.len())?.as_deref()
            != Some(MARKER)
        {
            return Err("completed covered origin lost its original marker".into());
        }
        guarded()?;
        data.digest()
    }
}

pub(in super::super) fn check_original_namespace(
    ledger: &storage::Ledger,
    store: &LocalCasStore,
    marker: &BftPrivateRetentionCreationMarkerV1,
) -> Result<(), String> {
    let dir = ledger.directory().join(storage::NODE_PRIVATE_DIRECTORY);
    preflight(&dir, store, marker, None).map(|_| ())
}

fn preflight(
    dir: &Path,
    store: &LocalCasStore,
    marker: &BftPrivateRetentionCreationMarkerV1,
    pending_basename: Option<&str>,
) -> Result<Option<Slot>, String> {
    let path = dir.join("origin.cbor");
    let existing = read_slot(&path)?;
    if existing
        .as_ref()
        .is_some_and(|slot| slot.data.schema_version != 8)
    {
        return Err("covered source cannot replace an original uncovered Node".into());
    }
    if existing.is_none() {
        let shard_prefix = format!("ledger-{}-", marker.ledger_origin_digest);
        if store
            .list_pin_scope_shards(SCOPE)
            .map_err(|_| "inspect covered source pins")?
            .iter()
            .any(|shard| shard.starts_with(&shard_prefix))
        {
            return Err("covered origin WAL disappeared while original pins remain".into());
        }
    }
    check_original_files(dir, existing.is_some(), pending_basename)?;
    if let Some(slot) = &existing {
        let activation = slot
            .data
            .source_activation
            .as_ref()
            .ok_or("original activation carrier missing")?;
        if DirectoryIdentity::open(dir)? != activation.directory
            || slot.data.instance_id != marker.creation_identity
            || slot.data.ledger_origin_digest != marker.ledger_origin_digest
            || slot.data.material_identity != marker.source.material_source_digest
            || slot.data.height.checked_add(1) != Some(marker.initial_height)
        {
            return Err("covered original local namespace identity changed".into());
        }
        if retention_required(&path)? {
            verify_original_slot(store, &path, slot)?;
        }
    }
    Ok(existing)
}

fn check_original_files(
    dir: &Path,
    has_origin: bool,
    pending_basename: Option<&str>,
) -> Result<(), String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("inspect covered source namespace".into()),
    };
    let bare = pending_basename;
    for (count, entry) in entries.enumerate() {
        let entry = entry.map_err(|_| "read covered source namespace")?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "covered source filename")?;
        let path = files::stage_target(&name).unwrap_or(&name);
        let is_origin = matches!(
            path,
            "origin.cbor" | "origin.intent.cbor" | "origin.reservation.cbor" | "ready"
        );
        let is_pending = bare
            .and_then(|basename| path.strip_prefix(basename))
            .is_some_and(|suffix| matches!(suffix, ".cbor" | ".intent.cbor" | ".reservation.cbor"));
        if count >= if bare.is_some() { 14 } else { 8 }
            || !has_origin
            || !entry
                .file_type()
                .map_err(|_| "covered source file type")?
                .is_file()
            || (!is_origin && !is_pending)
        {
            return Err("covered source initialization requires only original H files".into());
        }
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    static ABORT_AFTER_RESERVATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(in super::super) fn inject_reservation_cut() {
    ABORT_AFTER_RESERVATION.with(|abort| abort.set(true));
}

pub(in super::super) struct AuthenticatedOriginalV1 {
    directory: PathBuf,
    data: Data,
    pub(in super::super) activation: BftPrivateRetentionActivationV1,
}

impl AuthenticatedOriginalV1 {
    pub(in super::super) fn inventory(&self) -> Result<(String, BTreeSet<String>), String> {
        Ok((
            self.data.digest()?,
            self.data.references.iter().cloned().collect(),
        ))
    }

    pub(in super::super) fn archive_material(
        &self,
        store: &LocalCasStore,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        let carrier = self
            .data
            .source_activation
            .as_ref()
            .ok_or("covered original activation carrier absent")?;
        let path = self.directory.join("origin.cbor");
        verify_original_slot(
            store,
            &path,
            &Slot {
                data: self.data.clone(),
                complete: true,
            },
        )?;
        Ok((
            self.data.bytes()?,
            common_storage::read_cas_bounded(store, &carrier.reference, MAX_ACTIVATION_BYTES)?,
        ))
    }
}

/// Authenticate the preserved original bytes without reopening the live Node
/// directory. The directory identity remains part of the original Data hash;
/// it is not treated as an authorization for the independent archive location.
pub(in super::super) fn verify_archived_original(
    original_bytes: &[u8],
    capsule_bytes: &[u8],
    binding: &BftSignerBindingV3,
    attestor: &BftPrivateRetentionAttestorV1,
    marker: &BftPrivateRetentionCreationMarkerV1,
) -> Result<(BTreeSet<String>, BftPrivateRetentionActivationV1), String> {
    if original_bytes.len() > MAX_RECORD || capsule_bytes.len() > MAX_ACTIVATION_BYTES {
        return Err("archived original source capacity".into());
    }
    let data: Data = serde_cbor::from_slice(original_bytes)
        .map_err(|_| "decode archived original private Origin")?;
    let carrier = data
        .source_activation
        .as_ref()
        .ok_or("archived original activation carrier absent")?;
    if data.schema_version != 8
        || !data.origin
        || data.bytes()? != original_bytes
        || data.instance_id != marker.creation_identity
        || data.ledger_origin_digest != marker.ledger_origin_digest
        || data.material_identity != marker.source.material_source_digest
        || data.height.checked_add(1) != Some(marker.initial_height)
        || carrier.reference != blake3_hex(capsule_bytes)
        || carrier.encoded_bytes != capsule_bytes.len() as u64
    {
        return Err("archived original source differs from independent marker".into());
    }
    let record: SourceActivationRecordV1 = serde_cbor::from_slice(capsule_bytes)
        .map_err(|_| "decode archived original activation capsule")?;
    if record.schema_version != 1
        || record.domain != ACTIVATION_DOMAIN
        || &record.marker != marker
        || record.capability_digest != attestor.digest().map_err(|error| error.to_string())?
        || record.bytes()? != capsule_bytes
    {
        return Err("archived activation differs from independent source authority".into());
    }
    let activation = &record.original_empty_coverage;
    let body = &activation.receipt.body;
    let mut state = oasis7_distfs::protected_bft_v3::BftSignerStateV3::from_handoff(binding)
        .map_err(|error| error.to_string())?;
    state.revision = body.after_revision;
    state.state_digest = body.after_state_digest.clone();
    state.private_retention = Some(body.head.clone());
    activation
        .verify(
            binding,
            attestor,
            marker,
            &state,
            body.original_lease_generation,
        )
        .map_err(|error| error.to_string())?;
    Ok((
        data.references.into_iter().collect(),
        record.original_empty_coverage,
    ))
}

pub(in super::super) fn read_authenticated_original(
    ledger: &storage::Ledger,
    store: &LocalCasStore,
    binding: &BftSignerBindingV3,
    attestor: &BftPrivateRetentionAttestorV1,
    marker: &BftPrivateRetentionCreationMarkerV1,
    pending_basename: Option<&str>,
) -> Result<AuthenticatedOriginalV1, String> {
    let directory = ledger.directory().join(storage::NODE_PRIVATE_DIRECTORY);
    let path = directory.join("origin.cbor");
    ledger.verify_carriers()?;
    let original = preflight(&directory, store, marker, pending_basename)?
        .filter(|slot| slot.complete)
        .ok_or("complete authenticated original private Origin required")?;
    if common_storage::read_optional_bytes(&directory.join("ready"), MARKER.len())?.as_deref()
        != Some(MARKER)
    {
        return Err("covered Origin has no original ready marker".into());
    }
    verify_original_slot(store, &path, &original)?;
    let carrier = original
        .data
        .source_activation
        .as_ref()
        .ok_or("covered activation carrier absent")?;
    let bytes = common_storage::read_cas_bounded(store, &carrier.reference, MAX_ACTIVATION_BYTES)?;
    let record: SourceActivationRecordV1 = serde_cbor::from_slice(&bytes)
        .map_err(|_| "decode original protected activation capsule")?;
    if record.schema_version != 1
        || record.domain != ACTIVATION_DOMAIN
        || &record.marker != marker
        || record.capability_digest != attestor.digest().map_err(|error| error.to_string())?
        || record.bytes()? != bytes
    {
        return Err("original activation capsule differs from pinned source".into());
    }
    // Validate the signed historical transition in its own lease generation.
    // Staging a byte archive does not establish a current read or signing lease.
    let activation = &record.original_empty_coverage;
    let body = &activation.receipt.body;
    let mut original_state =
        oasis7_distfs::protected_bft_v3::BftSignerStateV3::from_handoff(binding)
            .map_err(|error| error.to_string())?;
    original_state.revision = body.after_revision;
    original_state.state_digest = body.after_state_digest.clone();
    original_state.private_retention = Some(body.head.clone());
    activation
        .verify(
            binding,
            attestor,
            marker,
            &original_state,
            body.original_lease_generation,
        )
        .map_err(|error| error.to_string())?;
    if DirectoryIdentity::open(&directory)? != carrier.directory {
        return Err("original private directory changed during activation verification".into());
    }
    ledger.verify_carriers()?;
    Ok(AuthenticatedOriginalV1 {
        directory,
        data: original.data,
        activation: record.original_empty_coverage,
    })
}

fn original_candidate_basename(data: &Data) -> Result<String, String> {
    match data.schema_version {
        9 => Ok(format!("candidate-{:020}", data.height)),
        10 => Ok(format!(
            "candidate-{:020}-{}",
            data.height,
            data.value
                .as_ref()
                .ok_or("original Received value absent")?
                .block_id()?
        )),
        _ => Err("first Pending requires original Local or Received Data".into()),
    }
}

pub(in super::super) fn pending_original_basename(
    input: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputV1,
) -> Result<String, String> {
    use oasis7_distfs::protected_bft_v3::BftPrivateRetentionInputObjectKindV1 as Kind;
    input.validate().map_err(|error| error.to_string())?;
    let object = input
        .objects
        .iter()
        .find(|object| object.kind == Kind::OriginalData)
        .ok_or("original Pending Data absent")?;
    let data: Data =
        serde_cbor::from_slice(&object.bytes).map_err(|_| "decode original Pending Data")?;
    if data.bytes()? != object.bytes {
        return Err("original Pending Data is not canonical".into());
    }
    original_candidate_basename(&data)
}

pub(in super::super) struct PreparedPendingOriginal<'a> {
    original: &'a AuthenticatedOriginalV1,
    data: Data,
    metadata: &'a oasis7_distfs::protected_bft_v3::BftPrivateRetentionInputObjectV1,
    dependencies: BTreeMap<String, Vec<u8>>,
    existing: Option<Slot>,
}

impl AuthenticatedOriginalV1 {
    pub(in super::super) fn prepare_pending_original<'a>(
        &'a self,
        input: &'a oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputV1,
        ledger: &mut storage::Ledger,
        store: &LocalCasStore,
    ) -> Result<PreparedPendingOriginal<'a>, String> {
        use oasis7_distfs::protected_bft_v3::BftPrivateRetentionInputObjectKindV1 as Kind;

        input.validate().map_err(|error| error.to_string())?;
        let data = input
            .objects
            .iter()
            .find(|object| object.kind == Kind::OriginalData)
            .ok_or("first Pending original candidate Data absent")?;
        let metadata = input
            .objects
            .iter()
            .find(|object| object.kind == Kind::PublicationMetadata)
            .ok_or("first Pending original publication metadata absent")?;
        let dependencies: BTreeMap<_, _> = input
            .objects
            .iter()
            .filter(|object| object.kind == Kind::Dependency)
            .map(|object| (object.reference.digest.clone(), object.bytes.clone()))
            .collect();
        let original_hash = self.data.digest()?;
        let candidate: Data = serde_cbor::from_slice(&data.bytes)
            .map_err(|_| "read exact original Local/Received WAL bytes")?;
        if !matches!(candidate.schema_version, 9 | 10)
            || candidate.bytes()? != data.bytes
            || candidate.ledger_origin_digest != self.data.ledger_origin_digest
            || candidate.material_identity != self.data.material_identity
            || candidate.instance_id != self.data.instance_id
            || self.data.height.checked_add(1) != Some(candidate.height)
            || candidate.height != input.key.height
            || input.key.ledger_origin_digest != candidate.ledger_origin_digest
            || candidate.protected_source_origin_ref.as_ref() != Some(&original_hash)
            || candidate.publication_reservation_ref.as_ref() != Some(&metadata.reference.digest)
            || candidate.references.iter().ne(dependencies.keys())
            || candidate
                .value
                .as_ref()
                .ok_or("original candidate public value absent")?
                .block_id()?
                != input.key.header_id
        {
            return Err("first Pending WAL differs from protected original input".into());
        }
        let path = self
            .directory
            .join(format!("{}.cbor", original_candidate_basename(&candidate)?));
        let existing = read_slot(&path)?;
        if existing.as_ref().is_some_and(|slot| slot.data != candidate) {
            return Err("original first candidate WAL names a different input".into());
        }
        let known_shards = BTreeSet::from([self.data.shard(), candidate.shard()]);
        let shard_prefix = format!("ledger-{}-", candidate.ledger_origin_digest);
        for shard in store
            .list_pin_scope_shards(SCOPE)
            .map_err(|_| "inspect covered original candidate pins")?
        {
            if shard.starts_with(&shard_prefix)
                && (!known_shards.contains(&shard)
                    || (shard == candidate.shard() && existing.is_none()))
            {
                return Err("covered original WAL/pin inventory changed".into());
            }
        }
        if let Some(slot) = &existing {
            verify_original_slot(store, &path, slot)?;
        }

        let source = self
            .data
            .source_activation
            .as_ref()
            .ok_or("original coverage capsule absent")?;
        let price = usize::try_from(input.publication_reservation_bytes)
            .map_err(|_| "first Pending original publication budget conversion")?;
        let mut use_bytes = self
            .data
            .bytes()?
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(source.accounted_bytes().ok()?))
            .and_then(|n| n.checked_add(price))
            .ok_or("first Pending original budget overflow")?;
        for hash in &self.data.references {
            let bytes = common_storage::read_cas_bounded(
                store,
                hash,
                formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            )?;
            use_bytes = use_bytes
                .checked_add(bytes.len())
                .ok_or("first Pending H usage overflow")?;
        }
        ledger.verify_carriers()?;
        ledger.reserve_external_material(use_bytes)?;
        Ok(PreparedPendingOriginal {
            original: self,
            data: candidate,
            metadata,
            dependencies,
            existing,
        })
    }
}

impl PreparedPendingOriginal<'_> {
    pub(in super::super) fn is_received(&self) -> bool {
        self.data.schema_version == 10
    }

    /// Called only after the actual remote lease validates the original
    /// Pending receipt and its exact authenticated checkpoint.
    pub(in super::super) fn publish(
        self,
        ledger: &storage::Ledger,
        store: &LocalCasStore,
        deadline: Instant,
    ) -> Result<String, String> {
        let path = self
            .original
            .directory
            .join(format!("{}.cbor", original_candidate_basename(&self.data)?));
        let carrier = self
            .original
            .data
            .source_activation
            .as_ref()
            .ok_or("covered Origin activation absent")?;
        let guard = || -> Result<(), String> {
            if Instant::now() >= deadline {
                return Err("first Pending original publication deadline expired".into());
            }
            ledger.verify_carriers()?;
            if DirectoryIdentity::open(&self.original.directory)? != carrier.directory {
                return Err("covered private Original changed during Pending publication".into());
            }
            #[cfg(test)]
            if path.with_extension("reservation.cbor").is_file()
                && ABORT_AFTER_RESERVATION.with(|abort| abort.replace(false))
            {
                return Err("injected cut after covered original reservation".into());
            }
            Ok(())
        };
        guard()?;
        let retained = retention_required(&path)?
            || store
                .list_pin_scope_shards(SCOPE)
                .map_err(|_| "inspect first Pending original pins")?
                .contains(&self.data.shard());
        if retained {
            verify_material(store, &self.data)?;
        } else {
            common_storage::put_cas_durable(store, &self.metadata.bytes, 16 * 1024)?;
        }
        publish_checked(
            store,
            &path,
            &self.data,
            &self.dependencies,
            self.existing.as_ref().is_some_and(|slot| slot.complete),
            false,
            &guard,
        )?;
        verify_original_slot(
            store,
            &path,
            &Slot {
                data: self.data.clone(),
                complete: true,
            },
        )?;
        guard()?;
        self.data.digest()
    }
}
