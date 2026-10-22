use super::*;
use std::{io::Write, net::TcpListener, thread};
const PASSWORD: &[u8] = b"synthetic private entry unlock";
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn config(endpoint: String) -> (Directory, ParticipantClientConfig) {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let root = Directory(
        std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("participant-sdk-{}", hex::encode(random))),
    );
    std::fs::create_dir(&root.0).unwrap();
    let config = ParticipantClientConfig {
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
        trusted_world_service_key: ws::sign_read_request("public", (), &hex::encode([77; 32]))
            .unwrap()
            .subject_public_key,
        owner_state_directory: root.0.join("owner"),
    };
    (root, config)
}
#[test]
fn owner_login_material_and_originals_are_separate_and_fail_closed() {
    let (_root, c) = config("https://identity.example".into());
    let login = IdentityClient::initialize(&c.identity, PASSWORD).unwrap();
    let owner = ParticipantClient::initialize_owner(&c, PASSWORD).unwrap();
    assert_ne!(login, owner);
    assert!(ParticipantClient::initialize_owner(&c, PASSWORD).is_err());
    let client = ParticipantClient::new(c.clone(), PASSWORD).unwrap();
    assert_eq!(client.status("op").unwrap().state, "not_started");
    assert!(
        client
            .register("op", "missing-login")
            .unwrap_err()
            .contains("fresh identity login")
    );
    drop(client);
    assert!(ParticipantClient::new(c.clone(), b"wrong synthetic password").is_err());
    std::fs::remove_file(c.owner_state_directory.join("owner-control.encrypted-v1")).unwrap();
    assert!(ParticipantClient::new(c.clone(), PASSWORD).is_err());
    assert_eq!(
        IdentityClient::new(c.identity, PASSWORD)
            .unwrap()
            .public_key(),
        login
    );
}
#[test]
fn lost_commit_reply_restarts_with_same_business_original_and_fresh_admission() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (_root, c) = config(format!("http://{}", listener.local_addr().unwrap()));
    IdentityClient::initialize(&c.identity, PASSWORD).unwrap();
    ParticipantClient::initialize_owner(&c, PASSWORD).unwrap();
    let client = ParticipantClient::new(c.clone(), PASSWORD).unwrap();
    client
        .identity
        .store
        .put_original(
            &client.identity.scope,
            "login",
            "login-session",
            &IdentitySession {
                account_id: "account".into(),
                cookie: format!("__Host-oasis7_sid={}", URL_SAFE_NO_PAD.encode([8; 32])),
            },
        )
        .unwrap();
    let world = c.world.clone();
    let server = thread::spawn(move || {
        let mut original: Option<SignedReadRequest<GameParticipantRegisterV1>> = None;
        let mut committed = None;
        let mut authorizations = 0;
        let mut lookup_count = 0;
        let mut owner_views = 0;
        for _ in 0..33 {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let (path, body) = loop {
                let mut chunk = [0u8; 2048];
                let n = socket.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() < 65536);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                    let length: usize = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .map(|s| s.trim().parse().unwrap())
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        let path = head
                            .lines()
                            .next()
                            .unwrap()
                            .split_whitespace()
                            .nth(1)
                            .unwrap()
                            .to_string();
                        break (
                            path,
                            if length == 0 {
                                Value::Null
                            } else {
                                parse_request::<Value>(&bytes[end + 4..end + 4 + length]).unwrap()
                            },
                        );
                    }
                }
            };
            let reply = if path.ends_with("/session") {
                json!({"api_version":2,"ok":true,"account":{"account_id":"account"}})
            } else if path.ends_with("/capabilities") {
                json!({"api_version":2,"ok":true,"csrf_token":"c".repeat(64)})
            } else if path.ends_with("/participant/owner-view") {
                owner_views += 1;
                if owner_views == 1 {
                    // Settlement exists, but a failed canonical read must not
                    // become ownership confirmation by the orchestration.
                    json!({"api_version":2,"ok":true,"world_response":null})
                } else {
                    let owner = &original.as_ref().unwrap().request;
                    let commit = json!({"world":world,"binding":{"provider_world_id":"world","branch_id":"main","finality_ref":"test","reorg_generation":0,"governing_manifest_ref":"test","authority_generation":1,"permission_generation":1},"position":1,"execution_block_hash":"test-block","state_root_ref":"test-root"});
                    let request: ws::participant_discovery::ParticipantOwnerViewRequestV1 =
                        serde_json::from_value(body.clone()).unwrap();
                    assert_eq!(request.response_profile, Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerControlV1));
                    let view: ws::participant_discovery::ParticipantOwnerControlViewV1 = serde_json::from_value(json!({"canonical_agent_owner":{"agent_id":"agent","player_id":"binding","public_key":owner.owner_public_key,"world_id":"world","reorg_epoch":0,"canonical_authority":{"branch_id":"main","agent_identity_generation":1},"current_intent_id":null},"game_task_control":null})).unwrap();
                    let response = ws::participant_discovery::ParticipantOwnerControlResponseV1 {
                        schema_revision: 1,
                        world: world.clone(),
                        version: ProjectionVersion {
                            commit: serde_json::from_value(commit).unwrap(),
                            projection_revision: ws::request_digest("projection", &view).unwrap(),
                            visibility_scope: request.read.request.scope_id.clone(),
                        },
                        logical_tick: 1,
                        view,
                    };
                    response.validate(&request).unwrap();
                    let signed = ws::sign_service_response(
                        ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                        ws::request_digest(
                            ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                            &request,
                        )
                        .unwrap(),
                        response,
                        &hex::encode([77; 32]),
                    )
                    .unwrap();
                    if owner_views == 3 {
                        // A legacy full view cannot bypass the existing 64KiB
                        // client bound merely because the caller requested compact.
                        json!({"api_version":2,"ok":true,"world_response":signed,"legacy_history":"x".repeat(70000)})
                    } else {
                        json!({"api_version":2,"ok":true,"world_response":signed})
                    }
                }
            } else if path.ends_with("/participant/authorize") {
                let owner: SignedReadRequest<GameParticipantRegisterV1> =
                    serde_json::from_value(body).unwrap();
                ws::verify_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN, &owner).unwrap();
                if let Some(old) = &original {
                    assert_eq!(old, &owner);
                } else {
                    original = Some(owner.clone());
                }
                authorizations += 1;
                let mut namespace = owner.request.identity_namespace.clone();
                namespace.instance_epoch = DecimalU64(authorizations);
                let admission=ws::sign_read_request(GAME_PARTICIPANT_ADMISSION_DOMAIN,GameParticipantAdmissionV1{
                    api_rev:1,business_digest:oasis7_client_api::world_service_correlation::participant_business_digest(&owner.request).unwrap(),
                    identity_namespace:namespace,account_id:owner.request.account_id.clone(),world_id:world.world_id.clone(),
                    genesis_digest:world.genesis_digest.clone(),owner_public_key:owner.subject_public_key.clone(),branch_id:"main".into(),reorg_epoch:DecimalU64(0),
                    policy_generation:DecimalU64(1),valid_from_tick:DecimalU64(authorizations),valid_until_tick:DecimalU64(authorizations+10)},&hex::encode([78;32])).unwrap();
                json!({"api_version":2,"ok":true,"phase":"participant_authorized","new_registration_enabled":true,"admission":admission})
            } else {
                let (payload, correlation, path) = if path.ends_with("/participant/submit") {
                    let request: SubmitIntentRequest<WorldServicePayloadV1> =
                        serde_json::from_value(body).unwrap();
                    committed = Some(request.correlation.clone());
                    // Canonical effect happened, but the entire response is lost.
                    drop(socket);
                    continue;
                } else {
                    let lookup: AuthenticatedLookup = serde_json::from_value(body.clone()).unwrap();
                    lookup_count += 1;
                    let correlation =
                        oasis7_client_api::world_service_correlation::derive_correlation(
                            world.clone(),
                            &lookup.original,
                        )
                        .unwrap();
                    (lookup.original, correlation, LOOKUP_PATH)
                };
                let outcome = if let Some(old) = &committed {
                    assert_eq!(old, &correlation);
                    let WorldServicePayloadV1::GameParticipantRegister { owner, .. } = payload
                    else {
                        panic!()
                    };
                    let r = owner.request;
                    IntentOutcome::Committed{commit:Box::new(CommitRef{world:world.clone(),binding:ExecutionBinding{provider_world_id:"world".into(),branch_id:"main".into(),
                        finality_ref:"test".into(),reorg_generation:0,governing_manifest_ref:"test".into(),authority_generation:1,permission_generation:1},position:1,
                        execution_block_hash:"test-block".into(),state_root_ref:"test-root".into()}),receipt:serde_json::to_value(GameParticipantReceiptV1{
                            api_rev:1,imported:false,identity_namespace:r.identity_namespace.clone(),account_id:r.account_id.clone(),world_id:r.world_id.clone(),genesis_digest:r.genesis_digest.clone(),
                            agent_id:"agent".into(),owner_public_key:r.owner_public_key.clone(),owner_binding:"binding".into(),owner_generation:DecimalU64(1),client_operation_id:r.client_operation_id.clone(),
                            business_digest:oasis7_client_api::world_service_correlation::participant_business_digest(&r).unwrap(),registered_tick:DecimalU64(1)}).unwrap()}
                } else {
                    IntentOutcome::Unknown
                };
                let response = IntentResponse {
                    contract_version: 1,
                    formal_profile: None,
                    execution_evidence_scope: None,
                    correlation,
                    outcome,
                };
                let signed = ws::sign_service_response(
                    path,
                    ws::request_digest(path, &body).unwrap(),
                    response,
                    &hex::encode([77; 32]),
                )
                .unwrap();
                json!({"api_version":2,"ok":true,"world_response":signed})
            };
            let body = serde_json::to_vec(&reply).unwrap();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            let written = socket.write_all(&body);
            if body.len() <= 65536 {
                written.unwrap();
            }
        }
        assert_eq!(authorizations, 5);
        assert_eq!(owner_views, 3);
        assert_eq!(lookup_count, 2);
    });
    assert!(client.register("original-op", "login").is_err());
    assert_eq!(client.status("original-op").unwrap().state, "unknown");
    drop(client);
    let mut advanced = c;
    advanced.identity_instance_epoch = DecimalU64(2);
    let restarted = ParticipantClient::new(advanced, PASSWORD).unwrap();
    let result = restarted.register("original-op", "login").unwrap();
    assert_eq!(result.agent_id.as_deref(), Some("agent"));
    assert_eq!(result.next_action, "verify_canonical_owner_view");
    assert_ne!(result.state, "confirmed");
    // Feed the actual signed SDK settlement through the cold-entry transition,
    // then perform its real HTTP canonical read. The old confirmed-only guard
    // stopped before this read and could never finish participant admission.
    let error = crate::cold_entry::participant_result(&result, "original-op", || {
        restarted.current_owner("original-op", "login").map(|_| ())
    })
    .unwrap_err();
    assert!(error.contains("owner view response malformed"));
    assert_eq!(
        restarted.status("original-op").unwrap().agent_id.as_deref(),
        Some("agent")
    );
    assert!(
        crate::cold_entry::participant_result(&result, "original-op", || {
            let current = restarted.current_owner("original-op", "login")?;
            assert_eq!(current.receipt.account_id, "account");
            assert_eq!(current.owner.agent_id, "agent");
            assert_eq!(current.owner.public_key, current.receipt.owner_public_key);
            assert_eq!(current.owner.world_id, "world");
            Ok(())
        })
        .unwrap()
        .is_none()
    );
    assert!(
        restarted
            .current_owner("original-op", "login")
            .err()
            .unwrap()
            .contains("identity response exceeds bound")
    );
    server.join().unwrap();
}
