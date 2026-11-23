//! Consumer acceptance: original signed HTTP bytes, native ownership, real
//! World retention, interrupted installation, and bounded periodic rollover.
use super::super::super::super::{local_io, model, process};
use super::super::super::{operation_snapshot, operation_status, operation_worker};
use super::*;
use std::{
    fs, io,
    net::Shutdown,
    sync::{
        atomic::{AtomicBool, AtomicU64},
        mpsc,
    },
};

#[derive(Clone)]
struct ExchangeObservation {
    path: String,
    tx: usize,
    rx: usize,
    status: u16,
}

struct PausedRequest {
    entered: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
}
impl PausedRequest {
    fn wait(&self) {
        self.entered.recv_timeout(Duration::from_secs(10)).unwrap();
    }
}
impl Drop for PausedRequest {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

type Pause = (mpsc::Sender<()>, mpsc::Receiver<()>);
struct ObservedProxy {
    endpoint: String,
    observations: Arc<Mutex<Vec<ExchangeObservation>>>,
    reject: Arc<Mutex<Option<String>>>,
    signed_reject: Arc<Mutex<Option<String>>>,
    pause: Arc<Mutex<Option<Pause>>>,
    delay_ms: Arc<AtomicU64>,
    stopped: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl ObservedProxy {
    fn new(upstream: &str) -> Self {
        let upstream = upstream.strip_prefix("http://").unwrap().to_owned();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let observations = Arc::new(Mutex::new(Vec::new()));
        let reject = Arc::new(Mutex::new(None::<String>));
        let signed_reject = Arc::new(Mutex::new(None::<String>));
        let pause = Arc::new(Mutex::new(None::<Pause>));
        let delay_ms = Arc::new(AtomicU64::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let (seen, denied, signed_denied, paused, delay, stop) = (
            Arc::clone(&observations),
            Arc::clone(&reject),
            Arc::clone(&signed_reject),
            Arc::clone(&pause),
            Arc::clone(&delay_ms),
            Arc::clone(&stopped),
        );
        let worker = std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(stream) => stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => break,
                };
                let _ = (|| -> Result<(), String> {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .map_err(|e| e.to_string())?;
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .map_err(|e| e.to_string())?;
                    let (header, body) = read_http_message(&mut stream, 256 * 1024)?;
                    let path = String::from_utf8_lossy(&header)
                        .split_whitespace()
                        .nth(1)
                        .ok_or("missing HTTP path")?
                        .to_owned();
                    if let Some((entered, release)) = paused.lock().unwrap().take() {
                        entered.send(()).map_err(|e| e.to_string())?;
                        release
                            .recv_timeout(Duration::from_secs(10))
                            .map_err(|e| e.to_string())?;
                    }
                    thread::sleep(Duration::from_millis(delay.load(Ordering::SeqCst)));
                    let rejected = {
                        let mut route = denied.lock().unwrap();
                        if route.as_deref() == Some(&path) {
                            route.take();
                            true
                        } else {
                            false
                        }
                    };
                    let signed_rejected = {
                        let mut route = signed_denied.lock().unwrap();
                        if route.as_deref() == Some(&path) {
                            route.take();
                            true
                        } else {
                            false
                        }
                    };
                    let (response_header, response_body) = if signed_rejected {
                        let body = signed_diagnostic_error(&path, &body)?;
                        (format!("HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes(), body)
                    } else if rejected {
                        let body = b"{\"error\":\"fixture temporary provider failure\",\"details\":\"count the entire error body\"}".to_vec();
                        (format!("HTTP/1.1 503 Service Unavailable\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes(), body)
                    } else {
                        let mut remote =
                            TcpStream::connect(&upstream).map_err(|e| e.to_string())?;
                        remote
                            .set_read_timeout(Some(Duration::from_secs(5)))
                            .map_err(|e| e.to_string())?;
                        remote.write_all(&header).map_err(|e| e.to_string())?;
                        remote.write_all(&body).map_err(|e| e.to_string())?;
                        read_http_message(&mut remote, 16 * 1024 * 1024)?
                    };
                    let status = String::from_utf8_lossy(&response_header)
                        .split_whitespace()
                        .nth(1)
                        .ok_or("missing status")?
                        .parse()
                        .map_err(|_| "invalid status")?;
                    // The independent observation is the actual signed octets
                    // forwarded, including envelope and any error body.
                    seen.lock().unwrap().push(ExchangeObservation {
                        path,
                        tx: body.len(),
                        rx: response_body.len(),
                        status,
                    });
                    stream
                        .write_all(&response_header)
                        .map_err(|e| e.to_string())?;
                    stream
                        .write_all(&response_body)
                        .map_err(|e| e.to_string())?;
                    let _ = stream.shutdown(Shutdown::Both);
                    Ok(())
                })();
            }
        });
        Self {
            endpoint,
            observations,
            reject,
            signed_reject,
            pause,
            delay_ms,
            stopped,
            worker: Some(worker),
        }
    }
    fn pause_next(&self) -> PausedRequest {
        let (entered, wait) = mpsc::channel();
        let (release, released) = mpsc::channel();
        assert!(
            self.pause
                .lock()
                .unwrap()
                .replace((entered, released))
                .is_none()
        );
        PausedRequest {
            entered: wait,
            release: Some(release),
        }
    }
    fn observed(&self) -> Vec<ExchangeObservation> {
        self.observations.lock().unwrap().clone()
    }
}
impl Drop for ObservedProxy {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

const REMOTE_DIAGNOSTIC: &str =
    "attempt_expired operation_budget_exhausted installation_repair_attempt_failed:";

fn signed_diagnostic_error(path: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    use oasis7_client_api::world_service::{SignedReadRequest, VIEW_PATH};
    fn digest<Q: serde::de::DeserializeOwned + serde::Serialize>(
        path: &str,
        body: &[u8],
    ) -> Result<String, String> {
        let request: SignedReadRequest<Q> =
            serde_json::from_slice(body).map_err(|e| e.to_string())?;
        authority::verify_read_request(path, &request)?;
        authority::request_digest(path, &request)
    }
    let request_digest = match path {
        VIEW_PATH => digest::<ReadWorldViewRequest>(path, body)?,
        FORMAL_FULL_SYNC_MANIFEST_PATH => digest::<FormalFullSyncManifestRequestV1>(path, body)?,
        oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH => {
            digest::<FormalFullSyncObjectRequestV1>(path, body)?
        }
        _ => return Err("unexpected signed rejection route".into()),
    };
    let payload = super::super::super::FormalFullSyncServiceErrorV1 {
        api_version: "oasis7.formal-live-error/v1".into(),
        code: "full_sync_rejected".into(),
        message: REMOTE_DIAGNOSTIC.into(),
    };
    let response = authority::sign_service_response(
        path,
        request_digest.clone(),
        payload,
        &hex::encode(test_key(91).to_bytes()),
    )?;
    let encoded = serde_json::to_vec(&response).map_err(|e| e.to_string())?;
    // Verify the emitted original, including the generic view error's decoder,
    // against the independently configured service key and exact signed request.
    let decoded: SignedServiceResponse<Value> =
        serde_json::from_slice(&encoded).map_err(|e| e.to_string())?;
    authority::verify_service_response(
        path,
        &request_digest,
        &decoded,
        &hex::encode(test_key(91).verifying_key().to_bytes()),
    )?;
    Ok(encoded)
}

fn read_http_message(stream: &mut TcpStream, limit: usize) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        header.push(byte[0]);
        if header.len() > 8192 {
            return Err("fixture HTTP header exceeds bound".into());
        }
    }
    let length: usize = String::from_utf8_lossy(&header)
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>())
        })
        .ok_or("fixture HTTP message has no Content-Length")?
        .map_err(|e| e.to_string())?;
    if length > limit {
        return Err("fixture HTTP body exceeds bound".into());
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok((header, body))
}

struct BudgetFixture {
    node: model::NodeConfig,
    instance: LocalInstance,
    expected: FormalFullSyncCutV1,
    expected_world: Vec<u8>,
    service: super::super::LiveServiceFixture,
    primary: ObservedProxy,
    alternate: Option<ObservedProxy>,
    cli: Option<CliNode>,
    _root: TestPrivateDir,
}
impl BudgetFixture {
    fn new(failover: bool, configure: impl FnOnce(&mut Value), binary: Option<PathBuf>) -> Self {
        Self::new_at(failover, false, configure, binary)
    }
    fn new_at(
        failover: bool,
        advance_from_c0: bool,
        configure: impl FnOnce(&mut Value),
        binary: Option<PathBuf>,
    ) -> Self {
        let (world, trust, anchor, material, proof) = full_sync_fixture();
        let successor =
            advance_from_c0.then(|| same_epoch_successor_fixture(&world, &anchor, &material));
        let expected_world = oasis7_world_state::codec::snapshot_cbor(
            &successor
                .as_ref()
                .map_or(&world, |next| &next.world)
                .snapshot(),
        )
        .unwrap();
        let expected = cut(successor.as_ref().map_or(&anchor, |next| &next.anchor));
        let service = start_live_service(world, trust.clone(), anchor, material, proof);
        if let Some(next) = successor {
            service.5.push(next.clone());
            service.4.publish(next.world, next.anchor).unwrap();
        }
        let primary = ObservedProxy::new(&service.1);
        let alternate = failover.then(|| ObservedProxy::new(&service.1));
        let mut seed = super::super::failover_tests::config(
            primary.endpoint.clone(),
            service.2.clone(),
            alternate
                .as_ref()
                .map(|proxy| FormalFullSyncEndpointV1 {
                    endpoint: proxy.endpoint.clone(),
                    trusted_service_public_key: service.2.clone(),
                })
                .into_iter()
                .collect(),
            trust,
            &test_key(92),
        );
        seed.grant = None;
        seed.poll_interval_ms = 100;
        seed.request_timeout_ms = 2000;
        let mut wire = serde_json::to_value(&seed).unwrap();
        wire["api_version"] = "oasis7.formal-full-state-sync/v2".into();
        wire["transfer_policy"] = serde_json::json!({
            "api_version": "oasis7.formal-full-sync-transfer-policy/v1",
            "attempt_timeout_ms": 10000, "max_attempts": 4,
            "max_encoded_io_bytes": 67108864, "max_exchanges": 256,
            "max_proof_pages": 64, "max_proof_bytes": 16777216,
            "max_inflight_requests": 1, "reopen_timeout_ms": 10000
        });
        configure(&mut wire);
        let root = TestPrivateDir::new();
        let data = root.0.join("original-budgeted-node");
        let chain_path = root.0.join("chain.json");
        let chain = serde_json::json!({"api_version":"oasis7.chain-config/v4", "runtime_profile":PROFILE, "formal_full_sync":wire});
        local_io::write(&chain_path, &chain).unwrap();
        let mut cli = binary.map(|binary| CliNode {
            binary,
            data: data.clone(),
            started: false,
        });
        let node = if let Some(cli) = &cli {
            let initialized = cli
                .command(&[
                    "init",
                    "--chain",
                    path(&chain_path).unwrap(),
                    "--data-dir",
                    path(&data).unwrap(),
                    "--rpc-bind",
                    "127.0.0.1:43170",
                ])
                .unwrap();
            assert_eq!(initialized["world_created"], false);
            assert_eq!(initialized["authority_created"], false);
            model::NodeConfig::load(&data.join(model::CONFIG_FILE)).unwrap()
        } else {
            let chain = model::ChainConfig::load(&chain_path).unwrap();
            model::initialize(chain, &data, None, "127.0.0.1:43170").unwrap()
        };
        let grant_path = root.0.join("issuer-grant.json");
        local_io::write(
            &grant_path,
            &grant(
                &node.identity.root_public_key,
                &expected.world,
                &expected.chain_id,
                expected.authority_epoch,
            ),
        )
        .unwrap();
        if let Some(cli) = &mut cli {
            cli.command(&[
                "full-sync-grant",
                "--data-dir",
                path(&data).unwrap(),
                "--grant",
                path(&grant_path).unwrap(),
            ])
            .unwrap();
        } else {
            super::super::super::install_grant(&data, &grant_path).unwrap();
        }
        let node = model::NodeConfig::load(&data.join(model::CONFIG_FILE)).unwrap();
        let instance = LocalInstance {
            incarnation: local_io::random_id().unwrap(),
            config_digest: local_io::digest(&node).unwrap(),
            ..super::super::failover_tests::local_instance("operation-consumer")
        };
        Self {
            node,
            instance,
            expected,
            expected_world,
            service,
            primary,
            alternate,
            cli,
            _root: root,
        }
    }
    fn ledger_path(&self) -> PathBuf {
        self.node
            .identity
            .data_dir
            .join("state/full-sync/operation.json")
    }
    fn snapshot(&self) -> operation_snapshot::Snapshot {
        operation_snapshot::read_snapshot(&self.ledger_path()).unwrap()
    }
    fn driver(&self, generation: u64) -> operation_worker::BudgetedDriver {
        let instance = LocalInstance {
            generation,
            incarnation: local_io::random_id().unwrap(),
            ..self.instance.clone()
        };
        operation_worker::BudgetedDriver::new(
            self.node.clone(),
            instance,
            Arc::new(process::lock(&self.node.identity.data_dir).unwrap()),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }
    fn require_world(&self, driver: &operation_worker::BudgetedDriver) -> Value {
        let status =
            operation_status::running(&self.node, &driver.observation.lock().unwrap()).unwrap();
        assert_eq!(status["ready"], true, "{status}");
        assert_eq!(status["world_loaded"], true);
        assert_eq!(
            status["loaded_cut"],
            serde_json::to_value(&self.expected).unwrap()
        );
        assert_eq!(
            oasis7_world_state::codec::snapshot_cbor(&driver.retained_world().unwrap().snapshot())
                .unwrap(),
            self.expected_world
        );
        status
    }
    fn observations(&self) -> Vec<ExchangeObservation> {
        let mut entries = self.primary.observed();
        if let Some(alternate) = &self.alternate {
            entries.extend(alternate.observed());
        }
        entries
    }
    fn require_wire_accounting(&self, snapshot: &operation_snapshot::Snapshot) {
        let observed = self.observations();
        assert_eq!(
            snapshot.encoded_tx_actual,
            observed.iter().map(|e| e.tx as u64).sum::<u64>()
        );
        assert_eq!(
            snapshot.encoded_rx_actual,
            observed.iter().map(|e| e.rx as u64).sum::<u64>()
        );
        assert_eq!(snapshot.exchanges_started, observed.len() as u64);
        for route in [
            oasis7_client_api::world_service::VIEW_PATH,
            FORMAL_FULL_SYNC_MANIFEST_PATH,
            oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH,
        ] {
            assert!(
                observed.iter().any(|e| e.path == route),
                "missing real route {route}"
            );
        }
    }
    fn interrupted_installation(&self, phase: &'static str) -> operation_snapshot::Snapshot {
        let owner = Arc::new(process::lock(&self.node.identity.data_dir).unwrap());
        let mut worker = operation_worker::InitialBudgetedSession::prepare(
            &self.node,
            &self.instance,
            Arc::clone(&owner),
        )
        .unwrap();
        worker.interrupt_publication_at(phase);
        let thread = Arc::clone(&owner)
            .spawn_bound_worker(move || worker.run_once())
            .unwrap();
        assert!(thread.join().is_err());
        assert!(process::lock(&self.node.identity.data_dir).is_err());
        drop(owner);
        let snapshot = self.snapshot();
        assert!(snapshot.publication.is_some());
        assert!(!snapshot.installation.as_ref().unwrap().completed);
        snapshot
    }
}
impl Drop for BudgetFixture {
    fn drop(&mut self) {
        if let Some(cli) = &mut self.cli
            && cli.started
        {
            let _ = cli.stop();
        }
        let _ = self.service.0.stop();
        let _ = self.service.3.lock().unwrap().stop();
    }
}

fn signed_renewal(fixture: &BudgetFixture) -> FormalNodeReadGrantV1 {
    let original = fixture
        .node
        .identity
        .chain
        .budgeted_formal_full_sync()
        .unwrap()
        .grant
        .as_ref()
        .unwrap();
    FormalNodeReadGrantV1::sign(
        oasis7_client_api::world_service_full_sync::FormalNodeReadGrantBodyV1 {
            grant_epoch: original.body.grant_epoch + 1,
            ..original.body.clone()
        },
        &hex::encode(test_key(9).to_bytes()),
    )
    .unwrap()
}

#[test]
fn budgeted_completed_grant_renewal_keeps_original_paid_ledger_and_cold_reopens() {
    let mut fixture = BudgetFixture::new(false, |_| {}, None);
    let mut first = fixture.driver(1);
    first.poll();
    fixture.require_world(&first);
    let before = fixture.snapshot();
    assert!(before.installation.as_ref().unwrap().completed);
    let prior = fixture.observations();
    drop(first);

    let replacement = fixture._root.0.join("renewed-issuer-grant.json");
    local_io::write(&replacement, &signed_renewal(&fixture)).unwrap();
    let changed = super::super::super::install_grant(
        &fixture.node.identity.data_dir,
        &replacement,
    )
    .unwrap();
    assert_eq!(changed["changed"], true);
    fixture.node =
        model::NodeConfig::load(&fixture.node.identity.data_dir.join(model::CONFIG_FILE)).unwrap();
    fixture.instance.config_digest = local_io::digest(&fixture.node).unwrap();

    let mut cold = fixture.driver(2);
    cold.poll();
    fixture.require_world(&cold);
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(fixture.observations().len(), prior.len());
    cold.poll();
    fixture.require_world(&cold);
    let after = fixture.snapshot();
    assert_eq!(after.terminal_index.as_ref().unwrap().completed_operations, 2);
    assert_ne!(after.identity.operation_id, before.identity.operation_id);
    assert_eq!(
        after.terminal_index.as_ref().unwrap().summaries[0].identity,
        before.identity
    );
}

#[test]
fn budgeted_unfinished_grant_renewal_never_replaces_the_original_request() {
    let fixture = BudgetFixture::new(false, |_| {}, None);
    let before = fixture.interrupted_installation("intent_durable");
    let replacement = fixture._root.0.join("invalid-renewal.json");
    local_io::write(&replacement, &signed_renewal(&fixture)).unwrap();
    let result = super::super::super::install_grant(
        &fixture.node.identity.data_dir,
        &replacement,
    );
    assert!(result.is_err());
    assert_eq!(fixture.snapshot(), before);
    assert_eq!(
        model::NodeConfig::load(&fixture.node.identity.data_dir.join(model::CONFIG_FILE)).unwrap(),
        fixture.node
    );
}

#[test]
fn budgeted_consumer_accounts_real_routes_and_failover_error_body() {
    let fixture = BudgetFixture::new(true, |_| {}, None);
    *fixture.primary.reject.lock().unwrap() = Some(FORMAL_FULL_SYNC_MANIFEST_PATH.into());
    let mut driver = fixture.driver(1);
    driver.poll();
    fixture.require_world(&driver);
    let snapshot = fixture.snapshot();
    fixture.require_wire_accounting(&snapshot);
    assert!(
        fixture
            .observations()
            .iter()
            .any(|e| e.status == 503 && e.rx > 0)
    );
    assert_eq!(snapshot.attempts_used, 1);
    assert_eq!(snapshot.unknown_final_charged, 0);
    assert!(snapshot.material_bytes > 0);
}

#[test]
fn budgeted_consumer_retains_world_and_rotates_seventeen_complete_cycles() {
    let mut fixture = BudgetFixture::new(false, |_| {}, None);
    let mut driver = fixture.driver(1);
    let mut identities = std::collections::BTreeSet::new();
    let mut totals = (0u64, 0u64, 0u64);
    for completed in 1..=17u64 {
        driver.poll();
        fixture.require_world(&driver);
        let snapshot = fixture.snapshot();
        assert!(identities.insert(snapshot.identity.operation_id.clone()));
        totals.0 += snapshot.encoded_tx_actual;
        totals.1 += snapshot.encoded_rx_actual;
        totals.2 += snapshot.exchanges_started;
        let terminal = snapshot.terminal_index.as_ref().unwrap();
        assert_eq!(terminal.completed_operations, completed);
        assert_eq!(terminal.summaries.len(), completed.min(16) as usize);
        assert_eq!(
            (
                terminal.encoded_tx_actual,
                terminal.encoded_rx_actual,
                terminal.exchanges_started
            ),
            totals
        );
        assert!(serde_json::to_vec(terminal).unwrap().len() <= 1024 * 1024);
        assert!(fs::metadata(fixture.ledger_path()).unwrap().len() <= 16 * 1024 * 1024);
    }
    let observed = fixture.observations();
    assert_eq!(totals.0, observed.iter().map(|e| e.tx as u64).sum::<u64>());
    assert_eq!(totals.1, observed.iter().map(|e| e.rx as u64).sum::<u64>());
    let before = fixture.snapshot();
    drop(driver);
    fixture.service.0.stop().unwrap();
    let mut reopened = fixture.driver(2);
    reopened.poll();
    let status = fixture.require_world(&reopened);
    assert_eq!(
        fixture.snapshot(),
        before,
        "cold local readback cannot allocate another operation or refund bytes"
    );
    assert!(status["operation"]["elapsed_ms"].is_null());
    assert!(status["checkpoint_age"]["elapsed_ms"].is_null());
    assert_eq!(fixture.observations().len(), observed.len());

    // Inject a valid retained ring whose historic cumulative counter has no
    // headroom. The next real poll must reject before admitting any exchange,
    // preserving both the original completed ledger and independent pins.
    let mut full = before;
    full.terminal_index.as_mut().unwrap().encoded_rx_actual = u64::MAX;
    full.validate().unwrap();
    local_io::write(&fixture.ledger_path(), &full).unwrap();
    let saved = fs::read(fixture.ledger_path()).unwrap();
    reopened.poll();
    assert_eq!(fs::read(fixture.ledger_path()).unwrap(), saved);
    assert_eq!(fixture.observations().len(), observed.len());
    assert_eq!(
        reopened.observation.lock().unwrap().base["blocker"],
        "operation_budget_exhausted"
    );
}

#[test]
fn budgeted_original_installation_recovers_after_both_real_publication_boundaries() {
    for phase in ["intent_durable", "pins_durable"] {
        let mut fixture = BudgetFixture::new(false, |_| {}, None);
        let before = fixture.interrupted_installation(phase);
        let exchanges = fixture.observations().len();
        fixture.service.0.stop().unwrap();
        let mut driver = fixture.driver(2);
        driver.poll();
        fixture.require_world(&driver);
        let after = fixture.snapshot();
        assert_eq!(after.identity, before.identity);
        assert_eq!(after.verified_target_json, before.verified_target_json);
        assert_eq!(after.charged().unwrap(), before.charged().unwrap());
        assert_eq!(after.attempts_used, before.attempts_used);
        assert_eq!(fixture.observations().len(), exchanges);
        assert!(after.installation.as_ref().unwrap().completed);
    }
}

#[test]
fn budgeted_bad_original_intent_cannot_replace_pins_before_rejection() {
    let fixture = BudgetFixture::new(false, |_| {}, None);
    let before = fixture.interrupted_installation("pins_durable");
    let pin = fixture
        .node
        .identity
        .data_dir
        .join("state/full-sync/pin_scopes/formal-full-sync/loaded-world.json");
    let pin_bytes = fs::read(&pin).unwrap();
    let mut bad = before.clone();
    let roots = &mut bad.installation.as_mut().unwrap().retained_cas_hashes;
    assert!(roots.len() > 1);
    roots.pop_first(); // All remaining names and blobs are legal; the intent differs.
    local_io::write(&fixture.ledger_path(), &bad).unwrap();
    let bad_bytes = fs::read(fixture.ledger_path()).unwrap();
    let mut driver = fixture.driver(2);
    driver.poll();
    assert!(driver.retained_world().is_none());
    assert_eq!(fs::read(&pin).unwrap(), pin_bytes);
    assert_eq!(fs::read(fixture.ledger_path()).unwrap(), bad_bytes);
}

#[test]
fn budgeted_missing_original_material_uses_new_attempt_without_new_target_or_view() {
    let fixture = BudgetFixture::new(false, |_| {}, None);
    let before = fixture.interrupted_installation("pins_durable");
    let hash = before.installation.as_ref().unwrap().manifest["objects"][0]["content_hash"]
        .as_str()
        .unwrap();
    let cas = LocalCasStore::new(fixture.node.identity.data_dir.join("state/full-sync"));
    fs::remove_file(cas.blobs_dir().join(format!("{hash}.blob"))).unwrap();
    let observed = fixture.observations().len();
    let mut driver = fixture.driver(2);
    driver.poll();
    fixture.require_world(&driver);
    let after = fixture.snapshot();
    assert_eq!(after.identity, before.identity);
    assert_eq!(after.verified_target_json, before.verified_target_json);
    assert_eq!(after.attempts_used, before.attempts_used + 1);
    assert_eq!(after.materials, before.materials);
    assert!(after.encoded_rx_actual > before.encoded_rx_actual);
    assert!(fixture.observations()[observed..].iter().all(
        |e| e.path == oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH
    ));
    fixture.require_wire_accounting(&after);
}

#[test]
fn budgeted_missing_original_with_no_attempt_cannot_download_or_change_pins() {
    let fixture = BudgetFixture::new(
        false,
        |wire| {
            wire["transfer_policy"]["max_attempts"] = 1.into();
        },
        None,
    );
    let original = fixture.interrupted_installation("pins_durable");
    let root = fixture.node.identity.data_dir.join("state/full-sync");
    let hash = original.installation.as_ref().unwrap().manifest["objects"][0]["content_hash"]
        .as_str()
        .unwrap();
    let cas = LocalCasStore::new(&root);
    fs::remove_file(cas.blobs_dir().join(format!("{hash}.blob"))).unwrap();
    let pin = root.join("pin_scopes/formal-full-sync/loaded-world.json");
    let original_pins = fs::read(&pin).unwrap();
    let original_ledger = fs::read(fixture.ledger_path()).unwrap();
    let exchanges = fixture.observations().len();
    let mut driver = fixture.driver(2);
    for _ in 0..3 {
        driver.poll();
    }
    assert!(driver.retained_world().is_none());
    assert_eq!(fs::read(&pin).unwrap(), original_pins);
    assert_eq!(fs::read(fixture.ledger_path()).unwrap(), original_ledger);
    assert_eq!(fixture.observations().len(), exchanges);
    let live = driver.observation.lock().unwrap();
    assert_eq!(live.base["blocker"], "operation_budget_exhausted");
}

#[test]
fn budgeted_bad_material_and_missing_history_cannot_authorize_original_repair() {
    for failure in ["material_hash", "history_page"] {
        // C0 intentionally has no proof pages. The missing-history vector must
        // first receive and persist a real C0 -> C1 signed page.
        let fixture = BudgetFixture::new_at(false, failure == "history_page", |_| {}, None);
        let original = fixture.interrupted_installation("pins_durable");
        let root = fixture.node.identity.data_dir.join("state/full-sync");
        let cas = LocalCasStore::new(&root);
        if failure == "material_hash" {
            let hash =
                original.installation.as_ref().unwrap().manifest["objects"][0]["content_hash"]
                    .as_str()
                    .unwrap();
            let blob = cas.blobs_dir().join(format!("{hash}.blob"));
            let mut bytes = fs::read(&blob).unwrap();
            bytes[0] ^= 1;
            fs::write(blob, bytes).unwrap();
        } else {
            let head: Value =
                local_io::read(&root.join("formal-full-sync-history-head.json")).unwrap();
            let manifest = head["manifest_digest"]
                .as_str()
                .expect("real fixture history page");
            assert!(head["page_count"].as_u64().unwrap() > 0);
            let bytes = cas.get_verified(manifest).unwrap();
            let entry: Value = serde_cbor::from_slice(&bytes).unwrap();
            let page = entry["page_digest"].as_str().unwrap();
            fs::remove_file(cas.blobs_dir().join(format!("{page}.blob"))).unwrap();
        }
        let pin = root.join("pin_scopes/formal-full-sync/loaded-world.json");
        let original_pins = fs::read(&pin).unwrap();
        let original_ledger = fs::read(fixture.ledger_path()).unwrap();
        let exchanges = fixture.observations().len();
        let mut driver = fixture.driver(2);
        driver.poll();
        assert!(driver.retained_world().is_none(), "{failure}");
        assert_eq!(fs::read(&pin).unwrap(), original_pins, "{failure}");
        assert_eq!(
            fs::read(fixture.ledger_path()).unwrap(),
            original_ledger,
            "{failure}"
        );
        assert_eq!(fixture.observations().len(), exchanges, "{failure}");
    }
}

#[test]
fn budgeted_same_native_owner_retries_original_after_real_view_error() {
    let fixture = BudgetFixture::new(false, |_| {}, None);
    *fixture.primary.reject.lock().unwrap() =
        Some(oasis7_client_api::world_service::VIEW_PATH.into());
    let mut driver = fixture.driver(1);
    driver.poll();
    let failed = fixture.snapshot();
    assert!(driver.retained_world().is_none());
    assert_eq!(failed.attempts_used, 1);
    assert_eq!(failed.exchanges_started, 1);
    assert!(failed.encoded_rx_actual > 0);
    assert!(process::lock(&fixture.node.identity.data_dir).is_err());
    driver.poll();
    fixture.require_world(&driver);
    let completed = fixture.snapshot();
    assert_eq!(completed.identity, failed.identity);
    assert_eq!(
        completed.original_request_json,
        failed.original_request_json
    );
    assert_eq!(completed.attempt_owner, failed.attempt_owner);
    assert_eq!(completed.attempts_used, 2);
    assert!(completed.encoded_rx_actual > failed.encoded_rx_actual);
    fixture.require_wire_accounting(&completed);
}

#[test]
fn budgeted_signed_remote_diagnostics_cannot_choose_local_blocker() {
    for route in [
        oasis7_client_api::world_service::VIEW_PATH,
        FORMAL_FULL_SYNC_MANIFEST_PATH,
        oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH,
    ] {
        let fixture = BudgetFixture::new(false, |_| {}, None);
        *fixture.primary.signed_reject.lock().unwrap() = Some(route.into());
        let mut driver = fixture.driver(1);
        driver.poll();
        assert!(driver.retained_world().is_none(), "{route}");
        let original = fixture.snapshot();
        assert_eq!(original.attempts_used, 1);
        assert!(original.charged().unwrap() < original.policy.max_encoded_io_bytes);
        let observed = fixture.observations();
        assert!(observed.iter().any(|e| e.path == route && e.status == 400));
        assert_eq!(
            original.encoded_tx_actual,
            observed.iter().map(|e| e.tx as u64).sum::<u64>()
        );
        assert_eq!(
            original.encoded_rx_actual,
            observed.iter().map(|e| e.rx as u64).sum::<u64>()
        );
        {
            let live = driver.observation.lock().unwrap();
            assert!(
                live.base["error"]
                    .as_str()
                    .unwrap()
                    .contains(REMOTE_DIAGNOSTIC),
                "{route}: {}",
                live.base
            );
            assert_eq!(
                live.base["blocker"], "operation_recovery_required",
                "{route}: {}",
                live.base
            );
        }
        // The remote text neither grants another attempt nor makes a normal
        // bounded retry permanently unavailable. The next poll keeps originals.
        driver.poll();
        fixture.require_world(&driver);
        let completed = fixture.snapshot();
        assert_eq!(completed.identity, original.identity);
        assert_eq!(
            completed.original_request_json,
            original.original_request_json
        );
        assert_eq!(completed.attempt_owner, original.attempt_owner);
        assert_eq!(completed.attempts_used, 2);
        fixture.require_wire_accounting(&completed);
    }
}

#[test]
fn budgeted_repeated_poll_cannot_reset_exhausted_operation_or_attempts() {
    let fixture = BudgetFixture::new(
        false,
        |wire| {
            wire["transfer_policy"]["max_exchanges"] = 1.into();
        },
        None,
    );
    let mut driver = fixture.driver(1);
    driver.poll();
    let original = fixture.snapshot();
    assert_eq!(original.exchanges_started, 1);
    for _ in 0..6 {
        driver.poll();
    }
    let after = fixture.snapshot();
    assert_eq!(after.identity, original.identity);
    assert_eq!(after.encoded_tx_actual, original.encoded_tx_actual);
    assert_eq!(after.encoded_rx_actual, original.encoded_rx_actual);
    assert_eq!(after.exchanges_started, 1);
    assert_eq!(after.attempts_used, after.policy.max_attempts);
    assert_eq!(
        driver.observation.lock().unwrap().base["blocker"],
        "operation_budget_exhausted"
    );
    assert!(driver.retained_world().is_none());
    assert_eq!(
        after.terminal_index.as_ref().unwrap().completed_operations,
        0
    );
}

#[test]
fn budgeted_real_deadline_stops_failover_without_refreshing_attempt() {
    let fixture = BudgetFixture::new(
        true,
        |wire| {
            wire["transfer_policy"]["attempt_timeout_ms"] = 1000.into();
        },
        None,
    );
    fixture.primary.delay_ms.store(1500, Ordering::SeqCst);
    let mut driver = fixture.driver(1);
    let started = Instant::now();
    driver.poll();
    assert!(started.elapsed() < Duration::from_secs(3));
    let after = fixture.snapshot();
    assert_eq!(after.attempts_used, 1);
    assert_eq!(after.exchanges_started, 1);
    assert!(fixture.alternate.as_ref().unwrap().observed().is_empty());
    assert!(driver.retained_world().is_none());
    assert_eq!(
        driver.observation.lock().unwrap().base["blocker"],
        "attempt_expired"
    );
}

#[path = "formal_full_sync_operation_shipping_renewal_tests.rs"]
mod shipping_renewal;

pub(super) fn budgeted_shipping_cli_renewal_preserves_original_and_advances() {
    shipping_renewal::run();
}

/// Executed by the already registered shipping lifecycle CLI case, with the
/// exact same candidate runtime binary supplied by the formal artifact runner.
pub(super) fn budgeted_shipping_cli_completes_seventeen_cycles_and_local_cold_readback() {
    eprintln!("formal_budgeted_cli case=seventeen_cycles_and_cold_readback phase=started");
    let binary =
        PathBuf::from(std::env::var_os("OASIS7_FORMAL_NODE_TEST_BIN").expect("shipping binary"));
    let mut fixture = BudgetFixture::new(
        false,
        |wire| {
            wire["poll_interval_ms"] = 500.into();
        },
        Some(binary.clone()),
    );
    fixture.cli.as_mut().unwrap().start().unwrap();
    fixture
        .cli
        .as_ref()
        .unwrap()
        .wait_ready(&fixture.expected)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = fixture.cli.as_ref().unwrap().status().unwrap();
        if status["formal_full_state_sync"]["operation"]["terminal"]["completed_operations"]
            .as_u64()
            .unwrap_or(0)
            >= 17
        {
            break;
        }
        assert!(Instant::now() < deadline, "{status}");
        thread::sleep(Duration::from_millis(50));
    }
    fixture.cli.as_mut().unwrap().stop().unwrap();
    let snapshot = fixture.snapshot();
    assert_eq!(
        snapshot.terminal_index.as_ref().unwrap().summaries.len(),
        16
    );

    // A separate long poll interval leaves a deterministic window to stop a
    // completed operation. The periodic capacity check above does not depend
    // on winning a race with the admission of operation eighteen.
    let mut cold = BudgetFixture::new(
        false,
        |wire| {
            wire["poll_interval_ms"] = 20000.into();
        },
        Some(binary),
    );
    cold.cli.as_mut().unwrap().start().unwrap();
    cold.cli
        .as_ref()
        .unwrap()
        .wait_ready(&cold.expected)
        .unwrap();
    cold.cli.as_mut().unwrap().stop().unwrap();
    let original = cold.snapshot();
    assert!(original.installation.as_ref().unwrap().completed);
    cold.service.0.stop().unwrap();
    cold.cli.as_mut().unwrap().start().unwrap();
    let ready = cold
        .cli
        .as_ref()
        .unwrap()
        .wait_ready(&cold.expected)
        .unwrap();
    assert_eq!(
        ready["formal_full_state_sync"]["operation"]["operation_id"],
        original.identity.operation_id
    );
    assert!(ready["formal_full_state_sync"]["checkpoint_age"]["elapsed_ms"].is_null());
    assert_eq!(cold.snapshot(), original);
    cold.cli.as_mut().unwrap().stop().unwrap();
    eprintln!(
        "formal_budgeted_cli case=seventeen_cycles_and_cold_readback phase=passed completed_operations={} retained_summaries=16 cold_original_unchanged=true",
        snapshot
            .terminal_index
            .as_ref()
            .unwrap()
            .completed_operations
    );
}

#[cfg(target_os = "linux")]
pub(super) fn budgeted_shipping_cli_native_takeover_keeps_original_unknown_charge() {
    eprintln!("formal_budgeted_cli case=native_takeover_original_unknown_charge phase=started");
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let binary =
        PathBuf::from(std::env::var_os("OASIS7_FORMAL_NODE_TEST_BIN").expect("shipping binary"));
    let mut fixture = BudgetFixture::new(
        false,
        |wire| {
            wire["poll_interval_ms"] = 20000.into();
        },
        Some(binary),
    );
    let pause = fixture.primary.pause_next();
    fixture.cli.as_mut().unwrap().start().unwrap();
    pause.wait();
    let live = fixture.cli.as_ref().unwrap().status().unwrap();
    let original = fixture.snapshot();
    assert_eq!(original.attempts_used, 1);
    assert!(original.unresolved_reserved().unwrap() > 0);
    assert_eq!(
        live["formal_full_state_sync"]["operation"]["attempt_state"],
        "active"
    );
    assert!(
        live["formal_full_state_sync"]["operation"]["remaining_ms"]
            .as_u64()
            .unwrap()
            <= 10000
    );
    assert!(process::lock(&fixture.node.identity.data_dir).is_err());
    let pid = live["instance"]["supervisor_pid"].as_u64().unwrap() as i32;
    // Pin the exact child process first, then re-query its authenticated private
    // control socket. A later PID reuse cannot become our signal target.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    assert!(fd >= 0, "{}", io::Error::last_os_error());
    let process_fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
    assert_eq!(
        fixture.cli.as_ref().unwrap().status().unwrap()["instance"],
        live["instance"]
    );
    let killed = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            process_fd.as_raw_fd(),
            libc::SIGKILL,
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    };
    assert_eq!(killed, 0, "{}", io::Error::last_os_error());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(owner) = process::lock(&fixture.node.identity.data_dir) {
            drop(owner);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "old worker still owns the directory"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fixture.snapshot(), original);
    drop(pause);
    fixture.cli.as_mut().unwrap().start().unwrap();
    let ready = fixture
        .cli
        .as_ref()
        .unwrap()
        .wait_ready(&fixture.expected)
        .unwrap();
    let completed = fixture.snapshot();
    assert_eq!(completed.identity, original.identity);
    assert_eq!(
        completed.original_request_json,
        original.original_request_json
    );
    assert_eq!(completed.attempts_used, 2);
    assert_eq!(ready["instance"]["generation"], 2);
    assert_eq!(completed.attempt_owner.as_ref().unwrap().generation, 2);
    assert_eq!(
        completed.unknown_final_charged,
        original.unresolved_reserved().unwrap()
    );
    assert_eq!(completed.unresolved_reserved().unwrap(), 0);
    assert!(completed.encoded_tx_actual >= original.encoded_tx_actual);
    assert!(completed.encoded_rx_actual >= original.encoded_rx_actual);
    assert_eq!(
        ready["formal_full_state_sync"]["operation"]["encoded_io"]["unknown_final_charged"],
        completed.unknown_final_charged
    );
    fixture.cli.as_mut().unwrap().stop().unwrap();
    eprintln!(
        "formal_budgeted_cli case=native_takeover_original_unknown_charge phase=passed generation=2 attempts=2 unknown_final_charged={}",
        completed.unknown_final_charged
    );
}
