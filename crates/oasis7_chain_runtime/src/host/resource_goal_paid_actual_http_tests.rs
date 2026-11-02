//! Actual registered participant -> HTTP Create/Goal/Bind -> protected archive
//! and actual private Goal -> paid ResourceFinal TCP. Account signer is a
//! component fixture, not CF funding; a blocked quote is never a Prepared basis.
use super::resource_preflight_http::{Participant, events, issue, private_directory};
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7::world_service::resource_world_paid_ledger_v1::{
    OwnerSetupResourceWorldBodyV2, PaidResourceReadProofV1, PaidResourceReadStageV1,
};
use oasis7::world_service::{
    formal_read_service::ResourceGoalArchiveReaderV2,
    private_goal_store::PrivateGoalStore,
    resource_world_paid_ledger_v1::{PaidResourceReadIssuerPinV1, WorldResourcePaidLedgerV1},
    resource_world_read_service_v1::ResourceWorldReadServiceV1,
};
use oasis7_client_api::world_service::{
    self as ws, participant_discovery as participant, resource_basis as resource,
};
use sha2::{Digest, Sha256};
pub(super) fn delegate(world: &mut World, p: &Participant, key: &str) {
    let owner = oasis7_runtime::service_admission::agent_chat::owner_view(world, &p.agent)
        .unwrap()
        .unwrap();
    assert_eq!(owner.public_key, p.owner.subject_public_key);
    assert_eq!(owner.world_id, world.chain_resource_manifest().world_id);
    let q = oasis7_runtime::service_admission::AgentSignerDelegationChangeV1 {
        world_id: owner.world_id.clone(),
        branch_id: owner.canonical_authority.branch_id.clone(),
        agent_id: p.agent.clone(),
        owner_binding: owner.player_id.clone(),
        agent_identity_generation: owner.canonical_authority.agent_identity_generation,
        generation: 1,
        delegate_public_key: key.into(),
        revoked: false,
        nonce: 1,
    };
    world
        .apply_agent_signer_delegation(
            &sign_read_request("delegation", q, &"44".repeat(32)).unwrap(),
        )
        .unwrap();
}
fn digest(value: &impl Serialize) -> String {
    Sha256::digest(oasis7_wasm_abi::encode_canonical_cbor(value).unwrap())
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
fn paid_exchange(
    service: &FormalLiveService,
    body: &OwnerSetupResourceWorldBodyV2,
    proof: &PaidResourceReadProofV1,
) -> (u16, Vec<u8>) {
    use base64::Engine;
    let body = serde_json::to_vec(body).unwrap();
    let header =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(proof).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    accepted.set_nonblocking(false).unwrap();
    write!(client,"POST {} HTTP/1.1\r\nHost: localhost\r\nOasis7-Custody-Resource-Read: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",resource::RESOURCE_GOAL_PATH_V2,header,body.len()).unwrap();
    client.write_all(&body).unwrap();
    http::serve_connection(service, &mut accepted).unwrap();
    drop(accepted);
    let mut raw = Vec::new();
    client.read_to_end(&mut raw).unwrap();
    let end = raw.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
    let status = std::str::from_utf8(&raw[..end])
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, raw[end..].to_vec())
}
pub(super) fn run_case(
    service: Arc<FormalLiveService>,
    node: Arc<Mutex<oasis7_node::NodeRuntime>>,
    publication: Arc<PublishedReadState>,
    root: std::path::PathBuf,
    p: Participant,
    delegate: &str,
    case: CloseReplyCase,
) {
    let goals_path = root.join("actual-private-goals");
    private_directory(&root, &goals_path);
    let goals = PrivateGoalStore::open(&goals_path, [55; 32]).unwrap();
    let control = install_task_for_agent_with_private_goal(
        &service,
        &publication,
        &"44".repeat(32),
        delegate,
        &service.world,
        &p.agent,
        Some(&goals),
    );
    assert_eq!(control.agent_id, p.agent);
    publication.request_resource_archive_stop().unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while publication.finish_resource_archive_stop().is_err() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    let service = Arc::try_unwrap(service)
        .ok()
        .expect("single actual service before receiver installation");
    let now = now_unix_ms();
    let pin=PaidResourceReadIssuerPinV1 {
        namespace:"resource-component-account".into(),service_id:"account-issuer".into(),environment:"private".into(),key_id:"component-paid-key".into(),
        public_key:hex::encode(SigningKey::from_bytes(&[81;32]).verifying_key().to_bytes()),issuer_epoch:DecimalU64(1),issuer_config_generation:DecimalU64(1),
        not_before:DecimalU64(now.saturating_sub(1000)),not_after:DecimalU64(now+30000),native_audience_origin:"https://localhost".into(),
        world_audience:serde_json::from_value(serde_json::json!({"world_id":service.world.world_id,"genesis_digest":service.world.genesis_digest,"branch_id":"main","reorg_epoch":"0","service_public_key":hex::encode(service.signer.verifying_key().to_bytes())})).unwrap(),
        allowed_stage:PaidResourceReadStageV1::OwnerSetupFinal,
    };
    let primary = root.join("actual-goal-paid-primary");
    let guard = root.join("actual-goal-paid-independent-guard");
    for path in [&primary, &guard] {
        private_directory(&root, path);
    }
    let ledger =
        WorldResourcePaidLedgerV1::initialize(&primary, &guard, [88; 32], pin.clone(), 64).unwrap();
    let archive = ResourceGoalArchiveReaderV2::open_existing_path(
        &root.join("resource-activation-archive"),
        service.trust.initial_trust.clone(),
        64,
        128 * 1024 * 1024,
        64 * 1024 * 1024,
    )
    .unwrap();
    let receiver = Arc::new(
        ResourceWorldReadServiceV1::from_configured_stores(
            service.world.clone(),
            hex::encode(service.signer.verifying_key().to_bytes()),
            ledger,
            archive,
            goals,
        )
        .unwrap(),
    );
    let service = Arc::new(service.with_resource_world_reads(receiver.clone()));
    let access = participant::ParticipantOwnerViewRequestV1 {
        owner: p.owner,
        admission: p.admission,
        read: sign_read_request(
            ws::VIEW_JSON_PATH,
            ReadWorldViewRequest {
                observation_nonce: Some("cd".repeat(32)),
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: service.world.clone(),
                scope_id: format!("agent:{}", p.agent),
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            },
            &"44".repeat(32),
        )
        .unwrap(),
        response_profile: Some(participant::ParticipantOwnerViewProfileV1::OwnerSetupV1),
    };
    let cut = publication.current().unwrap();
    let signer = cut
        .world
        .capability_revocation_state()
        .agent_signer_delegations
        .get(&p.agent)
        .unwrap();
    let request = resource::OwnerSetupFinalResourceReadV2 {
        api_version: 2,
        setup_id: "setup".into(),
        provision_id: "provision".into(),
        plan_digest: "ac".repeat(32),
        world: (&service.world).into(),
        expected_fence: GameTaskFenceV1::from_control(&control),
        expected_signer_delegation: resource::ResourceSignerRecordV1 {
            world_id: signer.world_id.clone(),
            branch_id: signer.branch_id.clone(),
            agent_id: signer.agent_id.clone(),
            owner_binding: signer.owner_binding.clone(),
            agent_identity_generation: DecimalU64(signer.agent_identity_generation),
            generation: DecimalU64(signer.generation),
            delegate_public_key: signer.delegate_public_key.clone(),
            revoked: signer.revoked,
            nonce: DecimalU64(signer.nonce),
        },
        goal_reference: control
            .active_goal_reference
            .as_ref()
            .unwrap()
            .goal_ref
            .clone(),
        selection: resource_transfer_ready::selection(case, &p.agent),
        observation_nonce: "73".repeat(32),
    };
    request.selection.validate().unwrap();
    let signed: resource::SignedResourceReadV1<_> =
        sign_read_request(resource::RESOURCE_GOAL_PATH_V2, request, &"44".repeat(32))
            .unwrap()
            .into();
    resource::verify_resource_goal_request(&signed).unwrap();
    let body = OwnerSetupResourceWorldBodyV2::from_originals(access.clone(), signed.clone());
    let attempt = match case {
        CloseReplyCase::ResourceGoalTransfer(false) => "actual-transfer-ready-physical-attempt",
        CloseReplyCase::ResourceGoalTransfer(true) => "actual-transfer-revoked-physical-attempt",
        _ => "actual-goal-physical-attempt",
    };
    let mut proof = issue(&pin, &access, attempt);
    proof.payload.world_request_digest =
        ws::request_digest(resource::RESOURCE_GOAL_PATH_V2, &signed)
            .unwrap()
            .strip_prefix("blake3:")
            .unwrap()
            .into();
    proof.payload.owner_binding = signed.request.expected_fence.owner_binding.clone();
    proof.payload.selection_digest = digest(&(
        "oasis7.custody.resource-selection.v1",
        &signed.request.selection,
    ));
    proof.payload.task_fence_digest = digest(&(
        "oasis7.custody.resource-task-fence.v1",
        &signed.request.expected_fence,
    ));
    proof.payload.signer_record_digest = digest(&(
        "oasis7.custody.resource-signer-record.v1",
        &signed.request.expected_signer_delegation,
    ));
    proof.signature_hex = hex::encode(
        SigningKey::from_bytes(&[81; 32])
            .sign(&proof.payload.signing_bytes().unwrap())
            .to_bytes(),
    );
    body.validate_bindings(&proof.payload).unwrap();
    let (status, preflight) = resource_preflight_http::exchange_paid(&service, &access, &proof);
    assert_eq!(
        status, 200,
        "same paid original must first reach actual OwnerPreflight"
    );
    let preflight = resource::decode_paid_resource_world_reply(&preflight).unwrap();
    let preflight_binding = resource::ResourcePaidWorldBindingV1 {
        api_version: 1,
        proof_digest: ws::request_digest(resource::RESOURCE_PAID_PROOF_DIGEST_DOMAIN, &proof)
            .unwrap(),
        stage: resource::ResourcePaidWorldStageV1::OwnerSetupFinal,
        substep: resource::ResourcePaidWorldSubstepV1::OwnerPreflight,
        world_path: participant::PARTICIPANT_OWNER_VIEW_PATH.into(),
        body_digest: ws::request_digest(participant::PARTICIPANT_OWNER_VIEW_PATH, &access).unwrap(),
    };
    resource::verify_paid_resource_world_reply(
        &preflight,
        &preflight_binding,
        &hex::encode(service.signer.verifying_key().to_bytes()),
    )
    .unwrap();
    assert_eq!(events(&primary), 1);
    let (status, raw) = paid_exchange(&service, &body, &proof);
    assert_eq!(
        status, 200,
        "actual paid Goal must load original archive and private Goal"
    );
    assert_eq!(events(&primary), 2);
    let outer = resource::decode_paid_resource_world_reply(&raw).unwrap();
    let binding = resource::ResourcePaidWorldBindingV1 {
        api_version: 1,
        proof_digest: ws::request_digest(resource::RESOURCE_PAID_PROOF_DIGEST_DOMAIN, &proof)
            .unwrap(),
        stage: resource::ResourcePaidWorldStageV1::OwnerSetupFinal,
        substep: resource::ResourcePaidWorldSubstepV1::ResourceFinal,
        world_path: resource::RESOURCE_GOAL_PATH_V2.into(),
        body_digest: ws::request_digest(resource::RESOURCE_GOAL_PATH_V2, &body).unwrap(),
    };
    let key = hex::encode(service.signer.verifying_key().to_bytes());
    let resource::ResourcePaidWorldReplyV1::ResourceFinal { observation, .. } =
        resource::verify_paid_resource_world_reply(&outer, &binding, &key).unwrap()
    else {
        panic!("fixed Goal variant")
    };
    resource::verify_resource_goal_response(&signed, observation, &key).unwrap();
    assert!(matches!(
        observation.payload.current_goal.activation_profile,
        Some(resource::ResourceGoalActivationProfileV2::FormalQualifiedV2 { .. })
    ));
    assert_eq!(
        observation
            .payload
            .current_goal
            .active_goal
            .as_ref()
            .unwrap()
            .goal_ref,
        signed.request.goal_reference
    );
    resource_transfer_ready::assert_resolution(
        case,
        &observation.payload.resource,
        &signed.request.selection,
        &p.agent,
    );
    let (status, _) = paid_exchange(&service, &body, &proof);
    assert_ne!(status, 200);
    assert_eq!(events(&primary), 2);
    let (_, ordinary) = exchange(
        &service,
        resource::RESOURCE_GOAL_PATH_V2,
        &serde_json::to_vec(&body).unwrap(),
    );
    assert!(resource::decode_paid_resource_world_reply(&ordinary).is_err());
    assert_eq!(events(&primary), 2);
    node.lock().unwrap().stop().unwrap();
    drop(cut);
    drop(receiver);
    drop(service);
    drop(node);
    drop(publication);
    std::fs::remove_dir_all(root).unwrap();
}
