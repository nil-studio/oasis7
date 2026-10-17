//! Shipping world-init CLI, externally signed approval and real protected P/R.
//! Custody oracle is a controlled fixture; no deployment/physical-fencing claim.
use super::*;
use ed25519_dalek::Signer;
use oasis7_distfs::controlled_authority::formal::{
    coordinator::FormalDurabilityEndpoint,
    endpoint::{FormalQueryOutcomeV1, FormalRequestStateV1},
};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(serde::Deserialize)]
struct PublicPreparedRequest {
    world: FormalWorldIdentityV1,
    activation_body: InitialActivationBodyV1,
    request_digest: String,
    trusted_issuer_public_key: String,
}
const CLI_IDLE: Duration = Duration::from_secs(40);
#[derive(Clone)]
struct CliWait {
    expires: Instant,
    progress: Arc<std::sync::Mutex<CliProgress>>,
}
struct CliProgress {
    last: Instant,
    completed: u64,
    last_rpc: String,
    error: Option<String>,
}
impl CliWait {
    fn new() -> Self {
        Self {
            expires: Instant::now() + Duration::from_secs(5 * 60),
            progress: Arc::new(std::sync::Mutex::new(CliProgress {
                last: Instant::now(),
                completed: 0,
                last_rpc: "none".into(),
                error: None,
            })),
        }
    }
    fn begin(&self) {
        self.progress.lock().unwrap().last = Instant::now();
    }
    fn failure(&self) -> Result<Option<String>, String> {
        let progress = self.progress.lock().map_err(|_| "CLI progress lock")?;
        let reason = if Instant::now() >= self.expires {
            Some("five-minute scenario deadline")
        } else if progress.last.elapsed() >= CLI_IDLE {
            Some("40s without a completed P/R RPC")
        } else {
            None
        };
        Ok(progress.error.clone().or_else(|| {
            reason.map(|reason| {
                format!(
                    "{reason}; completed={} last_rpc={} idle={:?}",
                    progress.completed,
                    progress.last_rpc,
                    progress.last.elapsed()
                )
            })
        }))
    }
}
type CliOutput = thread::JoinHandle<Result<Vec<u8>, String>>;
struct CliChild(
    Child,
    Option<CliOutput>,
    Option<CliOutput>,
    String,
    Arc<AtomicBool>,
    Option<CliWait>,
);
impl Drop for CliChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        self.4.store(true, Ordering::Release);
    }
}
fn drain_cli(
    mut pipe: impl Read + std::os::fd::AsRawFd + Send + 'static,
    stopped: Arc<AtomicBool>,
) -> CliOutput {
    thread::spawn(move || {
        // A descendant may inherit the pipe after the shipping child exits.
        // Nonblocking reads plus cancellation keep collection inside the test
        // deadline even when such a descendant retains an open descriptor.
        let fd = std::os::fd::AsRawFd::as_raw_fd(&pipe);
        // SAFETY: the reader owns this live pipe descriptor for this thread.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        const LIMIT: usize = 8 * 1024 * 1024;
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        let mut overflow = false;
        let mut after_stop = 0usize;
        loop {
            if stopped.load(Ordering::Acquire) && after_stop >= LIMIT {
                break;
            }
            let count = match pipe.read(&mut buffer) {
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if stopped.load(Ordering::Acquire) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => return Err(error.to_string()),
            };
            if count == 0 {
                break;
            }
            if stopped.load(Ordering::Acquire) {
                after_stop += count;
            }
            let keep = count.min(LIMIT - bytes.len());
            bytes.extend_from_slice(&buffer[..keep]);
            overflow |= keep != count;
        }
        if overflow {
            return Err("world-init CLI output exceeds 8 MiB".into());
        }
        Ok(bytes)
    })
}
fn start_cli(binary: &Path, args: &[&str]) -> Result<CliChild, String> {
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Drain from spawn, including while the fault gate waits for Prepare.
    // Waiting for exit before reading a full pipe can deadlock the child.
    let stopped = Arc::new(AtomicBool::new(false));
    let stdout = drain_cli(
        child.stdout.take().ok_or("world-init stdout missing")?,
        stopped.clone(),
    );
    let stderr = drain_cli(
        child.stderr.take().ok_or("world-init stderr missing")?,
        stopped.clone(),
    );
    let stage = args.iter().take(3).copied().collect::<Vec<_>>().join(" ");
    Ok(CliChild(
        child,
        Some(stdout),
        Some(stderr),
        stage,
        stopped,
        None,
    ))
}
fn collected(reader: Option<CliOutput>) -> Result<Vec<u8>, String> {
    reader
        .ok_or("world-init output reader missing")?
        .join()
        .map_err(|_| "world-init output reader panicked")?
}
fn finish_cli(mut child: CliChild, success: bool) -> Result<Vec<u8>, String> {
    let started = Instant::now();
    // Keep the original 40s inactivity bound. Native CLI work may span many
    // completed RPC pages inside the one fixed scenario expiry.
    let budget = CLI_IDLE;
    let mut timed_out = false;
    let mut timeout_reason = None;
    while child.0.try_wait().map_err(|e| e.to_string())?.is_none() {
        timeout_reason = if let Some(wait) = &child.5 {
            wait.failure()?
        } else {
            (started.elapsed() >= budget).then(|| "40s process deadline".to_owned())
        };
        if timeout_reason.is_some() {
            timed_out = true;
            child.0.kill().map_err(|e| e.to_string())?;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let status = child.0.wait().map_err(|e| e.to_string())?;
    if !timed_out && let Some(wait) = &child.5 {
        timeout_reason = wait.failure()?;
        timed_out = timeout_reason.is_some();
    }
    child.4.store(true, Ordering::Release);
    let mut stdout = collected(child.1.take())?;
    let stderr = collected(child.2.take())?;
    eprintln!(
        "world-init CLI stage={} elapsed={:?} status={status}",
        child.3,
        started.elapsed()
    );
    if timed_out || status.success() != success {
        return Err(format!(
            "world-init stage={} expected success={success} timed_out={timed_out} budget={budget:?} reason={timeout_reason:?}: {status}: stdout={} stderr={}",
            child.3,
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        ));
    }
    if !success {
        // The shipping tracing subscriber may write diagnostics to stdout.
        stdout.extend_from_slice(&stderr);
    }
    Ok(stdout)
}
#[test]
fn world_init_cli_drains_both_pipes_before_waiting_for_exit() {
    let child = start_cli(
        Path::new("/bin/sh"),
        &[
            "-c",
            "head -c 131072 /dev/zero; head -c 131072 /dev/zero >&2",
        ],
    )
    .unwrap();
    let stdout = finish_cli(child, true).unwrap();
    assert_eq!(stdout, vec![0; 131072]);
}
#[test]
fn world_init_cli_collection_does_not_wait_for_inherited_pipe_eof() {
    let (mut input, output) = std::os::unix::net::UnixStream::pair().unwrap();
    let inherited = input.try_clone().unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let reader = drain_cli(output, stopped.clone());
    input.write_all(b"original child output").unwrap();
    drop(input);
    stopped.store(true, Ordering::Release);
    assert_eq!(collected(Some(reader)).unwrap(), b"original child output");
    drop(inherited);
}
#[test]
fn world_init_cli_progress_requires_completed_rpc_and_preserves_total_deadline() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut wait = CliWait::new();
    let proxy =
        ReplicaPrepareGate::start(target.local_addr().unwrap(), false, "primary", wait.clone())
            .unwrap();
    let frame = serde_cbor::to_vec(&serde_json::json!({"body":{"command":"Head"}})).unwrap();
    let original = frame.clone();
    let (arrived, observed) = std::sync::mpsc::channel();
    let (release, response_ready) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = target.accept().unwrap();
        assert_eq!(read_frame(&mut stream).unwrap(), original);
        arrived.send(()).unwrap();
        response_ready.recv().unwrap();
        write_frame(&mut stream, b"original response").unwrap();
    });
    let address = proxy.address;
    let client = thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        write_frame(&mut stream, &frame).unwrap();
        assert_eq!(read_frame(&mut stream).unwrap(), b"original response");
    });
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    {
        let mut progress = wait.progress.lock().unwrap();
        assert_eq!(progress.completed, 0);
        progress.last = Instant::now() - Duration::from_secs(41);
    }
    assert!(wait.failure().unwrap().unwrap().contains("40s without"));
    release.send(()).unwrap();
    client.join().unwrap();
    server.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while wait.progress.lock().unwrap().completed == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert!(wait.failure().unwrap().is_none());
    assert_eq!(wait.progress.lock().unwrap().last_rpc, "primary:Head");
    wait.expires = Instant::now() - Duration::from_secs(1);
    wait.begin();
    assert!(
        wait.failure()
            .unwrap()
            .unwrap()
            .contains("five-minute scenario")
    );
}
fn p(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| "world-init non UTF8 fixture path".into())
}
fn approve(
    request: &Path,
    approval: &Path,
) -> Result<
    (
        PublicPreparedRequest,
        SignedInitialActivationV1,
        InitialActivationTrustV1,
    ),
    String,
> {
    let bytes = std::fs::read(request).map_err(|e| e.to_string())?;
    let prepared: PublicPreparedRequest =
        serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
    if prepared.trusted_issuer_public_key != fixtures::public(9)
        || prepared.world != prepared.activation_body.authority.world
        || prepared.activation_body.authority.profile_version != 2
    {
        return Err("world-init prepared request differs from independently provisioned issuer/world contract".into());
    }
    let certificate = SignedInitialActivationV1 {
        signature_hex: hex::encode(
            fixtures::key(9)
                .sign(
                    &initial_activation_signing_bytes(&prepared.activation_body)
                        .map_err(|e| e.to_string())?,
                )
                .to_bytes(),
        ),
        body: prepared.activation_body.clone(),
    };
    let trust = InitialActivationTrustV1 {
        issuer_public_key: fixtures::public(9),
        world: prepared.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate
            .body
            .recovery_policy
            .digest()
            .map_err(|e| e.to_string())?,
    };
    verify_initial_activation(&certificate, &trust).map_err(|e| e.to_string())?;
    oracle_support::write_private(
        approval,
        &serde_json::to_vec(&serde_json::json!({"certificate":certificate}))
            .map_err(|e| e.to_string())?,
    )?;
    Ok((prepared, certificate, trust))
}
fn activation_args<'a>(
    config: &'a Path,
    request: &'a Path,
    approval: &'a Path,
    key: &'a Path,
    primary: &'a str,
    replica: &'a str,
    output: &'a Path,
) -> Result<Vec<&'a str>, String> {
    Ok(vec![
        "world",
        "init",
        "activate",
        "--config",
        p(config)?,
        "--request",
        p(request)?,
        "--certificate",
        p(approval)?,
        "--writer-key",
        p(key)?,
        "--primary",
        primary,
        "--replica",
        replica,
        "--timeout-ms",
        "30000",
        "--output",
        p(output)?,
    ])
}
fn run_initialization() -> Result<(), String> {
    // Linux CI observed 77s of successful original native transfer before the
    // first Prepare. Bound inactivity across both endpoints, not total pages.
    let wait = CliWait::new();
    let execute = |binary: &Path, args: &[&str], success| {
        wait.begin();
        let mut child = start_cli(binary, args)?;
        child.5 = Some(wait.clone());
        finish_cli(child, success)
    };
    let binary = PathBuf::from(
        std::env::var_os("OASIS7_FORMAL_NODE_TEST_BIN")
            .ok_or("set shipping OASIS7_FORMAL_NODE_TEST_BIN")?,
    );
    let endpoint_binary = std::env::var_os("OASIS7_FORMAL_ENDPOINT_TEST_BIN")
        .ok_or("set OASIS7_FORMAL_ENDPOINT_TEST_BIN")?;
    let root_guard = oracle_support::PrivateRoot::new()?;
    let root = root_guard.path();
    let pdir = root.join("p");
    let rdir = root.join("r");
    oracle_support::private_directory(&pdir)?;
    oracle_support::private_directory(&rdir)?;
    let (mut world, _trust, seed, _proof) = fixtures::actual_world_initial_fixture();
    world.initialize_empty_formal_history()?;
    let identity = seed.authority().definition().world.clone();
    let mut definition = seed.authority().definition().clone();
    definition.profile_version = 2;
    definition.primary.instance.storage_fingerprint =
        directory_fingerprint(&pdir).map_err(|e| e.to_string())?;
    definition.replica.instance.storage_fingerprint =
        directory_fingerprint(&rdir).map_err(|e| e.to_string())?;
    let snapshot = serde_cbor::to_vec(&world.snapshot()).map_err(|e| e.to_string())?;
    let journal = serde_cbor::to_vec(world.journal()).map_err(|e| e.to_string())?;
    let snapshot_path = root.join("source-snapshot.cbor");
    let journal_path = root.join("source-journal.cbor");
    oracle_support::write_private(&snapshot_path, &snapshot)?;
    oracle_support::write_private(&journal_path, &journal)?;
    let config = root.join("world-init.json");
    let mut config_value = serde_json::json!({"schema_version":2,"mode":"genesis","world":identity,
        "snapshot_file":snapshot_path,"journal_file":journal_path,"trusted_issuer_public_key":fixtures::public(9),
        "node_bootstrap":{"height":0,"consensus_block_hash":fixtures::h(81),"execution_block_hash":fixtures::h(82),"execution_state_root":blake3_hex(&snapshot)},
        "authority":definition,"recovery_policy":seed.authority().policy()});
    oracle_support::write_private(
        &config,
        &serde_json::to_vec(&config_value).map_err(|e| e.to_string())?,
    )?;
    let request = root.join("original-request.cbor");
    let approval = root.join("original-approval.json");
    let output = root.join("original-qualified.cbor");
    let writer_key = root.join("writer-key");
    oracle_support::write_private(
        &writer_key,
        hex::encode(fixtures::key(4).to_bytes()).as_bytes(),
    )?;
    let prepare_args = [
        "world",
        "init",
        "prepare",
        "--config",
        p(&config)?,
        "--request",
        p(&request)?,
    ];
    if !cfg!(target_os = "linux") {
        let error = execute(&binary, &prepare_args, false)?;
        if !String::from_utf8_lossy(&error).contains("Unavailable") || request.exists() {
            return Err(format!(
                "unsupported native platform did not reject preparation without creating a request (request_exists={}): {}",
                request.exists(),
                String::from_utf8_lossy(&error),
            ));
        }
        return Ok(());
    }
    execute(&binary, &prepare_args, true)?;
    let request_bytes = std::fs::read(&request).map_err(|e| e.to_string())?;
    execute(&binary, &prepare_args, true)?;
    if std::fs::read(&request).map_err(|e| e.to_string())? != request_bytes {
        return Err("world-init repeated prepare changed original request".into());
    }
    let (prepared, certificate, trust) = approve(&request, &approval)?;
    if prepared.world != seed.authority().definition().world {
        return Err("world-init prepared new identity".into());
    }
    let quote = execute(
        &binary,
        &[
            "world",
            "init",
            "quote",
            "--config",
            p(&config)?,
            "--request",
            p(&request)?,
            "--certificate",
            p(&approval)?,
        ],
        true,
    )?;
    let quote: Value = serde_json::from_slice(&quote).map_err(|e| e.to_string())?;
    if quote["request_digest"] != prepared.request_digest
        || quote["world"] != serde_json::to_value(&prepared.world).map_err(|e| e.to_string())?
    {
        return Err("world-init quote did not bind original externally approved request".into());
    }
    let authority = verify_initial_activation(&certificate, &trust).map_err(|e| e.to_string())?;
    let oracle_fixture = oracle_support::Fixture::new(
        root.to_path_buf(),
        pdir.clone(),
        rdir.clone(),
        authority.clone(),
    );
    let p_oracle = oracle_support::start_oracle(&oracle_fixture, EndpointRole::Primary);
    let r_oracle = oracle_support::start_oracle(&oracle_fixture, EndpointRole::Replica);
    let oracle_auth = root.join("oracle-auth");
    oracle_support::write_private(&oracle_auth, hex::encode([17; 32]).as_bytes())?;
    let pcfg = root.join("p-endpoint.json");
    let rcfg = root.join("r-endpoint.json");
    write_endpoint_config(
        &pcfg,
        &pdir,
        &oracle_auth,
        p_oracle.address(),
        EndpointRole::Primary,
        &trust,
        &certificate,
    )?;
    write_endpoint_config(
        &rcfg,
        &rdir,
        &oracle_auth,
        r_oracle.address(),
        EndpointRole::Replica,
        &trust,
        &certificate,
    )?;
    let (_pchild, paddr) = oracle_support::start_endpoint(&endpoint_binary, &pcfg)?;
    let (_rchild, raddr) = oracle_support::start_endpoint(&endpoint_binary, &rcfg)?;
    let primary_proxy = ReplicaPrepareGate::start(paddr, false, "primary", wait.clone())?;
    let gate = ReplicaPrepareGate::start(raddr, true, "replica", wait.clone())?;
    let paddr_text = primary_proxy.address.to_string();
    let gate_text = gate.address.to_string();
    let raddr_text = gate.address.to_string();
    wait.begin();
    let mut interrupted = start_cli(
        &binary,
        &activation_args(
            &config,
            &request,
            &approval,
            &writer_key,
            &paddr_text,
            &gate_text,
            &output,
        )?,
    )?;
    interrupted.5 = Some(wait.clone());
    // Observe completed RPCs at both endpoints: primary verification can keep
    // progressing while replica is idle. Every step shares the original expiry.
    while !gate.blocked.load(Ordering::Acquire) {
        if let Some(status) = interrupted.0.try_wait().map_err(|e| e.to_string())? {
            let diagnostics = finish_cli(interrupted, status.success())?;
            return Err(format!(
                "world-init exited before durable boundary: {status}: {}",
                String::from_utf8_lossy(&diagnostics),
            ));
        }
        if let Some(error) = gate.error.lock().map_err(|_| "gate error lock")?.clone() {
            return Err(error);
        }
        if let Some(reason) = wait.failure()? {
            return Err(reason);
        }
        thread::sleep(Duration::from_millis(5));
    }
    interrupted.0.kill().map_err(|e| e.to_string())?;
    let killed = interrupted.0.wait().map_err(|e| e.to_string())?;
    if killed.signal() != Some(9) || output.exists() {
        return Err("world-init interruption did not preserve unknown/unpublished result".into());
    }
    gate.release();
    let genesis = authority
        .definition()
        .world
        .genesis_head()
        .map_err(|e| e.to_string())?;
    let mut pendpoint = RemoteFormalEndpoint::new(
        paddr,
        Duration::from_secs(30),
        authority.clone(),
        EndpointRole::Primary,
        fixtures::key(4),
        genesis.clone(),
    )
    .map_err(|e| e.to_string())?;
    let mut rendpoint = RemoteFormalEndpoint::new(
        raddr,
        Duration::from_secs(30),
        authority.clone(),
        EndpointRole::Replica,
        fixtures::key(4),
        genesis.clone(),
    )
    .map_err(|e| e.to_string())?;
    if !matches!(
        pendpoint
            .query(&prepared.request_digest)
            .map_err(|e| e.to_string())?,
        FormalQueryOutcomeV1::Recorded(state) if matches!(*state, FormalRequestStateV1::Prepared { .. })
    ) || !matches!(
        rendpoint
            .query(&prepared.request_digest)
            .map_err(|e| e.to_string())?,
        FormalQueryOutcomeV1::NotRecorded
    ) {
        return Err("world-init did not retain exactly primary durable Prepare".into());
    }
    let args = activation_args(
        &config,
        &request,
        &approval,
        &writer_key,
        &paddr_text,
        &raddr_text,
        &output,
    )?;
    execute(&binary, &args, true)?;
    let result = std::fs::read(&output).map_err(|e| e.to_string())?;
    let publication: crate::formal_live_config::ApprovedInitialPublicationV1 =
        serde_cbor::from_slice(&result).map_err(|e| e.to_string())?;
    verify_formal_qualified(&publication.proof, &authority, None, None)
        .map_err(|e| e.to_string())?;
    let record = &publication.proof.decision.record;
    if record.position != 1
        || record.world != prepared.world
        || record.parent_record_id != genesis.record_id
        || record.request_digest != prepared.request_digest
    {
        return Err("world-init resumed another initial identity/request".into());
    }
    execute(&binary, &args, true)?;
    if std::fs::read(&output).map_err(|e| e.to_string())? != result {
        return Err("world-init duplicate activate changed original qualified result".into());
    }
    let head = pendpoint.head().map_err(|e| e.to_string())?;
    if head.position != 1 || head != rendpoint.head().map_err(|e| e.to_string())? {
        return Err("world-init repeated activation created another head".into());
    }
    // The issuer may approve an alternate bootstrap; endpoint reconciliation
    // must still refuse a conflicting Initial at the same World/position.
    let conflict_config = root.join("conflict-config.json");
    config_value["node_bootstrap"]["consensus_block_hash"] = Value::String(fixtures::h(99));
    oracle_support::write_private(
        &conflict_config,
        &serde_json::to_vec(&config_value).map_err(|e| e.to_string())?,
    )?;
    let conflict_request = root.join("conflict-request.cbor");
    let conflict_approval = root.join("conflict-approval.json");
    let conflict_output = root.join("conflict-result.cbor");
    execute(
        &binary,
        &[
            "world",
            "init",
            "prepare",
            "--config",
            p(&conflict_config)?,
            "--request",
            p(&conflict_request)?,
        ],
        true,
    )?;
    let (conflict, _, _) = approve(&conflict_request, &conflict_approval)?;
    if conflict.request_digest == prepared.request_digest {
        return Err("world-init conflict fixture did not alter original obligation".into());
    }
    execute(
        &binary,
        &activation_args(
            &conflict_config,
            &conflict_request,
            &conflict_approval,
            &writer_key,
            &paddr_text,
            &raddr_text,
            &conflict_output,
        )?,
        false,
    )?;
    if conflict_output.exists()
        || pendpoint.head().map_err(|e| e.to_string())? != head
        || rendpoint.head().map_err(|e| e.to_string())? != head
        || std::fs::read(&snapshot_path).map_err(|e| e.to_string())? != snapshot
        || std::fs::read(&journal_path).map_err(|e| e.to_string())? != journal
    {
        return Err("world-init conflict changed original qualified head or source World".into());
    }
    Ok(())
}

struct ReplicaPrepareGate {
    address: SocketAddr,
    blocked: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    error: Arc<std::sync::Mutex<Option<String>>>,
    hold_prepare: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl ReplicaPrepareGate {
    fn start(
        target: SocketAddr,
        hold: bool,
        role: &'static str,
        wait: CliWait,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let blocked = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(std::sync::Mutex::new(None));
        let hold_prepare = Arc::new(AtomicBool::new(hold));
        let held = hold_prepare.clone();
        let (b, s, e) = (blocked.clone(), stop.clone(), error.clone());
        let worker = thread::spawn(move || {
            while !s.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut incoming, _)) => {
                        let outcome = (|| -> Result<(), String> {
                            incoming.set_nonblocking(false).map_err(|e| e.to_string())?;
                            incoming
                                .set_read_timeout(Some(Duration::from_secs(30)))
                                .map_err(|e| e.to_string())?;
                            let frame = read_frame(&mut incoming)?;
                            let probe: CommandProbe =
                                serde_cbor::from_slice(&frame).map_err(|e| e.to_string())?;
                            if probe.body.command.0 == "Prepare" && held.load(Ordering::Acquire) {
                                b.store(true, Ordering::Release);
                                while !s.load(Ordering::Acquire) && held.load(Ordering::Acquire) {
                                    thread::sleep(Duration::from_millis(5));
                                }
                                return Ok(());
                            }
                            let mut outgoing =
                                TcpStream::connect_timeout(&target, Duration::from_secs(30))
                                    .map_err(|e| e.to_string())?;
                            outgoing
                                .set_read_timeout(Some(Duration::from_secs(30)))
                                .map_err(|e| e.to_string())?;
                            outgoing
                                .set_write_timeout(Some(Duration::from_secs(30)))
                                .map_err(|e| e.to_string())?;
                            incoming
                                .set_write_timeout(Some(Duration::from_secs(30)))
                                .map_err(|e| e.to_string())?;
                            write_frame(&mut outgoing, &frame)?;
                            let response = read_frame(&mut outgoing)?;
                            write_frame(&mut incoming, &response)?;
                            let mut progress =
                                wait.progress.lock().map_err(|_| "gate progress lock")?;
                            progress.last = Instant::now();
                            progress.completed += 1;
                            progress.last_rpc = format!("{role}:{}", probe.body.command.0);
                            if progress.completed.is_multiple_of(128) {
                                eprintln!(
                                    "world-init RPC progress completed={} last={}",
                                    progress.completed, progress.last_rpc
                                );
                            }
                            Ok(())
                        })();
                        if let Err(reason) = outcome {
                            wait.progress.lock().unwrap().error =
                                Some(format!("{role} proxy: {reason}"));
                            *e.lock().unwrap() = Some(reason);
                            break;
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(err) => {
                        wait.progress.lock().unwrap().error = Some(format!("{role} accept: {err}"));
                        *e.lock().unwrap() = Some(err.to_string());
                        break;
                    }
                }
            }
        });
        Ok(Self {
            address,
            blocked,
            stop,
            error,
            hold_prepare,
            worker: Some(worker),
        })
    }
}
impl ReplicaPrepareGate {
    fn release(&self) {
        self.hold_prepare.store(false, Ordering::Release);
    }
}
impl Drop for ReplicaPrepareGate {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[derive(serde::Deserialize)]
struct CommandProbe {
    body: CommandBodyProbe,
}
#[derive(serde::Deserialize)]
struct CommandBodyProbe {
    command: IsPrepare,
}
struct IsPrepare(String);
impl<'de> serde::Deserialize<'de> for IsPrepare {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = IsPrepare;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("signed RPC command")
            }
            fn visit_enum<A: serde::de::EnumAccess<'de>>(
                self,
                a: A,
            ) -> Result<IsPrepare, A::Error> {
                use serde::de::VariantAccess;
                let (name, variant) = a.variant::<String>()?;
                if name == "Head" || name == "ArchiveManifest" {
                    variant.unit_variant()?;
                } else {
                    variant.newtype_variant::<serde::de::IgnoredAny>()?;
                }
                Ok(IsPrepare(name))
            }
        }
        d.deserialize_enum("Command", &[], Visitor)
    }
}
fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut size = [0; 4];
    stream.read_exact(&mut size).map_err(|e| e.to_string())?;
    let size = u32::from_be_bytes(size) as usize;
    if size == 0 || size > 64 * 1024 * 1024 {
        return Err("world-init gate RPC frame bound".into());
    }
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
fn write_frame(stream: &mut TcpStream, bytes: &[u8]) -> Result<(), String> {
    let size = u32::try_from(bytes.len()).map_err(|e| e.to_string())?;
    stream
        .write_all(&size.to_be_bytes())
        .and_then(|_| stream.write_all(bytes))
        .map_err(|e| e.to_string())
}
#[test]
#[ignore = "requires shipping runtime + protected endpoint binaries; real CLI SIGKILL activation"]
fn formal_world_initialization_cli_native_platform_and_original_activation() {
    thread::Builder::new()
        .name("formal-world-init-cli".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(run_initialization)
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
}
