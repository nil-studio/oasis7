use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::WorldError;

use super::super::durable_peer_record_publisher as durable_files;
use super::{
    DurableReceiver, PeerRecordReceiverMode, RECEIVER_STATE_MAX_BYTES, ReceiverBindingV2,
    ReceiverLedgerV2,
};

const STATE_FILE_NAME: &str = "peer-record-receiver-v2.state.cbor";
const STATE_TEMP_FILE_NAME: &str = "peer-record-receiver-v2.state.cbor.tmp";
const LOCK_FILE_NAME: &str = "peer-record-receiver-v2.lock";
const READY_FILE_NAME: &str = "peer-record-receiver-v2.ready";
const READY_TEMP_FILE_NAME: &str = "peer-record-receiver-v2.ready.tmp";
const INTENT_FILE_NAME: &str = "peer-record-receiver-v2.initializing.cbor";
const INTENT_TEMP_FILE_NAME: &str = "peer-record-receiver-v2.initializing.cbor.tmp";
const READY_MARKER: &[u8] = b"oasis7-peer-record-receiver-v2-ready\n";
const INIT_SCHEMA_VERSION: u16 = 2;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiverInitIntentV2 {
    schema_version: u16,
    binding: ReceiverBindingV2,
    mode: PeerRecordReceiverMode,
}

pub(super) fn initialize(
    directory: &Path,
    binding: ReceiverBindingV2,
    mode: PeerRecordReceiverMode,
) -> Result<(), WorldError> {
    durable_files::require_private_persistence_platform()?;
    durable_files::ensure_private_directory(directory)?;
    let lock = open_lock(directory, true)?;
    lock.try_lock().map_err(|error| {
        protocol_error(format!("receiver directory is already in use: {error:?}"))
    })?;
    durable_files::initialization::ensure_file_is_current_path(
        &lock,
        &path(directory, LOCK_FILE_NAME),
    )?;
    lock.sync_all().map_err(|error| {
        io_error(
            "persist receiver lock",
            &path(directory, LOCK_FILE_NAME),
            error,
        )
    })?;
    durable_files::sync_directory(directory)?;
    let intent = ReceiverInitIntentV2 {
        schema_version: INIT_SCHEMA_VERSION,
        binding: binding.clone(),
        mode,
    };
    let intent_bytes = durable_files::canonical_cbor(&intent)?;
    let empty_state = ReceiverLedgerV2 {
        schema_version: super::RECEIVER_SCHEMA_VERSION,
        binding: binding.clone(),
        mode,
        floors: Default::default(),
    };
    let empty_state_bytes = durable_files::canonical_cbor(&empty_state)?;

    let intent_path = path(directory, INTENT_FILE_NAME);
    let state_path = path(directory, STATE_FILE_NAME);
    let ready_path = path(directory, READY_FILE_NAME);
    if !durable_files::path_exists(&intent_path)? {
        if durable_files::path_exists(&state_path)? || durable_files::path_exists(&ready_path)? {
            return Err(protocol_error(
                "receiver state already exists or is incomplete; refusing reinitialization".into(),
            ));
        }
        recover_or_remove_prefix_temp(
            directory,
            INTENT_TEMP_FILE_NAME,
            &intent_bytes,
            "receiver init intent",
        )?;
        if !durable_files::path_exists(&intent_path)? {
            write_atomic_initial(
                directory,
                &intent_path,
                INTENT_TEMP_FILE_NAME,
                &intent_bytes,
                "receiver init intent",
            )?;
        }
    }
    if durable_files::path_exists(&path(directory, INTENT_TEMP_FILE_NAME))? {
        return Err(protocol_error(
            "receiver init intent and temp both exist; refusing initialization".into(),
        ));
    }
    require_exact_file(&intent_path, &intent_bytes, "receiver init intent")?;

    if durable_files::path_exists(&state_path)? {
        let bytes = read_file(&state_path, RECEIVER_STATE_MAX_BYTES)?;
        if bytes != empty_state_bytes {
            return Err(protocol_error(
                "receiver initialization intent does not match an empty state".into(),
            ));
        }
    } else {
        recover_or_remove_prefix_temp(
            directory,
            STATE_TEMP_FILE_NAME,
            &empty_state_bytes,
            "receiver initial state",
        )?;
        if !durable_files::path_exists(&state_path)? {
            write_atomic_initial(
                directory,
                &state_path,
                STATE_TEMP_FILE_NAME,
                &empty_state_bytes,
                "receiver initial state",
            )?;
        }
    }
    if durable_files::path_exists(&path(directory, STATE_TEMP_FILE_NAME))? {
        return Err(protocol_error(
            "receiver state and temp both exist; refusing initialization".into(),
        ));
    }
    require_exact_file(&state_path, &empty_state_bytes, "receiver initial state")?;

    recover_or_remove_prefix_temp(
        directory,
        READY_TEMP_FILE_NAME,
        READY_MARKER,
        "receiver ready marker",
    )?;
    if durable_files::path_exists(&ready_path)? {
        let ready = read_file(&ready_path, READY_MARKER.len())?;
        if ready != READY_MARKER {
            if !READY_MARKER.starts_with(&ready) {
                return Err(protocol_error(
                    "receiver ready marker is corrupt; refusing initialization".into(),
                ));
            }
            fs::remove_file(&ready_path).map_err(|error| {
                io_error("remove partial receiver ready marker", &ready_path, error)
            })?;
        }
    }
    if !durable_files::path_exists(&ready_path)? {
        write_atomic_initial(
            directory,
            &ready_path,
            READY_TEMP_FILE_NAME,
            READY_MARKER,
            "receiver ready marker",
        )?;
    }
    if durable_files::path_exists(&path(directory, READY_TEMP_FILE_NAME))? {
        return Err(protocol_error(
            "receiver ready marker and temp both exist; refusing initialization".into(),
        ));
    }
    require_exact_file(&ready_path, READY_MARKER, "receiver ready marker")?;
    let intent_path = path(directory, INTENT_FILE_NAME);
    require_exact_file(&intent_path, &intent_bytes, "receiver init intent")?;
    fs::remove_file(&intent_path)
        .map_err(|error| io_error("remove completed receiver init intent", &intent_path, error))?;
    durable_files::sync_directory(directory)
}

pub(super) fn open(
    directory: &Path,
    expected_binding: ReceiverBindingV2,
    expected_mode: PeerRecordReceiverMode,
) -> Result<DurableReceiver, WorldError> {
    durable_files::require_private_persistence_platform()?;
    durable_files::ensure_existing_private_directory(directory)?;
    let lock = open_lock(directory, false)?;
    lock.try_lock().map_err(|error| {
        protocol_error(format!("receiver directory is already in use: {error:?}"))
    })?;
    let lock_path = path(directory, LOCK_FILE_NAME);
    durable_files::initialization::ensure_file_is_current_path(&lock, &lock_path)?;
    reject_incomplete_state(directory)?;

    let ready_path = path(directory, READY_FILE_NAME);
    require_exact_file(&ready_path, READY_MARKER, "receiver ready marker")?;
    let state_path = path(directory, STATE_FILE_NAME);
    let bytes = read_file(&state_path, RECEIVER_STATE_MAX_BYTES)?;
    let ledger: ReceiverLedgerV2 = serde_cbor::from_slice(&bytes)
        .map_err(|error| protocol_error(format!("receiver state decode failed: {error}")))?;
    if durable_files::canonical_cbor(&ledger)? != bytes {
        return Err(protocol_error(
            "receiver state is not canonical CBOR".into(),
        ));
    }
    if ledger.schema_version != super::RECEIVER_SCHEMA_VERSION
        || ledger.binding != expected_binding
        || ledger.mode != expected_mode
    {
        return Err(protocol_error(
            "receiver state schema, identity, scope, or mode mismatch".into(),
        ));
    }
    super::validate_loaded_ledger(&ledger)?;
    Ok(DurableReceiver {
        directory: directory.to_path_buf(),
        _lock: lock,
        ledger,
        poisoned: false,
    })
}

pub(super) fn persist(directory: &Path, bytes: &[u8]) -> Result<(), WorldError> {
    if bytes.len() > RECEIVER_STATE_MAX_BYTES {
        return Err(protocol_error(
            "peer-record receiver ledger is oversized".into(),
        ));
    }
    let target = path(directory, STATE_FILE_NAME);
    let temp = path(directory, STATE_TEMP_FILE_NAME);
    if durable_files::path_exists(&temp)? {
        return Err(protocol_error(
            "receiver state temp file remains; refusing to overwrite evidence".into(),
        ));
    }
    durable_files::atomic_write_private_file_named(directory, &target, STATE_TEMP_FILE_NAME, bytes)
}

pub(super) fn receiver_path_exists(directory: &Path) -> Result<bool, WorldError> {
    durable_files::path_exists(directory)
}

fn reject_incomplete_state(directory: &Path) -> Result<(), WorldError> {
    for name in [
        INTENT_FILE_NAME,
        INTENT_TEMP_FILE_NAME,
        STATE_TEMP_FILE_NAME,
        READY_TEMP_FILE_NAME,
    ] {
        let path = path(directory, name);
        if durable_files::path_exists(&path)? {
            return Err(protocol_error(format!(
                "receiver has incomplete initialization material: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn open_lock(directory: &Path, create: bool) -> Result<File, WorldError> {
    let lock_path = path(directory, LOCK_FILE_NAME);
    durable_files::ensure_not_symlink(&lock_path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(create);
    durable_files::set_private_file_mode(&mut options);
    let lock = options
        .open(&lock_path)
        .map_err(|error| io_error("open receiver lock", &lock_path, error))?;
    durable_files::ensure_private_open_file(&lock, &lock_path)?;
    Ok(lock)
}

fn write_atomic_initial(
    directory: &Path,
    target: &Path,
    temp_name: &str,
    bytes: &[u8],
    kind: &str,
) -> Result<(), WorldError> {
    if durable_files::path_exists(target)? {
        return Err(protocol_error(format!(
            "{kind} already exists; refusing to replace it"
        )));
    }
    durable_files::atomic_write_private_file_named(directory, target, temp_name, bytes)
}

fn recover_or_remove_prefix_temp(
    directory: &Path,
    temp_name: &str,
    expected: &[u8],
    kind: &str,
) -> Result<(), WorldError> {
    recover_or_remove_prefix_temp_with_sync(directory, temp_name, expected, kind, |file, temp| {
        file.sync_all()
            .map_err(|error| io_error("sync complete receiver temp", temp, error))
    })
}

fn recover_or_remove_prefix_temp_with_sync(
    directory: &Path,
    temp_name: &str,
    expected: &[u8],
    kind: &str,
    sync_file: impl FnOnce(&File, &Path) -> Result<(), WorldError>,
) -> Result<(), WorldError> {
    let temp = path(directory, temp_name);
    if !durable_files::path_exists(&temp)? {
        return Ok(());
    }
    let mut file = open_private_file(&temp, expected.len())?;
    let mut bytes = Vec::with_capacity(expected.len());
    (&mut file)
        .take((expected.len() + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read receiver temp", &temp, error))?;
    if bytes.len() > expected.len() {
        return Err(protocol_error(format!("{kind} temp is oversized")));
    }
    durable_files::ensure_file_is_current_path(&file, &temp)?;
    if bytes == expected {
        let target = path_for_temp_target(temp_name, directory)?;
        if durable_files::path_exists(&target)? {
            return Err(protocol_error(format!(
                "{kind} target and completed temp both exist"
            )));
        }
        sync_file(&file, &temp)?;
        durable_files::ensure_file_is_current_path(&file, &temp)?;
        fs::rename(&temp, &target)
            .map_err(|error| io_error("recover complete receiver temp", &target, error))?;
        durable_files::sync_directory(directory)?;
        return Ok(());
    }
    if !expected.starts_with(&bytes) {
        return Err(protocol_error(format!(
            "{kind} temp is not a canonical interrupted write"
        )));
    }
    fs::remove_file(&temp)
        .map_err(|error| io_error("remove incomplete receiver temp", &temp, error))?;
    durable_files::sync_directory(directory)
}

fn path_for_temp_target(temp_name: &str, directory: &Path) -> Result<PathBuf, WorldError> {
    let name = match temp_name {
        INTENT_TEMP_FILE_NAME => INTENT_FILE_NAME,
        STATE_TEMP_FILE_NAME => STATE_FILE_NAME,
        READY_TEMP_FILE_NAME => READY_FILE_NAME,
        _ => return Err(protocol_error("unknown receiver temp file name".into())),
    };
    Ok(path(directory, name))
}

fn require_exact_file(path: &Path, expected: &[u8], kind: &str) -> Result<(), WorldError> {
    let bytes = read_file(path, expected.len())?;
    if bytes != expected {
        return Err(protocol_error(format!("{kind} is missing or invalid")));
    }
    Ok(())
}

fn read_file(path: &Path, maximum: usize) -> Result<Vec<u8>, WorldError> {
    let mut file = open_private_file(path, maximum)?;
    let mut bytes = Vec::new();
    (&mut file)
        .take((maximum + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read receiver file", path, error))?;
    if bytes.len() > maximum {
        return Err(protocol_error("receiver file is oversized".into()));
    }
    durable_files::ensure_file_is_current_path(&file, path)?;
    Ok(bytes)
}

fn open_private_file(path: &Path, maximum: usize) -> Result<File, WorldError> {
    durable_files::ensure_private_regular_file(path)?;
    let metadata =
        fs::metadata(path).map_err(|error| io_error("stat receiver file", path, error))?;
    if metadata.len() > maximum as u64 {
        return Err(protocol_error("receiver file is oversized".into()));
    }
    let file = File::open(path).map_err(|error| io_error("open receiver file", path, error))?;
    durable_files::ensure_private_open_file(&file, path)?;
    durable_files::ensure_file_is_current_path(&file, path)?;
    Ok(file)
}

fn path(directory: &Path, name: &str) -> PathBuf {
    directory.join(name)
}

fn protocol_error(message: String) -> WorldError {
    WorldError::NetworkProtocolUnavailable { protocol: message }
}

fn io_error(operation: &str, path: &Path, error: std::io::Error) -> WorldError {
    durable_files::io_error(operation, path, error)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn binding() -> ReceiverBindingV2 {
        ReceiverBindingV2 {
            schema_version: super::super::RECEIVER_SCHEMA_VERSION,
            local_peer_id: "12D3KooWReceiverFixture".into(),
            world_id: "world-a".into(),
            network_id: "network-a".into(),
            identity_public_key_protobuf_hex: "00".into(),
        }
    }

    fn private_directory(root: &Path) -> PathBuf {
        let directory = root.join("receiver");
        fs::create_dir(&directory).expect("create receiver directory");
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .expect("set private directory mode");
        directory
    }

    fn write_private(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).expect("write fixture");
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .expect("set private file mode");
    }

    #[test]
    fn explicit_initialize_recovers_complete_intent_state_and_ready_temps() {
        let mode = PeerRecordReceiverMode::DurableCompatible;
        let binding = binding();
        let intent_bytes = durable_files::canonical_cbor(&ReceiverInitIntentV2 {
            schema_version: INIT_SCHEMA_VERSION,
            binding: binding.clone(),
            mode,
        })
        .expect("encode init intent");
        let state_bytes = durable_files::canonical_cbor(&ReceiverLedgerV2 {
            schema_version: super::super::RECEIVER_SCHEMA_VERSION,
            binding: binding.clone(),
            mode,
            floors: Default::default(),
        })
        .expect("encode empty receiver state");

        for phase in ["intent", "state", "ready"] {
            let root = tempfile::tempdir_in(
                std::env::temp_dir()
                    .canonicalize()
                    .expect("canonical temp root"),
            )
            .expect("test root");
            let directory = private_directory(root.path());
            match phase {
                "intent" => write_private(&path(&directory, INTENT_TEMP_FILE_NAME), &intent_bytes),
                "state" => {
                    write_private(&path(&directory, INTENT_FILE_NAME), &intent_bytes);
                    write_private(&path(&directory, STATE_TEMP_FILE_NAME), &state_bytes);
                }
                "ready" => {
                    write_private(&path(&directory, INTENT_FILE_NAME), &intent_bytes);
                    write_private(&path(&directory, STATE_FILE_NAME), &state_bytes);
                    write_private(&path(&directory, READY_TEMP_FILE_NAME), READY_MARKER);
                }
                _ => unreachable!(),
            }

            initialize(&directory, binding.clone(), mode)
                .unwrap_or_else(|error| panic!("resume {phase} temp: {error:?}"));
            assert!(!path(&directory, INTENT_FILE_NAME).exists());
            require_exact_file(
                &path(&directory, STATE_FILE_NAME),
                &state_bytes,
                "receiver initial state",
            )
            .expect("completed state");
            require_exact_file(
                &path(&directory, READY_FILE_NAME),
                READY_MARKER,
                "receiver ready marker",
            )
            .expect("completed ready marker");
            assert!(!path(&directory, INTENT_TEMP_FILE_NAME).exists());
            assert!(!path(&directory, STATE_TEMP_FILE_NAME).exists());
            assert!(!path(&directory, READY_TEMP_FILE_NAME).exists());
        }
    }

    #[test]
    fn failed_sync_of_complete_recovery_temp_preserves_it_without_publishing_target() {
        let root = tempfile::tempdir_in(
            std::env::temp_dir()
                .canonicalize()
                .expect("canonical temp root"),
        )
        .expect("test root");
        let directory = private_directory(root.path());
        let expected = b"canonical persisted state";
        let temp_path = path(&directory, STATE_TEMP_FILE_NAME);
        let target_path = path(&directory, STATE_FILE_NAME);
        write_private(&temp_path, expected);

        let result = recover_or_remove_prefix_temp_with_sync(
            &directory,
            STATE_TEMP_FILE_NAME,
            expected,
            "receiver initial state",
            |_file, _path| Err(protocol_error("injected temp sync failure".to_string())),
        );

        assert!(result.is_err());
        assert_eq!(fs::read(&temp_path).expect("retained temp"), expected);
        assert!(!target_path.exists());
    }
}
