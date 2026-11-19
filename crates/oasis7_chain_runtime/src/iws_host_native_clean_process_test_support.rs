//! A fresh same-ELF process consumes the shipping configuration, checkpoint,
//! Cold constructor and JSONL reader. Only DATA survives the old owner drop.
//! The parent keeps the original TCP protocol fixture alive; its fixed-key
//! oracle is not evidence of physical custody or a provisioned shipping ELF.
use crate::{
    iws_host_runtime::{IwsLiveNodeSettingsV1, node_checkpoint::IwsNodeCheckpointLimitsV1},
    iws_live_config::{
        CheckpointLimitsDataV1, EndpointConfigV1, IwsLiveConfigV1, ProviderPolicyDataV1,
    },
    iws_live_limits::{InputLimitsDataV1, PageIoLimitsV1, ValueIoLimitsV1, WorkerLimitsDataV1},
    iws_live_work::{ColdWorkConfigV1, IoCeilingsV1},
};
use oasis7::world_service::client::FormalReadTrustConfigV1;
use oasis7_distfs::controlled_authority::fencing::RemoteMonotonicEndpointProvider;
use oasis7_formal_protocol::controlled_authority::formal::{
    InitialActivationTrustV1, VerifiedAuthorityV1, VerifiedFormalCommitV1,
    VerifiedFormalReadAnchorV1,
};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use super::super::endpoint_work;

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub(crate) struct CleanProcessInputV1<'a> {
    pub root: &'a Path,
    pub settings: &'a IwsLiveNodeSettingsV1,
    pub checkpoint_root: &'a Path,
    pub checkpoint_limits: IwsNodeCheckpointLimitsV1,
    pub initial_anchor: &'a VerifiedFormalReadAnchorV1,
    pub trust: &'a InitialActivationTrustV1,
    pub actual_authority: &'a VerifiedAuthorityV1,
    pub actual_head: &'a VerifiedFormalCommitV1,
    pub expected: Value,
}

/// No typed authority, accepted source, provider object or budget owner crosses
/// the complete-release boundary. The child rereads all admission inputs.
pub(crate) struct PreparedCleanProcessV1 {
    config: PathBuf,
    wrong_pin_config: PathBuf,
    expected: Value,
}

fn private_directory(path: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(path).map_err(error)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(error)?;
    fs::canonicalize(path).map_err(error)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(error)?;
    file.write_all(bytes).map_err(error)?;
    file.sync_all().map_err(error)
}

fn endpoint(
    root: &Path,
    name: &str,
    directories: [&Path; 2],
    provider: &RemoteMonotonicEndpointProvider,
    policy: &oasis7_distfs::controlled_authority::fencing::TrustedFencingPolicyV1,
) -> Result<(EndpointConfigV1, u64), String> {
    // These are the actual already provisioned connection parameters. No
    // request, checkpoint DATA or child input registers an oracle identity.
    let (address, timeout, authentication_key) = provider.connection_data_for_fixture_v1();
    let authentication = root.join(format!("{name}.authentication.key"));
    let policy_path = root.join(format!("{name}.provider-policy.json"));
    write_private(&authentication, hex::encode(authentication_key).as_bytes())?;
    write_private(
        &policy_path,
        &serde_json::to_vec(&ProviderPolicyDataV1::from(policy)).map_err(error)?,
    )?;
    Ok((
        EndpointConfigV1 {
            endpoint_directory: private_directory(directories[0])?,
            pages_directory: private_directory(directories[1])?,
            provider_address: address,
            provider_authentication_key_file: authentication,
            provider_policy_file: policy_path,
        },
        u64::try_from(timeout.as_millis()).map_err(error)?,
    ))
}

fn io_limits() -> IoCeilingsV1 {
    // A single new process receives explicit finite local ceilings. The
    // physical/Value and Runtime limits are the established genuine GEN/Cold
    // fixture values; these DATA never copy a spent Common account.
    IoCeilingsV1 {
        physical: endpoint_work::physical().into(),
        logical: endpoint_work::logical().into(),
        steps_per_counter: 1_000_000,
        bytes_per_counter: 512 << 20,
        max_original_bytes: 16 << 20,
        max_heap_bytes: 32 << 20,
        max_depth: 64,
        max_map_keys: 1024,
        max_key_bytes: 4096,
        max_pending_node_bytes: 16 << 20,
        max_pending_pairs: 65_536,
        max_new_rows: 65_536,
        max_synced_pages: 131_072,
        sync_memo_steps: 4_000_000_000,
    }
}

pub(crate) fn prepare(input: CleanProcessInputV1<'_>) -> Result<PreparedCleanProcessV1, String> {
    let CleanProcessInputV1 {
        root,
        settings,
        checkpoint_root,
        checkpoint_limits,
        initial_anchor,
        trust,
        actual_authority,
        actual_head,
        expected,
    } = input;
    let record = hex::encode(actual_head.verified_record_id());
    if expected["record"] != record {
        return Err("clean child expectation differs from actual accepted head".into());
    }
    // The external test operator pins a head derived from the original GEN
    // trust and a genuine accepted source. The checkpoint does not self-pin.
    let current = initial_anchor
        .after_verified_commit(actual_authority.clone(), actual_head.clone())
        .map_err(error)?;
    let checkpoint = current.checkpoint();
    let pin = FormalReadTrustConfigV1 {
        initial_trust: trust.clone(),
        checkpoint_digest: checkpoint.digest().map_err(error)?,
        checkpoint,
    };
    let directory = private_directory(&root.join("clean-child-input"))?;
    let node_directory = private_directory(&directory.join("node-keys"))?;
    let writer_directory = private_directory(&directory.join("writer"))?;
    let writer_key = directory.join("writer.key");
    write_private(
        &writer_key,
        hex::encode(settings.writer.to_bytes()).as_bytes(),
    )?;
    // Provision the separate Node identity before the child. Its production
    // loader and lease still validate ownership, keypair and world binding.
    let node_key = ed25519_dalek::SigningKey::from_bytes(&[91; 32]);
    let node_config = format!(
        "[node]\nprivate_key = \"{}\"\npublic_key = \"{}\"\n",
        hex::encode(node_key.to_bytes()),
        hex::encode(node_key.verifying_key().to_bytes()),
    );
    write_private(
        &node_directory.join("node-keypair.toml"),
        node_config.as_bytes(),
    )?;
    let trust_path = directory.join("formal-read-trust.json");
    write_private(&trust_path, &serde_json::to_vec(&pin).map_err(error)?)?;
    let cold = &settings.cold;
    let (primary, timeout_ms) = endpoint(
        &directory,
        "primary",
        [&cold.endpoint_paths[0], &cold.pages_paths[0]],
        &settings.primary_provider,
        &settings.primary_policy,
    )?;
    let (replica, replica_timeout_ms) = endpoint(
        &directory,
        "replica",
        [&cold.endpoint_paths[1], &cold.pages_paths[1]],
        &settings.replica_provider,
        &settings.replica_policy,
    )?;
    if timeout_ms != replica_timeout_ms {
        return Err("clean child provider timeout configuration differs".into());
    }
    let runtime = crate::iws_live_limits::RuntimeLimitsDataV1::from(
        oasis7_runtime::two_agent_host_work_limits_v1(),
    );
    let io = io_limits();
    let cold_work = ColdWorkConfigV1 {
        runtime: runtime.clone(),
        io: io.clone(),
    };
    let mut observation = io.clone();
    observation.physical = PageIoLimitsV1 {
        reads_remaining: 65_536,
        read_bytes_remaining: 64 << 20,
        writes_remaining: 0,
        write_bytes_remaining: 0,
    };
    observation.logical = ValueIoLimitsV1 {
        nodes_remaining: 65_536,
        read_bytes_remaining: 32 << 20,
        write_bytes_remaining: 32 << 20,
    };
    observation.steps_per_counter = 65_536;
    observation.bytes_per_counter = 64 << 20;
    let mut config = IwsLiveConfigV1 {
        schema_version: 1,
        runtime_mode: crate::iws_live_config::MODE.into(),
        node_id: "iws-clean-cold-child".into(),
        player_id: "sender".into(),
        node_tick_ms: 20,
        start_automatically: false,
        node_keypair_directory: node_directory,
        writer_key_file: writer_key,
        formal_read_trust_file: trust_path,
        writer_directory,
        checkpoint_directory: private_directory(checkpoint_root)?,
        checkpoint_record: record,
        primary,
        replica,
        timeout_ms,
        memory_bytes: 2usize << 30,
        recovery_reserve_bytes: 32 << 20,
        common_steps: 1_000_000_000,
        common_read_bytes: 16u64 << 30,
        cli_steps: 100_000,
        cli_bytes: 16 << 20,
        node_queue_bytes: 1 << 20,
        node_committed_bytes: 1 << 20,
        node_committed_batches: 8,
        runtime_memory: cold.configuration.clone(),
        worker: WorkerLimitsDataV1 {
            address_space_bytes: cold.worker.address_space_bytes,
            max_request_bytes: cold.worker.max_request_bytes,
            max_output_bytes: cold.worker.max_output_bytes,
            wall_time_ms: cold.worker.wall_time_ms,
        },
        current: cold.current_limits.into(),
        endpoint_budgets: cold.budgets.clone().into(),
        projection: cold.projection_limits.into(),
        transaction: settings.policy.transaction.into(),
        input: InputLimitsDataV1 {
            max_wire_bytes: settings.input_limits.max_wire_bytes,
            max_graph_bytes: settings.input_limits.max_graph_bytes,
            max_depth: settings.input_limits.max_depth,
        },
        actions: InputLimitsDataV1 {
            max_wire_bytes: settings.action_limits.max_wire_bytes,
            max_graph_bytes: settings.action_limits.max_graph_bytes,
            max_depth: settings.action_limits.max_depth,
        },
        max_action_bytes: settings.max_action_bytes,
        max_grant_payload_bytes: settings.max_grant_payload_bytes,
        checkpoint_limits: CheckpointLimitsDataV1 {
            max_projection_bytes: checkpoint_limits.max_projection_bytes,
            max_mailbox_entries: checkpoint_limits.max_mailbox_entries,
            max_path_bytes: checkpoint_limits.max_path_bytes,
        },
        initial_cold: [cold_work.clone(), cold_work.clone()],
        recovery_cold: [cold_work.clone(), cold_work],
        runtime_work: runtime,
        normal_io: [io.clone(), io.clone()],
        normal_sources: [io.clone(), io.clone()],
        input_work: io.clone(),
        cognition_work: io,
        observation_work: observation,
        max_request_bytes: 1 << 20,
        max_reply_bytes: 4 << 20,
    };
    let config_path = directory.join("clean-live.json");
    write_private(&config_path, &serde_json::to_vec(&config).map_err(error)?)?;
    // A distinct invalid startup request, never a failed owner's retry. Its
    // original exact-current pin is unchanged while selection names GEN.
    config.checkpoint_record = hex::encode(initial_anchor.head().verified_record_id());
    let wrong_pin_config = directory.join("wrong-selection.json");
    write_private(
        &wrong_pin_config,
        &serde_json::to_vec(&config).map_err(error)?,
    )?;
    Ok(PreparedCleanProcessV1 {
        config: config_path,
        wrong_pin_config,
        expected,
    })
}

// Test supervision only. These bounds do not alter any service, Cold, Common
// or business account. One observation may use the configured 4 MiB reply;
// the two small control replies have a separate finite framing margin.
const CHILD_WALL: Duration = Duration::from_secs(120);
const CHILD_POLL: Duration = Duration::from_millis(10);
const STDOUT_CAP: usize = (4 << 20) + (64 << 10);
const STDERR_CAP: usize = 1 << 20;
const STDIN_CAP: usize = 1024;
type DrainResult = (usize, Result<Vec<u8>, String>);

fn nonblocking(pipe: &impl AsRawFd) -> Result<(), String> {
    // SAFETY: the caller owns this live pipe. Only its existing descriptor
    // flags are read and O_NONBLOCK is added; no raw descriptor is retained.
    let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

struct OwnedChild {
    child: Child,
    cancel: Arc<AtomicBool>,
    drains: Vec<thread::JoinHandle<()>>,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        // Nonblocking readers observe cancellation even if a descendant kept
        // the pipe open after the direct child was killed or exited.
        for drain in self.drains.drain(..) {
            let _ = drain.join();
        }
    }
}
impl OwnedChild {
    fn drain(
        &mut self,
        mut pipe: impl Read + AsRawFd + Send + 'static,
        index: usize,
        cap: usize,
        sender: mpsc::Sender<DrainResult>,
    ) -> Result<(), String> {
        nonblocking(&pipe)?;
        let cancel = self.cancel.clone();
        let drain = thread::Builder::new()
            .name(format!("clean-child-output-{index}"))
            .spawn(move || {
                let result = (|| {
                    let mut bytes = Vec::new();
                    bytes.try_reserve_exact(cap).map_err(error)?;
                    let mut buffer = [0u8; 8192];
                    loop {
                        if cancel.load(Ordering::Acquire) {
                            return Err("clean child output collection cancelled".into());
                        }
                        match pipe.read(&mut buffer) {
                            Ok(0) => return Ok(bytes),
                            Ok(count) => {
                                if count > cap.saturating_sub(bytes.len()) {
                                    return Err(format!(
                                        "clean child output {index} exceeded {cap} bytes"
                                    ));
                                }
                                bytes.extend_from_slice(&buffer[..count]);
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                thread::sleep(CHILD_POLL);
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                            Err(e) => return Err(e.to_string()),
                        }
                    }
                })();
                let _ = sender.send((index, result));
            })
            .map_err(error)?;
        self.drains.push(drain);
        Ok(())
    }
}

fn run_child(path: &Path, commands: &[u8]) -> Result<(u32, std::process::Output), String> {
    if commands.len() > STDIN_CAP {
        return Err("clean child input exceeds the fixed test command bound".into());
    }
    let deadline = Instant::now() + CHILD_WALL;
    let image = std::env::current_exe().map_err(error)?;
    let child = Command::new(image)
        .arg("--formal-live-config")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(error)?;
    let mut owned = OwnedChild {
        child,
        cancel: Arc::new(AtomicBool::new(false)),
        drains: Vec::with_capacity(2),
    };
    let pid = owned.child.id();
    if pid == std::process::id() {
        return Err("clean Cold request did not cross a process boundary".into());
    }
    let stdout = owned
        .child
        .stdout
        .take()
        .ok_or("clean child stdout missing")?;
    let stderr = owned
        .child
        .stderr
        .take()
        .ok_or("clean child stderr missing")?;
    let (sender, receiver) = mpsc::channel();
    owned.drain(stdout, 0, STDOUT_CAP, sender.clone())?;
    owned.drain(stderr, 1, STDERR_CAP, sender)?;
    let mut input = owned
        .child
        .stdin
        .take()
        .ok_or("clean child stdin missing")?;
    nonblocking(&input)?;
    let mut sent = 0;
    while sent < commands.len() {
        if Instant::now() >= deadline {
            return Err("clean child input exceeded the 120s test deadline".into());
        }
        match input.write(&commands[sent..]) {
            Ok(0) => return Err("clean child input pipe stopped accepting bytes".into()),
            Ok(count) => sent += count,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(CHILD_POLL),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    drop(input);
    let mut collected: [Option<Vec<u8>>; 2] = [None, None];
    loop {
        loop {
            match receiver.try_recv() {
                Ok((index, output)) => collected[index] = Some(output?),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if collected.iter().any(Option::is_none) {
                        return Err("clean child output reader ended without a result".into());
                    }
                    break;
                }
            }
        }
        // Success requires the actual child status and EOF on BOTH streams,
        // under the same deadline. Overflow/read errors kill and reap via Drop.
        if Instant::now() >= deadline {
            return Err("clean child startup/read/EOF exceeded the 120s test deadline".into());
        }
        if let Some(status) = owned.child.try_wait().map_err(error)?
            && collected.iter().all(Option::is_some)
        {
            return Ok((
                pid,
                std::process::Output {
                    status,
                    stdout: collected[0]
                        .take()
                        .ok_or("clean child stdout result missing")?,
                    stderr: collected[1]
                        .take()
                        .ok_or("clean child stderr result missing")?,
                },
            ));
        }
        thread::sleep(CHILD_POLL);
    }
}

impl PreparedCleanProcessV1 {
    /// Call only after the parent asserts its complete old pool release.
    pub(crate) fn run(self) -> Result<Value, String> {
        let (refused_pid, refusal) = run_child(&self.wrong_pin_config, b"")?;
        if refusal.status.success()
            || !refusal.stdout.is_empty()
            || !String::from_utf8_lossy(&refusal.stderr)
                .contains("independent pin of the exact selected record")
        {
            return Err(format!(
                "clean child admitted a mismatched independent pin: {}",
                String::from_utf8_lossy(&refusal.stderr),
            ));
        }
        let (pid, output) = run_child(
            &self.config,
            b"{\"command\":\"observe\",\"sender\":\"sender\",\"receiver\":\"receiver\"}\n{\"command\":\"stop\"}\n",
        )?;
        if !output.status.success() {
            return Err(format!(
                "fresh process production Cold/JSONL failed: {}",
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        let lines = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(serde_json::from_slice::<Value>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        if lines.len() != 3
            || lines.iter().any(|line| line["ok"] != true)
            || lines[0]["kind"] != "local_service_stopped"
            || lines[1]["kind"] != "accepted_local_data"
            || lines[2]["kind"] != "clean_checkpoint"
        {
            return Err(format!(
                "fresh process production JSONL contract: {lines:?}"
            ));
        }
        let actual = &lines[1]["data"];
        let balances = json!([
            actual["sender"]["electricity"],
            actual["sender"]["data"],
            actual["receiver"]["electricity"],
            actual["receiver"]["data"],
        ]);
        if actual["record"] != self.expected["record"]
            || actual["body"] != self.expected["body"]
            || actual["height"] != self.expected["height"]
            || actual["sender"]["id"] != "sender"
            || actual["receiver"]["id"] != "receiver"
            || balances != self.expected["balances"]
            || actual["events"] != self.expected["events"]
            || actual["qualified_head_proof"] != self.expected["qualified_head_proof"]
            || lines[2]["data"] != self.expected["record"]
        {
            return Err(
                "fresh process accepted head, balances or ordered Event originals differ".into(),
            );
        }
        Ok(json!({
            "schema":"oasis7.iws.clean_process.fixture.v1",
            "parent_pid":std::process::id(), "child_pid":pid,
            "refused_child_pid":refused_pid,
            "same_fixture_elf":true, "production_config_loader":true,
            "start_automatically":false, "exact_head":actual["record"],
            "height":actual["height"], "balances":balances,
            "ordered_event_originals_match":true,
            "independent_pin_mismatch_refused":true,
            "provider_evidence":"fixed-key TCP protocol fixture; not physical custody",
        }))
    }
}
