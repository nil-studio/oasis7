use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamError as WorkError, CanonicalCborWorkV1,
    CapabilityCanonicalWorkV1,
};
use std::cell::{Cell, RefCell, RefMut};
struct Paid {
    memory: MemoryBudget,
    steps: Cell<u64>,
    bytes: Cell<u64>,
    stop: Cell<bool>,
    canonical: RefCell<CanonicalCborWorkV1>,
    calls: Cell<usize>,
    fail_at: Cell<usize>,
    fail_cancel: Cell<bool>,
    recorded: RefCell<Vec<String>>,
}
impl Paid {
    fn new() -> Self {
        Self {
            memory: MemoryBudget::new(64 << 20, 0).unwrap(),
            steps: Cell::new(8_000_000),
            bytes: Cell::new(64 << 20),
            stop: Cell::new(false),
            canonical: RefCell::new(CanonicalCborWorkV1 {
                visits_remaining: 1_000_000,
                bytes_remaining: 32 << 20,
                max_depth: 128,
                max_map_keys: 1024,
                max_key_bytes: 4096,
            }),
            calls: Cell::new(0),
            fail_at: Cell::new(usize::MAX),
            fail_cancel: Cell::new(false),
            recorded: RefCell::new(Vec::new()),
        }
    }
}
impl CanonicalCborAllocator for Paid {
    type Charge = MemoryReservation;
    fn reserve(&self, n: usize) -> Result<Self::Charge, WorkError> {
        self.memory
            .reserve(n, MemoryClass::Ordinary)
            .map_err(|_| WorkError::ResourceLimited("test graph"))
    }
    fn charge_work(&self, s: u64, b: u64) -> Result<(), WorkError> {
        let calls = self.calls.get() + 1;
        self.calls.set(calls);
        if calls >= self.fail_at.get() {
            return Err(if self.fail_cancel.get() {
                WorkError::Cancelled
            } else {
                WorkError::ResourceLimited("original service work refusal")
            });
        }
        if self.stop.get() {
            return Err(WorkError::Cancelled);
        }
        let old = (self.steps.get(), self.bytes.get());
        self.steps.set(old.0.saturating_sub(s));
        self.bytes.set(old.1.saturating_sub(b));
        if s > old.0 || b > old.1 {
            Err(WorkError::ResourceLimited("test continuing work"))
        } else {
            Ok(())
        }
    }
}
impl CapabilityCanonicalWorkV1 for Paid {
    type Loan<'a> = RefMut<'a, CanonicalCborWorkV1>;
    fn is_metered(&self) -> bool {
        true
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        self.canonical.borrow_mut()
    }
    fn record_stream_error(&self, error: WorkError) -> WorkError {
        self.recorded.borrow_mut().push(error.to_string());
        error
    }
}
fn world() -> WorldIdentity {
    WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
    }
}
fn key() -> (String, String) {
    let secret = hex::encode([47u8; 32]);
    let signed = crate::world_service::sign_read_request("key", (), &secret).unwrap();
    (secret, signed.subject_public_key)
}
#[test]
fn paid_collect_fallback_keeps_correlation_heap_and_continuing_codec_work() {
    let (secret, public) = key();
    let mut command = oasis7_proto::viewer::CollectDataCommand::Submit {
        request: oasis7_proto::viewer::CollectDataRequest {
            electricity_cost: 10,
            data_amount: 2,
            player_id: "player".into(),
            public_key: Some(public.clone()),
            auth: None,
        },
    };
    let proof =
        oasis7_proto::player_auth::sign_collect_data_auth_proof(&command, 7, &public, &secret)
            .unwrap();
    let oasis7_proto::viewer::CollectDataCommand::Submit { request } = &mut command else {
        unreachable!()
    };
    request.auth = Some(proof);
    let payload = WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&command).unwrap());
    let source = Paid::new();
    let initial = source.steps.get();
    let value = derive_correlation_with_work_v1(world(), &payload, &source).unwrap();
    assert_eq!(value.correlation().key.operation_domain, "collect_data");
    assert_eq!(value.correlation().key.request_id_or_nonce, "7");
    assert!(source.memory.used() > 0);
    assert!(source.steps.get() < initial);
    let visits = source.canonical.borrow().visits_remaining;
    assert!(visits < 1_000_000);
    drop(value);
    assert_eq!(source.memory.used(), 0);
    let again = derive_correlation_with_work_v1(world(), &payload, &source).unwrap();
    assert!(source.canonical.borrow().visits_remaining < visits);
    drop(again);
    assert_eq!(source.memory.used(), 0);
}
#[test]
fn parsed_gameplay_failure_does_not_fall_back_and_cancellation_stays_typed() {
    let payload=WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&serde_json::json!({"action_id":"act","target_agent_id":"agent","player_id":"player","mode":"submit","request":{"electricity_cost":1,"data_amount":1,"player_id":"other"}})).unwrap());
    let source = Paid::new();
    let error = derive_correlation_with_work_v1(world(), &payload, &source)
        .err()
        .unwrap();
    match &error {
        crate::world_service::ServiceAuthorityWorkErrorV1::Protocol(error) => {
            assert_eq!(error.message(), "missing gameplay proof")
        }
        _ => panic!("unexpected work failure"),
    }
    assert!(source.memory.used() > 0);
    drop(error);
    assert_eq!(source.memory.used(), 0);
    source.stop.set(true);
    let error = derive_correlation_with_work_v1(world(), &payload, &source)
        .err()
        .unwrap();
    assert!(matches!(
        error,
        crate::world_service::ServiceAuthorityWorkErrorV1::Work(WorkError::Cancelled)
    ));
    assert_eq!(source.memory.used(), 0);
}
#[test]
fn chat_intent_gate_precedes_signature_prefix_and_work_is_not_refilled() {
    let (_, public) = key();
    let request = oasis7_proto::viewer::AgentChatRequest {
        agent_id: "agent".into(),
        message: "message".into(),
        player_id: Some("player".into()),
        public_key: Some(public.clone()),
        auth: Some(oasis7_proto::viewer::PlayerAuthProof {
            scheme: oasis7_proto::viewer::PlayerAuthScheme::Ed25519,
            player_id: "player".into(),
            public_key: public,
            nonce: 1,
            signature: "bad-prefix".into(),
        }),
        intent_tick: None,
        intent_seq: Some(0),
        world_id: None,
        reorg_epoch: None,
        authority_scope: None,
        replaces_intent_id: None,
        canonical_authority: None,
    };
    let payload = WorldServicePayloadV1::AgentChat(request);
    let source = Paid::new();
    let error = derive_correlation_with_work_v1(world(), &payload, &source)
        .err()
        .unwrap();
    match &error {
        crate::world_service::ServiceAuthorityWorkErrorV1::Protocol(error) => {
            assert_eq!(
                error.message(),
                "agent_chat intent_seq must be greater than zero"
            );
            let root = std::mem::size_of::<
                crate::world_service::ServiceAuthorityWorkErrorV1<MemoryReservation>,
            >()
            .max(std::mem::size_of::<
                crate::world_service::PaidServiceProtocolRefusalV1<MemoryReservation>,
            >());
            assert!(source.memory.used() >= root + error.message().len());
        }
        _ => panic!("unexpected work failure"),
    };
    drop(error);
    assert_eq!(source.memory.used(), 0);
    let WorldServicePayloadV1::AgentChat(request) = &payload else {
        unreachable!()
    };
    let original = oasis7_proto::player_auth::verify_agent_chat_auth_proof_with_work_v1(
        request,
        request.auth.as_ref().unwrap(),
        &source,
    )
    .err()
    .unwrap();
    let oasis7_proto::player_auth::PlayerAuthWorkErrorV1::Protocol(original) = original else {
        panic!("expected the original player protocol refusal")
    };
    assert!(source.memory.used() > 0);
    source.steps.set(0);
    assert!(matches!(
        crate::world_service::PaidServiceProtocolRefusalV1::from_player_refusal(original, &source),
        Err(crate::world_service::ServiceAuthorityWorkErrorV1::Work(
            WorkError::ResourceLimited(_)
        ))
    ));
    assert_eq!(source.memory.used(), 0);
    let error = derive_correlation_with_work_v1(world(), &payload, &source)
        .err()
        .unwrap();
    assert!(matches!(
        error,
        crate::world_service::ServiceAuthorityWorkErrorV1::Work(WorkError::ResourceLimited(_))
    ));
    assert_eq!(source.steps.get(), 0);
    let error = derive_correlation_with_work_v1(world(), &payload, &source)
        .err()
        .unwrap();
    assert!(matches!(
        error,
        crate::world_service::ServiceAuthorityWorkErrorV1::Work(WorkError::ResourceLimited(_))
    ));
    assert_eq!(source.steps.get(), 0);
    assert_eq!(source.memory.used(), 0);
}

#[test]
fn signing_stream_records_the_typed_first_failure_once_and_releases_both_passes() {
    let payload = serde_json::json!({"text": "中文\0payload", "sequence": [1, 2, 3]});
    let complete = Paid::new();
    drop(crate::world_service::signing_bytes_with_work_v1("domain", &payload, &complete).unwrap());
    let mut saw_hook = false;
    for cancel in [false, true] {
        for fail_at in 1..=complete.calls.get() {
            let source = Paid::new();
            source.fail_at.set(fail_at);
            source.fail_cancel.set(cancel);
            let error =
                crate::world_service::signing_bytes_with_work_v1("domain", &payload, &source)
                    .err()
                    .expect("injected failure");
            let expected = if cancel {
                WorkError::Cancelled
            } else {
                WorkError::ResourceLimited("original service work refusal")
            };
            let crate::world_service::ServiceAuthorityWorkErrorV1::Work(error) = error else {
                panic!("work failure became a protocol rejection")
            };
            assert_eq!(error.to_string(), expected.to_string());
            assert_eq!(source.calls.get(), fail_at, "work continued after failure");
            assert_eq!(source.memory.used(), 0);
            let recorded = source.recorded.borrow();
            assert!(recorded.len() <= 1, "failure recorded twice");
            if let Some(error) = recorded.first() {
                assert_eq!(error, &expected.to_string());
                saw_hook = true;
            }
        }
    }
    assert!(saw_hook, "the real stream error hook must be exercised");
}

#[test]
fn paid_signing_and_strict_verification_keep_original_bytes_and_charge_lifetimes() {
    use crate::world_service::{
        request_digest_with_work_v1, signing_bytes_with_work_v1, verify_read_request_with_work_v1,
    };
    let source = Paid::new();
    let domain = "service-test";
    let request = serde_json::json!({"text": "中文\0原文", "chunk": "x".repeat(8193)});
    let original =
        oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.world-service.v1", domain, &request))
            .unwrap();
    let image = signing_bytes_with_work_v1(domain, &request, &source).unwrap();
    assert_eq!(image.bytes(), original);
    assert!(source.memory.used() > 0);
    drop(image);
    assert_eq!(source.memory.used(), 0);
    let digest = request_digest_with_work_v1(domain, &request, &source).unwrap();
    assert_eq!(
        digest.as_str(),
        format!("blake3:{}", blake3::hash(&original))
    );
    assert!(source.memory.used() > 0);
    drop(digest);
    assert_eq!(source.memory.used(), 0);

    let (secret, _) = key();
    let signed = crate::world_service::sign_read_request(domain, request, &secret).unwrap();
    verify_read_request_with_work_v1(domain, &signed, &source).unwrap();
    let prepared = crate::world_service::private_goal::prepare_owner_signature_strict_with_work_v1(
        domain,
        &signed.request,
        &signed.subject_public_key,
        &signed.signature_hex,
        &source,
    )
    .unwrap();
    assert_eq!(prepared.canonical_bytes(), original);
    assert!(source.memory.used() > 0);
    prepared.verify_strict_with_work_v1(&source).unwrap();
    drop(prepared);
    assert_eq!(source.memory.used(), 0);

    let mut tampered = signed;
    tampered.request["text"] = serde_json::json!("changed after signing");
    let error = verify_read_request_with_work_v1(domain, &tampered, &source).unwrap_err();
    assert!(matches!(
        &error,
        crate::world_service::ServiceAuthorityWorkErrorV1::Protocol(_)
    ));
    assert!(source.memory.used() > 0);
    drop(error);
    assert_eq!(source.memory.used(), 0);
    source.stop.set(true);
    assert!(matches!(
        verify_read_request_with_work_v1(domain, &tampered, &source),
        Err(crate::world_service::ServiceAuthorityWorkErrorV1::Work(
            WorkError::Cancelled
        ))
    ));
    assert_eq!(source.memory.used(), 0);
}

fn native_delegation_payload(
    operation: oasis7_agent_api::game::GameNativeDelegationOperationV1,
) -> WorldServicePayloadV1 {
    use oasis7_agent_api::game::*;
    let (secret, public) = key();
    let request = GameNativeDelegationControlV1 {
        api_rev: 1,
        client_operation_id: "native-operation".into(),
        fence: GameTaskFenceV1 {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            agent_id: "agent".into(),
            owner_binding: "owner".into(),
            owner_generation: DecimalU64(1),
            owner_public_key: public,
            task_id: "task".into(),
            task_revision: DecimalU64(1),
            task_status: GameTaskStatusV1::Suspended,
            latest_goal_revision: DecimalU64(0),
            active_goal_revision: DecimalU64(0),
            active_goal_reference: None,
            executor_epoch: DecimalU64(0),
            executor_binding: None,
        },
        operation,
    };
    WorldServicePayloadV1::GameNativeDelegation(
        crate::world_service::sign_read_request(GAME_NATIVE_DELEGATION_DOMAIN, request, &secret)
            .unwrap(),
    )
}

#[test]
fn paid_correlation_keeps_main_native_delegation_variants_and_wire_identity() {
    use oasis7_agent_api::game::{
        DecimalU64, GAME_NATIVE_DELEGATION_DOMAIN, GameNativeDelegationOperationV1,
    };
    for operation in [
        GameNativeDelegationOperationV1::Install {
            grant_id: "grant".into(),
            object_id: "factory".into(),
            action_kinds: vec!["ScheduleRecipe".into()],
            period_id: "period".into(),
            valid_from_tick: DecimalU64(1),
            valid_until_tick: DecimalU64(10),
            limit_units: DecimalU64(8),
        },
        GameNativeDelegationOperationV1::Revoke {
            grant_id: "grant".into(),
            expected_revision: DecimalU64(1),
        },
    ] {
        let payload = native_delegation_payload(operation);
        let expected_digest = format!(
            "blake3:{}",
            blake3::hash(
                &oasis7_wasm_abi::encode_canonical_cbor(&(
                    "oasis7.world-service.v1",
                    "intent",
                    &payload,
                ))
                .unwrap()
            )
        );
        let original = derive_correlation(world(), &payload).unwrap();
        let source = Paid::new();
        let paid = derive_correlation_with_work_v1(world(), &payload, &source).unwrap();
        assert_eq!(paid.correlation(), &original);
        assert_eq!(original.payload_digest, expected_digest);
        assert_eq!(original.key.operation_domain, GAME_NATIVE_DELEGATION_DOMAIN);
        assert_eq!(original.key.nonce_scope, "task:task");
        assert_eq!(original.key.request_id_or_nonce, "native-operation");
        assert!(source.memory.used() > 0);
        drop(paid);
        assert_eq!(source.memory.used(), 0);
    }
    let layout = crate::consensus_action_wire::legacy_payload_owned_layout_v1(
        crate::consensus_action_wire::ConsensusPayloadOwnedLayoutV1::scalar::<()>(),
        &mut || Ok::<_, ()>(()),
    )
    .unwrap();
    assert!(layout.missing.is_none());
}

#[test]
fn native_delegation_keeps_validation_owner_signature_and_world_error_order() {
    use oasis7_agent_api::game::{
        DecimalU64, GAME_NATIVE_DELEGATION_DOMAIN, GameNativeDelegationOperationV1,
    };
    let payload = native_delegation_payload(GameNativeDelegationOperationV1::Revoke {
        grant_id: "grant".into(),
        expected_revision: DecimalU64(1),
    });
    for (case, expected) in [
        (0, "invalid native delegation scope, budget or revision"),
        (1, "native delegation control requires owner signature"),
        (2, "invalid private goal owner signature"),
        (3, "native delegation world identity mismatch"),
    ] {
        let mut changed = payload.clone();
        let WorldServicePayloadV1::GameNativeDelegation(signed) = &mut changed else {
            unreachable!()
        };
        match case {
            0 => {
                signed.request.operation = GameNativeDelegationOperationV1::Revoke {
                    grant_id: "grant".into(),
                    expected_revision: DecimalU64(0),
                };
                signed.signature_hex = "bad".into();
            }
            1 => {
                signed.subject_public_key = "ab".repeat(32);
                signed.signature_hex = "bad".into();
            }
            2 => signed.request.fence.world_id = "other".into(),
            _ => {
                signed.request.fence.world_id = "other".into();
                *signed = crate::world_service::sign_read_request(
                    GAME_NATIVE_DELEGATION_DOMAIN,
                    signed.request.clone(),
                    &key().0,
                )
                .unwrap();
            }
        }
        assert_eq!(derive_correlation(world(), &changed).unwrap_err(), expected);
        let source = Paid::new();
        let error = derive_correlation_with_work_v1(world(), &changed, &source)
            .err()
            .unwrap();
        match &error {
            crate::world_service::ServiceAuthorityWorkErrorV1::Protocol(error) => {
                assert_eq!(error.message(), expected);
            }
            _ => panic!("unexpected work failure for case {case}"),
        }
        drop(error);
        assert_eq!(source.memory.used(), 0);
    }
}
