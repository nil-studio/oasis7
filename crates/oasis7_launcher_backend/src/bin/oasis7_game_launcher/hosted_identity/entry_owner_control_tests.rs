use super::*;
use oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1;
use oasis7_proto::viewer::{CanonicalAgentChatAuthorityV1, CanonicalAgentOwnerFenceV1};
fn owner() -> CanonicalAgentOwnerFenceV1 {
    CanonicalAgentOwnerFenceV1 {
        agent_id: "agent-a".into(),
        player_id: "account-a".into(),
        public_key: "a".repeat(64),
        world_id: "world".into(),
        reorg_epoch: 3,
        canonical_authority: CanonicalAgentChatAuthorityV1 {
            branch_id: "main".into(),
            agent_identity_generation: 2,
        },
        current_intent_id: None,
    }
}
fn world() -> WorldIdentity {
    WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
    }
}
#[test]
fn owner_control_never_admits_cross_resource_task_or_wrong_generation() {
    let owner = owner();
    let request = GameTaskCreateV1 {
        api_rev: 1,
        client_operation_id: "op".into(),
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(3),
        agent_id: "agent-a".into(),
        owner_binding: "binding-a".into(),
        owner_generation: DecimalU64(2),
    };
    let signed = SignedReadRequest {
        request,
        subject_public_key: owner.public_key.clone(),
        signature_hex: "test".into(),
    };
    assert!(
        validate_payload(
            &WorldServicePayloadV1::GameTaskCreate(signed.clone()),
            &owner,
            &world()
        )
        .is_ok()
    );
    for field in ["agent", "key", "world", "genesis", "branch", "generation"] {
        let mut bad = signed.clone();
        match field {
            "agent" => bad.request.agent_id = "agent-b".into(),
            "key" => bad.subject_public_key = "b".repeat(64),
            "world" => bad.request.world_id = "other".into(),
            "genesis" => bad.request.genesis_digest = "other".into(),
            "branch" => bad.request.branch_id = "other".into(),
            _ => bad.request.owner_generation = DecimalU64(1),
        };
        assert!(
            validate_payload(
                &WorldServicePayloadV1::GameTaskCreate(bad),
                &owner,
                &world()
            )
            .is_err(),
            "{field}"
        );
    }
    assert!(
        validate_payload(
            &WorldServicePayloadV1::GameplayJson(vec![1]),
            &owner,
            &world()
        )
        .is_err()
    );
}
#[test]
fn owner_key_cannot_be_installed_as_delegated_executor() {
    let owner = owner();
    let signed = SignedReadRequest {
        request: AgentSignerDelegationChangeV1 {
            world_id: "world".into(),
            branch_id: "main".into(),
            agent_id: "agent-a".into(),
            owner_binding: "binding-a".into(),
            agent_identity_generation: 2,
            generation: 1,
            delegate_public_key: owner.public_key.clone(),
            revoked: false,
            nonce: 1,
        },
        subject_public_key: owner.public_key.clone(),
        signature_hex: "test".into(),
    };
    assert!(
        matches!(validate_payload(&WorldServicePayloadV1::Delegation(signed.clone()),&owner,&world()),Err(StoreError::Rejected(code)) if code=="owner_key_cannot_be_delegate")
    );
    let mut distinct = signed;
    distinct.request.delegate_public_key = "b".repeat(64);
    assert!(
        validate_payload(
            &WorldServicePayloadV1::Delegation(distinct),
            &owner,
            &world()
        )
        .is_ok()
    );
}
#[test]
fn absent_or_disabled_owner_gateway_and_invalid_session_fail_closed() {
    let mut f = IdentityService::model_executor_test_fixture();
    assert!(
        f.service
            .entry_owner_route(
                "POST",
                "/entry-owner/control",
                Some(&f.token),
                &serde_json::json!({}),
                super::super::now_ms()
            )
            .unwrap()
            .is_err()
    );
    assert!(
        f.service
            .entry_owner_route(
                "POST",
                "/entry-owner/control",
                None,
                &serde_json::json!({}),
                super::super::now_ms()
            )
            .unwrap()
            .is_err()
    );
    assert!(
        f.service
            .entry_owner_route(
                "GET",
                "/entry-owner/control",
                Some(&f.token),
                &serde_json::json!({}),
                super::super::now_ms()
            )
            .unwrap()
            .is_err()
    );
}
#[test]
fn independent_revocation_during_owner_observation_prevents_mutation_delivery() {
    revoked_owner_observation(None, false, false);
    revoked_owner_observation(
        Some(ParticipantOwnerViewProfileV1::OwnerControlV1),
        false,
        false,
    );
}
#[test]
fn compact_owner_observation_rejects_legacy_signature_domain() {
    revoked_owner_observation(
        Some(ParticipantOwnerViewProfileV1::OwnerControlV1),
        true,
        false,
    );
}
#[test]
fn compact_owner_observation_binds_full_access_wrapper() {
    revoked_owner_observation(
        Some(ParticipantOwnerViewProfileV1::OwnerControlV1),
        false,
        true,
    );
}
fn revoked_owner_observation(
    profile: Option<ParticipantOwnerViewProfileV1>,
    wrong_domain: bool,
    wrong_wrapper: bool,
) {
    use super::super::model::AccountRecord;
    use super::super::participant_entry::{EntryTrust, ParticipantEntry};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    let mut f = IdentityService::model_executor_test_fixture();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server_key = hex::encode([60; 32]);
    let mut entry = ParticipantEntry::new(
        EntryTrust {
            endpoint: format!("http://{}", listener.local_addr().unwrap()),
            service_public_key: sign_read_request("key", (), &server_key)
                .unwrap()
                .subject_public_key,
            world: world(),
        },
        zeroize::Zeroizing::new(hex::encode([61; 32])),
    )
    .unwrap();
    entry.owner_control_enabled = true;
    f.service.participant_entry = Some(entry);
    let private = hex::encode([62; 32]);
    let mut fence = owner();
    fence.public_key = sign_read_request("key", (), &private)
        .unwrap()
        .subject_public_key;
    let original = sign_read_request(
        GAME_PARTICIPANT_REGISTER_DOMAIN,
        GameParticipantRegisterV1 {
            api_rev: 1,
            client_operation_id: "join".into(),
            identity_namespace: GameIdentityNamespaceV1 {
                service_id: "model-contract".into(),
                environment_id: "test".into(),
                instance_epoch: DecimalU64(4),
            },
            account_id: "a".into(),
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            owner_public_key: fence.public_key.clone(),
            owner_key_version: DecimalU64(1),
        },
        &private,
    )
    .unwrap();
    let admission = sign_read_request(
        GAME_PARTICIPANT_ADMISSION_DOMAIN,
        GameParticipantAdmissionV1 {
            api_rev: 1,
            business_digest: "test".into(),
            identity_namespace: original.request.identity_namespace.clone(),
            account_id: "a".into(),
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            owner_public_key: fence.public_key.clone(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(3),
            policy_generation: DecimalU64(1),
            valid_from_tick: DecimalU64(1),
            valid_until_tick: DecimalU64(30),
        },
        &hex::encode([61; 32]),
    )
    .unwrap();
    let read = sign_read_request(
        VIEW_JSON_PATH,
        ReadWorldViewRequest {
            contract_version: 1,
            observation_nonce: Some("c".repeat(64)),
            formal_anchor: None,
            world: world(),
            scope_id: "agent:agent-a".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        },
        &private,
    )
    .unwrap();
    let task = sign_read_request(
        GAME_TASK_CREATE_DOMAIN,
        GameTaskCreateV1 {
            api_rev: 1,
            client_operation_id: "task".into(),
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(3),
            agent_id: "agent-a".into(),
            owner_binding: "binding".into(),
            owner_generation: DecimalU64(2),
        },
        &private,
    )
    .unwrap();
    let payload = WorldServicePayloadV1::GameTaskCreate(task);
    let correlation =
        oasis7_client_api::world_service_correlation::derive_correlation(world(), &payload)
            .unwrap();
    let request = EntryOwnerControlRequestV1 {
        access: ParticipantOwnerViewRequestV1 {
            owner: original,
            admission,
            read,
            response_profile: profile,
        },
        operation: EntryOwnerOperationV1::Submit(SubmitIntentRequest {
            contract_version: 1,
            correlation,
            deadline_unix_ms: None,
            signed_payload: payload,
        }),
    };
    let path = f.service.config.sqlite_path.clone();
    let ns = f.service.config.namespace.clone();
    let rp = f.service.config.rp.clone();
    let (done, finish) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let body = loop {
            let mut chunk = [0; 2048];
            let n = socket.read(&mut chunk).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                let length: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break bytes[end + 4..end + 4 + length].to_vec();
                }
            }
        };
        let access: ParticipantOwnerViewRequestV1 = parse_request(&body).unwrap();
        let commit = CommitRef {
            world: world(),
            binding: ExecutionBinding {
                provider_world_id: "world".into(),
                branch_id: "main".into(),
                finality_ref: "test".into(),
                reorg_generation: 3,
                governing_manifest_ref: "test".into(),
                authority_generation: 1,
                permission_generation: 1,
            },
            position: 1,
            execution_block_hash: "block".into(),
            state_root_ref: "root".into(),
        };
        let view = ReadWorldViewResponse {
            contract_version: 1,
            formal_profile: None,
            formal_proof: None,
            execution_evidence_scope: None,
            version: ProjectionVersion {
                commit: commit.clone(),
                projection_revision: "test".into(),
                visibility_scope: "agent:agent-a".into(),
            },
            logical_tick: 1,
            continuation: EventCursor {
                stream_id: "test".into(),
                scope_id: "agent:agent-a".into(),
                era: 0,
                sequence: 0,
                commit,
            },
            view: serde_json::json!({"canonical_agent_owner":fence.clone()}),
        };
        assert_eq!(access.response_profile, profile);
        let signed = if profile.is_some() {
            let compact = ParticipantOwnerControlViewV1 {
                canonical_agent_owner: fence,
                game_task_control: None,
            };
            let response = ParticipantOwnerControlResponseV1 {
                schema_revision: 1,
                world: world(),
                version: ProjectionVersion {
                    commit: view.version.commit.clone(),
                    projection_revision: request_digest("projection", &compact).unwrap(),
                    visibility_scope: "agent:agent-a".into(),
                },
                logical_tick: 1,
                view: compact,
            };
            response.validate(&access).unwrap();
            let path = if wrong_domain {
                VIEW_JSON_PATH
            } else {
                PARTICIPANT_OWNER_VIEW_PATH
            };
            serde_json::to_value(
                sign_service_response(
                    path,
                    if wrong_wrapper {
                        request_digest(path, &access.read).unwrap()
                    } else {
                        request_digest(path, &access).unwrap()
                    },
                    response,
                    &server_key,
                )
                .unwrap(),
            )
            .unwrap()
        } else {
            serde_json::to_value(
                sign_service_response(
                    VIEW_JSON_PATH,
                    request_digest(VIEW_JSON_PATH, &access.read).unwrap(),
                    view,
                    &server_key,
                )
                .unwrap(),
            )
            .unwrap()
        };
        let mut store = super::super::store::IdentityStore::open(&path, ns, rp).unwrap();
        store
            .transaction(|tx| {
                let mut row = tx.get::<AccountRecord>("account:a")?.unwrap();
                row.value.security_epoch += 1;
                tx.replace(
                    "account:a",
                    row.row_version,
                    "account",
                    Some("a"),
                    None,
                    &row.value,
                )
            })
            .unwrap();
        let reply = serde_json::to_vec(&signed).unwrap();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            reply.len()
        )
        .unwrap();
        socket.write_all(&reply).unwrap();
        drop(socket);
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut sent_mutation = false;
        while Instant::now() < deadline {
            if let Ok((stream, _)) = listener.accept() {
                sent_mutation = true;
                drop(stream);
            }
            if finish.try_recv().is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !sent_mutation,
            "revoked account sent a second upstream mutation"
        );
    });
    let result = f
        .service
        .entry_owner_route(
            "POST",
            "/entry-owner/control",
            Some(&f.token),
            &serde_json::to_value(request).unwrap(),
            super::super::now_ms(),
        )
        .unwrap();
    done.send(()).unwrap();
    server.join().unwrap();
    if wrong_domain || wrong_wrapper {
        assert!(matches!(result, Err(StoreError::OutcomeUnknown)));
    } else {
        assert!(matches!(result,Err(StoreError::Rejected(code)) if code=="session_expired"));
    }
}
