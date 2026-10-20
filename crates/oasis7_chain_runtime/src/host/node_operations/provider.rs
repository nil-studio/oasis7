//! Fixed-command SSH/systemd adapter and its remote action agent.
//!
//! The SSH command line is constant; all request data travels as bounded JSON
//! on stdin. A remote action intent is persisted before systemd is touched.
//! A pending intent is query-only after restart.
use super::super::node_lifecycle::{LocalInstance, actions, local_io as io};
use super::model::{LinuxSshSystemdTargetV1, ManagedTargetV1, Record};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};
#[cfg(target_os = "linux")]
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_AGENT_MESSAGE: usize = 64 * 1024;
#[cfg(target_os = "linux")]
const MAX_AGENT_REPLY: usize = 1024 * 1024;
#[cfg(target_os = "linux")]
const AGENT_COMMAND: &str = "/usr/bin/oasis7_chain_runtime op agent";
const NODE_EXECUTABLE: &str = "/usr/bin/oasis7_chain_runtime";
const SYSTEMCTL: &str = "/usr/bin/systemctl";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RemoteStepV1 {
    Start,
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RemoteOutcomeV1 {
    Pending,
    Running,
    Stopped,
    Unknown,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RemoteActionReceiptV1 {
    pub api_version: String,
    pub operation_id: String,
    pub action_id: String,
    pub request_digest: String,
    pub operation_request_digest: String,
    pub node_id: String,
    pub world_id: String,
    pub chain_id: String,
    pub step: RemoteStepV1,
    pub outcome: RemoteOutcomeV1,
    pub expected_generation: u64,
    pub expected_instance: Option<LocalInstance>,
    pub instance: Option<LocalInstance>,
    pub blocker: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentTargetV1 {
    node_id: String,
    service_user: String,
    unit: String,
    config: PathBuf,
    data_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AgentModeV1 {
    Query,
    Apply,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AgentRequestV1 {
    api_version: String,
    mode: AgentModeV1,
    operation_id: String,
    action_id: String,
    request_digest: String,
    operation_request_digest: String,
    operation_request_json: String,
    world_id: String,
    chain_id: String,
    target: AgentTargetV1,
    step: RemoteStepV1,
    expected_generation: u64,
    expected_config_digest: String,
    expected_instance: Option<LocalInstance>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentReplyV1 {
    api_version: String,
    receipt: Option<RemoteActionReceiptV1>,
}

pub(super) trait AgentTransport {
    fn exchange(
        &mut self,
        target: &LinuxSshSystemdTargetV1,
        message: &[u8],
    ) -> Result<Vec<u8>, String>;
}

struct SshTransport;

#[path = "provider_agent.rs"]
mod agent;
#[cfg(test)]
use agent::{Agent, SystemdRunner, receipt, validate_unit_contents};
pub(super) use agent::{run_agent_stdin, validate_receipt};

impl AgentTransport for SshTransport {
    fn exchange(
        &mut self,
        target: &LinuxSshSystemdTargetV1,
        message: &[u8],
    ) -> Result<Vec<u8>, String> {
        validate_target(target)?;
        if message.len() > MAX_AGENT_MESSAGE {
            return Err("remote action request size limit".into());
        }
        #[cfg(not(target_os = "linux"))]
        return Err("Linux SSH/systemd provider is unsupported on this host".into());
        #[cfg(target_os = "linux")]
        {
            let mut child = Command::new("/usr/bin/ssh")
                .env_clear()
                .arg("-T")
                .arg("-p")
                .arg(target.port.to_string())
                .arg("-oBatchMode=yes")
                .arg("-oIdentitiesOnly=yes")
                .arg("-oStrictHostKeyChecking=yes")
                .arg(format!(
                    "-oUserKnownHostsFile={}",
                    target.known_hosts_file.display()
                ))
                .arg("-i")
                .arg(&target.identity_file)
                .arg("--")
                .arg(format!("{}@{}", target.username, target.host))
                // This is a literal command with no request-derived bytes.
                .arg(AGENT_COMMAND)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| format!("start fixed SSH client: {error}"))?;
            let stdout = child.stdout.take().ok_or("SSH stdout unavailable")?;
            let stderr = child.stderr.take().ok_or("SSH stderr unavailable")?;
            let stdout_thread = thread::spawn(move || read_capped(stdout, MAX_AGENT_REPLY));
            let stderr_thread = thread::spawn(move || read_capped(stderr, 16 * 1024));
            let mut stdin = child.stdin.take().ok_or("SSH stdin unavailable")?;
            let request_bytes = message.to_vec();
            let stdin_thread = thread::spawn(move || {
                stdin
                    .write_all(&request_bytes)
                    .map_err(|error| format!("write SSH request: {error}"))
            });
            let timeout = Duration::from_millis(target.timeout_ms);
            let started = Instant::now();
            let status = loop {
                match child.try_wait().map_err(|error| error.to_string())? {
                    Some(status) => break Some(status),
                    None if started.elapsed() < timeout => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    None => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                }
            };
            let stdin_result = stdin_thread
                .join()
                .map_err(|_| "SSH stdin writer panicked")?;
            let stdout = stdout_thread
                .join()
                .map_err(|_| "SSH stdout reader panicked")??;
            let stderr = stderr_thread
                .join()
                .map_err(|_| "SSH stderr reader panicked")??;
            let Some(status) = status else {
                return Err("ORIGINAL_OUTCOME_UNKNOWN: SSH deadline elapsed; query the same action identity".into());
            };
            stdin_result?;
            if !status.success() {
                return Err(format!(
                    "ORIGINAL_OUTCOME_UNKNOWN: SSH exited with {status}; {}",
                    String::from_utf8_lossy(&stderr).trim()
                ));
            }
            Ok(stdout)
        }
    }
}

#[cfg(target_os = "linux")]
fn read_capped(reader: impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err("remote helper response size limit".into());
    }
    Ok(bytes)
}

pub(super) fn validate_target(target: &LinuxSshSystemdTargetV1) -> Result<(), String> {
    validate_target_shape(target)?;
    for path in [&target.identity_file, &target.known_hosts_file] {
        io::reject_symlinks(path)?;
        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        let max_bytes = if path == &target.identity_file {
            1024 * 1024
        } else {
            4 * 1024 * 1024
        };
        if !metadata.is_file() || metadata.len() > max_bytes {
            return Err("SSH credential/known-hosts path is not a bounded regular file".into());
        }
    }
    super::super::controlled_live_config::require_private_owned(&target.identity_file, false)?;
    Ok(())
}

fn validate_target_shape(target: &LinuxSshSystemdTargetV1) -> Result<(), String> {
    if target.port == 0
        || !(1..=60_000).contains(&target.timeout_ms)
        || !safe_host(&target.host)
        || !safe_user(&target.username)
        || !safe_unit(&target.unit)
        || !safe_node_id(&target.node_id)
        || !target.remote_config.is_absolute()
        || !target.remote_data_dir.is_absolute()
        || !safe_remote_path(&target.remote_config)
        || !safe_remote_path(&target.remote_data_dir)
        || target.remote_config != target.remote_data_dir.join("node.json")
        || !target.identity_file.is_absolute()
        || !target.known_hosts_file.is_absolute()
    {
        return Err("invalid Linux SSH/systemd target identity or paths".into());
    }
    Ok(())
}

fn safe_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte))
        && !value.starts_with('-')
}
fn safe_user(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        && !value.starts_with('-')
}
fn safe_unit(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@_.:-".contains(&byte))
        && !value.starts_with('-')
}
fn safe_node_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}
fn safe_remote_path(path: &Path) -> bool {
    let value = path.to_string_lossy();
    path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
        && value.len() <= 4096
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-".contains(&byte))
}

pub(super) fn request_for(record: &Record, step: RemoteStepV1) -> Result<AgentRequestV1, String> {
    let Some(ManagedTargetV1::LinuxSshSystemd(target)) = &record.request.managed_target else {
        return Err("managed SSH/systemd target missing".into());
    };
    validate_target_shape(target)?;
    let action_id = record.action_id(match step {
        RemoteStepV1::Start => "start",
        RemoteStepV1::Stop => "stop",
    });
    let expected_instance = (step == RemoteStepV1::Stop)
        .then(|| record.request.expected_instance.clone())
        .flatten();
    let operation_request_digest = record.request_digest.clone();
    let operation_request_json =
        serde_json::to_string(&record.request).map_err(|error| error.to_string())?;
    if operation_request_json.len() > MAX_AGENT_MESSAGE / 2 {
        return Err("remote operation request exceeds bounded agent envelope".into());
    }
    let agent_target = AgentTargetV1 {
        node_id: target.node_id.clone(),
        service_user: target.username.clone(),
        unit: target.unit.clone(),
        config: target.remote_config.clone(),
        data_dir: target.remote_data_dir.clone(),
    };
    let request_digest = action_digest(ActionDigestV1 {
        operation_request_digest: &operation_request_digest,
        operation_id: &record.request.operation_id,
        action_id: &action_id,
        world_id: &record.request.world_id,
        chain_id: &record.request.chain_id,
        target: &agent_target,
        step,
        expected_generation: record.request.expected_generation,
        expected_config_digest: &record.request.expected_config_digest,
        expected_instance: expected_instance.as_ref(),
    })?;
    Ok(AgentRequestV1 {
        api_version: "oasis7.remote-operation-agent/v1".into(),
        mode: AgentModeV1::Query,
        operation_id: record.request.operation_id.clone(),
        action_id,
        request_digest,
        operation_request_digest,
        operation_request_json,
        world_id: record.request.world_id.clone(),
        chain_id: record.request.chain_id.clone(),
        target: agent_target,
        step,
        expected_generation: record.request.expected_generation,
        expected_config_digest: record.request.expected_config_digest.clone(),
        expected_instance,
    })
}

struct ActionDigestV1<'a> {
    operation_request_digest: &'a str,
    operation_id: &'a str,
    action_id: &'a str,
    world_id: &'a str,
    chain_id: &'a str,
    target: &'a AgentTargetV1,
    step: RemoteStepV1,
    expected_generation: u64,
    expected_config_digest: &'a str,
    expected_instance: Option<&'a LocalInstance>,
}

fn action_digest(input: ActionDigestV1<'_>) -> Result<String, String> {
    io::digest(&(
        "oasis7.remote-operation-action/v1",
        input.operation_request_digest,
        input.operation_id,
        input.action_id,
        input.world_id,
        input.chain_id,
        input.target,
        input.step,
        input.expected_generation,
        input.expected_config_digest,
        input.expected_instance,
    ))
}

pub(super) fn query_record(
    record: &Record,
    step: RemoteStepV1,
) -> Result<Option<RemoteActionReceiptV1>, String> {
    let target = remote_target(record)?;
    let request = request_for(record, step)?;
    exchange(target, &request)
}

pub(super) fn apply_record(
    record: &Record,
    step: RemoteStepV1,
) -> Result<Option<RemoteActionReceiptV1>, String> {
    let target = remote_target(record)?;
    let mut request = request_for(record, step)?;
    request.mode = AgentModeV1::Apply;
    exchange(target, &request)
}

fn remote_target(record: &Record) -> Result<&LinuxSshSystemdTargetV1, String> {
    let Some(ManagedTargetV1::LinuxSshSystemd(target)) = &record.request.managed_target else {
        return Err("managed SSH/systemd target missing".into());
    };
    validate_target(target)?;
    Ok(target)
}

fn exchange(
    target: &LinuxSshSystemdTargetV1,
    request: &AgentRequestV1,
) -> Result<Option<RemoteActionReceiptV1>, String> {
    let mut transport = SshTransport;
    exchange_with(target, request, &mut transport)
}

fn exchange_with<T: AgentTransport>(
    target: &LinuxSshSystemdTargetV1,
    request: &AgentRequestV1,
    transport: &mut T,
) -> Result<Option<RemoteActionReceiptV1>, String> {
    let mut bytes = serde_json::to_vec(request).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    let reply = transport.exchange(target, &bytes)?;
    let response: AgentReplyV1 = serde_json::from_slice(&reply)
        .map_err(|error| format!("invalid bounded remote helper reply: {error}"))?;
    if response.api_version != "oasis7.remote-operation-agent-reply/v1" {
        return Err("unsupported remote helper reply schema".into());
    }
    if let Some(receipt) = &response.receipt
        && (receipt.api_version != "oasis7.remote-action-receipt/v1"
            || receipt.operation_id != request.operation_id
            || receipt.action_id != request.action_id
            || receipt.request_digest != request.request_digest
            || receipt.operation_request_digest != request.operation_request_digest
            || receipt.node_id != request.target.node_id
            || receipt.world_id != request.world_id
            || receipt.chain_id != request.chain_id
            || receipt.step != request.step
            || receipt.expected_generation != request.expected_generation
            || receipt.expected_instance != request.expected_instance)
    {
        return Err("remote action receipt belongs to another request/instance".into());
    }
    Ok(response.receipt)
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
