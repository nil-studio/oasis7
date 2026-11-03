//! Independent Read Directory protected configuration. Historical verification
//! is available while admission is disabled; no ordinary or Execution fallback.
use crate::directory_config::denied;
use oasis7_custody_core::{
    DecimalU64, read_directory_admission_v3::ReadDirectoryAdmissionConfigV3,
    read_directory_v3::ReadDirectoryVerifierV3,
};
use worker::{Env, Result};
pub(crate) fn configuration(env: &Env) -> Result<ReadDirectoryAdmissionConfigV3> {
    let value = |key| env.var(key).map(|v| v.to_string()).map_err(|_| denied());
    let capacity = value("CUSTODY_READ_DIRECTORY_ADMISSION_CAPACITY")?;
    if capacity.is_empty()
        || capacity.len() > 10
        || capacity.starts_with('0')
        || !capacity.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(denied());
    }
    let parsed = capacity.parse::<u32>().map_err(|_| denied())?;
    if parsed == 0 || parsed.to_string() != capacity {
        return Err(denied());
    }
    let history = value("CUSTODY_READ_DIRECTORY_ADMISSION_VERIFIER_HISTORY")?;
    let history: Vec<ReadDirectoryVerifierV3> =
        ReadDirectoryVerifierV3::decode_history(history.as_bytes()).map_err(|_| denied())?;
    let enabled = env
        .var("CUSTODY_READ_DIRECTORY_V3_ENABLED")
        .ok()
        .is_some_and(|v| v.to_string() == "true");
    let active = if enabled {
        let raw = value("CUSTODY_READ_DIRECTORY_ADMISSION_ACTIVE_VERIFIER")?;
        let active: ReadDirectoryVerifierV3 =
            ReadDirectoryVerifierV3::decode_configuration(raw.as_bytes()).map_err(|_| denied())?;
        if active.native_key_id != value("CUSTODY_NATIVE_APPROVAL_KEY_ID")?
            || active.native_public_key != value("CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY")?
        {
            return Err(denied());
        }
        Some(active)
    } else {
        None
    };
    let environment = value("CUSTODY_NATIVE_ENVIRONMENT")?;
    if history.iter().any(|v| {
        v.directory_pin.environment != environment || v.coordinator_pin.environment != environment
    }) {
        return Err(denied());
    }
    let result = ReadDirectoryAdmissionConfigV3 {
        capacity: DecimalU64(u64::from(parsed)),
        active,
        history,
    };
    result.validate().map_err(|_| denied())?;
    Ok(result)
}
