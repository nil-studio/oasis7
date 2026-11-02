//! Local scheduling only; neither Active metadata nor a pending reply grants dispatch.
use super::*;
use oasis7_custody_core::{
    approval_admission::ApprovalAdmissionInput,
    native_model_proxy_query_v2::SignedNativeModelReservationQueryV2,
};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::Weak,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(super) struct LocalRuntime {
    identity: Weak<Mutex<Option<IdentityService>>>,
    client: PathBuf,
    socket: PathBuf,
    // Single assignment: no refresh/rebind/retry after failure or lost reply.
    assigned: Mutex<Option<String>>,
}
impl LocalRuntime {
    pub(super) fn from_env(identity: &Identity) -> Result<Option<Self>, String> {
        let client = std::env::var_os("OASIS7_CUSTODY_MODEL_LOCAL_CLIENT");
        let socket = std::env::var_os("OASIS7_CUSTODY_MODEL_LOCAL_H_SOCKET");
        match (client, socket) {
            (None, None) => Ok(None),
            (Some(client), Some(socket)) => {
                let client = PathBuf::from(client);
                let socket = PathBuf::from(socket);
                if !client.is_absolute() || !socket.is_absolute() {
                    return Err(failure());
                }
                Ok(Some(Self {
                    identity: Arc::downgrade(identity),
                    client,
                    socket,
                    assigned: Mutex::new(None),
                }))
            }
            _ => Err(failure()),
        }
    }
    pub(super) fn notify(&self, signal: super::super::hosted_identity::ActiveModelSignal) {
        let input = signal.into_input();
        let Ok(digest) = input.approved_test_session_v2_digest() else {
            return;
        };
        let Ok(mut assigned) = self.assigned.lock() else {
            return;
        };
        if assigned.is_some() {
            return;
        }
        *assigned = Some(digest);
        let identity = self.identity.clone();
        let client = self.client.clone();
        let socket = self.socket.clone();
        // Native still owns its mutex here. The new thread only locks it after
        // the activation handler returns; no lock is held while awaiting H.
        let _ = std::thread::Builder::new()
            .name("custody-local-n".into())
            .spawn(move || {
                let _ = run(identity, client, socket, input);
            });
    }
}
fn failure() -> String {
    "Native local model handoff unavailable".into()
}
fn now() -> Result<u64, String> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| failure())?
            .as_millis(),
    )
    .map_err(|_| failure())
}
fn run(
    identity: Weak<Mutex<Option<IdentityService>>>,
    client: PathBuf,
    socket: PathBuf,
    input: ApprovalAdmissionInput,
) -> Result<(), String> {
    let oasis7_custody_core::approval::CustodyIntentV1::TestSessionV2 { plan, .. } =
        &input.request.intent
    else {
        return Err(failure());
    };
    let until = plan.write_deadline.0;
    // Presence is only a readiness hint. The fixed CLI subsequently verifies
    // kernel H peer, both root registries and exact private session binding.
    loop {
        if identity.strong_count() == 0 || now()? >= until {
            return Err(failure());
        }
        if std::fs::symlink_metadata(&socket).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let query = {
        let identity = identity.upgrade().ok_or_else(failure)?;
        let mut guard = identity.lock().map_err(|_| failure())?;
        guard
            .as_mut()
            .ok_or_else(failure)?
            .model_proxy_first_reservation_query(&input, now()?)
            .map_err(|_| failure())?
    };
    // Unsupported fixed graph driver/price can never reach here. The actual query
    // factory rechecks completed owner and commits the exact original first.
    send_once(&client, &socket, &query)
}
fn send_once(
    client: &std::path::Path,
    socket: &std::path::Path,
    query: &SignedNativeModelReservationQueryV2,
) -> Result<(), String> {
    let issued_now = now()?;
    let remaining = query
        .query
        .admit_until
        .0
        .checked_sub(issued_now)
        .filter(|v| *v > 0)
        .ok_or_else(failure)?;
    let started = Instant::now();
    let completion_bound = Duration::from_millis(remaining);
    let raw = serde_json::to_vec(query).map_err(|_| failure())?;
    if raw.len() > 32768 {
        return Err(failure());
    }
    let child = Command::new(client)
        .env_clear()
        .arg("custody-native-reserve-call")
        .arg("--socket")
        .arg(socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| failure())?;
    let response = wait_client(
        child,
        raw,
        query.query.admit_until.0,
        completion_bound,
        started,
    )?;
    let pending: Pending = serde_json::from_slice(&response).map_err(|_| failure())?;
    pending.validate(query)
}
fn wait_client(
    mut child: std::process::Child,
    raw: Vec<u8>,
    admit_until: u64,
    completion_bound: Duration,
    started: Instant,
) -> Result<Vec<u8>, String> {
    // A bounded input can exceed a platform pipe buffer. The supervisor must
    // retain the deadline while a stalled client is not reading stdin.
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(failure());
    };
    let writer = match std::thread::Builder::new()
        .name("custody-n-query-write".into())
        .spawn(move || stdin.write_all(&raw).map_err(|_| failure()))
    {
        Ok(writer) => writer,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(failure());
        }
    };
    let outcome = (|| {
        loop {
            if let Some(status) = child.try_wait().map_err(|_| failure())? {
                if !status.success() {
                    return Err(failure());
                }
                break;
            }
            if now()? >= admit_until || started.elapsed() >= completion_bound {
                return Err(failure());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut response = Vec::new();
        child
            .stdout
            .take()
            .ok_or_else(failure)?
            .take(8193)
            .read_to_end(&mut response)
            .map_err(|_| failure())?;
        if response.len() > 8192 {
            return Err(failure());
        }
        Ok(response)
    })();
    if outcome.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let written = writer.join().map_err(|_| failure()).and_then(|value| value);
    // Killing/reaping the fixed client closes its pipe before the writer joins.
    // No retry, floor reconstruction, probe or provider I/O from this result.
    written.and(outcome)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    api_version: u8,
    domain: String,
    session_id: String,
    reservation_id: String,
    query_digest: String,
    status: PendingStatus,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum PendingStatus {
    PaidPending,
    ExactHistory,
}
impl Pending {
    fn validate(&self, query: &SignedNativeModelReservationQueryV2) -> Result<(), String> {
        if self.api_version != 2
            || self.domain != "oasis7.custody.native-model-reservation-pending.v2"
            || self.session_id != query.query.session_id
            || self.reservation_id != query.query.reservation_id
            || self.query_digest
                != oasis7_custody_core::approval::bytes_digest(
                    &serde_json::to_vec(query).map_err(|_| failure())?,
                )
        {
            return Err(failure());
        }
        match self.status {
            PendingStatus::PaidPending | PendingStatus::ExactHistory => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "custody_proxy_local_tests.rs"]
mod tests;
