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
        let live = || {
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
        let existing = preflight(&dir, self.store, marker)?;
        ensure_private_directory(&dir)?;
        let directory = DirectoryIdentity::open(&dir)?;
        let guarded = || {
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
            source_activation: Some(activation.clone()),
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
        let guarded = || {
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
    preflight(&dir, store, marker).map(|_| ())
}

fn preflight(
    dir: &Path,
    store: &LocalCasStore,
    marker: &BftPrivateRetentionCreationMarkerV1,
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
    check_original_files(dir, existing.is_some())?;
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

fn check_original_files(dir: &Path, has_origin: bool) -> Result<(), String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("inspect covered source namespace".into()),
    };
    for (count, entry) in entries.enumerate() {
        let entry = entry.map_err(|_| "read covered source namespace")?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "covered source filename")?;
        if count >= 8
            || !has_origin
            || !entry
                .file_type()
                .map_err(|_| "covered source file type")?
                .is_file()
            || !matches!(
                files::stage_target(&name).unwrap_or(&name),
                "origin.cbor" | "origin.intent.cbor" | "origin.reservation.cbor" | "ready"
            )
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
