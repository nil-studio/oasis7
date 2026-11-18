//! One-way policy transaction. Reuse the initialization barrier filename so
//! older readers and initializers also fail closed while an upgrade is pending.
use super::*;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::libp2p_net::receiver_floor) struct UpgradeIntent {
    pub(in crate::libp2p_net::receiver_floor) schema_version: u16,
    pub(in crate::libp2p_net::receiver_floor) binding: ReceiverBindingV2,
    pub(in crate::libp2p_net::receiver_floor) source_blake3: String,
    pub(in crate::libp2p_net::receiver_floor) target_blake3: String,
}

pub(in crate::libp2p_net::receiver_floor) fn upgrade_to_v2_only(
    directory: &Path,
    binding: ReceiverBindingV2,
) -> Result<(), WorldError> {
    upgrade_with_finish_sync(directory, binding, durable_files::sync_directory)
}

pub(in crate::libp2p_net::receiver_floor) fn upgrade_with_finish_sync(
    directory: &Path,
    binding: ReceiverBindingV2,
    finish_sync: impl FnOnce(&Path) -> Result<(), WorldError>,
) -> Result<(), WorldError> {
    durable_files::require_private_persistence_platform()?;
    durable_files::ensure_existing_private_directory(directory, "receiver")?;
    let lock = open_lock(directory, false)?;
    lock.try_lock().map_err(|error| {
        protocol_error(format!("receiver directory is already in use: {error:?}"))
    })?;
    durable_files::ensure_file_is_current_path(&lock, &path(directory, LOCK_FILE_NAME))?;
    require_exact_file(
        &path(directory, READY_FILE_NAME),
        READY_MARKER,
        "receiver ready marker",
    )?;
    if durable_files::path_exists(&path(directory, READY_TEMP_FILE_NAME))? {
        return Err(protocol_error(
            "receiver initialization is incomplete".into(),
        ));
    }
    let bytes = read_file(&path(directory, STATE_FILE_NAME), RECEIVER_STATE_MAX_BYTES)?;
    let mut ledger: ReceiverLedgerV2 = serde_cbor::from_slice(&bytes)
        .map_err(|error| protocol_error(format!("decode receiver ledger: {error}")))?;
    if durable_files::canonical_cbor(&ledger)? != bytes
        || ledger.schema_version != super::super::RECEIVER_SCHEMA_VERSION
        || ledger.binding != binding
        || !matches!(
            ledger.mode,
            PeerRecordReceiverMode::DurableCompatible | PeerRecordReceiverMode::V2Only
        )
    {
        return Err(protocol_error(
            "receiver upgrade source schema, identity, scope, or mode mismatch".into(),
        ));
    }
    super::super::validate_loaded_ledger(&ledger)?;
    let intent_path = path(directory, INTENT_FILE_NAME);
    let intent_exists = durable_files::path_exists(&intent_path)?;
    let intent_temp_exists = durable_files::path_exists(&path(directory, INTENT_TEMP_FILE_NAME))?;
    if !intent_exists && !intent_temp_exists {
        // Do not adopt an ambiguous interrupted ordinary ingress write.
        reject_incomplete_state(directory)?;
        if ledger.mode == PeerRecordReceiverMode::V2Only {
            return finish_sync(directory);
        }
    }

    // Reconstruct both canonical images from either side of the atomic rename.
    // Every field except mode (including signed bytes) stays byte-for-byte intact.
    ledger.mode = PeerRecordReceiverMode::DurableCompatible;
    let source = durable_files::canonical_cbor(&ledger)?;
    ledger.mode = PeerRecordReceiverMode::V2Only;
    let target = durable_files::canonical_cbor(&ledger)?;
    let intent = durable_files::canonical_cbor(&UpgradeIntent {
        schema_version: 3,
        binding,
        source_blake3: blake3::hash(&source).to_hex().to_string(),
        target_blake3: blake3::hash(&target).to_hex().to_string(),
    })?;
    if intent_exists {
        require_exact_file(&intent_path, &intent, "receiver upgrade intent")?;
        if intent_temp_exists {
            return Err(protocol_error(
                "receiver upgrade intent and temp both exist".into(),
            ));
        }
    } else {
        recover_or_remove_prefix_temp(
            directory,
            INTENT_TEMP_FILE_NAME,
            &intent,
            "receiver upgrade intent",
        )?;
        if !durable_files::path_exists(&intent_path)? {
            write_atomic_initial(
                directory,
                &intent_path,
                INTENT_TEMP_FILE_NAME,
                &intent,
                "receiver upgrade intent",
            )?;
        }
    }
    // A known prefix is a torn write of this exact target, not a second truth.
    // Other bytes are retained for diagnosis and never silently discarded.
    let state_temp = path(directory, STATE_TEMP_FILE_NAME);
    if durable_files::path_exists(&state_temp)? {
        let temp = read_file(&state_temp, target.len())?;
        if !target.starts_with(&temp) {
            return Err(protocol_error(
                "receiver upgrade temp does not match target".into(),
            ));
        }
        fs::remove_file(&state_temp)
            .map_err(|error| io_error("remove interrupted upgrade temp", &state_temp, error))?;
        durable_files::sync_directory(directory)?;
    }
    if bytes != target {
        persist(directory, &target)?;
    }
    require_exact_file(
        &path(directory, STATE_FILE_NAME),
        &target,
        "upgraded receiver ledger",
    )?;
    // Also covers a prior rename whose directory sync returned an unknown result.
    durable_files::sync_directory(directory)?;
    fs::remove_file(&intent_path)
        .map_err(|error| io_error("finish receiver upgrade", &intent_path, error))?;
    finish_sync(directory)
}
