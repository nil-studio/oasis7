//! Shipping CLI processes join a real signed HTTP/profile-2 fixture. No local
//! management service or new World/authority is provisioned by these tests.
use super::*;
use serde_json::Value;
#[path = "formal_full_sync_cli_recovery_fixture.rs"]
mod recovery_fixture;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Instant,
};

struct CliNode {
    binary: PathBuf,
    data: PathBuf,
    started: bool,
}
impl CliNode {
    fn command(&self, args: &[&str]) -> Result<Value, String> {
        let mut child = Command::new(&self.binary)
            .arg("node")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("node CLI spawn: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("node CLI deadline: {}", args[0]));
            }
            thread::sleep(Duration::from_millis(10));
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "node CLI {} failed status={}: stdout={} stderr={}",
                args[0],
                out.status,
                String::from_utf8_lossy(&out.stdout[..out.stdout.len().min(2048)]),
                String::from_utf8_lossy(&out.stderr[..out.stderr.len().min(2048)])
            ));
        }
        serde_json::from_slice(&out.stdout).map_err(|e| format!("node CLI {} JSON: {e}", args[0]))
    }
    fn start(&mut self) -> Result<(), String> {
        self.command(&["start", "--config", path(&self.data.join("node.json"))?])?;
        self.started = true;
        Ok(())
    }
    fn stop(&mut self) -> Result<(), String> {
        self.command(&[
            "stop",
            "--data-dir",
            path(&self.data)?,
            "--timeout-ms",
            "10000",
        ])?;
        self.started = false;
        Ok(())
    }
    fn status(&self) -> Result<Value, String> {
        self.command(&["status", "--data-dir", path(&self.data)?])
    }
    fn wait_ready(&self, expected: &FormalFullSyncCutV1) -> Result<Value, String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let status = self.status()?;
            let sync = &status["formal_full_state_sync"];
            if status["formal_readiness"] == true && sync["ready"] == true {
                let loaded: FormalFullSyncCutV1 =
                    serde_json::from_value(sync["loaded_cut"].clone())
                        .map_err(|e| e.to_string())?;
                if loaded == *expected {
                    if sync["state_root_verified"] != true
                        || sync["world_loaded"] != true
                        || status["can_propose"] != false
                        || !status["governance_membership"].is_null()
                        || !status["applied_consensus_membership"].is_null()
                    {
                        return Err("full sync readiness unexpectedly granted authority or skipped World validation".into());
                    }
                    return Ok(status);
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "node CLI did not verify exact cut: {}",
                    compact_status(&status)
                ));
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}
impl Drop for CliNode {
    fn drop(&mut self) {
        if self.started {
            let _ = self.stop();
        }
    }
}
fn path(p: &Path) -> Result<&str, String> {
    p.to_str().ok_or_else(|| "non UTF-8 fixture path".into())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn grant(public: &str, world: &WorldIdentity, chain: &str, epoch: u64) -> FormalNodeReadGrantV1 {
    FormalNodeReadGrantV1::sign(
        FormalNodeReadGrantBodyV1 {
            schema_version: 1,
            world: world.clone(),
            chain_id: chain.into(),
            node_public_key: public.into(),
            role: FormalNodeReadRoleV1::StateSync,
            grant_epoch: epoch,
            authority_epoch: epoch,
            not_before_unix_ms: now().saturating_sub(1000),
            expires_unix_ms: now() + 300_000,
        },
        &hex::encode(test_key(9).to_bytes()),
    )
    .unwrap()
}
fn run_cli_join(moving: bool) -> Result<(), String> {
    run_cli_join_with_recovery(moving, false)
}
fn run_cli_join_with_recovery(moving: bool, evolve_epoch: bool) -> Result<(), String> {
    run_cli_join_with_sources(moving, evolve_epoch, false)
}
fn run_cli_join_with_sources(
    moving: bool,
    evolve_epoch: bool,
    failed_seed: bool,
) -> Result<(), String> {
    let binary = PathBuf::from(
        std::env::var_os("OASIS7_FORMAL_NODE_TEST_BIN")
            .ok_or("set OASIS7_FORMAL_NODE_TEST_BIN to shipping oasis7_chain_runtime")?,
    );
    if !std::fs::metadata(&binary)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("shipping node test binary is not a regular file".into());
    }
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let next = same_epoch_successor_fixture(&world, &anchor, &material);
    let (mut server, endpoint, service_key, node_runtime, published, history) =
        start_live_service(world, trust.clone(), anchor.clone(), material, proof);
    let root = TestPrivateDir::new();
    let data = root.0.join("a-third-party-any-local-name");
    let chain = root.0.join("fixed-c0-chain.json");
    let (endpoint, alternate_endpoints) = if failed_seed {
        let unavailable = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let address = unavailable.local_addr().map_err(|e| e.to_string())?;
        drop(unavailable);
        (
            format!("http://{address}"),
            vec![FormalFullSyncEndpointV1 {
                endpoint,
                trusted_service_public_key: service_key.clone(),
            }],
        )
    } else {
        (endpoint, vec![])
    };
    let config = FormalFullSyncConfigV1 {
        api_version: API_VERSION.into(),
        endpoint,
        trusted_service_public_key: service_key,
        alternate_endpoints,
        world_id: trust.initial_trust.world.world_id.clone(),
        chain_id: trust.initial_trust.world.chain_id.clone(),
        genesis_digest: trust.initial_trust.world.genesis_digest.clone(),
        formal_read_trust: trust.clone(),
        grant: None,
        expected_role: FormalNodeReadRoleV1::StateSync,
        poll_interval_ms: 100,
        request_timeout_ms: 2000,
        max_total_sync_bytes: 64 * 1024 * 1024,
    };
    super::super::super::local_io::write(
        &chain,
        &serde_json::json!({"api_version":"oasis7.chain-config/v3",
        "runtime_profile":PROFILE,"formal_full_sync":config}),
    )?;
    let fixed_chain = std::fs::read(&chain).map_err(|e| e.to_string())?;
    let mut cli = CliNode {
        binary,
        data: data.clone(),
        started: false,
    };
    let probe = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let bind = probe.local_addr().map_err(|e| e.to_string())?.to_string();
    drop(probe);
    let initialized = cli.command(&[
        "init",
        "--chain",
        path(&chain)?,
        "--data-dir",
        path(&data)?,
        "--node-id",
        "third-party-local-role-only",
        "--rpc-bind",
        &bind,
    ])?;
    if initialized["world_created"] != false || initialized["authority_created"] != false {
        return Err("node init created World or authority".into());
    }
    let node_public = initialized["config"]["identity"]["root_public_key"]
        .as_str()
        .ok_or("node init public identity missing")?
        .to_owned();
    let identity_before =
        std::fs::read(data.join("keys/node-keypair.toml")).map_err(|e| e.to_string())?;
    let world_id = WorldIdentity {
        world_id: config.world_id.clone(),
        genesis_digest: config.genesis_digest.clone(),
    };
    let grant_path = root.0.join("independent-issuer-grant.json");
    super::super::super::local_io::write(
        &grant_path,
        &grant(&node_public, &world_id, &config.chain_id, 1),
    )?;
    cli.command(&[
        "full-sync-grant",
        "--data-dir",
        path(&data)?,
        "--grant",
        path(&grant_path)?,
    ])?;
    if moving {
        history.advance_after_next_material(Arc::clone(&published), next.clone());
    }
    cli.start()?;
    let mut expected = if moving {
        cut(&next.anchor)
    } else {
        cut(&anchor)
    };
    let first = cli.wait_ready(&expected)?;
    let generation = first["instance"]["generation"].as_u64();
    if evolve_epoch {
        let original = if moving {
            next.clone()
        } else {
            history.cuts.lock().map_err(|_| "fixture cuts lock")?[0].clone()
        };
        let (recovered, steps) = recovery_fixture::empty_recovery_fixture(&original, &trust)?;
        history
            .transition_steps
            .lock()
            .map_err(|_| "fixture transition lock")?
            .insert(
                recovered
                    .anchor
                    .checkpoint()
                    .digest()
                    .map_err(|e| e.to_string())?,
                steps,
            );
        history.push(recovered.clone());
        published.publish(recovered.world.clone(), recovered.anchor.clone())?;
        // A separate ordinary reader starts at the unchanged C0. Prove the
        // fixture's recovery page actually reaches E2 before judging the CLI
        // worker's grant denial; a raw/current service head is never a pin.
        use oasis7::world_service::client::{
            RemoteWorldServiceClient, WorldServiceClientConfig, WorldServicePort,
        };
        let probe = RemoteWorldServiceClient::new(WorldServiceClientConfig {
            formal_read_trust: Some(trust.clone()),
            endpoint: config.endpoint.clone(),
            trusted_service_public_key: config.trusted_service_public_key.clone(),
            expected_world: world_id.clone(),
            scope_id: "public".into(),
            read_private_key_hex: hex::encode(test_key(92).to_bytes()),
            timeout: Duration::from_millis(config.request_timeout_ms),
            max_response_bytes: 16 * 1024 * 1024,
        })
        .map_err(|e| e.to_string())?;
        probe
            .read_view(oasis7_client_api::world_service::ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: oasis7_client_api::world_service::WORLD_SERVICE_CONTRACT_VERSION,
                world: world_id.clone(),
                scope_id: "public".into(),
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            })
            .map_err(|e| format!("CLI E2 fixture ordinary C0 recovery replay failed: {e}"))?;
        let independently_replayed = probe
            .formal_current_anchor()
            .map_err(|e| format!("E2 ordinary C0 replay opaque anchor: {e}"))?;
        if independently_replayed.checkpoint() != recovered.anchor.checkpoint() {
            return Err(
                "CLI E2 fixture ordinary C0 replay did not reach exact recovered context".into(),
            );
        }
        assert_signed_epoch_grant_rejection(
            &config,
            &data,
            &world_id,
            &node_public,
            &cut(&recovered.anchor),
        )
        .map_err(|e| format!("E2 independent HTTP grant gate: {e}"))?;
        let exact_recovered_cut =
            serde_json::to_value(cut(&recovered.anchor)).map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let status = cli.status()?;
            if status["formal_full_state_sync"]["last_verified_cut"] == exact_recovered_cut
                && exact_old_epoch_grant_denied(&status)
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "CLI old epoch grant was not rejected: {}",
                    compact_status(&status)
                ));
            }
            thread::sleep(Duration::from_millis(50));
        }
        // The verified E2 witness and precise rejection remain visible while
        // the unchanged E1 grant is retried; begin_poll must not erase them.
        for _ in 0..3 {
            thread::sleep(Duration::from_millis(50));
            let retry = cli.status()?;
            if retry["formal_full_state_sync"]["last_verified_cut"] != exact_recovered_cut
                || !exact_old_epoch_grant_denied(&retry)
            {
                return Err(format!(
                    "CLI retry erased durable E2/grant rejection: {}",
                    compact_status(&retry)
                ));
            }
        }
        cli.stop()?;
        super::super::super::local_io::write(
            &grant_path,
            &grant(&node_public, &world_id, &config.chain_id, 2),
        )?;
        cli.command(&[
            "full-sync-grant",
            "--data-dir",
            path(&data)?,
            "--grant",
            path(&grant_path)?,
        ])?;
        expected = cut(&recovered.anchor);
    } else {
        cli.stop()?;
    }
    if std::fs::read(&chain).map_err(|e| e.to_string())? != fixed_chain
        || std::fs::read(data.join("keys/node-keypair.toml")).map_err(|e| e.to_string())?
            != identity_before
    {
        return Err("CLI sync/stop changed fixed C0 or local identity".into());
    }
    cli.start()?;
    let reopened = cli.wait_ready(&expected)?;
    if reopened["formal_full_state_sync"]["node_public_key"] != node_public
        || reopened["instance"]["generation"].as_u64() == generation
    {
        return Err("CLI restart did not preserve identity and create a new incarnation".into());
    }
    cli.stop()?;
    if std::fs::read(&chain).map_err(|e| e.to_string())? != fixed_chain {
        return Err("CLI restart replaced external C0".into());
    }
    server.stop()?;
    node_runtime
        .lock()
        .map_err(|_| "fixture node lock poisoned")?
        .stop()
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[test]
#[ignore = "requires shipping OASIS7_FORMAL_NODE_TEST_BIN; actual autonomous full/state-sync CLI"]
fn formal_full_state_sync_cli_init_join_restart_preserves_external_c0() {
    run_cli_join(false).unwrap();
}
#[test]
#[ignore = "requires shipping OASIS7_FORMAL_NODE_TEST_BIN; actual moving-head state-sync CLI"]
fn formal_full_state_sync_cli_moving_head_restart_catches_up_from_fixed_c0() {
    run_cli_join(true).unwrap();
}

#[test]
#[ignore = "requires shipping OASIS7_FORMAL_NODE_TEST_BIN; real signed recovery history from unchanged C0"]
fn formal_full_state_sync_cli_legal_epoch_new_grant_replays_from_unchanged_c0() {
    run_cli_join_with_recovery(true, true).unwrap();
}

#[test]
#[ignore = "requires shipping OASIS7_FORMAL_NODE_TEST_BIN; dead seed and independently pinned alternate"]
fn formal_full_state_sync_cli_dead_seed_joins_alternate_and_restarts_from_fixed_c0() {
    run_cli_join_with_sources(true, false, true).unwrap();
}

fn compact_status(status: &Value) -> Value {
    let sync = &status["formal_full_state_sync"];
    serde_json::json!({"formal_readiness":status["formal_readiness"],"blocker":status["blocker"],
        "generation":status["local_generation"],"state":sync["state"],"error":sync["error"],
        "last_verified_cut":sync["last_verified_cut"],"loaded_cut":sync["loaded_cut"],
        "last_checked_unix_ms":sync["last_checked_unix_ms"],"grant_epoch":sync["grant_epoch"]})
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SignedEpochGrantError {
    api_version: String,
    code: String,
    message: String,
}
fn assert_signed_epoch_grant_rejection(
    config: &FormalFullSyncConfigV1,
    data: &Path,
    world: &WorldIdentity,
    node_public: &str,
    current: &FormalFullSyncCutV1,
) -> Result<(), String> {
    if current.authority_epoch != 2 {
        return Err("epoch rejection fixture did not have verified E2 cut".into());
    }
    let (node_key, _) =
        crate::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &data.join("keys"),
        )?;
    if node_key.public_key_hex != node_public {
        return Err("epoch probe changed local node principal".into());
    }
    let request = FormalFullSyncManifestRequestV1 {
        schema_version: 1,
        grant: grant(node_public, world, &config.chain_id, 1),
        cut: current.clone(),
    };
    let signed = authority::sign_read_request(
        FORMAL_FULL_SYNC_MANIFEST_PATH,
        request,
        &node_key.private_key_hex,
    )
    .map_err(|e| format!("E2 signed manifest request: {e}"))?;
    let digest = authority::request_digest(FORMAL_FULL_SYNC_MANIFEST_PATH, &signed)
        .map_err(|e| format!("E2 signed manifest envelope digest: {e}"))?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(config.request_timeout_ms))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .post(format!(
            "{}{}",
            config.endpoint, FORMAL_FULL_SYNC_MANIFEST_PATH
        ))
        .json(&signed)
        .send()
        .map_err(|e| format!("independent epoch grant HTTP probe: {e}"))?;
    if response.status().as_u16() != 403 {
        return Err(format!(
            "independent old-epoch grant expected403, got{}",
            response.status()
        ));
    }
    let bytes = response.bytes().map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 {
        return Err("epoch rejection response exceeded fixture error bound".into());
    }
    let envelope: SignedServiceResponse<SignedEpochGrantError> =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    authority::verify_service_response(
        FORMAL_FULL_SYNC_MANIFEST_PATH,
        &digest,
        &envelope,
        &config.trusted_service_public_key,
    )
    .map_err(|e| format!("E2 HTTP403 signed manifest response verification: {e}"))?;
    if envelope.payload.code != "read_grant_rejected" || !envelope.payload.message.contains("grant")
    {
        return Err("independent epoch grant response was not an exact signed grant denial".into());
    }
    Ok(())
}

fn exact_old_epoch_grant_denied(status: &Value) -> bool {
    status["formal_readiness"] == false
        && status["formal_full_state_sync"]["last_verified_cut"]["authority_epoch"] == 2
        && status["formal_full_state_sync"]["grant_epoch"] == 1
        && status["formal_full_state_sync"]["error"]
            .as_str()
            .is_some_and(|error| {
                error.contains("read_grant_rejected")
                    || error
                        .contains("formal node read grant identity, epoch, role, or lease mismatch")
            })
}
