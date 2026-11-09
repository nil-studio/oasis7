//! Canonical execution record normalization and durable record publication.
use super::*;

pub(in super::super) fn load_execution_bridge_record(
    path: &Path,
) -> Result<ExecutionBridgeRecord, String> {
    let bytes = fs::read(path).map_err(|err| {
        format!(
            "read execution bridge record {} failed: {}",
            path.display(),
            err
        )
    })?;
    serde_json::from_slice::<ExecutionBridgeRecord>(bytes.as_slice()).map_err(|err| {
        format!(
            "parse execution bridge record {} failed: {}",
            path.display(),
            err
        )
    })
}

pub(super) fn normalize_execution_bridge_record_for_persist(
    record: &ExecutionBridgeRecord,
) -> ExecutionBridgeRecord {
    let mut normalized = record.clone();
    normalized.schema_version = EXECUTION_BRIDGE_RECORD_SCHEMA_V3;
    if normalized.latest_state_ref.is_none() {
        normalized.latest_state_ref = normalized.snapshot_ref.clone();
    }
    normalized
}

pub(in super::super) fn persist_execution_bridge_record_only(
    execution_records_dir: &Path,
    record: &ExecutionBridgeRecord,
) -> Result<Vec<u8>, String> {
    let normalized = normalize_execution_bridge_record_for_persist(record);
    let bytes = serde_json::to_vec_pretty(&normalized)
        .map_err(|err| format!("serialize execution bridge record failed: {}", err))?;
    let path = execution_bridge_record_path(execution_records_dir, normalized.height);
    if normalized.controlled_capture_ref.is_some() {
        super::super::durable_transaction::write_file_durable(&path, &bytes)?;
    } else {
        write_bytes_atomic(path.as_path(), bytes.as_slice())?;
    }
    Ok(bytes)
}

pub(in super::super) fn persist_execution_bridge_record(
    execution_records_dir: &Path,
    record: &ExecutionBridgeRecord,
) -> Result<(), String> {
    let bytes = persist_execution_bridge_record_only(execution_records_dir, record)?;
    let latest_path = execution_records_dir.join("latest.json");
    if record.controlled_capture_ref.is_some() {
        super::super::durable_transaction::write_file_durable(&latest_path, &bytes)
    } else {
        write_bytes_atomic(latest_path.as_path(), bytes.as_slice())
    }
}
