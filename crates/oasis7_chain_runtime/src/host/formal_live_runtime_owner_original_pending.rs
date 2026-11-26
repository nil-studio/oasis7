use super::*;

pub(super) fn owner_root(config_path: &Path) -> Result<PathBuf, String> {
    let validated = super::FormalLiveConfigV1::load(config_path)?;
    Ok(validated.config.writer_directory.join("runtime-owner"))
}

pub(super) fn operation_root(root: &Path, operation_id: &str) -> Result<PathBuf, String> {
    if operation_id.is_empty()
        || operation_id.len() > 128
        || !operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_:".contains(&byte))
    {
        return Err("invalid formal runtime owner operation identity".into());
    }
    let digest = super::super::super::node_lifecycle::local_io::digest(&operation_id)?;
    Ok(root.join("operations").join(digest))
}

pub(super) fn paths(
    config_path: &Path,
    operation_id: &str,
) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let root = owner_root(config_path)?;
    let operation = operation_root(&root, operation_id)?;
    Ok((
        root,
        operation.join("owner.json"),
        super::socket_path(config_path, operation_id)?,
    ))
}

pub(crate) fn freeze_original_pending(
    config_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    timeout: Duration,
) -> Result<super::super::formal_live_original_pending_producer::OriginalPendingPinV1, String> {
    if timeout.is_zero() || timeout > Duration::from_secs(60) {
        return Err("original pending owner control timeout in (0,60s]".into());
    }
    let (_, state_path, socket_path) = paths(config_path, &instance.operation_id)?;
    let current = read_owner_record(&state_path)?.ok_or("original pending owner receipt missing")?;
    if current.snapshot.instance != *instance || current.snapshot.state != "ready" {
        return Err("original pending freeze requires the exact ready owner".into());
    }
    let stream = UnixStream::connect(socket_path)
        .map_err(|_| "original pending exact owner control unavailable")?;
    let reply = control::send_control_reply(
        &state_path,
        instance,
        "freeze_original_pending",
        Instant::now() + timeout,
        stream,
    )?;
    if let Some(error) = reply.control_error {
        return Err(error);
    }
    if reply.snapshot.state != "stopped" {
        return Err("original pending owner freeze did not stop the exact Node".into());
    }
    let pin = reply
        .original_pending_pin
        .ok_or("original pending owner freeze reply omitted its prepared pin")?;
    let expected_owner =
        super::super::formal_live_original_pending_producer::OriginalPendingOwnerV1 {
            authority_id: instance.authority_id.clone(),
            operation_id: instance.operation_id.clone(),
            request_digest: instance.request_digest.clone(),
            config_digest: instance.config_digest.clone(),
            incarnation: instance.incarnation.clone(),
        };
    pin.validate(&expected_owner)?;
    Ok(pin)
}

pub(crate) fn read_original_pending_pin_candidate(
    config_path: &Path,
    instance: &FormalRuntimeInstanceV1,
) -> Result<super::super::formal_live_original_pending_producer::OriginalPendingPinV1, String> {
    let (root, state_path, _) = paths(config_path, &instance.operation_id)?;
    let record = read_owner_record(&state_path)?.ok_or("original pending owner receipt missing")?;
    validate_snapshot(
        &record.snapshot,
        &instance.operation_id,
        &instance.request_digest,
        &instance.config_digest,
        Some(&instance.incarnation),
    )?;
    if !matches!(record.snapshot.state.as_str(), "stopping" | "stopped" | "unknown") {
        return Err("original pending pin candidate requires a freeze attempt".into());
    }
    let directory = operation_root(&root, &instance.operation_id)?.join("original-pending");
    let expected_owner =
        super::super::formal_live_original_pending_producer::OriginalPendingOwnerV1 {
            authority_id: instance.authority_id.clone(),
            operation_id: instance.operation_id.clone(),
            request_digest: instance.request_digest.clone(),
            config_digest: instance.config_digest.clone(),
            incarnation: instance.incarnation.clone(),
        };
    super::super::formal_live_original_pending_producer::read_prepared_pin_candidate(
        &directory,
        &expected_owner,
    )
}

pub(crate) fn cold_read_original_pending_for_owner(
    config_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    independent_root: &str,
    independent_count: usize,
) -> Result<oasis7_formal_protocol::controlled_authority::formal::RecoveryPendingHandoffV1, String> {
    let (root, state_path, _) = paths(config_path, &instance.operation_id)?;
    let record = read_owner_record(&state_path)?.ok_or("original pending owner receipt missing")?;
    validate_snapshot(
        &record.snapshot,
        &instance.operation_id,
        &instance.request_digest,
        &instance.config_digest,
        Some(&instance.incarnation),
    )?;
    if !matches!(record.snapshot.state.as_str(), "stopping" | "stopped" | "unknown") {
        return Err("original pending cold read requires a non-serving exact owner".into());
    }
    let directory = operation_root(&root, &instance.operation_id)?.join("original-pending");
    let expected_owner =
        super::super::formal_live_original_pending_producer::OriginalPendingOwnerV1 {
            authority_id: instance.authority_id.clone(),
            operation_id: instance.operation_id.clone(),
            request_digest: instance.request_digest.clone(),
            config_digest: instance.config_digest.clone(),
            incarnation: instance.incarnation.clone(),
        };
    super::super::formal_live_original_pending_producer::cold_read_exact_frozen(
        &directory,
        &expected_owner,
        independent_root,
        independent_count,
    )
}
