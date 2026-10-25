use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use libp2p::identity::Keypair;
use oasis7_proto::distributed_dht::PeerRecord;
use serde::{Deserialize, Serialize};

use crate::error::WorldError;

use super::{
    LOCK_FILE_NAME, LOCK_INITIALIZED_MARKER, MAX_PERSISTED_STATE_BYTES, PublisherBindingV2,
    PublisherStateV2, STATE_FILE_NAME, STATE_SCHEMA_VERSION, TEMP_FILE_NAME,
    atomic_write_private_file, canonical_cbor, ensure_not_symlink, ensure_private_directory,
    ensure_private_open_file, ensure_private_regular_file, io_error, path_exists, protocol_error,
    publisher_binding, read_bounded_private_file, require_private_persistence_platform,
    set_private_file_mode, sync_directory, validate_state,
};

pub(super) const INTENT_FILE_NAME: &str = "peer-record-publication-v2.initializing.cbor";
pub(super) const INTENT_TEMP_FILE_NAME: &str = "peer-record-publication-v2.initializing.cbor.tmp";
const MAX_INTENT_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InitializationIntentV2 {
    schema_version: u16,
    status: String,
    binding: PublisherBindingV2,
}

pub(super) fn initialize(
    directory: &Path,
    keypair: &Keypair,
    template: &PeerRecord,
) -> Result<(), WorldError> {
    require_private_persistence_platform()?;
    let (_, expected_binding) = publisher_binding(template, keypair)?;
    ensure_private_directory(directory)?;

    let intent_lock = acquire_or_create_intent(directory, &expected_binding)?;
    let stable_lock = acquire_stable_lock(directory)?;
    finish_initialization(
        directory,
        stable_lock,
        intent_lock,
        &expected_binding,
        keypair,
    )
}

pub(super) fn ensure_no_incomplete_bootstrap(directory: &Path) -> Result<(), WorldError> {
    let intent_path = directory.join(INTENT_FILE_NAME);
    let temp_path = directory.join(INTENT_TEMP_FILE_NAME);
    ensure_not_symlink(&intent_path)?;
    ensure_not_symlink(&temp_path)?;
    if path_exists(&intent_path)? || path_exists(&temp_path)? {
        return Err(protocol_error(
            "peer publication initialization is incomplete; resume it with the explicit initializer"
                .into(),
        ));
    }
    Ok(())
}

pub(in crate::libp2p_net) fn ensure_file_is_current_path(
    file: &File,
    path: &Path,
) -> Result<(), WorldError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        ensure_not_symlink(path)?;
        let open_metadata = file
            .metadata()
            .map_err(|error| io_error("inspect locked publication file", path, error))?;
        let path_metadata = fs::symlink_metadata(path)
            .map_err(|error| io_error("inspect locked publication path", path, error))?;
        if !open_metadata.is_file()
            || !path_metadata.is_file()
            || open_metadata.dev() != path_metadata.dev()
            || open_metadata.ino() != path_metadata.ino()
        {
            return Err(protocol_error(
                "locked publication file no longer matches its path".into(),
            ));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (file, path);
        Err(protocol_error(
            "durable peer publication V2 is unsupported on this platform".into(),
        ))
    }
}

fn acquire_or_create_intent(
    directory: &Path,
    expected_binding: &PublisherBindingV2,
) -> Result<File, WorldError> {
    let intent_path = directory.join(INTENT_FILE_NAME);
    let temp_path = directory.join(INTENT_TEMP_FILE_NAME);
    ensure_not_symlink(&intent_path)?;
    ensure_not_symlink(&temp_path)?;

    if path_exists(&intent_path)? {
        let intent = open_locked_path(&intent_path)?;
        read_and_validate_intent(&intent, &intent_path, expected_binding)?;
        remove_linked_intent_temp(&intent, &intent_path, &temp_path, directory)?;
        return Ok(intent);
    }

    ensure_no_permanent_material(directory)?;
    if path_exists(&temp_path)? {
        let mut temp = open_locked_path(&temp_path)?;
        ensure_no_permanent_material(directory)?;

        if path_exists(&intent_path)? {
            ensure_file_is_current_path(&temp, &intent_path)?;
            read_and_validate_intent(&temp, &intent_path, expected_binding)?;
            remove_linked_intent_temp(&temp, &intent_path, &temp_path, directory)?;
            return Ok(temp);
        }

        let bytes = read_locked_file(&temp, &temp_path, MAX_INTENT_BYTES)?;
        let expected_intent = encode_intent(expected_binding)?;
        if bytes != expected_intent
            && (bytes.len() >= expected_intent.len() || !expected_intent.starts_with(&bytes))
        {
            return Err(protocol_error(
                "bootstrap intent temp is unsupported or does not match the current initialization"
                    .into(),
            ));
        }
        ensure_file_is_current_path(&temp, &temp_path)?;
        if bytes != expected_intent {
            temp.set_len(0)
                .and_then(|()| temp.seek(SeekFrom::Start(0)).map(|_| ()))
                .and_then(|()| temp.write_all(&expected_intent))
                .and_then(|()| temp.sync_all())
                .map_err(|error| {
                    io_error("complete unpublished bootstrap intent", &temp_path, error)
                })?;
        }
        // Complete bytes can survive in page cache even if the previous file sync did not.
        temp.sync_all()
            .map_err(|error| io_error("sync recovered bootstrap intent", &temp_path, error))?;
        publish_intent_no_replace(&temp, &temp_path, &intent_path, directory)?;
        return Ok(temp);
    }

    let mut file = create_locked_intent_temp(&temp_path)?;
    ensure_no_permanent_material(directory)?;
    let intent_bytes = encode_intent(expected_binding)?;
    ensure_file_is_current_path(&file, &temp_path)?;
    file.write_all(&intent_bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io_error("persist bootstrap intent", &temp_path, error))?;
    publish_intent_no_replace(&file, &temp_path, &intent_path, directory)?;
    Ok(file)
}

fn create_locked_intent_temp(path: &Path) -> Result<File, WorldError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    set_private_file_mode(&mut options);
    let file = options
        .open(path)
        .map_err(|error| io_error("create bootstrap intent temp", path, error))?;
    ensure_private_open_file(&file, path)?;
    file.try_lock().map_err(|error| {
        protocol_error(format!("bootstrap intent is already in use: {error:?}"))
    })?;
    ensure_file_is_current_path(&file, path)?;
    Ok(file)
}

fn open_locked_path(path: &Path) -> Result<File, WorldError> {
    ensure_not_symlink(path)?;
    ensure_private_regular_file(path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| io_error("open bootstrap intent", path, error))?;
    ensure_private_open_file(&file, path)?;
    file.try_lock().map_err(|error| {
        protocol_error(format!("bootstrap intent is already in use: {error:?}"))
    })?;
    ensure_file_is_current_path(&file, path)?;
    Ok(file)
}

pub(super) fn publish_intent_no_replace(
    file: &File,
    temp_path: &Path,
    intent_path: &Path,
    directory: &Path,
) -> Result<(), WorldError> {
    ensure_file_is_current_path(file, temp_path)?;
    ensure_not_symlink(intent_path)?;
    if path_exists(intent_path)? {
        return Err(protocol_error(
            "bootstrap intent already exists; refusing replacement".into(),
        ));
    }
    fs::hard_link(temp_path, intent_path).map_err(|error| {
        io_error(
            "publish bootstrap intent without replacement",
            intent_path,
            error,
        )
    })?;
    ensure_file_is_current_path(file, intent_path)?;
    sync_directory(directory)?;
    ensure_file_is_current_path(file, temp_path)?;
    fs::remove_file(temp_path)
        .map_err(|error| io_error("remove published bootstrap intent temp", temp_path, error))?;
    sync_directory(directory)
}

fn remove_linked_intent_temp(
    file: &File,
    intent_path: &Path,
    temp_path: &Path,
    directory: &Path,
) -> Result<(), WorldError> {
    ensure_file_is_current_path(file, intent_path)?;
    ensure_not_symlink(temp_path)?;
    if !path_exists(temp_path)? {
        return Ok(());
    }
    ensure_file_is_current_path(file, temp_path)?;
    fs::remove_file(temp_path)
        .map_err(|error| io_error("remove linked bootstrap intent temp", temp_path, error))?;
    sync_directory(directory)
}

fn read_and_validate_intent(
    file: &File,
    path: &Path,
    expected_binding: &PublisherBindingV2,
) -> Result<(), WorldError> {
    ensure_file_is_current_path(file, path)?;
    let bytes = read_locked_file(file, path, MAX_INTENT_BYTES)?;
    let intent: InitializationIntentV2 = serde_cbor::from_slice(&bytes)
        .map_err(|error| protocol_error(format!("bootstrap intent decode failed: {error}")))?;
    if canonical_cbor(&intent)? != bytes {
        return Err(protocol_error(
            "bootstrap intent is not canonical CBOR".into(),
        ));
    }
    validate_intent(&intent, expected_binding)
}

fn validate_intent(
    intent: &InitializationIntentV2,
    expected_binding: &PublisherBindingV2,
) -> Result<(), WorldError> {
    if intent.schema_version != STATE_SCHEMA_VERSION
        || intent.status != "initializing"
        || &intent.binding != expected_binding
    {
        return Err(protocol_error(
            "bootstrap intent schema, status, or identity binding mismatch".into(),
        ));
    }
    Ok(())
}

fn encode_intent(binding: &PublisherBindingV2) -> Result<Vec<u8>, WorldError> {
    let bytes = canonical_cbor(&InitializationIntentV2 {
        schema_version: STATE_SCHEMA_VERSION,
        status: "initializing".into(),
        binding: binding.clone(),
    })?;
    if bytes.len() > MAX_INTENT_BYTES {
        return Err(protocol_error("bootstrap intent capacity exceeded".into()));
    }
    Ok(bytes)
}

fn read_locked_file(file: &File, path: &Path, maximum: usize) -> Result<Vec<u8>, WorldError> {
    ensure_private_open_file(file, path)?;
    let metadata = file
        .metadata()
        .map_err(|error| io_error("inspect bootstrap intent", path, error))?;
    if metadata.len() > maximum as u64 {
        return Err(protocol_error("bootstrap intent is oversized".into()));
    }
    let mut reader = file
        .try_clone()
        .map_err(|error| io_error("clone bootstrap intent", path, error))?;
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|error| io_error("seek bootstrap intent", path, error))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    reader
        .take((maximum + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read bootstrap intent", path, error))?;
    if bytes.len() > maximum {
        return Err(protocol_error("bootstrap intent is oversized".into()));
    }
    Ok(bytes)
}

fn ensure_no_permanent_material(directory: &Path) -> Result<(), WorldError> {
    for name in [LOCK_FILE_NAME, STATE_FILE_NAME, TEMP_FILE_NAME] {
        let path = directory.join(name);
        ensure_not_symlink(&path)?;
        if path_exists(&path)? {
            return Err(protocol_error(format!(
                "peer publication initialization refuses existing material: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn acquire_stable_lock(directory: &Path) -> Result<File, WorldError> {
    let path = directory.join(LOCK_FILE_NAME);
    ensure_not_symlink(&path)?;
    let file = match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            set_private_file_mode(&mut options);
            options
                .open(&path)
                .map_err(|error| io_error("create publication lock", &path, error))?
        }
        Err(error) => return Err(io_error("open publication lock", &path, error)),
    };
    ensure_private_open_file(&file, &path)?;
    file.try_lock().map_err(|error| {
        protocol_error(format!(
            "publication directory is already in use: {error:?}"
        ))
    })?;
    ensure_file_is_current_path(&file, &path)?;
    Ok(file)
}

fn finish_initialization(
    directory: &Path,
    mut stable_lock: File,
    intent_lock: File,
    expected_binding: &PublisherBindingV2,
    keypair: &Keypair,
) -> Result<(), WorldError> {
    let lock_path = directory.join(LOCK_FILE_NAME);
    ensure_file_is_current_path(&stable_lock, &lock_path)?;
    let marker = read_marker(&mut stable_lock, &lock_path)?;
    let ready = marker.as_slice() == LOCK_INITIALIZED_MARKER;
    let marker_is_valid_prefix =
        !marker.is_empty() && LOCK_INITIALIZED_MARKER.starts_with(marker.as_slice());
    if !marker.is_empty() && !ready && !marker_is_valid_prefix {
        return Err(protocol_error("publication lock marker is corrupt".into()));
    }

    let state_path = directory.join(STATE_FILE_NAME);
    ensure_not_symlink(&state_path)?;
    let has_state = path_exists(&state_path)?;
    if !marker.is_empty() && !has_state {
        return Err(protocol_error(
            "initialized publication state is missing; refusing sequence reset".into(),
        ));
    }

    if has_state {
        let state = read_and_validate_zero_state(&state_path, expected_binding, keypair)?;
        if state.last_sequence != 0 || state.pending.is_some() {
            return Err(protocol_error(
                "bootstrap intent cannot reset issued publication state".into(),
            ));
        }
    }
    recover_state_temp(directory, expected_binding, ready)?;

    if !has_state {
        let initial_state = PublisherStateV2 {
            schema_version: STATE_SCHEMA_VERSION,
            binding: expected_binding.clone(),
            last_sequence: 0,
            pending: None,
        };
        atomic_write_private_file(directory, &state_path, &canonical_cbor(&initial_state)?)?;
    }
    if !ready {
        write_marker(&mut stable_lock, &lock_path, directory)?;
    }
    clear_intent(directory, &intent_lock)?;
    Ok(())
}

fn read_and_validate_zero_state(
    path: &Path,
    expected_binding: &PublisherBindingV2,
    keypair: &Keypair,
) -> Result<PublisherStateV2, WorldError> {
    let bytes = read_bounded_private_file(path, MAX_PERSISTED_STATE_BYTES)?;
    let state: PublisherStateV2 = serde_cbor::from_slice(&bytes)
        .map_err(|error| protocol_error(format!("publication state decode failed: {error}")))?;
    if canonical_cbor(&state)? != bytes {
        return Err(protocol_error(
            "publication state is not canonical CBOR".into(),
        ));
    }
    validate_state(&state, expected_binding, keypair)?;
    Ok(state)
}

fn recover_state_temp(
    directory: &Path,
    expected_binding: &PublisherBindingV2,
    ready: bool,
) -> Result<(), WorldError> {
    let temp_path = directory.join(TEMP_FILE_NAME);
    ensure_not_symlink(&temp_path)?;
    if !path_exists(&temp_path)? {
        return Ok(());
    }
    if ready && !path_exists(&directory.join(STATE_FILE_NAME))? {
        return Err(protocol_error(
            "initialized publication state is missing; refusing sequence reset".into(),
        ));
    }
    let bytes = read_bounded_private_file(&temp_path, MAX_PERSISTED_STATE_BYTES)?;
    let expected_state = PublisherStateV2 {
        schema_version: STATE_SCHEMA_VERSION,
        binding: expected_binding.clone(),
        last_sequence: 0,
        pending: None,
    };
    let expected_bytes = canonical_cbor(&expected_state)?;
    if bytes != expected_bytes
        && (bytes.len() >= expected_bytes.len() || !expected_bytes.starts_with(&bytes))
    {
        return Err(protocol_error(
            "bootstrap state temp is unsupported or does not match initial state".into(),
        ));
    }
    ensure_private_regular_file(&temp_path)?;
    fs::remove_file(&temp_path)
        .map_err(|error| io_error("remove unpublished initial state temp", &temp_path, error))?;
    sync_directory(directory)
}

fn read_marker(lock: &mut File, path: &Path) -> Result<Vec<u8>, WorldError> {
    lock.seek(SeekFrom::Start(0))
        .map_err(|error| io_error("seek publication lock", path, error))?;
    let mut marker = Vec::new();
    (&mut *lock)
        .take((LOCK_INITIALIZED_MARKER.len() + 1) as u64)
        .read_to_end(&mut marker)
        .map_err(|error| io_error("read publication lock", path, error))?;
    if marker.len() > LOCK_INITIALIZED_MARKER.len() {
        return Err(protocol_error(
            "publication lock marker is oversized".into(),
        ));
    }
    Ok(marker)
}

fn write_marker(lock: &mut File, path: &Path, directory: &Path) -> Result<(), WorldError> {
    ensure_file_is_current_path(lock, path)?;
    lock.set_len(0)
        .and_then(|()| lock.seek(SeekFrom::Start(0)).map(|_| ()))
        .and_then(|()| lock.write_all(LOCK_INITIALIZED_MARKER))
        .and_then(|()| lock.sync_all())
        .map_err(|error| io_error("persist publication lock marker", path, error))?;
    sync_directory(directory)
}

fn clear_intent(directory: &Path, intent_lock: &File) -> Result<(), WorldError> {
    let intent_path = directory.join(INTENT_FILE_NAME);
    let temp_path = directory.join(INTENT_TEMP_FILE_NAME);
    remove_linked_intent_temp(intent_lock, &intent_path, &temp_path, directory)?;
    ensure_file_is_current_path(intent_lock, &intent_path)?;
    fs::remove_file(&intent_path)
        .map_err(|error| io_error("clear completed bootstrap intent", &intent_path, error))?;
    sync_directory(directory)
}
