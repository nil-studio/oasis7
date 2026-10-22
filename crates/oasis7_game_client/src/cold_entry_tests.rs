use super::*;
#[test]
fn missing_owner_capability_stops_before_any_local_identity_creation() {
    let discovery = serde_json::json!({"phase_capabilities":{"machine_registration":true,"participant_registration":true,"executor_control":true}});
    assert!(
        require_enabled(&discovery).is_err(),
        "a cold entry must not create keys or register while owner control is absent"
    );
}

use crate::identity_client::IdentityClientConfig;
use oasis7_client_api::world_service::WorldIdentity;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
pub(super) const PASSWORD: &[u8] = b"synthetic cold entry private unlock";
pub(super) struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) fn fixture(endpoint: String) -> (Directory, ColdEntryPlan) {
    let root = Directory(
        std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("cold-entry-{}", random_operation().unwrap())),
    );
    let private = private_storage::open_root(&root.0).unwrap();
    drop(private);
    let plan = ColdEntryPlan {
        owner_approved: true,
        entry: ParticipantClientConfig {
            identity: IdentityClientConfig {
                endpoint,
                identity_service_id: "identity".into(),
                environment_id: "test".into(),
                audience: "https://identity.example".into(),
                state_directory: root.0.join("identity"),
                test_loopback: true,
            },
            identity_instance_epoch: DecimalU64(1),
            world: WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
            },
            trusted_world_service_key: hex::encode(
                SigningKey::from_bytes(&[77; 32]).verifying_key().to_bytes(),
            ),
            owner_state_directory: root.0.join("owner"),
        },
        orchestration_directory: root.0.join("orchestration"),
        executor_vault_directory: root.0.join("executor"),
        executor_state_directory: root.0.join("operations"),
        executor_config_directory: root.0.join("exports"),
        display_name: "Independent cold entry".into(),
        starter: Starter::Claim,
        executor_id: "cold-executor".into(),
        delegation_generation: DecimalU64(1),
        delegation_nonce: DecimalU64(1),
        goal: GoalContentV1 {
            goal_text: "PRIVATE intact target with whitespace  ".into(),
            soft_constraints: vec!["no spending beyond approved plan".into()],
        },
    };
    (root, plan)
}
fn discovery(p: &ColdEntryPlan) -> Value {
    json!({"api_version":2,"ok":true,"schema_revision":1,"identity":{"service_id":"identity","environment_id":"test","instance_epoch":"1"},"world":p.entry.world,"world_service_public_key":p.entry.trusted_world_service_key,"phase_capabilities":{"machine_registration":true,"participant_registration":true,"owner_control":true,"executor_control":true,"complete_cold_entry":false},"runtime_profiles":[]})
}
fn request(stream: &mut std::net::TcpStream) -> (String, String) {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let mut data = Vec::new();
    let mut buf = [0; 4096];
    let mut size = None;
    loop {
        let n = stream.read(&mut buf).unwrap();
        assert_ne!(n, 0);
        data.extend_from_slice(&buf[..n]);
        if let Some(offset) = data.windows(4).position(|v| v == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..offset]);
            let length = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            size = Some((offset + 4, length));
        }
        if size.is_some_and(|(offset, n)| data.len() >= offset + n) {
            break;
        }
    }
    let (offset, n) = size.unwrap();
    (
        String::from_utf8_lossy(&data[..offset])
            .lines()
            .next()
            .unwrap()
            .into(),
        String::from_utf8(data[offset..offset + n].to_vec()).unwrap(),
    )
}
fn response(stream: &mut std::net::TcpStream, value: &Value, cookie: bool) {
    let bytes = serde_json::to_vec(value).unwrap();
    let header = if cookie {
        format!(
            "Set-Cookie: __Host-oasis7_pre={}; Secure; HttpOnly; Path=/\r\n",
            URL_SAFE_NO_PAD.encode([8; 32])
        )
    } else {
        String::new()
    };
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
        bytes.len(),
        header
    )
    .unwrap();
    stream.write_all(&bytes).unwrap();
}
fn init_server(listener: TcpListener, view: Value) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (path, _) = request(&mut stream);
        assert!(path.contains("/entry/discovery"));
        response(&mut stream, &view, false);
    })
}
#[test]
fn preflight_binds_every_pin_and_requires_no_unpublished_runtime_claim() {
    let (_d, plan) = fixture("https://identity.example".into());
    let view = discovery(&plan);
    assert!(validate_discovery(&plan, &view).is_ok());
    for path in [
        "/world/genesis_digest",
        "/world/world_id",
        "/world_service_public_key",
        "/identity/instance_epoch",
    ] {
        let mut wrong = view.clone();
        *wrong.pointer_mut(path).unwrap() = json!("different");
        assert!(validate_discovery(&plan, &wrong).is_err());
    }
    // Discovery epochs are canonical decimal strings. A JSON number must not
    // become acceptable merely because its numeric value matches the pin.
    for epoch in [json!(1), json!("01"), json!("+1"), json!("1 "), Value::Null] {
        let mut wrong = view.clone();
        wrong["identity"]["instance_epoch"] = epoch;
        assert!(validate_discovery(&plan, &wrong).is_err());
    }
    for capability in [
        "machine_registration",
        "participant_registration",
        "owner_control",
        "executor_control",
    ] {
        let mut wrong = view.clone();
        wrong["phase_capabilities"][capability] = json!(false);
        assert!(validate_discovery(&plan, &wrong).is_err());
    }
}
#[test]
fn private_plan_rejects_links_permissions_and_overlapping_vaults() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (d, mut plan) = fixture("https://identity.example".into());
    let dir = private_storage::open_root(&d.0).unwrap();
    private_storage::publish_immutable(&dir, "approved.json", &serde_json::to_vec(&plan).unwrap())
        .unwrap();
    assert!(private_json::<ColdEntryPlan>(&d.0.join("approved.json"), 16384).is_ok());
    symlink("approved.json", d.0.join("symlink.json")).unwrap();
    assert!(private_json::<ColdEntryPlan>(&d.0.join("symlink.json"), 16384).is_err());
    std::fs::hard_link(d.0.join("approved.json"), d.0.join("hardlink.json")).unwrap();
    assert!(private_json::<ColdEntryPlan>(&d.0.join("approved.json"), 16384).is_err());
    std::fs::remove_file(d.0.join("hardlink.json")).unwrap();
    std::fs::set_permissions(
        d.0.join("approved.json"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(private_json::<ColdEntryPlan>(&d.0.join("approved.json"), 16384).is_err());
    plan.executor_vault_directory = plan.entry.owner_state_directory.join("child");
    assert!(plan.validate().is_err());
}
#[test]
fn durable_plan_preserves_goal_ids_and_rejects_missing_or_replaced_role_vault() {
    let (d, plan) = fixture("https://identity.example".into());
    initialize_vaults(plan.clone(), PASSWORD).unwrap();
    let (store, original) = open(&plan.orchestration_directory, PASSWORD).unwrap();
    assert_eq!(original.plan.goal, plan.goal);
    let ids = original.operations.clone();
    drop(store);
    let (store, reopened) = open(&plan.orchestration_directory, PASSWORD).unwrap();
    assert_eq!(ids, reopened.operations);
    assert!(execution::verify_keys(&store, &reopened.plan, PASSWORD).is_ok());
    for file in std::fs::read_dir(&plan.orchestration_directory).unwrap() {
        let p = file.unwrap().path();
        if p.is_file() {
            assert!(
                !std::fs::read(p)
                    .unwrap()
                    .windows(14)
                    .any(|b| b == b"PRIVATE intact")
            );
        }
    }
    let keyfile = plan
        .executor_vault_directory
        .join("executor-control.encrypted-v1");
    std::fs::remove_file(keyfile).unwrap();
    assert!(execution::verify_keys(&store, &reopened.plan, PASSWORD).is_err());
    assert_eq!(status(&store, &reopened).unwrap()["stage"], "registration");
    assert!(
        !plan
            .executor_config_directory
            .join("executor.json")
            .exists()
    );
    // No deleted role key can be regenerated by opening or resuming the plan.
    assert!(
        !plan
            .executor_vault_directory
            .join("executor-control.encrypted-v1")
            .exists()
    );
    let _ = d;
}
#[test]
fn partial_initialization_is_explicitly_blocked_and_never_rekeys() {
    let (_d, plan) = fixture("https://identity.example".into());
    let store = OwnerControlStore::create(&plan.orchestration_directory, PASSWORD).unwrap();
    let immutable = ImmutablePlan {
        plan: plan.clone(),
        operations: (0..STAGES.len())
            .map(|_| random_operation().unwrap())
            .collect(),
    };
    store.put_original(SCOPE, PLAN, "plan", &immutable).unwrap();
    let partial = status(&store, &immutable).unwrap();
    assert_eq!(partial["state"], "action_required");
    assert!(partial.get("participant_operation_id").is_none());
    assert!(partial.get("active_login_operation_id").is_none());
    assert!(execution::verify_keys(&store, &plan, PASSWORD).is_err());
    assert!(!plan.entry.identity.state_directory.exists());
}
#[test]
fn unknown_registration_restart_replays_exact_signed_original_and_stops_same_stage() {
    use oasis7_agent_api::identity::{NativeIdentityChallengeV1, NativeIdentityPurposeV1};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (_d, plan) = fixture("https://identity.example".into());
    initialize_vaults(plan.clone(), PASSWORD).unwrap();
    let (store, mut immutable) = open(&plan.orchestration_directory, PASSWORD).unwrap();
    // Only this compiled test injects its loopback transport; the production
    // plan codec keeps rejecting a supplied test_loopback field.
    immutable.plan.entry.identity.endpoint = format!("http://{}", listener.local_addr().unwrap());
    immutable.plan.entry.identity.test_loopback = true;
    let keys = execution::verify_keys(&store, &plan, PASSWORD).unwrap();
    let view = discovery(&plan);
    let op = immutable.operations[0].clone();
    let identity = plan.entry.identity.clone();
    let server = thread::spawn(move || {
        let mut originals = Vec::new();
        for _ in 0..6 {
            let (mut stream, _) = listener.accept().unwrap();
            let (path, body) = request(&mut stream);
            if path.contains("/entry/discovery") {
                response(&mut stream, &view, false);
            } else if path.contains("/capabilities") {
                response(
                    &mut stream,
                    &json!({"api_version":2,"ok":true,"native_key":{"identity_service_id":"identity","environment_id":"test","algorithm":"Ed25519","available":true},"machine_registration":{"available":true},"csrf_token":"a".repeat(64)}),
                    true,
                );
            } else if path.contains("/machine-registration/options") {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                let key: [u8; 32] = hex::decode(&keys.identity).unwrap().try_into().unwrap();
                let nonce = [2; 32];
                let signed = NativeIdentityChallengeV1 {
                    identity_service_id: &identity.identity_service_id,
                    environment_id: &identity.environment_id,
                    audience: &identity.audience,
                    purpose: NativeIdentityPurposeV1::Register,
                    account_context: "",
                    operation_id: &op,
                    public_key: &key,
                    nonce: &nonce,
                    issued_at: now,
                    expires_at: now + 120000,
                }
                .signing_bytes()
                .unwrap();
                response(
                    &mut stream,
                    &json!({"api_version":2,"ok":true,"operation_id":op,"ceremony_id":op,"expires_at":now+120000,"options":{"identity_service_id":identity.identity_service_id,"environment_id":identity.environment_id,"audience":identity.audience,"purpose":"register","account_context":"","operation_id":op,"public_key":keys.identity,"nonce":URL_SAFE_NO_PAD.encode(nonce),"issued_at":now,"expires_at":now+120000,"signed_bytes":URL_SAFE_NO_PAD.encode(signed)}}),
                    false,
                );
            } else {
                assert!(path.contains("/machine-registration/complete"));
                originals.push(body);
                response(
                    &mut stream,
                    &json!({"api_version":2,"ok":true,"phase":"outcome_unknown"}),
                    false,
                );
            }
        }
        assert_eq!(originals.len(), 2);
        assert_eq!(originals[0], originals[1]);
    });
    for _ in 0..2 {
        let result = execution::execute(&store, &immutable, PASSWORD).unwrap();
        assert_eq!(result["stage"], "registration");
        assert_eq!(result["state"], "unknown");
    }
    server.join().unwrap();
    let progress = status(&store, &immutable).unwrap();
    assert_eq!(progress["stage"], "registration");
    assert_eq!(progress["state"], "unknown");
    assert_eq!(
        store
            .read_original::<bool>(SCOPE, &immutable.operations[1], "started")
            .unwrap(),
        None
    );
}

#[test]
fn disabled_discovery_performs_no_vault_creation() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (_d, plan) = fixture(format!("http://{}", listener.local_addr().unwrap()));
    let mut view = discovery(&plan);
    view["phase_capabilities"]["owner_control"] = json!(false);
    let server = init_server(listener, view);
    assert!(
        initialize(plan.clone(), PASSWORD)
            .unwrap_err()
            .contains("unsupported")
    );
    server.join().unwrap();
    for path in [
        &plan.entry.identity.state_directory,
        &plan.entry.owner_state_directory,
        &plan.orchestration_directory,
        &plan.executor_vault_directory,
    ] {
        assert!(!path.exists());
    }
}

#[test]
fn lost_login_cookie_requires_explicit_fresh_id_and_resumes_same_new_original() {
    use oasis7_agent_api::identity::{NativeIdentityChallengeV1, NativeIdentityPurposeV1};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (_d, plan) = fixture("https://identity.example".into());
    initialize_vaults(plan.clone(), PASSWORD).unwrap();
    let (store, mut immutable) = open(&plan.orchestration_directory, PASSWORD).unwrap();
    immutable.plan.entry.identity.endpoint = format!("http://{}", listener.local_addr().unwrap());
    immutable.plan.entry.identity.test_loopback = true;
    store
        .put_original(SCOPE, PLAN, "registered-account", &"account".to_string())
        .unwrap();
    let keys = execution::verify_keys(&store, &immutable.plan, PASSWORD).unwrap();
    let view = discovery(&immutable.plan);
    let identity = immutable.plan.entry.identity.clone();
    let old = immutable.operations[1].clone();
    let fresh = random_operation().unwrap();
    let old_server = old.clone();
    let fresh_server = fresh.clone();
    let business = immutable.operations.clone();
    let server = thread::spawn(move || {
        let mut options = 0;
        let mut old_wires = Vec::new();
        let mut new_wires = Vec::new();
        for _ in 0..10 {
            let (mut stream, _) = listener.accept().unwrap();
            let (path, body) = request(&mut stream);
            if path.contains("/entry/discovery") {
                response(&mut stream, &view, false);
            } else if path.contains("/capabilities") {
                response(
                    &mut stream,
                    &json!({"api_version":2,"ok":true,"native_key":{"identity_service_id":"identity","environment_id":"test","algorithm":"Ed25519","available":true},"machine_registration":{"available":true},"csrf_token":"a".repeat(64)}),
                    true,
                );
            } else if path.contains("/native-key/options") {
                let operation = if options == 0 {
                    &old_server
                } else {
                    &fresh_server
                };
                options += 1;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                let key: [u8; 32] = hex::decode(&keys.identity).unwrap().try_into().unwrap();
                let nonce = [3; 32];
                let signed = NativeIdentityChallengeV1 {
                    identity_service_id: &identity.identity_service_id,
                    environment_id: &identity.environment_id,
                    audience: &identity.audience,
                    purpose: NativeIdentityPurposeV1::Login,
                    account_context: "",
                    operation_id: operation,
                    public_key: &key,
                    nonce: &nonce,
                    issued_at: now,
                    expires_at: now + 120000,
                }
                .signing_bytes()
                .unwrap();
                response(
                    &mut stream,
                    &json!({"api_version":2,"ok":true,"operation_id":operation,"ceremony_id":operation,"expires_at":now+120000,"options":{"identity_service_id":identity.identity_service_id,"environment_id":identity.environment_id,"audience":identity.audience,"purpose":"login","account_context":"","operation_id":operation,"public_key":keys.identity,"nonce":URL_SAFE_NO_PAD.encode(nonce),"issued_at":now,"expires_at":now+120000,"signed_bytes":URL_SAFE_NO_PAD.encode(signed)}}),
                    false,
                );
            } else {
                assert!(path.contains("/native-key/complete"));
                let value: Value = serde_json::from_str(&body).unwrap();
                if value["operation_id"] == old_server {
                    old_wires.push(body);
                    response(
                        &mut stream,
                        &json!({"api_version":2,"ok":true,"phase":if old_wires.len()==1{"unknown"}else{"operation_committed"}}),
                        false,
                    );
                } else {
                    assert_eq!(value["operation_id"], fresh_server);
                    new_wires.push(body);
                    if new_wires.len() == 1 {
                        response(
                            &mut stream,
                            &json!({"api_version":2,"ok":true,"phase":"unknown"}),
                            false,
                        );
                    } else {
                        let bytes=serde_json::to_vec(&json!({"api_version":2,"ok":true,"phase":"authenticated","account":{"account_id":"account"}})).unwrap();
                        write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nSet-Cookie: __Host-oasis7_sid={}; Secure; HttpOnly; Path=/\r\nConnection: close\r\n\r\n",bytes.len(),URL_SAFE_NO_PAD.encode([9;32])).unwrap();
                        stream.write_all(&bytes).unwrap();
                    }
                }
            }
        }
        assert_eq!(old_wires.len(), 2);
        assert_eq!(new_wires.len(), 2);
        assert_eq!(old_wires[0], old_wires[1]);
        assert_eq!(new_wires[0], new_wires[1]);
    });
    let client = IdentityClient::new(immutable.plan.entry.identity.clone(), PASSWORD).unwrap();
    assert!(client.login(&old).is_err());
    drop(client);
    let first = login::fresh_login(&store, &immutable, PASSWORD, &fresh).unwrap();
    assert_eq!(first["state"], "unknown");
    let pending_status = status(&store, &immutable).unwrap();
    assert_eq!(pending_status["participant_operation_id"], business[2]);
    assert_eq!(pending_status["active_login_operation_id"], old);
    assert_eq!(pending_status["operation_id"], fresh);
    let blocked = execution::execute(&store, &immutable, PASSWORD).unwrap();
    assert_eq!(blocked["stage"], "login_refresh");
    assert_eq!(blocked["state"], "action_required");
    let second = login::fresh_login(&store, &immutable, PASSWORD, &fresh).unwrap();
    assert_eq!(second["state"], "confirmed");
    server.join().unwrap();
    let selected = login::selection(&store, &immutable).unwrap();
    assert_eq!(selected.active, fresh);
    assert_eq!(selected.pending, None);
    assert_eq!(business, immutable.operations);
    let refreshed_status = status(&store, &immutable).unwrap();
    assert_eq!(refreshed_status["participant_operation_id"], business[2]);
    assert_eq!(refreshed_status["active_login_operation_id"], fresh);
}

#[test]
fn participant_pending_or_unknown_requires_original_reconciliation_before_canonical_read() {
    use crate::identity_client::ParticipantProgress;
    for (state, next) in [
        ("pending", "query_original"),
        ("unknown", "query_original"),
        ("unknown", "verify_canonical_owner_view"),
        ("blocked", "inspect_original_rejection"),
    ] {
        let progress = ParticipantProgress {
            stage: "participant_binding".into(),
            state: state.into(),
            operation_id: "original".into(),
            account_id: "account".into(),
            agent_id: Some("agent".into()),
            next_action: next.into(),
        };
        let stopped = execution::participant_result(&progress, "original", || {
            panic!("unsettled original must not consult canonical owner")
        })
        .unwrap()
        .unwrap();
        assert_eq!(stopped["state"], state);
        assert_eq!(stopped["next_action"], next);
        assert_eq!(stopped["operation_id"], "original");
    }
}

#[test]
fn completed_historical_status_exposes_only_original_owner_cli_references() {
    let (_d, plan) = fixture("https://identity.example".into());
    initialize_vaults(plan.clone(), PASSWORD).unwrap();
    let (store, immutable) = open(&plan.orchestration_directory, PASSWORD).unwrap();
    // This is a local history projection, deliberately not world certification.
    // Reconstruct persisted completed history without any network authority.
    for operation in &immutable.operations {
        store
            .put_original(SCOPE, operation, "confirmed", &true)
            .unwrap();
    }
    let result = status(&store, &immutable).unwrap();
    assert_eq!(
        result,
        json!({
            "stage":"complete", "state":"historically_confirmed",
            "next_action":"fresh_canonical_revalidation_required_before_executor_use",
            "participant_operation_id": immutable.operations[2],
            "active_login_operation_id": immutable.operations[1],
        })
    );
    let original = immutable.operations.clone();
    drop(store);
    let (reopened, immutable) = open(&plan.orchestration_directory, PASSWORD).unwrap();
    assert_eq!(status(&reopened, &immutable).unwrap(), result);
    assert_eq!(immutable.operations, original);
}
