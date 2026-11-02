//! Actual Node dispatcher + World durable ledger + qualified current + strong outer.
//! Initial/governance/Account issuer are explicit component authorities, not CF payment.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7::world_service::{
    formal_read_service::ResourceGoalArchiveReaderV2,
    private_goal_store::PrivateGoalStore,
    resource_world_paid_ledger_v1::{
        PaidResourceReadIssuerFloorV1, PaidResourceReadIssuerPinV1, PaidResourceReadPayloadV1,
        PaidResourceReadProofV1,
        PaidResourceReadStageV1, WorldResourcePaidLedgerV1,
    },
    resource_world_read_service_v1::ResourceWorldReadServiceV1,
};
use oasis7_client_api::world_service::{
    self as ws, participant_discovery as participant, resource_basis as resource,
};
use oasis7_runtime::World as RuntimeWorld;
use oasis7_runtime::service_admission::participant::ParticipantEntryPolicyV1;
fn namespace() -> GameIdentityNamespaceV1 {
    GameIdentityNamespaceV1 {
        service_id: "resource-component-identity".into(),
        environment_id: "private".into(),
        instance_epoch: DecimalU64(1),
    }
}
// Reuse the real governed policy producer, including its proposal/finality signatures.
#[path = "execution_bridge/tests/participant_governance.rs"]
mod governed_policy;
pub(super) struct Participant {
    owner: SignedReadRequest<GameParticipantRegisterV1>,
    admission: SignedReadRequest<GameParticipantAdmissionV1>,
    agent: String,
}
pub(super) fn participant(world: &mut World) -> Participant {
    let identity = WorldIdentity {
        world_id: world.chain_resource_manifest().world_id.clone(),
        genesis_digest: world.chain_resource_manifest().genesis_ref.clone().unwrap(),
    };
    governed_policy::install(world, &identity, 1, true);
    let owner = sign_read_request(
        GAME_PARTICIPANT_REGISTER_DOMAIN,
        GameParticipantRegisterV1 {
            api_rev: 1,
            client_operation_id: "resource-component-registration".into(),
            identity_namespace: namespace(),
            account_id: "resource-component-account".into(),
            world_id: identity.world_id.clone(),
            genesis_digest: identity.genesis_digest.clone(),
            owner_public_key: sign_read_request("key", (), &"44".repeat(32))
                .unwrap()
                .subject_public_key,
            owner_key_version: DecimalU64(1),
        },
        &"44".repeat(32),
    )
    .unwrap();
    let admission = sign_read_request(
        GAME_PARTICIPANT_ADMISSION_DOMAIN,
        GameParticipantAdmissionV1 {
            api_rev: 1,
            policy_generation: DecimalU64(1),
            identity_namespace: namespace(),
            account_id: owner.request.account_id.clone(),
            world_id: identity.world_id.clone(),
            genesis_digest: identity.genesis_digest.clone(),
            owner_public_key: owner.subject_public_key.clone(),
            business_digest: oasis7_runtime::service_admission::participant::business_digest(
                &owner.request,
            )
            .unwrap(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            valid_from_tick: DecimalU64(world.state().time),
            valid_until_tick: DecimalU64(world.state().time + 16),
        },
        &"47".repeat(32),
    )
    .unwrap();
    // 0x47 is the actual issuer seed 71 used by the reused governed producer.
    let receipt = world
        .apply_participant_registration(&owner, &admission, "main", 0)
        .unwrap();
    World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    Participant {
        owner,
        admission,
        agent: receipt.agent_id,
    }
}
fn exchange_paid(
    service: &FormalLiveService,
    request: &participant::ParticipantOwnerViewRequestV1,
    proof: &PaidResourceReadProofV1,
) -> (u16, Vec<u8>) {
    use base64::Engine;
    let body = serde_json::to_vec(request).unwrap();
    let header =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(proof).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    accepted.set_nonblocking(false).unwrap();
    write!(client,"POST {} HTTP/1.1\r\nHost: localhost\r\nOasis7-Custody-Resource-Read: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",participant::PARTICIPANT_OWNER_VIEW_PATH,header,body.len()).unwrap();
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
fn private_directory(root: &std::path::Path, path: &std::path::Path) {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::create_dir_all(root).unwrap();
    std::fs::DirBuilder::new().mode(0o700).create(path).unwrap();
}
fn events(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .unwrap()
        .map(|v| v.unwrap().file_name().into_string().unwrap())
        .filter(|name| {
            name.strip_prefix("resource-paid-")
                .and_then(|v| v.strip_suffix(".v1"))
                .is_some_and(|v| v.len() == 16 && v.bytes().all(|c| c.is_ascii_hexdigit()))
        })
        .count()
}
fn issue(
    pin: &PaidResourceReadIssuerPinV1,
    q: &participant::ParticipantOwnerViewRequestV1,
    attempt: &str,
) -> PaidResourceReadProofV1 {
    let now = now_unix_ms();
    let digest = ws::request_digest(participant::PARTICIPANT_OWNER_VIEW_PATH, q).unwrap();
    let payload = PaidResourceReadPayloadV1 {
        api_version: 1,
        issuer_namespace: pin.namespace.clone(),
        issuer_service_id: pin.service_id.clone(),
        issuer_environment: pin.environment.clone(),
        issuer_key_id: pin.key_id.clone(),
        issuer_epoch: pin.issuer_epoch,
        issuer_config_generation: pin.issuer_config_generation,
        native_audience_origin: pin.native_audience_origin.clone(),
        world_audience: pin.world_audience.clone(),
        native_path: "/entry-owner/control".into(),
        world_path: resource::RESOURCE_GOAL_PATH_V2.into(),
        stage: PaidResourceReadStageV1::OwnerSetupFinal,
        custody_account_id: "account".into(),
        manager_native_account_id: "manager".into(),
        participant_native_account_id: q.owner.request.account_id.clone(),
        provision_id: "provision".into(),
        setup_id: "setup".into(),
        test_session_id: None,
        approval_operation_id: "setup".into(),
        approval_input_digest: "ab".repeat(32),
        plan_digest: "ac".repeat(32),
        prepared_resource_basis_digest: None,
        selection_digest: "ad".repeat(32),
        owner_public_key: q.owner.subject_public_key.clone(),
        owner_binding: oasis7_runtime::service_admission::participant::account_key(&q.owner.request).unwrap(),
        task_fence_digest: "ae".repeat(32),
        signer_record_digest: "af".repeat(32),
        native_request_digest: "ba".repeat(32),
        world_request_digest: "bb".repeat(32),
        preflight_world_request_digest: Some(digest.strip_prefix("blake3:").unwrap().into()),
        source_operation_id: "source".into(),
        source_record_digest: "bc".repeat(32),
        source_security_epoch: DecimalU64(1),
        service_epoch: DecimalU64(1),
        directory_generation: DecimalU64(1),
        resource_flag_generation: DecimalU64(1),
        paid_attempt_id: attempt.into(),
        paid_record_digest: "bd".repeat(32),
        native_http_units: DecimalU64(1),
        world_http_units: DecimalU64(2),
        issued_at: DecimalU64(now),
        expires_at: DecimalU64(now + 4000),
    };
    let mut proof = PaidResourceReadProofV1 {
        payload,
        signature_hex: String::new(),
    };
    proof.signature_hex = hex::encode(
        SigningKey::from_bytes(&[81; 32])
            .sign(&proof.payload.signing_bytes().unwrap())
            .to_bytes(),
    );
    proof
        .verify_issuer(
            pin,
            &PaidResourceReadIssuerFloorV1 {
                issuer_epoch: pin.issuer_epoch,
                config_generation: pin.issuer_config_generation,
            },
            now,
        )
        .unwrap();
    proof
}
pub(super) fn run(
    service: Arc<FormalLiveService>,
    node: Arc<Mutex<oasis7_node::NodeRuntime>>,
    publication: Arc<PublishedReadState>,
    root: std::path::PathBuf,
    participant: Participant,
) {
    let service = Arc::try_unwrap(service)
        .ok()
        .expect("single actual service owner before install");
    let now = now_unix_ms();
    let audience=serde_json::from_value(serde_json::json!({"world_id":service.world.world_id,"genesis_digest":service.world.genesis_digest,
        "branch_id":"main","reorg_epoch":"0","service_public_key":hex::encode(service.signer.verifying_key().to_bytes())})).unwrap();
    let pin = PaidResourceReadIssuerPinV1 {
        namespace: "resource-component-account".into(),
        service_id: "account-issuer".into(),
        environment: "private".into(),
        key_id: "component-paid-key".into(),
        public_key: hex::encode(SigningKey::from_bytes(&[81; 32]).verifying_key().to_bytes()),
        issuer_epoch: DecimalU64(1),
        issuer_config_generation: DecimalU64(1),
        not_before: DecimalU64(now.saturating_sub(1000)),
        not_after: DecimalU64(now + 30000),
        native_audience_origin: "https://localhost".into(),
        world_audience: audience,
        allowed_stage: PaidResourceReadStageV1::OwnerSetupFinal,
    };
    let primary = root.join("resource-paid-primary");
    let guard = root.join("resource-paid-independent-guard");
    let archive = root.join("resource-paid-archive");
    let goals = root.join("resource-paid-goals");
    for path in [&primary, &guard, &archive, &goals] {
        private_directory(&root, path);
    }
    let ledger =
        WorldResourcePaidLedgerV1::initialize(&primary, &guard, [88; 32], pin.clone(), 64).unwrap();
    let archive = ResourceGoalArchiveReaderV2::open_existing_path(
        &archive,
        service.trust.initial_trust.clone(),
        64,
        128 * 1024 * 1024,
        64 * 1024 * 1024,
    )
    .unwrap();
    let goals = PrivateGoalStore::open(&goals, [55; 32]).unwrap();
    let resource = Arc::new(
        ResourceWorldReadServiceV1::from_configured_stores(
            service.world.clone(),
            hex::encode(service.signer.verifying_key().to_bytes()),
            ledger,
            archive,
            goals,
        )
        .unwrap(),
    );
    let service = Arc::new(service.with_resource_world_reads(resource.clone()));
    let q = participant::ParticipantOwnerViewRequestV1 {
        owner: participant.owner,
        admission: participant.admission,
        read: sign_read_request(
            ws::VIEW_JSON_PATH,
            ReadWorldViewRequest {
                observation_nonce: Some("cd".repeat(32)),
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: service.world.clone(),
                scope_id: format!("agent:{}", participant.agent),
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            },
            &"44".repeat(32),
        )
        .unwrap(),
        response_profile: Some(participant::ParticipantOwnerViewProfileV1::OwnerSetupV1),
    };
    let proof = issue(&pin, &q, "first-physical-world");
    assert_eq!(events(&primary), 0);
    let mut bad = proof.clone();
    bad.signature_hex = "00".repeat(64);
    assert!(
        bad.verify_issuer(
            &pin,
            &PaidResourceReadIssuerFloorV1 {
                issuer_epoch: pin.issuer_epoch,
                config_generation: pin.issuer_config_generation
            },
            now_unix_ms()
        )
        .is_err()
    );
    let (status, _) = exchange_paid(&service, &q, &bad);
    assert_ne!(status, 200);
    assert_eq!(
        events(&primary),
        0,
        "invalid issuer cannot consume a World substep or produce a paid reply"
    );
    let (status, raw) = exchange_paid(&service, &q, &proof);
    assert_eq!(
        status, 200,
        "actual paid preflight must reach the real strong-outer producer"
    );
    let outer = resource::decode_paid_resource_world_reply(&raw).unwrap();
    let binding = resource::ResourcePaidWorldBindingV1 {
        api_version: 1,
        proof_digest: ws::request_digest(resource::RESOURCE_PAID_PROOF_DIGEST_DOMAIN, &proof)
            .unwrap(),
        stage: resource::ResourcePaidWorldStageV1::OwnerSetupFinal,
        substep: resource::ResourcePaidWorldSubstepV1::OwnerPreflight,
        world_path: participant::PARTICIPANT_OWNER_VIEW_PATH.into(),
        body_digest: ws::request_digest(participant::PARTICIPANT_OWNER_VIEW_PATH, &q).unwrap(),
    };
    let node_key = hex::encode(service.signer.verifying_key().to_bytes());
    let resource::ResourcePaidWorldReplyV1::OwnerPreflight { observation, .. } =
        resource::verify_paid_resource_world_reply(&outer, &binding, &node_key).unwrap()
    else {
        panic!("fixed preflight variant")
    };
    ws::verify_service_response(
        participant::PARTICIPANT_OWNER_VIEW_PATH,
        &ws::request_digest(participant::PARTICIPANT_OWNER_VIEW_PATH, &q).unwrap(),
        observation,
        &node_key,
    )
    .unwrap();
    observation.payload.validate(&q).unwrap();
    assert_eq!(
        events(&primary),
        1,
        "Node success must follow a real durable physical event"
    );
    let (status, _) = exchange_paid(&service, &q, &proof);
    assert_ne!(status, 200);
    assert_eq!(events(&primary), 1);
    let (_, ordinary) = exchange(
        &service,
        participant::PARTICIPANT_OWNER_VIEW_PATH,
        &serde_json::to_vec(&q).unwrap(),
    );
    assert!(
        resource::decode_paid_resource_world_reply(&ordinary).is_err(),
        "header removal never yields a paid strong outer"
    );
    assert_eq!(events(&primary), 1);
    node.lock().unwrap().stop().unwrap();
    let failed = issue(&pin, &q, "not-ready-physical-world");
    let (status, _) = exchange_paid(&service, &q, &failed);
    assert_ne!(status, 200);
    assert_eq!(
        events(&primary),
        2,
        "actual not-ready Current cannot refund the prior durable claim"
    );
    drop(service);
    drop(resource);
    drop(node);
    drop(publication);
    let mut ledger =
        WorldResourcePaidLedgerV1::open_existing(&primary, &guard, [88; 32], pin.clone(), 64)
            .unwrap();
    failed
        .verify_issuer(
            &pin,
            &PaidResourceReadIssuerFloorV1 {
                issuer_epoch: pin.issuer_epoch,
                config_generation: pin.issuer_config_generation,
            },
            now_unix_ms(),
        )
        .unwrap();
    assert!(
        ledger.claim_owner_preflight(failed, q).is_err(),
        "cold restart cannot reissue the Current-failed physical original"
    );
    drop(ledger);
    std::fs::remove_dir_all(root).unwrap();
}
