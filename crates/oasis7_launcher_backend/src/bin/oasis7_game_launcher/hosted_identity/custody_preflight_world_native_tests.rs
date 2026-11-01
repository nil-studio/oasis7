//! Real Native ceremony/session/adapter and real loopback World HTTP, with a
//! fixture Account issuer. This is not Account payment or production deployment.
use super::super::super::super as identity;
use super::*;
use oasis7_custody_core::native_cleanup_v2::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
#[path = "custody_preflight_world_native_certificate_v2.rs"]
mod native_certificate;
#[path = "custody_preflight_world_native_fixture_v2.rs"]
mod native_fixture;
type LockProbe = Arc<std::sync::Mutex<Option<Box<dyn Fn() -> bool + Send>>>>;
struct WorldServer {
    probe: LockProbe,
    endpoint: String,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    join: std::thread::JoinHandle<()>,
}
impl WorldServer {
    fn start(world: &str, genesis: &str, node_seed: u8, namespace: Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let world = world.to_string();
        let genesis = genesis.to_string();
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let probe: LockProbe = Arc::new(std::sync::Mutex::new(None));
        let lock_probe = probe.clone();
        let join = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            while !stopping.load(Ordering::SeqCst)
                && Instant::now() < until
                && observed.load(Ordering::SeqCst) < 2
            {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(e) => panic!("world accept {:?}", e.kind()),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                let (head, length) = loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() <= 65536);
                    if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        let header = std::str::from_utf8(&bytes[..i]).unwrap();
                        assert!(header.starts_with("POST /v1/world/participant-policy HTTP/1.1"));
                        let length: usize = header
                            .lines()
                            .find_map(|l| {
                                l.split_once(':')
                                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                    .map(|(_, v)| v.trim().parse().unwrap())
                            })
                            .unwrap();
                        break (i + 4, length);
                    }
                };
                while bytes.len() < head + length {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                assert!(
                    lock_probe
                        .lock()
                        .unwrap()
                        .as_ref()
                        .expect("installed actual runtime probe")(),
                    "World send must release Native identity mutex"
                );
                let request:oasis7_client_api::world_service::SignedReadRequest<oasis7_client_api::world_service::participant_discovery::ParticipantPolicyRequestV1>=serde_json::from_slice(&bytes[head..head+length]).unwrap();
                assert!(
                    request.request.expected_world.world_id == world
                        && request.request.expected_world.genesis_digest == genesis
                );
                oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
                    "/v1/world/participant-policy",
                    &request.request,
                    &hex::encode(SigningKey::from_bytes(&[83; 32]).verifying_key().to_bytes()),
                    &request.signature_hex,
                )
                .unwrap();
                let payload = json!({"world_id":world,"genesis_digest":genesis,"branch_id":"main","reorg_epoch":0,"logical_tick":42,
                    "policy":{"world_id":world,"genesis_digest":genesis,"enabled":true,"registry_complete":true,"identity_namespace":namespace,
                        "issuer_public_key":hex::encode(SigningKey::from_bytes(&[83;32]).verifying_key().to_bytes()),"generation":1,"max_grant_ticks":100}});
                let response = oasis7_client_api::world_service::sign_service_response(
                    "/v1/world/participant-policy",
                    oasis7_client_api::world_service::request_digest(
                        "/v1/world/participant-policy",
                        &request,
                    )
                    .unwrap(),
                    payload,
                    &hex::encode([node_seed; 32]),
                )
                .unwrap();
                let body = serde_json::to_vec(&response).unwrap();
                observed.fetch_add(1, Ordering::SeqCst);
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
                stream.flush().unwrap();
            }
        });
        Self {
            probe,
            endpoint,
            count,
            stop,
            join,
        }
    }
    fn finish(self) -> usize {
        self.stop.store(true, Ordering::SeqCst);
        self.join.join().unwrap();
        self.count.load(Ordering::SeqCst)
    }
}
fn exchange(
    service: &Arc<std::sync::Mutex<Option<identity::IdentityService>>>,
    path: &str,
    headers: &str,
    body: &impl Serialize,
) -> Result<Value, u16> {
    let reply = crate::static_http::cleanup_dispatch_for_test(
        service.clone(),
        path,
        headers,
        &serde_json::to_vec(body).unwrap(),
    );
    if reply.status == 200 {
        Ok(reply.body)
    } else {
        Err(reply.status)
    }
}

fn subprocess(scenario: &str) {
    let directory = std::env::temp_dir().join(format!("world2-native-{}", random_token()));
    std::fs::create_dir(&directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let now = now_ms();
    let pin = |purpose: &str| {
        json!({"namespace":"account","service_id":"account-service","environment":"test","key_id":"account-key",
        "public_key":hex::encode(SigningKey::from_bytes(&[81;32]).verifying_key().to_bytes()),"issuer_epoch":"1","config_generation":"1",
        "not_before":now.saturating_sub(1000).to_string(),"not_after":(now+600000).to_string(),"native_audience_origin":"https://localhost","allowed_purpose":purpose})
    };
    let config = json!({"api_version":2,"pending_pin":pin("consumed_pending"),"active_pin":pin("active_three_roots"),"current_pin":pin("native_current_attempt"),
        "term_current_pin":pin("native_term_current_attempt"),"preflight_access_pin":pin("native_preflight_access_v2"),"issuer_floor":{"issuer_epoch":"1","config_generation":"1"},"native_origin":"https://localhost"});
    let status=std::process::Command::new(std::env::current_exe().unwrap()).arg("world3_fixture_issuer_native_adapter_spends_each_real_world_read_once")
        .arg("--nocapture").arg("--test-threads=1").env("OASIS7_WORLD2_NATIVE_SCENARIO",scenario)
        .env("OASIS7_CUSTODY_MANAGED_BINDINGS_ENABLED","true")
        .env("OASIS7_CUSTODY_MANAGED_BINDING_OWNER_PIN",json!({"key_id":"owner-binding","public_key":hex::encode(SigningKey::from_bytes(&[72;32]).verifying_key().to_bytes()),"environment":"test","service_identity":"owner-service","service_epoch":"1"}).to_string())
        .env("OASIS7_CUSTODY_CLEANUP_MACHINE_V2_ENABLED","true").env("OASIS7_CUSTODY_CLEANUP_ACCOUNT_ISSUER_CONFIG",config.to_string())
        .env("OASIS7_CUSTODY_CLEANUP_SQLITE_RESTORE_GUARD_PATH",directory.join("lineage")).status().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(status.success(), "Native World2 component child failed");
}
#[test]
fn world3_fixture_issuer_native_adapter_spends_each_real_world_read_once() {
    let Some(scenario) = std::env::var("OASIS7_WORLD2_NATIVE_SCENARIO").ok() else {
        for scenario in [
            "normal",
            "wrong-world",
            "wrong-genesis",
            "wrong-node",
            "missing-provenance",
            "current-config",
        ] {
            subprocess(scenario);
        }
        return;
    };
    let mut f = native_fixture::managed_fixture();
    let world = if scenario == "wrong-world" {
        "other-world"
    } else {
        "world"
    };
    let genesis = if scenario == "wrong-genesis" {
        "other-genesis"
    } else {
        "genesis"
    };
    let seed = if scenario == "wrong-node" { 84 } else { 82 };
    let namespace = json!({"service_id":f.f.service.config.namespace.identity_service_id,"environment_id":f.f.service.config.namespace.environment_id,
        "instance_epoch":f.f.service.config.rp.identity_instance_epoch.to_string()});
    let server = WorldServer::start(world, genesis, seed, namespace);
    let trust=serde_json::from_value(json!({"endpoint":server.endpoint.clone(),"service_public_key":hex::encode(SigningKey::from_bytes(&[seed;32]).verifying_key().to_bytes()),
        "world":{"world_id":world,"genesis_digest":genesis}})).unwrap();
    f.f.service.participant_entry = Some(
        identity::participant_entry::ParticipantEntry::new(
            trust,
            zeroize::Zeroizing::new(hex::encode([83; 32])),
        )
        .unwrap(),
    );
    f.f.service
        .store
        .attach_machine_restore_guard_from_env(&f.f.service.config.sqlite_path)
        .unwrap();
    let request = native_certificate::options_for(&f, now_ms());
    let (input, consumed_at) = native_certificate::approve_and_consume(&mut f, request);
    let proof = native_certificate::certificate(&f, input, consumed_at);
    if scenario == "missing-provenance" {
        let config = f.f.service.custody.as_ref().unwrap();
        let id = proof.certificate.input.approval.proof.approval_id.as_str();
        f.f.service
            .store
            .transaction(|tx| {
                let saved = tx.get::<ApprovalRecord>(&approval_key(id))?.unwrap();
                identity::custody::guarded_consent::verify_existing_guarded_consent_v2(
                    tx,
                    config,
                    &f.f.service.config,
                    &saved.value,
                )?;
                assert!(saved.value.guarded_consent_provenance.is_some());
                let before = serde_json::to_value(&saved.value).unwrap();
                let mut without = saved.value.clone();
                without.guarded_consent_provenance = None;
                assert!(
                    identity::custody::guarded_consent::verify_existing_guarded_consent_v2(
                        tx,
                        config,
                        &f.f.service.config,
                        &without
                    )
                    .is_err(),
                    "new World3 complete must reject missing protected provenance"
                );
                assert_eq!(
                    serde_json::to_value(
                        &tx.get::<ApprovalRecord>(&approval_key(id))?.unwrap().value
                    )
                    .unwrap(),
                    before
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(server.finish(), 0);
        return;
    }
    if scenario == "current-config" {
        use identity::custody::{
            cleanup_config::MachineConfig, preflight_world_registry as registry,
        };
        let machine_a = MachineConfig::from_env().unwrap();
        let mut changed = serde_json::to_value(&machine_a).unwrap();
        let until = changed["pending_pin"]["not_after"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        changed["pending_pin"]["not_after"] = json!((until - 1).to_string());
        let machine_b: MachineConfig = serde_json::from_value(changed).unwrap();
        let config = f.f.service.custody.as_ref().unwrap();
        let gateway =
            f.f.service
                .participant_entry
                .as_ref()
                .unwrap()
                .owned_owner_read_gateway();
        let actual_world = gateway.fingerprint().unwrap();
        let old_configuration = machine_a
            .fingerprint(config, &f.f.service.config, &actual_world)
            .unwrap();
        let current_configuration = machine_b
            .fingerprint(config, &f.f.service.config, &actual_world)
            .unwrap();
        assert_ne!(old_configuration, current_configuration);
        let storage = session_key(&f.f.service.config.session_key, &f.sid);
        let scope = registry::WorldStageScope {
            config,
            identity: &f.f.service.config,
            storage_key: &storage,
            configuration_digest: &current_configuration,
            pin: machine_b.preflight_access_pin().unwrap(),
            floor: &machine_b.issuer_floor,
            now: now_ms(),
        };
        let claimed =
            f.f.service
                .store
                .transaction(|tx| {
                    registry::claim(
                        tx,
                        &scope,
                        proof.clone(),
                        NativePreflightWorldOperationV2::PolicyOriginal,
                    )
                })
                .unwrap();
        // The paid/once transaction above already committed; failed current cannot refund it.
        assert!(
            f.f.service
                .store
                .transaction(|tx| registry::observe(tx, &scope, claimed))
                .is_err(),
            "actual changed configuration cannot reuse historical consent provenance"
        );
        assert!(
            f.f.service
                .store
                .transaction(|tx| registry::claim(
                    tx,
                    &scope,
                    proof.clone(),
                    NativePreflightWorldOperationV2::PolicyOriginal
                ))
                .is_err()
        );
        assert_eq!(server.finish(), 0);
        return;
    }
    let csrf = digest(&f.f.service.config.session_key, "csrf", &f.sid);
    let headers = format!(
        "POST / HTTP/1.1\r\nOrigin: https://localhost\r\nSec-Fetch-Site: same-origin\r\nContent-Type: application/json\r\nX-Oasis7-CSRF: {csrf}\r\nCookie: __Host-oasis7_sid={}\r\n\r\n",
        f.sid
    );
    let policy_request = NativePreflightPolicyRequestV2 {
        proof: Box::new(proof.clone()),
        body: PreflightPolicyBodyV2 {
            observation_nonce: proof.certificate.descriptor.access_nonces.policy.clone(),
        },
    };
    let service = Arc::new(std::sync::Mutex::new(Some(f.f.service)));
    let probe_service = service.clone();
    *server.probe.lock().unwrap() = Some(Box::new(move || probe_service.try_lock().is_ok()));
    let result = exchange(
        &service,
        NATIVE_PREFLIGHT_POLICY_V2_PATH,
        &headers,
        &policy_request,
    );
    if scenario != "normal" {
        assert!(
            result.is_err(),
            "approved World and Node pin must match before World I/O"
        );
        assert_eq!(server.finish(), 0);
        return;
    }
    let policy: NativePreflightPolicyReplyV2 = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(server.count.load(Ordering::SeqCst), 1);
    policy
        .receipt
        .verify_native(
            &proof,
            &NativePreflightWorldReceiptV2::policy_result_digest(&policy.policy).unwrap(),
            "approval-test-1",
            &hex::encode(SigningKey::from_bytes(&[51; 32]).verifying_key().to_bytes()),
        )
        .unwrap();
    assert!(
        service
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .prepare_preflight_world_v2(
                "POST",
                NATIVE_PREFLIGHT_POLICY_V2_PATH,
                &headers,
                &serde_json::to_vec(&policy_request).unwrap()
            )
            .is_err()
    );
    assert_eq!(server.count.load(Ordering::SeqCst), 1); // no successful replay cache or extra World send
    let authorize = NativePreflightAuthorizeRequestV2 {
        proof: Box::new(proof.clone()),
        body: proof.certificate.descriptor.participant_owner.clone(),
    };
    let reply: NativePreflightAuthorizeReplyV2 = serde_json::from_value(
        exchange(
            &service,
            NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH,
            &headers,
            &authorize,
        )
        .unwrap(),
    )
    .unwrap();
    reply
        .receipt
        .verify_native(
            &proof,
            &NativePreflightWorldReceiptV2::admission_result_digest(&reply.admission).unwrap(),
            "approval-test-1",
            &hex::encode(SigningKey::from_bytes(&[51; 32]).verifying_key().to_bytes()),
        )
        .unwrap();
    assert!(
        service
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .prepare_preflight_world_v2(
                "POST",
                NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH,
                &headers,
                &serde_json::to_vec(&authorize).unwrap()
            )
            .is_err()
    );
    assert_eq!(server.finish(), 2);
}
