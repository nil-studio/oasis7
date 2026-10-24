use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use libp2p::{PeerId, identity::Keypair};
use oasis7_proto::distributed_dht::{
    PeerRecord, PeerRecordPublicationStateV2, PeerRecordPublicationV2, SignedPeerRecord,
};
use serde::{Deserialize, Serialize};

use crate::error::WorldError;

use super::peer_record::{sign_peer_record_v2, verify_signed_peer_record};

#[path = "durable_peer_record_publisher/initialization.rs"]
mod initialization;

const STATE_SCHEMA_VERSION: u16 = 2;
const STATE_FILE_NAME: &str = "peer-record-publication-v2.cbor";
const TEMP_FILE_NAME: &str = "peer-record-publication-v2.cbor.tmp";
const LOCK_FILE_NAME: &str = "peer-record-publication-v2.lock";
const LOCK_INITIALIZED_MARKER: &[u8] = b"oasis7-peer-publication-v2-initialized\n";
const MAX_PERSISTED_STATE_BYTES: usize = 64 * 1024;
const MAX_RECORD_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublisherBindingV2 {
    schema_version: u16,
    peer_id: String,
    world_id: String,
    network_id: String,
    identity_public_key_protobuf_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingPublicationV2 {
    sequence: u64,
    materialized_record_cbor: Vec<u8>,
    signed_record_cbor: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublisherStateV2 {
    schema_version: u16,
    binding: PublisherBindingV2,
    last_sequence: u64,
    pending: Option<PendingPublicationV2>,
}

pub(super) struct DurablePeerRecordPublisher {
    directory: PathBuf,
    lock: File,
    keypair: Keypair,
    template: PeerRecord,
    state: PublisherStateV2,
    poisoned: bool,
}

pub(super) enum LocalPeerRecordPublisher {
    Legacy,
    Durable(Box<DurablePeerRecordPublisher>),
}

impl LocalPeerRecordPublisher {
    pub(super) fn open(
        directory: Option<&Path>,
        keypair: &Keypair,
        template: Option<&PeerRecord>,
    ) -> Result<Self, WorldError> {
        let Some(directory) = directory else {
            return Ok(Self::Legacy);
        };
        let template = template.ok_or_else(|| {
            protocol_error(
                "durable peer publication requires a configured peer-record template".into(),
            )
        })?;
        Ok(Self::Durable(Box::new(DurablePeerRecordPublisher::open(
            directory, keypair, template,
        )?)))
    }

    pub(super) fn sign(
        &mut self,
        record: &PeerRecord,
        keypair: &Keypair,
    ) -> Result<SignedPeerRecord, WorldError> {
        match self {
            Self::Legacy => super::peer_record::sign_peer_record(record, keypair),
            Self::Durable(publisher) => publisher.publish_materialized(record),
        }
    }

    pub(super) fn signed_record_for_remote_request(
        &self,
        current_record: &PeerRecord,
        keypair: &Keypair,
    ) -> Result<SignedPeerRecord, WorldError> {
        match self {
            Self::Legacy => {
                let mut current_record = current_record.clone();
                current_record.published_at_ms = super::now_ms();
                super::peer_record::sign_peer_record(&current_record, keypair)
            }
            Self::Durable(publisher) => publisher
                .existing_signed_record_for_current_declaration(current_record)?
                .ok_or_else(|| {
                    protocol_error(
                        "local peer record unavailable until a fresh local publication".into(),
                    )
                }),
        }
    }
}

impl DurablePeerRecordPublisher {
    pub(super) fn initialize(
        directory: &Path,
        keypair: &Keypair,
        template: &PeerRecord,
    ) -> Result<(), WorldError> {
        initialization::initialize(directory, keypair, template)
    }

    pub(super) fn open(
        directory: &Path,
        keypair: &Keypair,
        template: &PeerRecord,
    ) -> Result<Self, WorldError> {
        let (template, binding) = publisher_binding(template, keypair)?;
        require_private_persistence_platform()?;
        ensure_existing_private_directory(directory)?;
        initialization::ensure_no_incomplete_bootstrap(directory)?;

        let lock_path = directory.join(LOCK_FILE_NAME);
        ensure_not_symlink(&lock_path)?;
        let mut lock_options = OpenOptions::new();
        lock_options.read(true).write(true);
        let lock = lock_options
            .open(&lock_path)
            .map_err(|error| io_error("open publication lock", &lock_path, error))?;
        ensure_private_open_file(&lock, &lock_path)?;
        lock.try_lock().map_err(|error| {
            protocol_error(format!(
                "publication directory is already in use: {error:?}"
            ))
        })?;
        initialization::ensure_file_is_current_path(&lock, &lock_path)?;
        initialization::ensure_no_incomplete_bootstrap(directory)?;

        let mut publisher = Self {
            directory: directory.to_path_buf(),
            lock,
            keypair: keypair.clone(),
            template,
            state: PublisherStateV2 {
                schema_version: STATE_SCHEMA_VERSION,
                binding: binding.clone(),
                last_sequence: 0,
                pending: None,
            },
            poisoned: false,
        };
        publisher.remove_stale_temp_file()?;
        publisher.load_existing_state(binding)?;
        Ok(publisher)
    }

    pub(super) fn publish_materialized(
        &mut self,
        record: &PeerRecord,
    ) -> Result<SignedPeerRecord, WorldError> {
        self.ensure_usable()?;
        let record = self.normalize_and_check_materialized_record(record)?;
        ensure_record_is_unexpired(&record)?;
        let record_bytes = canonical_cbor(&record)?;
        ensure_record_capacity(record_bytes.len())?;

        if let Some(pending) = &self.state.pending
            && pending.materialized_record_cbor == record_bytes
        {
            if let Some(signed_bytes) = &pending.signed_record_cbor {
                return self.decode_and_check_signed_record(pending, signed_bytes);
            }
            return self.sign_and_persist_pending();
        }

        let sequence = self.next_sequence()?;
        let pending = PendingPublicationV2 {
            sequence,
            materialized_record_cbor: record_bytes,
            signed_record_cbor: None,
        };
        let mut next = self.state.clone();
        next.last_sequence = sequence;
        next.pending = Some(pending);
        self.persist_state(next)?;
        self.sign_and_persist_pending()
    }

    fn existing_signed_record_for_current_declaration(
        &self,
        current_record: &PeerRecord,
    ) -> Result<Option<SignedPeerRecord>, WorldError> {
        self.ensure_usable()?;
        let Some(pending) = self.state.pending.as_ref() else {
            return Ok(None);
        };
        let Some(signed_bytes) = pending.signed_record_cbor.as_deref() else {
            return Ok(None);
        };
        let signed = self.decode_and_check_signed_record(pending, signed_bytes)?;
        let mut expected = current_record.clone();
        expected.published_at_ms = signed.record.published_at_ms;
        let expected = self.normalize_and_check_materialized_record(&expected)?;
        if expected != signed.record {
            return Ok(None);
        }
        Ok(Some(signed))
    }

    fn load_existing_state(
        &mut self,
        expected_binding: PublisherBindingV2,
    ) -> Result<(), WorldError> {
        let state_path = self.state_path();
        ensure_not_symlink(&state_path)?;
        let marker = self.read_lock_marker()?;
        if marker != LOCK_INITIALIZED_MARKER {
            return Err(protocol_error(
                "publication lock marker is missing or corrupt".into(),
            ));
        }
        if !path_exists(&state_path)? {
            return Err(protocol_error(
                "initialized publication state is missing; refusing sequence reset".into(),
            ));
        }

        let bytes = read_bounded_private_file(&state_path, MAX_PERSISTED_STATE_BYTES)?;
        let state: PublisherStateV2 = serde_cbor::from_slice(&bytes)
            .map_err(|error| protocol_error(format!("publication state decode failed: {error}")))?;
        if canonical_cbor(&state)? != bytes {
            return Err(protocol_error(
                "publication state is not canonical CBOR".into(),
            ));
        }
        validate_state(&state, &expected_binding, &self.keypair)?;
        self.state = state;
        Ok(())
    }

    fn sign_and_persist_pending(&mut self) -> Result<SignedPeerRecord, WorldError> {
        self.ensure_usable()?;
        let pending = self
            .state
            .pending
            .clone()
            .ok_or_else(|| protocol_error("publication reservation is missing".into()))?;
        if let Some(signed_bytes) = pending.signed_record_cbor.as_deref() {
            return self.decode_and_check_signed_record(&pending, signed_bytes);
        }
        let record: PeerRecord = serde_cbor::from_slice(&pending.materialized_record_cbor)
            .map_err(|error| protocol_error(format!("reserved record decode failed: {error}")))?;
        let signed = sign_peer_record_v2(
            &record,
            PeerRecordPublicationV2 {
                schema_version: 2,
                sequence: pending.sequence,
                state: PeerRecordPublicationStateV2::Live,
            },
            &self.keypair,
        )?;
        let signed_bytes = canonical_cbor(&signed)?;
        ensure_record_capacity(signed_bytes.len())?;
        let mut next = self.state.clone();
        next.pending
            .as_mut()
            .ok_or_else(|| protocol_error("publication reservation disappeared".into()))?
            .signed_record_cbor = Some(signed_bytes);
        self.persist_state(next)?;
        let stored_pending = self
            .state
            .pending
            .clone()
            .ok_or_else(|| protocol_error("publication reservation disappeared".into()))?;
        self.decode_and_check_signed_record(
            &stored_pending,
            stored_pending
                .signed_record_cbor
                .as_deref()
                .ok_or_else(|| protocol_error("signed publication was not persisted".into()))?,
        )
    }

    fn decode_and_check_signed_record(
        &self,
        pending: &PendingPublicationV2,
        signed_bytes: &[u8],
    ) -> Result<SignedPeerRecord, WorldError> {
        let signed: SignedPeerRecord = serde_cbor::from_slice(signed_bytes)
            .map_err(|error| protocol_error(format!("stored signature decode failed: {error}")))?;
        verify_signed_peer_record(&signed)?;
        ensure_record_is_unexpired(&signed.record)?;
        let record: PeerRecord = serde_cbor::from_slice(&pending.materialized_record_cbor)
            .map_err(|error| protocol_error(format!("reserved record decode failed: {error}")))?;
        let publication = signed
            .publication
            .as_ref()
            .ok_or_else(|| protocol_error("stored publication is not V2".into()))?;
        if publication.sequence != pending.sequence || signed.record != record {
            return Err(protocol_error(
                "stored signed publication does not match its reservation".into(),
            ));
        }
        if signed.identity_public_key_protobuf_hex
            != hex::encode(self.keypair.public().encode_protobuf())
        {
            return Err(protocol_error(
                "stored signed publication identity does not match key binding".into(),
            ));
        }
        Ok(signed)
    }

    #[cfg(test)]
    fn reserve_materialized_record(&mut self, record: &PeerRecord) -> Result<u64, WorldError> {
        self.ensure_usable()?;
        let record = self.normalize_and_check_materialized_record(record)?;
        ensure_record_is_unexpired(&record)?;
        let record_bytes = canonical_cbor(&record)?;
        ensure_record_capacity(record_bytes.len())?;
        let sequence = self.next_sequence()?;
        let mut next = self.state.clone();
        next.last_sequence = sequence;
        next.pending = Some(PendingPublicationV2 {
            sequence,
            materialized_record_cbor: record_bytes,
            signed_record_cbor: None,
        });
        self.persist_state(next)?;
        Ok(sequence)
    }

    fn next_sequence(&self) -> Result<u64, WorldError> {
        self.state
            .last_sequence
            .checked_add(1)
            .filter(|sequence| *sequence > 0)
            .ok_or_else(|| protocol_error("peer record publication sequence exhausted".into()))
    }

    fn normalize_and_check_materialized_record(
        &self,
        record: &PeerRecord,
    ) -> Result<PeerRecord, WorldError> {
        let mut normalized = record.clone();
        let expected_peer_id = PeerId::from(self.keypair.public()).to_string();
        if normalized.peer_id.trim().is_empty() {
            normalized.peer_id = expected_peer_id;
        }
        normalized.validate_policy().map_err(protocol_error)?;

        let mut expected = self.template.clone();
        expected.published_at_ms = normalized.published_at_ms;
        if expected.direct_addrs.is_empty() {
            expected.direct_addrs.clone_from(&normalized.direct_addrs);
        }
        if expected.relay_addrs.is_empty() {
            expected.relay_addrs.clone_from(&normalized.relay_addrs);
        }
        if expected != normalized {
            return Err(protocol_error(
                "materialized peer record does not match durable publisher template".into(),
            ));
        }
        Ok(normalized)
    }

    fn persist_state(&mut self, state: PublisherStateV2) -> Result<(), WorldError> {
        self.ensure_usable()?;
        let bytes = canonical_cbor(&state)?;
        if bytes.len() > MAX_PERSISTED_STATE_BYTES {
            return Err(protocol_error(
                "durable peer publication state capacity exceeded".into(),
            ));
        }
        match atomic_write_private_file(&self.directory, &self.state_path(), &bytes) {
            Ok(()) => {
                self.state = state;
                Ok(())
            }
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }

    fn read_lock_marker(&mut self) -> Result<Vec<u8>, WorldError> {
        self.lock
            .seek(SeekFrom::Start(0))
            .map_err(|error| io_error("seek publication lock", &self.lock_path(), error))?;
        let mut marker = Vec::new();
        (&mut self.lock)
            .take((LOCK_INITIALIZED_MARKER.len() + 1) as u64)
            .read_to_end(&mut marker)
            .map_err(|error| io_error("read publication lock", &self.lock_path(), error))?;
        if marker.len() > LOCK_INITIALIZED_MARKER.len() {
            return Err(protocol_error(
                "publication lock marker is oversized".into(),
            ));
        }
        Ok(marker)
    }

    fn remove_stale_temp_file(&self) -> Result<(), WorldError> {
        let temp_path = self.directory.join(TEMP_FILE_NAME);
        ensure_not_symlink(&temp_path)?;
        if !path_exists(&temp_path)? {
            return Ok(());
        }
        ensure_private_regular_file(&temp_path)?;
        fs::remove_file(&temp_path)
            .map_err(|error| io_error("remove stale publication temp", &temp_path, error))?;
        sync_directory(&self.directory)
    }

    fn ensure_usable(&self) -> Result<(), WorldError> {
        if self.poisoned {
            return Err(protocol_error(
                "durable peer publisher is poisoned after uncertain persistence failure".into(),
            ));
        }
        Ok(())
    }

    fn state_path(&self) -> PathBuf {
        self.directory.join(STATE_FILE_NAME)
    }

    fn lock_path(&self) -> PathBuf {
        self.directory.join(LOCK_FILE_NAME)
    }
}

fn publisher_binding(
    template: &PeerRecord,
    keypair: &Keypair,
) -> Result<(PeerRecord, PublisherBindingV2), WorldError> {
    let mut normalized_template = template.clone();
    normalized_template
        .validate_policy()
        .map_err(protocol_error)?;
    let peer_id = PeerId::from(keypair.public()).to_string();
    if normalized_template.peer_id.trim().is_empty() {
        normalized_template.peer_id = peer_id.clone();
    } else if normalized_template.peer_id != peer_id {
        return Err(protocol_error(
            "durable peer template identity does not match configured keypair".into(),
        ));
    }
    normalized_template.published_at_ms = 0;
    ensure_record_capacity(canonical_cbor(&normalized_template)?.len())?;
    let binding = PublisherBindingV2 {
        schema_version: STATE_SCHEMA_VERSION,
        peer_id,
        world_id: normalized_template.world_id.clone(),
        network_id: normalized_template.network_id.clone(),
        identity_public_key_protobuf_hex: hex::encode(keypair.public().encode_protobuf()),
    };
    Ok((normalized_template, binding))
}

fn validate_state(
    state: &PublisherStateV2,
    expected_binding: &PublisherBindingV2,
    keypair: &Keypair,
) -> Result<(), WorldError> {
    if state.schema_version != STATE_SCHEMA_VERSION || &state.binding != expected_binding {
        return Err(protocol_error(
            "durable publication state schema or identity binding mismatch".into(),
        ));
    }
    match (state.last_sequence, state.pending.as_ref()) {
        (0, None) => Ok(()),
        (sequence, Some(pending)) if sequence > 0 && pending.sequence == sequence => {
            let record: PeerRecord = serde_cbor::from_slice(&pending.materialized_record_cbor)
                .map_err(|error| {
                    protocol_error(format!("reserved record decode failed: {error}"))
                })?;
            if canonical_cbor(&record)? != pending.materialized_record_cbor
                || pending.materialized_record_cbor.len() > MAX_RECORD_BYTES
            {
                return Err(protocol_error("reserved record encoding is invalid".into()));
            }
            record.validate_policy().map_err(protocol_error)?;
            if record.peer_id != expected_binding.peer_id
                || record.world_id != expected_binding.world_id
                || record.network_id != expected_binding.network_id
            {
                return Err(protocol_error(
                    "reserved record does not match publisher identity scope".into(),
                ));
            }
            if let Some(signed_bytes) = &pending.signed_record_cbor {
                if signed_bytes.len() > MAX_RECORD_BYTES {
                    return Err(protocol_error("stored signed record is oversized".into()));
                }
                let signed: SignedPeerRecord =
                    serde_cbor::from_slice(signed_bytes).map_err(|error| {
                        protocol_error(format!("stored signature decode failed: {error}"))
                    })?;
                if canonical_cbor(&signed)? != *signed_bytes {
                    return Err(protocol_error(
                        "stored signed record is not canonical".into(),
                    ));
                }
                verify_signed_peer_record(&signed)?;
                let publication = signed
                    .publication
                    .as_ref()
                    .ok_or_else(|| protocol_error("stored record has no V2 publication".into()))?;
                if publication.sequence != sequence || signed.record != record {
                    return Err(protocol_error(
                        "stored signed record does not match pending reservation".into(),
                    ));
                }
                if signed.identity_public_key_protobuf_hex
                    != hex::encode(keypair.public().encode_protobuf())
                {
                    return Err(protocol_error(
                        "stored signed record key does not match publisher binding".into(),
                    ));
                }
            }
            Ok(())
        }
        _ => Err(protocol_error(
            "durable publication sequence and pending reservation disagree".into(),
        )),
    }
}

fn canonical_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, WorldError> {
    crate::util::to_canonical_cbor(value)
}

fn ensure_record_capacity(bytes: usize) -> Result<(), WorldError> {
    if bytes > MAX_RECORD_BYTES {
        return Err(protocol_error("peer record capacity exceeded".into()));
    }
    Ok(())
}

fn ensure_record_is_unexpired(record: &PeerRecord) -> Result<(), WorldError> {
    let expiry = record.expires_at_ms().map_err(protocol_error)?;
    if crate::util::unix_now_ms_i64() >= expiry {
        return Err(protocol_error(
            "materialized peer record has expired; allocate a fresh publication sequence".into(),
        ));
    }
    Ok(())
}

fn ensure_private_directory(directory: &Path) -> Result<(), WorldError> {
    let absolute = absolute_path(directory)?;
    let mut current = PathBuf::new();
    for component in absolute.components() {
        if matches!(component, std::path::Component::CurDir) {
            continue;
        }
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure_directory_component(&current, &metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = current
                    .parent()
                    .ok_or_else(|| protocol_error("publication directory has no parent".into()))?;
                fs::create_dir(&current)
                    .map_err(|error| io_error("create publication directory", &current, error))?;
                set_private_directory_permissions(&current)?;
                sync_directory(&current)?;
                sync_directory(parent)?;
            }
            Err(error) => {
                return Err(io_error(
                    "inspect publication directory component",
                    &current,
                    error,
                ));
            }
        }
    }
    let metadata = fs::symlink_metadata(&absolute)
        .map_err(|error| io_error("inspect publication directory", &absolute, error))?;
    ensure_directory_component(&absolute, &metadata)?;
    ensure_private_mode(metadata.permissions().mode(), "publication directory")?;
    Ok(())
}

fn ensure_existing_private_directory(directory: &Path) -> Result<(), WorldError> {
    let absolute = absolute_path(directory)?;
    let mut current = PathBuf::new();
    for component in absolute.components() {
        if matches!(component, std::path::Component::CurDir) {
            continue;
        }
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure_directory_component(&current, &metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(protocol_error(
                    "publication directory is missing; restore the original publication state instead of reinitializing"
                        .into(),
                ));
            }
            Err(error) => {
                return Err(io_error(
                    "inspect existing publication directory",
                    &current,
                    error,
                ));
            }
        }
    }
    let metadata = match fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(protocol_error(
                "publication directory is missing; restore the original publication state instead of reinitializing"
                    .into(),
            ));
        }
        Err(error) => {
            return Err(io_error(
                "inspect existing publication directory",
                &absolute,
                error,
            ));
        }
    };
    ensure_directory_component(&absolute, &metadata)?;
    ensure_private_mode(metadata.permissions().mode(), "publication directory")
}

fn absolute_path(path: &Path) -> Result<PathBuf, WorldError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|current| current.join(path))
        .map_err(|error| io_error("resolve publication directory", path, error))
}

fn ensure_directory_component(path: &Path, metadata: &fs::Metadata) -> Result<(), WorldError> {
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(protocol_error(format!(
            "publication path component must be a real directory: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn require_private_persistence_platform() -> Result<(), WorldError> {
    Ok(())
}

#[cfg(not(unix))]
fn require_private_persistence_platform() -> Result<(), WorldError> {
    Err(protocol_error(
        "durable peer publication V2 is unsupported on this platform".into(),
    ))
}

fn ensure_not_symlink(path: &Path) -> Result<(), WorldError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(protocol_error(format!(
            "publication path must not be a symlink: {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("inspect publication path", path, error)),
    }
}

fn path_exists(path: &Path) -> Result<bool, WorldError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error("inspect publication path", path, error)),
    }
}

fn ensure_private_regular_file(path: &Path) -> Result<(), WorldError> {
    ensure_not_symlink(path)?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| io_error("inspect publication file", path, error))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(protocol_error(format!(
            "publication path must be a regular file: {}",
            path.display()
        )));
    }
    ensure_private_mode(metadata.permissions().mode(), "publication file")
}

fn ensure_private_open_file(file: &File, path: &Path) -> Result<(), WorldError> {
    let metadata = file
        .metadata()
        .map_err(|error| io_error("inspect publication file", path, error))?;
    if !metadata.is_file() {
        return Err(protocol_error(
            "publication lock is not a regular file".into(),
        ));
    }
    ensure_private_mode(metadata.permissions().mode(), "publication lock")
}

fn read_bounded_private_file(path: &Path, maximum: usize) -> Result<Vec<u8>, WorldError> {
    ensure_private_regular_file(path)?;
    let metadata =
        fs::metadata(path).map_err(|error| io_error("stat publication state", path, error))?;
    if metadata.len() > maximum as u64 {
        return Err(protocol_error(
            "durable publication state is oversized".into(),
        ));
    }
    let file = File::open(path).map_err(|error| io_error("open publication state", path, error))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((maximum + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read publication state", path, error))?;
    if bytes.len() > maximum {
        return Err(protocol_error(
            "durable publication state is oversized".into(),
        ));
    }
    Ok(bytes)
}

fn atomic_write_private_file(
    directory: &Path,
    target: &Path,
    bytes: &[u8],
) -> Result<(), WorldError> {
    let temp = directory.join(TEMP_FILE_NAME);
    ensure_not_symlink(target)?;
    if path_exists(target)? {
        ensure_private_regular_file(target)?;
    }
    ensure_not_symlink(&temp)?;
    if path_exists(&temp)? {
        ensure_private_regular_file(&temp)?;
        fs::remove_file(&temp).map_err(|error| io_error("remove stale temp", &temp, error))?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    set_private_file_mode(&mut options);
    let mut file = options
        .open(&temp)
        .map_err(|error| io_error("create publication temp", &temp, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io_error("persist publication temp", &temp, error))?;
    fs::rename(&temp, target)
        .map_err(|error| io_error("replace publication state", target, error))?;
    sync_directory(directory)
}

fn sync_directory(directory: &Path) -> Result<(), WorldError> {
    let dir = File::open(directory)
        .map_err(|error| io_error("open publication directory", directory, error))?;
    dir.sync_all()
        .map_err(|error| io_error("sync publication directory", directory, error))
}

#[cfg(unix)]
fn set_private_file_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_file_mode(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn set_private_directory_permissions(directory: &Path) -> Result<(), WorldError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| io_error("secure publication directory", directory, error))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_directory: &Path) -> Result<(), WorldError> {
    Ok(())
}

#[cfg(unix)]
fn ensure_private_mode(mode: u32, kind: &str) -> Result<(), WorldError> {
    if mode & 0o077 != 0 {
        return Err(protocol_error(format!(
            "{kind} permissions must not grant group or other access"
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_mode(_mode: u32, _kind: &str) -> Result<(), WorldError> {
    Ok(())
}

#[cfg(unix)]
trait FilePermissionsMode {
    fn mode(&self) -> u32;
}

#[cfg(unix)]
impl FilePermissionsMode for fs::Permissions {
    fn mode(&self) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        PermissionsExt::mode(self)
    }
}

#[cfg(not(unix))]
trait FilePermissionsMode {
    fn mode(&self) -> u32;
}

#[cfg(not(unix))]
impl FilePermissionsMode for fs::Permissions {
    fn mode(&self) -> u32 {
        0
    }
}

fn protocol_error(message: String) -> WorldError {
    WorldError::NetworkProtocolUnavailable { protocol: message }
}

fn io_error(operation: &str, path: &Path, error: std::io::Error) -> WorldError {
    protocol_error(format!(
        "{operation} failed for {}: {error}",
        path.display()
    ))
}

#[cfg(test)]
#[path = "durable_peer_record_publisher_tests.rs"]
mod tests;
