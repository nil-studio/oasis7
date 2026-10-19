//! Fresh OS-process writer acceptance. Files carry bounded fixture inputs and
//! actual read receipts, never an in-memory World, Arc, or validated cache.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    process::{Child, Command, Stdio},
};
#[path = "formal_live_protected_full_node_cold_spool.rs"]
mod spool;
#[path = "formal_live_protected_full_node_cold_worker.rs"]
mod worker;
const WORKER: &str = "formal_live_runtime::tests::protected_process_tests::full_node_storage_tests::cold_process::worker::formal_full_node_cold_writer_worker";
const INPUT_ENV: &str = "OASIS7_FULL_NODE_COLD_WORKER_INPUT";
const LIMIT: usize = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    schema: u32,
    nonce: String,
    parent_pid: u32,
    runtime_config: PathBuf,
    owner: Client,
    identity: WorldIdentity,
    owner_key: String,
    original: SubmitIntentRequest<WorldServicePayloadV1>,
    original_commit: oasis7_client_api::world_service::CommitRef,
    requested: u64,
    remaining_ms: u64,
    fixed_head: String,
    fixed_position: u64,
    authority: String,
    typed: Value,
    result: String,
    effect: String,
    emits: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Client {
    formal_read_trust: Value,
    endpoint: String,
    trusted_service_public_key: String,
    expected_world: WorldIdentity,
    scope_id: String,
    read_private_key_hex: String,
    timeout_ms: u64,
    max_response_bytes: usize,
}
impl Client {
    fn capture(value: &WorldServiceClientConfig) -> Result<Self, String> {
        Ok(Self {
            formal_read_trust: serde_json::to_value(&value.formal_read_trust)
                .map_err(|e| e.to_string())?,
            endpoint: value.endpoint.clone(),
            trusted_service_public_key: value.trusted_service_public_key.clone(),
            expected_world: value.expected_world.clone(),
            scope_id: value.scope_id.clone(),
            read_private_key_hex: value.read_private_key_hex.clone(),
            timeout_ms: value
                .timeout
                .as_millis()
                .try_into()
                .map_err(|_| "client timeout overflow")?,
            max_response_bytes: value.max_response_bytes,
        })
    }
    fn restore(&self) -> Result<WorldServiceClientConfig, String> {
        Ok(WorldServiceClientConfig {
            formal_read_trust: serde_json::from_value(self.formal_read_trust.clone())
                .map_err(|e| e.to_string())?,
            endpoint: self.endpoint.clone(),
            trusted_service_public_key: self.trusted_service_public_key.clone(),
            expected_world: self.expected_world.clone(),
            scope_id: self.scope_id.clone(),
            read_private_key_hex: self.read_private_key_hex.clone(),
            timeout: Duration::from_millis(self.timeout_ms),
            max_response_bytes: self.max_response_bytes,
        })
    }
}
impl Input {
    #[expect(
        clippy::too_many_arguments,
        reason = "Exact cold acceptance binds the original fixture and independently read receipts"
    )]
    pub(super) fn new(
        config: &Path,
        owner: &WorldServiceClientConfig,
        identity: &WorldIdentity,
        owner_key: &SigningKey,
        original: &SubmitIntentRequest<WorldServicePayloadV1>,
        original_commit: &oasis7_client_api::world_service::CommitRef,
        requested: u64,
        fixed_head: &str,
        fixed_position: u64,
        authority: &str,
        typed: Value,
        result: String,
        effect: String,
        emits: usize,
        budget: &Budget,
    ) -> Result<Self, String> {
        let remaining = (budget.started + budget.maximum)
            .checked_duration_since(Instant::now())
            .filter(|v| !v.is_zero())
            .ok_or("cold worker whole-case deadline expired")?;
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
        Ok(Self {
            schema: 1,
            nonce: hex::encode(nonce),
            parent_pid: std::process::id(),
            runtime_config: config.to_owned(),
            owner: Client::capture(owner)?,
            identity: identity.clone(),
            owner_key: hex::encode(owner_key.to_bytes()),
            original: original.clone(),
            original_commit: original_commit.clone(),
            requested,
            remaining_ms: remaining
                .as_millis()
                .try_into()
                .map_err(|_| "cold deadline overflow")?,
            fixed_head: fixed_head.into(),
            fixed_position,
            authority: authority.into(),
            typed,
            result,
            effect,
            emits,
        })
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    schema: u32,
    nonce: String,
    parent_pid: u32,
    child_pid: u32,
    stage: String,
    fixed_head: String,
    authority: String,
    pub(super) typed: Value,
    result: String,
    effect: String,
    emits: usize,
    original_commit: Option<oasis7_client_api::world_service::CommitRef>,
    final_position: u64,
    continuation_completed: bool,
    ready_node_running: bool,
}
fn validate(input: &Input, receipt: &Receipt, child: u32, stage: &str) -> Result<(), String> {
    if receipt.schema != 1
        || receipt.nonce != input.nonce
        || receipt.parent_pid != input.parent_pid
        || receipt.parent_pid != std::process::id()
        || receipt.child_pid != child
        || child == std::process::id()
        || receipt.stage != stage
        || receipt.fixed_head != input.fixed_head
        || receipt.authority != input.authority
        || receipt.typed != input.typed
        || receipt.result != input.result
        || receipt.effect != input.effect
        || receipt.emits != input.emits
        || receipt.ready_node_running
        || (stage == "ready" && receipt.final_position != input.fixed_position)
        || (stage == "done" && receipt.final_position <= input.fixed_position)
        || (stage == "ready"
            && (receipt.continuation_completed || receipt.original_commit.is_some()))
        || (stage == "done"
            && (!receipt.continuation_completed
                || receipt.original_commit.as_ref() != Some(&input.original_commit)))
    {
        return Err("fresh writer receipt PID/nonce/stage/original-read binding mismatch".into());
    }
    Ok(())
}
fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("cold worker IPC byte budget".into());
    }
    oracle_support::write_private(path, &bytes)
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    crate::controlled_live_config::require_private_owned(path, false)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("cold IPC regular file required".into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("cold IPC byte budget".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
// Atomic visibility prevents a reader from seeing a partially written receipt.
fn publish<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let temporary = path.with_extension("writing");
    write(&temporary, value)?;
    if path.try_exists().map_err(|e| e.to_string())? {
        return Err("cold IPC receipt already exists".into());
    }
    fs::rename(temporary, path).map_err(|e| e.to_string())
}
pub(super) struct Session {
    root: oracle_support::PrivateRoot,
    child: Child,
    output: spool::Output,
    input: Input,
    complete: bool,
}
impl Session {
    pub(super) fn spawn(input: Input, deadline: Instant) -> Result<Self, String> {
        if Instant::now() >= deadline {
            return Err("cold worker spawn deadline expired".into());
        }
        let root = oracle_support::PrivateRoot::new()?;
        let path = root.path().join("input.json");
        write(&path, &input)?;
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .env(INPUT_ENV, path)
            .args([
                WORKER,
                "--ignored",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        // No process-group/session changes: CI retains ownership of this child.
        let output = match spool::Output::start(&mut child, &root.path().join("worker.log")) {
            Ok(output) => output,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        Ok(Self {
            root,
            child,
            output,
            input,
            complete: false,
        })
    }
    fn await_receipt(&mut self, stage: &str, deadline: Instant) -> Result<Receipt, String> {
        let path = self.root.path().join(format!("{stage}.json"));
        loop {
            if Instant::now() >= deadline {
                return Err(format!("cold writer {stage} exceeded whole-case deadline"));
            }
            if self.output.failed() {
                return Err("cold worker output exceeded bounded spool or failed".into());
            }
            if path.try_exists().map_err(|e| e.to_string())? {
                let receipt = read(&path)?;
                validate(&self.input, &receipt, self.child.id(), stage)?;
                return Ok(receipt);
            }
            if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
                return Err(format!("cold worker exited before {stage}: {status}"));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
    pub(super) fn ready(&mut self, deadline: Instant) -> Result<Receipt, String> {
        let receipt = self.await_receipt("ready", deadline)?;
        if self.child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("cold worker exited instead of waiting at ready".into());
        }
        Ok(receipt)
    }
    pub(super) fn resume(&self) -> Result<(), String> {
        publish(
            &self.root.path().join("continue.json"),
            &(
                1_u32,
                &self.input.nonce,
                self.input.parent_pid,
                self.child.id(),
            ),
        )
    }
    pub(super) fn finish(&mut self, deadline: Instant) -> Result<Receipt, String> {
        let receipt = self.await_receipt("done", deadline)?;
        loop {
            if Instant::now() >= deadline {
                return Err("cold writer completion exit exceeded whole-case deadline".into());
            }
            if self.output.failed() {
                return Err("cold worker output failure".into());
            }
            if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
                if !status.success() {
                    return Err(format!("cold worker completion exited {status}"));
                }
                self.output.finish(deadline)?;
                self.complete = true;
                return Ok(receipt);
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        terminate(&mut self.child);
        self.output.stop();
        if !self.complete {
            self.output.failure_tail();
        }
    }
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn test_input() -> Input {
    let identity = WorldIdentity {
        world_id: "cold-ipc".into(),
        genesis_digest: fixtures::h(8),
    };
    let key = fixtures::key(71);
    let commit = serde_json::from_value(serde_json::json!({
        "world":identity,"binding":{"provider_world_id":"cold-ipc","branch_id":"main",
        "finality_ref":"fixed","reorg_generation":0,"governing_manifest_ref":"governed",
        "authority_generation":1,"permission_generation":1},"position":1,
        "execution_block_hash":"original-block","state_root_ref":"original-state"
    }))
    .unwrap();
    Input {
        schema: 1,
        nonce: "ab".repeat(32),
        parent_pid: std::process::id(),
        runtime_config: "unused".into(),
        owner: Client {
            formal_read_trust: Value::Null,
            endpoint: "http://127.0.0.1:1".into(),
            trusted_service_public_key: "unused".into(),
            expected_world: identity.clone(),
            scope_id: "agent:agent".into(),
            read_private_key_hex: hex::encode(key.to_bytes()),
            timeout_ms: 30000,
            max_response_bytes: 16 * 1024 * 1024,
        },
        original: request_for_owner(&key, &identity),
        identity,
        owner_key: hex::encode(key.to_bytes()),
        original_commit: commit,
        requested: 10,
        remaining_ms: 10000,
        fixed_head: "fixed".into(),
        fixed_position: 1,
        authority: "authority".into(),
        typed: serde_json::json!({"records":1,"digest":"actual"}),
        result: "result".into(),
        effect: "effect".into(),
        emits: 1,
    }
}
#[test]
fn cold_writer_receipt_rejects_pid_nonce_and_actual_read_mismatches() {
    let input = test_input();
    let child = std::process::id().checked_add(1).unwrap();
    let mut receipt = Receipt {
        schema: 1,
        nonce: input.nonce.clone(),
        parent_pid: input.parent_pid,
        child_pid: child,
        stage: "ready".into(),
        fixed_head: input.fixed_head.clone(),
        authority: input.authority.clone(),
        typed: input.typed.clone(),
        result: input.result.clone(),
        effect: input.effect.clone(),
        emits: input.emits,
        original_commit: None,
        final_position: 1,
        continuation_completed: false,
        ready_node_running: false,
    };
    validate(&input, &receipt, child, "ready").unwrap();
    receipt.child_pid = std::process::id();
    assert!(validate(&input, &receipt, child, "ready").is_err());
    receipt.child_pid = child;
    receipt.nonce = "stale".into();
    assert!(validate(&input, &receipt, child, "ready").is_err());
    receipt.nonce = input.nonce.clone();
    receipt.typed = Value::Null;
    assert!(validate(&input, &receipt, child, "ready").is_err());
    receipt.typed = input.typed.clone();
    receipt.continuation_completed = true;
    assert!(validate(&input, &receipt, child, "ready").is_err());
    receipt.stage = "done".into();
    assert!(validate(&input, &receipt, child, "done").is_err());
    receipt.original_commit = Some(input.original_commit.clone());
    assert!(validate(&input, &receipt, child, "done").is_err());
    receipt.final_position = 2;
    validate(&input, &receipt, child, "done").unwrap();
    receipt.authority = "other".into();
    assert!(validate(&input, &receipt, child, "done").is_err());
}
#[test]
fn cold_worker_ready_timeout_kills_and_reaps_actual_owned_process() {
    let root = oracle_support::PrivateRoot::new().unwrap();
    let mut child = Command::new("sleep")
        .arg("60")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let output = spool::Output::start(&mut child, &root.path().join("worker.log")).unwrap();
    let mut session = Session {
        root,
        child,
        output,
        input: test_input(),
        complete: false,
    };
    assert!(
        session
            .ready(Instant::now() + Duration::from_millis(25))
            .is_err()
    );
    drop(session);
    // SAFETY: signal zero probes only this known, already reaped child PID.
    assert_eq!(unsafe { libc::kill(pid as libc::pid_t, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}
