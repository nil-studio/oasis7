//! Small exact-owner freeze control; original bytes remain in a bounded private
//! immutable export, then the independent issuer binds them into its plan.
use super::*;
use oasis7_distfs::controlled_authority::formal::RecoveryPendingHandoffV1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingHandoffExportV1 {
    pub(crate) root: String,
    pub(crate) count: usize,
    pub(crate) path: PathBuf,
}

pub(super) fn freeze(
    runtime: &FormalLiveRuntime,
    timeout: Duration,
) -> Result<PendingHandoffExportV1, String> {
    let pending = runtime.freeze_pending_handoff(timeout)?;
    Ok(PendingHandoffExportV1 {
        root: pending.root().map_err(|error| error.to_string())?,
        count: pending.requests.len(),
        path: runtime.pending_handoff_export_path().to_path_buf(),
    })
}

/// The export is local owner evidence, not protected acceptance or a recovery
/// permission. The caller must pass these exact bytes to its verified issuer.
pub(crate) fn control_freeze_pending_handoff(
    config_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    timeout: Duration,
) -> Result<RecoveryPendingHandoffV1, String> {
    if timeout.is_zero() || timeout > Duration::from_secs(60) {
        return Err("pending handoff owner control timeout in (0,60s]".into());
    }
    let deadline = Instant::now() + timeout;
    let (_, state_path, socket_path) = paths(config_path, &instance.operation_id)?;
    let current = read_owner_record(&state_path)?.ok_or("pending handoff owner receipt missing")?;
    if current.snapshot.instance != *instance
        || !matches!(current.snapshot.state.as_str(), "ready" | "frozen")
    {
        return Err("pending handoff control requires the exact live or frozen owner".into());
    }
    let stream = UnixStream::connect(socket_path).map_err(|error| error.to_string())?;
    let reply = control::send_control_reply(
        &state_path,
        instance,
        "freeze_pending_handoff",
        deadline,
        stream,
    )?;
    if let Some(error) = reply.control_error {
        return Err(error);
    }
    let export = reply
        .pending_handoff
        .ok_or("original pending handoff freeze outcome unknown")?;
    if reply.snapshot.instance != *instance || reply.snapshot.state != "frozen" {
        return Err("pending handoff freeze reply differs from exact frozen owner".into());
    }
    let validated = FormalLiveConfigV1::load(config_path)?;
    let expected_path = validated
        .config
        .writer_directory
        .join("pending-handoff.cbor");
    if config_digest(config_path)? != instance.config_digest || export.path != expected_path {
        return Err("pending handoff freeze config or original export path changed".into());
    }
    let pending =
        super::super::pending_handoff::read_export(&expected_path, &instance.authority_id)?
            .ok_or("frozen owner is missing its complete original handoff")?;
    if pending.root().map_err(|error| error.to_string())? != export.root
        || pending.requests.len() != export.count
    {
        return Err("pending handoff private export differs from exact owner receipt".into());
    }
    if Instant::now() >= deadline {
        return Err(
            "OutcomeUnknown: pending handoff freeze readback exceeded original deadline".into(),
        );
    }
    Ok(pending)
}

#[cfg(test)]
pub(crate) fn wait_for_exact_owner_release(
    config: &Path,
    instance: &FormalRuntimeInstanceV1,
    timeout: Duration,
) -> Result<(), String> {
    let (_, state_path, _) = paths(config, &instance.operation_id)?;
    let record = read_owner_record(&state_path)?.ok_or("stopped owner receipt missing")?;
    if record.snapshot.instance != *instance || record.snapshot.state != "stopped" {
        return Err("cold fixture requires the exact stopped owner receipt".into());
    }
    let deadline = Instant::now() + timeout;
    while !super::attempt::owner_lock_available(config)? {
        if Instant::now() >= deadline {
            return Err("stopped owner has not released its exact owner resources".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
