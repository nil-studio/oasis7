use super::*;
use oasis7::{
    runtime::{Action, SchedulerPolicyV1, WorldState},
    world_service::{
        authority::{self, sign_read_request, verify_service_response},
        formal_read::FormalReadHistoryV1,
        formal_read_service::{FormalCommittedReadService, FormalReadPublication},
        wire::{
            AuthenticatedLookup, SignedReadRequest, SignedServiceResponse, VIEW_PATH,
            WorldServicePayloadV1,
        },
    },
};
use oasis7_agent_api::game::{DecimalU64, GAME_TASK_CREATE_DOMAIN, GameTaskCreateV1};
use oasis7_client_api::world_service::{LookupIntentRequest, ReadWorldChangesRequest};
use oasis7_node::{NodeConfig, NodeRole};
use std::{
    net::{TcpListener, TcpStream},
    sync::atomic::Ordering,
    time::Duration,
};

use crate::formal_read_fixtures as fixtures;

struct CurrentOnlyResolver {
    world: Mutex<World>,
    anchor: VerifiedFormalReadAnchorV1,
    fixed: Mutex<Option<CommitRef>>,
    business_reads: std::sync::atomic::AtomicUsize,
}
impl CurrentOnlyResolver {
    fn new(world: World, anchor: VerifiedFormalReadAnchorV1) -> Self {
        Self {
            world: Mutex::new(world),
            anchor,
            fixed: Mutex::new(None),
            business_reads: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn pin_fixed(&self, commit: CommitRef) {
        *self.fixed.lock().unwrap() = Some(commit);
    }
}
impl FormalLiveHistoryResolver for CurrentOnlyResolver {
    fn business_result_for_publication(
        &self,
        cut: FormalReadPublication,
        _correlation: &oasis7_client_api::world_service::RequestCorrelation,
        _scope: &str,
        _reader: &str,
    ) -> Result<
        Option<(
            oasis7_client_api::world_service::FormalBusinessResultProofV1,
            Option<String>,
        )>,
        String,
    > {
        assert!(same_anchor(cut.anchor(), &self.anchor)?);
        assert_eq!(
            cut.world().snapshot(),
            self.world.lock().unwrap().snapshot()
        );
        self.business_reads.fetch_add(1, Ordering::SeqCst);
        // This fixture has no business result. It proves exact-cut selection
        // without fabricating a qualified outcome or an absence proof.
        Err("test exact fixed publication selected; business proof unavailable".into())
    }
    fn origin_publication(
        &self,
        request: &FormalReadAnchorRefV1,
    ) -> Result<FormalReadPublication, String> {
        validate_anchor_ref(request, &self.anchor)?;
        FormalReadPublication::new(self.anchor.clone(), self.world.lock().unwrap().clone())
    }

    fn fixed_publication(&self, commit: &CommitRef) -> Result<FormalReadPublication, String> {
        if self.fixed.lock().unwrap().as_ref() != Some(commit) {
            return Err("HistoryUnavailable: fixed commit is not in the exact test history".into());
        }
        FormalReadPublication::new(self.anchor.clone(), self.world.lock().unwrap().clone())
    }

    fn forward_publication(
        &self,
        origin: &VerifiedFormalReadAnchorV1,
        live: &VerifiedFormalReadAnchorV1,
        max_steps: usize,
    ) -> Result<FormalReadPublication, String> {
        if max_steps > MAX_FORMAL_HISTORY_STEPS {
            return Err("HistoryUnavailable: test resolver page exceeds the bound".into());
        }
        if !same_anchor(origin, &self.anchor)? || !same_anchor(live, &self.anchor)? {
            return Err("HistoryUnavailable: current-only test has no forward history".into());
        }
        FormalReadPublication::new(self.anchor.clone(), self.world.lock().unwrap().clone())
    }

    fn history_page(
        &self,
        origin: &VerifiedFormalReadAnchorV1,
        target: &VerifiedFormalReadAnchorV1,
        max_steps: usize,
    ) -> Result<FormalReadHistoryV1, String> {
        assert!(max_steps <= MAX_FORMAL_HISTORY_STEPS);
        if !same_anchor(origin, target)? {
            return Err("HistoryUnavailable: current-only test has no intervening proof".into());
        }
        Ok(FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        })
    }
}

fn fixture_trust() -> (
    World,
    oasis7::world_service::client::FormalReadTrustConfigV1,
    VerifiedFormalReadAnchorV1,
) {
    let (world, initial_trust, anchor, _) = fixtures::actual_world_initial_fixture();
    let checkpoint = anchor.checkpoint();
    let trust = oasis7::world_service::client::FormalReadTrustConfigV1 {
        initial_trust,
        checkpoint_digest: checkpoint.digest().unwrap(),
        checkpoint,
    };
    (world, trust, anchor)
}

fn identity(anchor: &VerifiedFormalReadAnchorV1) -> WorldIdentity {
    WorldIdentity {
        world_id: anchor.head().record().world.world_id.clone(),
        genesis_digest: anchor.head().record().world.genesis_digest.clone(),
    }
}

fn live_node(world_id: &str) -> Arc<Mutex<NodeRuntime>> {
    let mut config = NodeConfig::new("formal-live-test-node", world_id, NodeRole::Sequencer)
        .unwrap()
        .with_tick_interval(Duration::from_secs(60))
        .unwrap()
        .with_auto_attest_all_validators(false)
        .with_require_execution_on_commit(true)
        .with_require_peer_execution_hashes(false);
    config.gossip = None;
    config.replication = None;
    config.feedback_p2p = None;
    let mut runtime = NodeRuntime::new(config);
    runtime.start().unwrap();
    Arc::new(Mutex::new(runtime))
}

fn live_service(
    world: World,
    anchor: VerifiedFormalReadAnchorV1,
    trust: oasis7::world_service::client::FormalReadTrustConfigV1,
    history: Arc<dyn FormalLiveHistoryResolver>,
    node: Arc<Mutex<NodeRuntime>>,
    published: Arc<PublishedReadState>,
) -> (FormalLiveService, FormalLiveState, SigningKey) {
    let node_signer = fixtures::key(91);
    let state = FormalLiveState::default();
    let admission_gate = Arc::new(Mutex::new(()));
    state.node_running.store(true, Ordering::Release);
    state.ready.store(true, Ordering::Release);
    let world_id = identity(&anchor);
    let service = FormalLiveService::new(
        "formal-live-test-node".into(),
        world_id,
        node_signer.clone(),
        node,
        published,
        trust,
        history,
        state.clone(),
        admission_gate,
    );
    let _ = world;
    (service, state, node_signer)
}

fn published(world: World, anchor: VerifiedFormalReadAnchorV1) -> Arc<PublishedReadState> {
    let state = Arc::new(PublishedReadState::default());
    state.publish(world, anchor).unwrap();
    state
}

fn read_request(
    anchor: &VerifiedFormalReadAnchorV1,
    reader_private: &str,
    fixed: Option<CommitRef>,
) -> SignedReadRequest<ReadWorldViewRequest> {
    sign_read_request(
        VIEW_PATH,
        ReadWorldViewRequest {
            observation_nonce: Some("07".repeat(32)),
            formal_anchor: Some(anchor_ref(anchor).unwrap()),
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: identity(anchor),
            scope_id: "public".into(),
            min_commit: None,
            fixed_commit: fixed,
            deadline_unix_ms: None,
        },
        reader_private,
    )
    .unwrap()
}

fn stop_test_node(node: &Arc<Mutex<NodeRuntime>>) {
    let _ = node.lock().unwrap().stop();
}

#[test]
fn current_and_fixed_reads_are_signed_by_existing_node_key_and_formal_proof_path() {
    let (world, trust, anchor) = fixture_trust();
    let resolver = Arc::new(CurrentOnlyResolver::new(world.clone(), anchor.clone()));
    let node = live_node(&anchor.head().record().world.world_id);
    let published = published(world.clone(), anchor.clone());
    let (service, _state, signer) = live_service(
        world,
        anchor.clone(),
        trust,
        resolver.clone(),
        Arc::clone(&node),
        published,
    );
    let reader_private = hex::encode(fixtures::key(92).to_bytes());

    let first = read_request(&anchor, &reader_private, None);
    let first_reply = service.handle("POST", VIEW_PATH, &serde_json::to_vec(&first).unwrap());
    assert_eq!(first_reply.status, 200);
    let first_signed: SignedServiceResponse<
        ReadWorldViewResponse<oasis7::world_service::projection::WorldServiceProjection>,
    > = serde_json::from_slice(&first_reply.body).unwrap();
    let first_digest = authority::request_digest(VIEW_PATH, &first).unwrap();
    verify_service_response(
        VIEW_PATH,
        &first_digest,
        &first_signed,
        &hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    assert!(first_signed.payload.formal_proof.is_some());
    first_signed.payload.validate(&first.request).unwrap();

    let fixed_commit = first_signed.payload.version.commit.clone();
    resolver.pin_fixed(fixed_commit.clone());
    let fixed = read_request(&anchor, &reader_private, Some(fixed_commit));
    let fixed_reply = service.handle("POST", VIEW_PATH, &serde_json::to_vec(&fixed).unwrap());
    assert_eq!(fixed_reply.status, 200);
    let fixed_signed: SignedServiceResponse<
        ReadWorldViewResponse<oasis7::world_service::projection::WorldServiceProjection>,
    > = serde_json::from_slice(&fixed_reply.body).unwrap();
    let digest = authority::request_digest(VIEW_PATH, &fixed).unwrap();
    verify_service_response(
        VIEW_PATH,
        &digest,
        &fixed_signed,
        &hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    fixed_signed.payload.validate(&fixed.request).unwrap();
    assert_eq!(
        fixed_signed.payload.formal_proof,
        first_signed.payload.formal_proof
    );

    stop_test_node(&node);
}

#[test]
fn formal_changes_empty_range_is_qualified_and_substituted_stream_is_rejected() {
    use oasis7::world_service::{
        formal_read::FormalReadVerifier,
        formal_result::{VerifiedFormalChangesCursorV1, verify_formal_changes_page},
        wire::FormalAuthenticatedChangesV1,
    };
    use oasis7_client_api::world_service::FormalBusinessChangesPageV1;
    let (world, trust, anchor) = fixture_trust();
    let resolver = Arc::new(CurrentOnlyResolver::new(world.clone(), anchor.clone()));
    let node = live_node(&anchor.head().record().world.world_id);
    let (service, _, signer) = live_service(
        world.clone(),
        anchor.clone(),
        trust.clone(),
        resolver,
        Arc::clone(&node),
        published(world, anchor.clone()),
    );
    let read = read_request(&anchor, &hex::encode(fixtures::key(92).to_bytes()), None);
    let (view, selected_publication) = service.read_view_with_publication(&read).unwrap();
    assert_eq!(
        selected_publication.anchor().checkpoint(),
        anchor.checkpoint(),
        "internal lookup context must retain the exact verified view cut"
    );
    let mut verifier = FormalReadVerifier::new(&trust.initial_trust, anchor).unwrap();
    let verified = verifier
        .verify_view(&view, &read.request, &read.subject_public_key)
        .unwrap();
    let cursor = VerifiedFormalChangesCursorV1::from_verified_view(
        &verified,
        &view,
        &read.request,
        &read.subject_public_key,
        "test-authority",
    )
    .unwrap();
    let mut request = FormalAuthenticatedChangesV1 {
        schema_version: 1,
        cursor: cursor.cursor().clone(),
        max_items: 8,
        max_bytes: 4 * 1024 * 1024,
        read,
    };
    let reply = service.handle("POST", CHANGES_PATH, &serde_json::to_vec(&request).unwrap());
    assert_eq!(reply.status, 200);
    let signed: SignedServiceResponse<
        FormalBusinessChangesPageV1<oasis7::world_service::projection::WorldServiceProjection>,
    > = serde_json::from_slice(&reply.body).unwrap();
    verify_service_response(
        CHANGES_PATH,
        &authority::request_digest(CHANGES_PATH, &request).unwrap(),
        &signed,
        &hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    let page = verify_formal_changes_page(
        &mut verifier,
        &cursor,
        "test-authority",
        &signed.payload,
        &request.read.subject_public_key,
    )
    .unwrap();
    assert!(page.changes().is_empty());
    assert_eq!(page.next_cursor().cursor(), cursor.cursor());
    request.cursor.stream_id = "substituted-stream".into();
    let reply = service.handle("POST", CHANGES_PATH, &serde_json::to_vec(&request).unwrap());
    assert_eq!(reply.status, 503);
    stop_test_node(&node);
}

fn world_with_owner() -> (World, SigningKey) {
    let owner = fixtures::key(71);
    let owner_public = hex::encode(owner.verifying_key().to_bytes());
    let mut world = oasis7::runtime_host::world::new_with_state(WorldState::default())
        .try_with_cognition_scheduler(
            SchedulerPolicyV1 {
                schema_version: SchedulerPolicyV1::SCHEMA_VERSION.into(),
                max_total_wakes_per_tick: 1,
                max_wakes_per_agent_per_tick: 1,
                aging_after_ticks: 1,
                max_starvation_ticks: 1,
                initial_priority: 0,
                comparator: SchedulerPolicyV1::COMPARATOR.into(),
                service_order: SchedulerPolicyV1::SERVICE_ORDER.into(),
            },
            1,
        )
        .unwrap();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "agent".into(),
        player_id: "owner".into(),
        public_key: Some(owner_public),
    });
    world.step().unwrap();
    world
        .install_capability_agent_identity("agent", "owner", 1)
        .unwrap();
    world
        .bind_cognition_runtime("formal-fixture-world", "main", 0, None, "pending", 0)
        .unwrap();
    (world, owner)
}

fn valid_submit(owner: &SigningKey) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let request = GameTaskCreateV1 {
        api_rev: 1,
        client_operation_id: "live-submit-1".into(),
        world_id: "formal-fixture-world".into(),
        genesis_digest: fixtures::h(8),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(1),
    };
    let payload = WorldServicePayloadV1::GameTaskCreate(
        sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            request,
            &hex::encode(owner.to_bytes()),
        )
        .unwrap(),
    );
    SubmitIntentRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: oasis7::world_service::correlation::derive_correlation(
            WorldIdentity {
                world_id: "formal-fixture-world".into(),
                genesis_digest: fixtures::h(8),
            },
            &payload,
        )
        .unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    }
}

#[test]
fn signed_submit_queues_real_node_action_without_mutating_world_and_lookup_fails_closed() {
    let (base_world, trust, anchor) = fixture_trust();
    let (world, owner) = world_with_owner();
    let node = live_node(&anchor.head().record().world.world_id);
    // This service test isolates action admission. Production publication uses
    // the hook sink, which rejects this deliberately different World/root pair.
    let published = Arc::new(PublishedReadState {
        current: Mutex::new(Some(PublishedReadCut {
            world: world.clone(),
            anchor: anchor.clone(),
        })),
    });
    let resolver = Arc::new(CurrentOnlyResolver::new(base_world.clone(), anchor.clone()));
    let (service, _, signer) = live_service(
        world.clone(),
        anchor.clone(),
        trust.clone(),
        resolver,
        Arc::clone(&node),
        published.clone(),
    );
    let before = world.snapshot();
    let request = valid_submit(&owner);
    let request_digest = authority::request_digest(SUBMIT_PATH, &request).unwrap();
    let reply = service.handle("POST", SUBMIT_PATH, &serde_json::to_vec(&request).unwrap());
    assert_eq!(reply.status, 200);
    let response: SignedServiceResponse<IntentResponse<serde_json::Value>> =
        serde_json::from_slice(&reply.body).unwrap();
    verify_service_response(
        SUBMIT_PATH,
        &request_digest,
        &response,
        &hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    assert!(matches!(
        response.payload.outcome,
        IntentOutcome::Received {
            durability: AdmissionDurability::Volatile
        }
    ));
    assert_eq!(
        node.lock()
            .unwrap()
            .snapshot()
            .consensus
            .pending_consensus_actions
            .submit_buffer_action_count,
        1,
        "the signed request must enter the real NodeRuntime queue",
    );
    assert_eq!(published.current().unwrap().world.snapshot(), before);

    let lookup = AuthenticatedLookup {
        request: LookupIntentRequest {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            key: request.correlation.key.clone(),
        },
        original: request.signed_payload.clone(),
    };
    let lookup_digest = authority::request_digest(LOOKUP_PATH, &lookup).unwrap();
    let lookup_reply = service.handle("POST", LOOKUP_PATH, &serde_json::to_vec(&lookup).unwrap());
    assert_eq!(lookup_reply.status, 503);
    let lookup_error: SignedServiceResponse<FormalServiceErrorV1> =
        serde_json::from_slice(&lookup_reply.body).unwrap();
    verify_service_response(
        LOOKUP_PATH,
        &lookup_digest,
        &lookup_error,
        &hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    assert_eq!(
        lookup_error.payload.code,
        "formal_business_proof_unavailable"
    );

    let reader_private = hex::encode(fixtures::key(92).to_bytes());
    let publication = FormalReadPublication::new(anchor.clone(), base_world.clone()).unwrap();
    let formal_trust = oasis7::world_service::client::FormalReadTrustConfigV1 {
        initial_trust: trust.initial_trust,
        checkpoint_digest: trust.checkpoint_digest,
        checkpoint: trust.checkpoint,
    };
    let view_req = read_request(&anchor, &reader_private, None);
    let current =
        FormalCommittedReadService::new(formal_trust.initial_trust.clone(), publication).unwrap();
    let view = current
        .read(
            &view_req,
            &FormalReadPublication::new(anchor.clone(), base_world).unwrap(),
            &FormalReadHistoryV1 {
                schema_version: 1,
                steps: vec![],
            },
        )
        .unwrap();
    let changes = ReadWorldChangesRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        cursor: view.continuation,
        max_items: 32,
        max_bytes: 4096,
    };
    let changes = sign_read_request(CHANGES_PATH, changes, &reader_private).unwrap();
    let changes_digest = authority::request_digest(CHANGES_PATH, &changes).unwrap();
    let changes_reply =
        service.handle("POST", CHANGES_PATH, &serde_json::to_vec(&changes).unwrap());
    assert_eq!(changes_reply.status, 503);
    let changes_error: SignedServiceResponse<FormalServiceErrorV1> =
        serde_json::from_slice(&changes_reply.body).unwrap();
    verify_service_response(
        CHANGES_PATH,
        &changes_digest,
        &changes_error,
        &hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    assert_eq!(
        changes_error.payload.code,
        "formal_business_proof_unavailable"
    );

    stop_test_node(&node);
}

#[test]
fn strict_json_rejects_unknown_fields_and_formal_reads_reject_private_scopes() {
    let (world, trust, anchor) = fixture_trust();
    let node = live_node(&anchor.head().record().world.world_id);
    let published = published(world.clone(), anchor.clone());
    let resolver = Arc::new(CurrentOnlyResolver::new(world.clone(), anchor.clone()));
    let (service, _, _) = live_service(
        world,
        anchor.clone(),
        trust,
        resolver,
        node.clone(),
        published,
    );
    let reader_private = hex::encode(fixtures::key(92).to_bytes());
    let mut request = read_request(&anchor, &reader_private, None);
    request.request.scope_id = "agent:private".into();
    request = sign_read_request(VIEW_PATH, request.request, &reader_private).unwrap();
    let reply = service.handle("POST", VIEW_PATH, &serde_json::to_vec(&request).unwrap());
    assert_eq!(reply.status, 400);
    let error: SignedServiceResponse<FormalServiceErrorV1> =
        serde_json::from_slice(&reply.body).unwrap();
    assert_eq!(error.payload.code, "formal_read_rejected");

    let mut unknown = serde_json::to_value(read_request(&anchor, &reader_private, None)).unwrap();
    unknown["untrusted_extension"] = true.into();
    assert!(
        parse_strict::<SignedReadRequest<ReadWorldViewRequest>>(
            &serde_json::to_vec(&unknown).unwrap()
        )
        .unwrap_err()
        .contains("unsupported request field")
    );
    stop_test_node(&node);
}

#[test]
fn loopback_status_uses_the_running_node_and_server_shutdown_is_bounded() {
    let (world, trust, anchor) = fixture_trust();
    let node = live_node(&anchor.head().record().world.world_id);
    let published = published(world.clone(), anchor.clone());
    let resolver = Arc::new(CurrentOnlyResolver::new(world.clone(), anchor.clone()));
    let (service, _, signer) = live_service(
        world,
        anchor.clone(),
        trust,
        resolver,
        node.clone(),
        published,
    );
    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = probe.local_addr().unwrap();
    drop(probe);
    let mut server = Arc::new(service).serve_loopback(address).unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .write_all(b"GET /v1/chain/status HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut response_bytes = Vec::new();
    stream.read_to_end(&mut response_bytes).unwrap();
    let split = response_bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap()
        + 4;
    let response: SignedServiceResponse<FormalLiveStatusV1> =
        serde_json::from_slice(&response_bytes[split..]).unwrap();
    let digest = authority::request_digest(CHAIN_STATUS_PATH, &()).unwrap();
    verify_service_response(
        CHAIN_STATUS_PATH,
        &digest,
        &response,
        &hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    assert!(response.payload.ready);
    assert!(response.payload.node_running);
    server.stop().unwrap();
    stop_test_node(&node);
}

#[test]
fn http_parser_enforces_an_absolute_partial_request_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client = TcpStream::connect(address).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    let started = std::time::Instant::now();
    let result = read_http_request(&mut server, started + Duration::from_millis(50));
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(client);
}

#[test]
fn formal_http_fragmented_body_on_nonblocking_accepted_stream_reaches_grant_rejection() {
    use oasis7_client_api::world_service_full_sync::{
        FORMAL_FULL_SYNC_MANIFEST_PATH, FormalFullSyncCutV1, FormalFullSyncManifestRequestV1,
        FormalNodeReadGrantBodyV1, FormalNodeReadGrantV1, FormalNodeReadRoleV1,
    };
    use std::io::{Read, Write};
    let (world, trust, anchor) = fixture_trust();
    let node = live_node(&anchor.head().record().world.world_id);
    let resolver = Arc::new(CurrentOnlyResolver::new(world.clone(), anchor.clone()));
    let (service, _, _) = live_service(
        world.clone(),
        anchor.clone(),
        trust,
        resolver,
        node.clone(),
        published(world, anchor.clone()),
    );
    let reader_key = fixtures::key(92);
    let reader_public = hex::encode(reader_key.verifying_key().to_bytes());
    let now = now_unix_ms();
    let epoch = anchor.authority().definition().epoch;
    let grant = FormalNodeReadGrantV1::sign(
        FormalNodeReadGrantBodyV1 {
            schema_version: 1,
            world: identity(&anchor),
            chain_id: anchor.head().record().world.chain_id.clone(),
            node_public_key: reader_public,
            role: FormalNodeReadRoleV1::StateSync,
            grant_epoch: epoch,
            authority_epoch: epoch,
            not_before_unix_ms: now.saturating_sub(2_000),
            expires_unix_ms: now.saturating_sub(1_000),
        },
        &hex::encode(fixtures::key(9).to_bytes()),
    )
    .unwrap();
    let request = FormalFullSyncManifestRequestV1 {
        schema_version: 1,
        grant,
        cut: FormalFullSyncCutV1 {
            world: identity(&anchor),
            chain_id: anchor.head().record().world.chain_id.clone(),
            position: anchor.head().record().position,
            record_id: anchor.head().head().record_id,
            after_state_root: anchor.head().record().after_state_root.clone(),
            active_authority_id: anchor.authority().authority_id().unwrap(),
            finalizing_authority_id: anchor.head().finalizing_authority_id().into(),
            authority_epoch: epoch,
        },
    };
    let signed = sign_read_request(
        FORMAL_FULL_SYNC_MANIFEST_PATH,
        request,
        &hex::encode(reader_key.to_bytes()),
    )
    .unwrap();
    let body = serde_json::to_vec(&signed).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    // Make the BSD inheritance condition deterministic on every host.
    accepted.set_nonblocking(true).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(client, "POST {FORMAL_FULL_SYNC_MANIFEST_PATH} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n", body.len()).unwrap();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        client.write_all(&body).unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        response
    });
    serve_connection(&service, &mut accepted).unwrap();
    drop(accepted);
    let response = writer.join().unwrap();
    let split = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap()
        + 4;
    let status = std::str::from_utf8(&response[..split])
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap();
    assert_eq!(status, "403", "{}", String::from_utf8_lossy(&response));
    let error: SignedServiceResponse<FormalServiceErrorV1> =
        serde_json::from_slice(&response[split..]).unwrap();
    assert_eq!(error.payload.code, "read_grant_rejected");
    stop_test_node(&node);
}

#[test]
fn fixed_formal_lookup_selects_exact_publication_and_rejects_identity_or_signature_drift() {
    use oasis7::world_service::wire::FormalAuthenticatedLookupV1;
    let (world, trust, anchor) = fixture_trust();
    let resolver = Arc::new(CurrentOnlyResolver::new(world.clone(), anchor.clone()));
    let node = live_node(&anchor.head().record().world.world_id);
    let (service, _, _) = live_service(
        world.clone(),
        anchor.clone(),
        trust,
        resolver.clone(),
        Arc::clone(&node),
        published(world.clone(), anchor.clone()),
    );
    let fixed = fixtures::commit_ref(&anchor, &fixtures::projection(&world));
    let original = valid_submit(&fixtures::key(7));
    let reader = hex::encode(fixtures::key(92).to_bytes());
    let request = FormalAuthenticatedLookupV1 {
        schema_version: 1,
        lookup: AuthenticatedLookup {
            request: LookupIntentRequest {
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                key: original.correlation.key.clone(),
            },
            original: original.signed_payload,
        },
        read: read_request(&anchor, &reader, Some(fixed.clone())),
    };
    let reply = service.handle("POST", LOOKUP_PATH, &serde_json::to_vec(&request).unwrap());
    assert_eq!(reply.status, 503);
    assert!(
        String::from_utf8_lossy(&reply.body)
            .contains("test exact fixed publication selected; business proof unavailable")
    );
    assert_eq!(resolver.business_reads.load(Ordering::SeqCst), 1);
    for variant in 0..3 {
        let mut invalid = request.clone();
        match variant {
            0 => {
                let mut wrong = fixed.clone();
                wrong.position += 1;
                invalid.read = read_request(&anchor, &reader, Some(wrong));
            }
            1 => invalid.read.signature_hex = "00".repeat(64),
            _ => invalid.lookup.request.key.verified_subject = fixtures::public(8),
        }
        let reply = service.handle("POST", LOOKUP_PATH, &serde_json::to_vec(&invalid).unwrap());
        assert_eq!(reply.status, 503);
        assert!(
            !String::from_utf8_lossy(&reply.body).contains("test exact fixed publication selected")
        );
        assert_eq!(
            resolver.business_reads.load(Ordering::SeqCst),
            1,
            "invalid lookup must not resolve business material"
        );
    }
    stop_test_node(&node);
}
