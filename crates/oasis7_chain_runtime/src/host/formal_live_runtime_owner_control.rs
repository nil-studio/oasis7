//! Authenticated, deadline-bounded query/stop exchanges for formal owners.
use super::control_wire::{self, DeadlineStream, read_message, write_message};
use super::{
    ControlReply, ControlRequest, FormalRuntimeInstanceV1, FormalRuntimeOwnerSnapshotV1, OWNER_API,
    paths, read_owner_record, validate_snapshot,
};
use std::{
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};

pub(super) fn send_control(
    config_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    action: &str,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    if !matches!(action, "query" | "stop") {
        return Err("unsupported formal runtime owner control action".into());
    }
    let deadline = Instant::now() + timeout.min(Duration::from_secs(60));
    let (_, state_path, socket_path) = paths(config_path, &instance.operation_id)?;
    let stream = UnixStream::connect(&socket_path)
        .map_err(|error| format!("formal runtime owner control unavailable: {error}"))?;
    send_control_on_stream(&state_path, instance, action, deadline, stream)
}

pub(super) fn send_control_on_stream(
    state_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    action: &str,
    deadline: Instant,
    stream: UnixStream,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    Ok(send_control_reply(state_path, instance, action, deadline, stream)?.snapshot)
}

pub(super) fn send_control_reply(
    state_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    action: &str,
    deadline: Instant,
    mut stream: UnixStream,
) -> Result<ControlReply, String> {
    if !matches!(
        action,
        "query" | "stop" | "observe_protected" | "freeze_pending_handoff"
    ) {
        return Err("unsupported formal runtime owner control action".into());
    }
    let remaining = control_wire::remaining(deadline).map_err(|error| error.to_string())?;
    let work_timeout = control_work_timeout(action, remaining)?;
    // The caller's deadline covers every request/reply fragment. A stop must
    // return Pending before that deadline, not spend it all inside runtime.stop.
    let mut stream = DeadlineStream::new(&mut stream, deadline);
    let request = ControlRequest {
        api_version: OWNER_API.into(),
        action: action.into(),
        operation_id: instance.operation_id.clone(),
        request_digest: instance.request_digest.clone(),
        config_digest: instance.config_digest.clone(),
        incarnation: instance.incarnation.clone(),
        timeout_ms: work_timeout.as_millis() as u64,
        durable_set: None,
    };
    write_message(&mut stream, &request)?;
    let reply: ControlReply = read_message(&mut stream)?;
    if reply.api_version != OWNER_API {
        return Err("formal runtime owner response schema mismatch".into());
    }
    validate_snapshot(
        &reply.snapshot,
        &instance.operation_id,
        &instance.request_digest,
        &instance.config_digest,
        Some(&instance.incarnation),
    )?;
    if let Some(record) = read_owner_record(state_path)?
        && record.snapshot != reply.snapshot
    {
        return Err("formal runtime owner durable/live status mismatch".into());
    }
    Ok(reply)
}

// Reserve transport, durable result publication and reply time *inside* the
// caller budget. Small budgets retain the same ratio instead of growing the
// caller deadline; sub-millisecond wire budgets cannot authorize a stop.
pub(super) fn control_work_timeout(action: &str, timeout: Duration) -> Result<Duration, String> {
    let timeout = timeout.min(Duration::from_secs(60));
    let work = if matches!(
        action,
        "stop" | "observe_protected" | "freeze_pending_handoff"
    ) {
        timeout - (timeout / 2).min(Duration::from_secs(2))
    } else {
        timeout
    };
    if work.as_millis() == 0 {
        return Err("formal runtime owner control budget is too small".into());
    }
    Ok(work)
}

pub(super) fn validate_control(
    request: &ControlRequest,
    instance: &FormalRuntimeInstanceV1,
) -> Result<(), String> {
    if request.api_version != OWNER_API
        || (request.action == "durable_set") != request.durable_set.is_some()
        || request.operation_id != instance.operation_id
        || request.request_digest != instance.request_digest
        || request.config_digest != instance.config_digest
        || request.incarnation != instance.incarnation
        || !(1..=60_000).contains(&request.timeout_ms)
    {
        return Err("formal runtime control request differs from exact owner incarnation".into());
    }
    Ok(())
}

pub(super) fn configure_control_stream(stream: &UnixStream) -> Result<(), String> {
    // BSD may inherit the listener's nonblocking flag. Control frames use
    // bounded blocking reads after accept.
    stream
        .set_nonblocking(false)
        .map_err(|error| format!("configure owner control blocking mode: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("configure owner control read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("configure owner control write timeout: {error}"))
}
