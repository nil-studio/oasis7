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
    mut stream: UnixStream,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    if !matches!(action, "query" | "stop") {
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
    Ok(reply.snapshot)
}

// Reserve transport, durable result publication and reply time *inside* the
// caller budget. Small budgets retain the same ratio instead of growing the
// caller deadline; sub-millisecond wire budgets cannot authorize a stop.
pub(super) fn control_work_timeout(action: &str, timeout: Duration) -> Result<Duration, String> {
    let timeout = timeout.min(Duration::from_secs(60));
    let work = if action == "stop" {
        timeout - (timeout / 2).min(Duration::from_secs(2))
    } else {
        timeout
    };
    if work.as_millis() == 0 {
        return Err("formal runtime owner control budget is too small".into());
    }
    Ok(work)
}
