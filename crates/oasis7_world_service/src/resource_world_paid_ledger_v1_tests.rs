//! Storage/configuration component tests only. No fixture issuer is a real
//! Account prepayment, and these cases do not qualify a World HTTP receiver.
use super::*;
use ed25519_dalek::SigningKey;
use oasis7_agent_api::game::DecimalU64;
use oasis7_custody_core::{ApprovedWorldV1, provisioning::PaidResourceReadStageV1};
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        use std::os::unix::fs::DirBuilderExt;
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("oasis7-world-paid-{}", hex::encode(random)));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        for name in ["primary", "guard", "backup"] {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(path.join(name))
                .unwrap();
        }
        Self(path)
    }
    fn path(&self, name: &str) -> std::path::PathBuf {
        self.0.join(name)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn pin() -> PaidResourceReadIssuerPinV1 {
    PaidResourceReadIssuerPinV1 {
        namespace: "account".into(),
        service_id: "account-service".into(),
        environment: "test".into(),
        key_id: "issuer-one".into(),
        public_key: hex::encode(SigningKey::from_bytes(&[81; 32]).verifying_key().to_bytes()),
        issuer_epoch: DecimalU64(1),
        issuer_config_generation: DecimalU64(1),
        not_before: DecimalU64(1),
        not_after: DecimalU64(u64::MAX),
        native_audience_origin: "https://native.example".into(),
        world_audience: ApprovedWorldV1 {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            service_public_key: hex::encode(
                SigningKey::from_bytes(&[82; 32]).verifying_key().to_bytes(),
            ),
        },
        allowed_stage: PaidResourceReadStageV1::OwnerSetupFinal,
    }
}
fn copy(source: &Path, target: &Path) {
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        std::fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
    }
}
fn clear(path: &Path) {
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        std::fs::remove_file(entry.path()).unwrap();
    }
}
#[test]
fn resource_world_paid_floor_open_never_bootstraps_and_backup_rollback_is_blocked() {
    let d = Directory::new();
    let a = d.path("primary");
    let g = d.path("guard");
    let key = [101; 32];
    assert!(WorldResourcePaidLedgerV1::open_existing(&a, &g, key, pin(), 16).is_err());
    assert_eq!(std::fs::read_dir(&a).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&g).unwrap().count(), 0);
    let initial = WorldResourcePaidLedgerV1::initialize(&a, &g, key, pin(), 16).unwrap();
    copy(&a, &d.path("backup"));
    drop(initial);
    let mut newer = pin();
    newer.issuer_config_generation = DecimalU64(2);
    let rotated = WorldResourcePaidLedgerV1::open_existing(&a, &g, key, newer.clone(), 16).unwrap();
    assert_eq!(rotated.floor.config_generation.0, 2);
    assert_eq!(rotated.head.sequence, 1);
    drop(rotated);
    // Restore exactly the ordinary primary backup; the independent guard is
    // deliberately not in that restore unit. Whole-disk/VM rollback is not tested.
    clear(&a);
    copy(&d.path("backup"), &a);
    assert!(WorldResourcePaidLedgerV1::open_existing(&a, &g, key, newer, 16).is_err());
    assert!(WorldResourcePaidLedgerV1::initialize(&a, &g, key, pin(), 16).is_err());
}
#[test]
fn resource_world_paid_floor_rejects_unversioned_pin_change_without_mutating_records() {
    let d = Directory::new();
    let a = d.path("primary");
    let g = d.path("guard");
    let key = [102; 32];
    let initial = WorldResourcePaidLedgerV1::initialize(&a, &g, key, pin(), 16).unwrap();
    drop(initial);
    let head = std::fs::read(a.join(HEAD)).unwrap();
    let guard = std::fs::read(g.join(GUARD)).unwrap();
    let mut wrong = pin();
    wrong.key_id = "different-key".into();
    assert!(WorldResourcePaidLedgerV1::open_existing(&a, &g, key, wrong, 16).is_err());
    assert_eq!(std::fs::read(a.join(HEAD)).unwrap(), head);
    assert_eq!(std::fs::read(g.join(GUARD)).unwrap(), guard);
    let mut epoch = pin();
    epoch.issuer_epoch = DecimalU64(2);
    let rotated = WorldResourcePaidLedgerV1::open_existing(&a, &g, key, epoch, 16).unwrap();
    assert_eq!(rotated.floor.issuer_epoch.0, 2);
    assert_eq!(rotated.floor.config_generation.0, 1);
    drop(rotated);
    assert!(WorldResourcePaidLedgerV1::open_existing(&a, &g, key, pin(), 16).is_err());
}

// This is a real closed-codec/strong-signature transport original. The local
// issuer is a fixture, not a claim of real Account prepayment or World Current.
fn preflight_original() -> (PaidResourceReadProofV1, ParticipantOwnerViewRequestV1) {
    use ed25519_dalek::Signer;
    use oasis7_agent_api::game::*;
    use oasis7_client_api::world_service::{self as ws, authority::sign_read_request};
    use oasis7_custody_core::provisioning::PaidResourceReadPayloadV1;
    let p = pin();
    let owner_seed = hex::encode([83; 32]);
    let owner_public = hex::encode(SigningKey::from_bytes(&[83; 32]).verifying_key().to_bytes());
    let ns = GameIdentityNamespaceV1 {
        service_id: "identity".into(),
        environment_id: "test".into(),
        instance_epoch: DecimalU64(1),
    };
    let register = GameParticipantRegisterV1 {
        api_rev: 1,
        client_operation_id: "register".into(),
        identity_namespace: ns.clone(),
        account_id: "participant".into(),
        world_id: p.world_audience.world_id.clone(),
        genesis_digest: p.world_audience.genesis_digest.clone(),
        owner_public_key: owner_public.clone(),
        owner_key_version: DecimalU64(1),
    };
    let admission = GameParticipantAdmissionV1 {
        api_rev: 1,
        business_digest: "11".repeat(32),
        identity_namespace: ns,
        account_id: "participant".into(),
        world_id: p.world_audience.world_id.clone(),
        genesis_digest: p.world_audience.genesis_digest.clone(),
        owner_public_key: owner_public.clone(),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        policy_generation: DecimalU64(1),
        valid_from_tick: DecimalU64(0),
        valid_until_tick: DecimalU64(100),
    };
    let at = now().unwrap();
    let read = ws::ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: 1,
        world: ws::WorldIdentity {
            world_id: p.world_audience.world_id.clone(),
            genesis_digest: p.world_audience.genesis_digest.clone(),
        },
        scope_id: "agent:agent".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: Some(at + 5000),
    };
    let request = ParticipantOwnerViewRequestV1 {
        owner: sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN, register, &owner_seed).unwrap(),
        admission: sign_read_request(
            GAME_PARTICIPANT_ADMISSION_DOMAIN,
            admission,
            &hex::encode([84; 32]),
        )
        .unwrap(),
        read: sign_read_request(ws::VIEW_JSON_PATH, read, &owner_seed).unwrap(),
        response_profile: Some(
            ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1,
        ),
    };
    let digest = ws::request_digest(
        ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
        &request,
    )
    .unwrap();
    let payload = PaidResourceReadPayloadV1 {
        api_version: 1,
        issuer_namespace: p.namespace,
        issuer_service_id: p.service_id,
        issuer_environment: p.environment,
        issuer_key_id: p.key_id,
        issuer_epoch: p.issuer_epoch,
        issuer_config_generation: p.issuer_config_generation,
        native_audience_origin: p.native_audience_origin,
        world_audience: p.world_audience,
        native_path: "/entry-owner/control".into(),
        world_path: RESOURCE_GOAL_PATH_V2.into(),
        stage: PaidResourceReadStageV1::OwnerSetupFinal,
        custody_account_id: "custody".into(),
        manager_native_account_id: "manager".into(),
        participant_native_account_id: "participant".into(),
        provision_id: "provision".into(),
        setup_id: "setup".into(),
        test_session_id: None,
        approval_operation_id: "approve".into(),
        approval_input_digest: "12".repeat(32),
        plan_digest: "13".repeat(32),
        prepared_resource_basis_digest: None,
        selection_digest: "14".repeat(32),
        owner_public_key: owner_public,
        owner_binding: "binding".into(),
        task_fence_digest: "15".repeat(32),
        signer_record_digest: "16".repeat(32),
        native_request_digest: "17".repeat(32),
        world_request_digest: "18".repeat(32),
        preflight_world_request_digest: Some(digest.strip_prefix("blake3:").unwrap().to_owned()),
        source_operation_id: "source".into(),
        source_record_digest: "19".repeat(32),
        source_security_epoch: DecimalU64(1),
        service_epoch: DecimalU64(1),
        directory_generation: DecimalU64(1),
        resource_flag_generation: DecimalU64(1),
        paid_attempt_id: "attempt".into(),
        paid_record_digest: "20".repeat(32),
        native_http_units: DecimalU64(1),
        world_http_units: DecimalU64(2),
        issued_at: DecimalU64(at),
        expires_at: DecimalU64(at + 5000),
    };
    let signature_hex = hex::encode(
        SigningKey::from_bytes(&[81; 32])
            .sign(&payload.signing_bytes().unwrap())
            .to_bytes(),
    );
    let proof = PaidResourceReadProofV1 {
        payload,
        signature_hex,
    };
    proof
        .verify_issuer(
            &pin(),
            &PaidResourceReadIssuerFloorV1 {
                issuer_epoch: DecimalU64(1),
                config_generation: DecimalU64(1),
            },
            at,
        )
        .unwrap();
    OwnerSetupResourceWorldBodyV2::validate_preflight_bindings(&request, &proof.payload).unwrap();
    (proof, request)
}
#[test]
fn resource_world_paid_floor_strong_original_once_survives_unknown_and_cold_restore() {
    let d = Directory::new();
    let a = d.path("primary");
    let g = d.path("guard");
    let key = [103; 32];
    let mut ledger = WorldResourcePaidLedgerV1::initialize(&a, &g, key, pin(), 16).unwrap();
    // A second process/open must fail promptly, rather than waiting on the
    // service-local process lock while another thread owns the ledger.
    assert!(WorldResourcePaidLedgerV1::open_existing(&a, &g, key, pin(), 16).is_err());
    let (proof, request) = preflight_original();
    let mut wrong = proof.clone();
    wrong.signature_hex = "00".repeat(64);
    let unchanged = std::fs::read(a.join(HEAD)).unwrap();
    assert!(
        ledger
            .claim_owner_preflight(wrong, request.clone())
            .is_err()
    );
    assert_eq!(std::fs::read(a.join(HEAD)).unwrap(), unchanged);
    let consumed = ledger
        .claim_owner_preflight(proof.clone(), request.clone())
        .unwrap();
    assert!(consumed.owner_preflight().is_some());
    ledger.validate_delivery(&consumed).unwrap();
    // Simulate a downstream Current/archive rejection or lost delivery: no
    // observer callback can refund this already committed physical claim.
    drop(consumed);
    let spent = std::fs::read(a.join(HEAD)).unwrap();
    assert!(
        ledger
            .claim_owner_preflight(proof.clone(), request.clone())
            .is_err()
    );
    assert_eq!(std::fs::read(a.join(HEAD)).unwrap(), spent);
    drop(ledger);
    let mut restored = WorldResourcePaidLedgerV1::open_existing(&a, &g, key, pin(), 16).unwrap();
    assert!(restored.claim_owner_preflight(proof, request).is_err());
    assert_eq!(std::fs::read(a.join(HEAD)).unwrap(), spent);
}

#[test]
fn resource_world_paid_floor_durable_fault_never_returns_a_free_retry_token() {
    for fault in [
        TestFault::AfterPrepared,
        TestFault::AfterEvent,
        TestFault::AfterHead,
    ] {
        let d = Directory::new();
        let a = d.path("primary");
        let g = d.path("guard");
        let key = [104; 32];
        let mut ledger = WorldResourcePaidLedgerV1::initialize(&a, &g, key, pin(), 16).unwrap();
        let (proof, request) = preflight_original();
        ledger.test_fault = Some(fault);
        assert!(
            ledger
                .claim_owner_preflight(proof.clone(), request.clone())
                .is_err()
        );
        assert!(!ledger.healthy, "I/O ambiguity poisons this instance");
        assert!(
            ledger
                .claim_owner_preflight(proof.clone(), request.clone())
                .is_err()
        );
        let protected: Guard = read(&ledger.guard, &ledger.key, GUARD, HEAD_BYTES).unwrap();
        assert!(
            protected.prepared.is_some(),
            "actual independent prepared write survived failure"
        );
        drop(ledger);
        let reopened = WorldResourcePaidLedgerV1::open_existing(&a, &g, key, pin(), 16);
        if fault == TestFault::AfterHead {
            let mut reopened = reopened.unwrap();
            assert_eq!(reopened.head.sequence, 1);
            assert_eq!(reopened.attempts.len(), 1);
            let protected: Guard = read(&reopened.guard, &reopened.key, GUARD, HEAD_BYTES).unwrap();
            assert!(
                protected.prepared.is_none(),
                "only full actual event replay finalizes protected intent"
            );
            assert!(reopened.claim_owner_preflight(proof, request).is_err());
        } else {
            assert!(
                reopened.is_err(),
                "old primary plus prepared guard is ambiguous, never reset/refund"
            );
        }
    }
}
