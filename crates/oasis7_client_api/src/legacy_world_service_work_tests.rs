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
        crate::world_service::ServiceAuthorityWorkErrorV1::Protocol(error) => assert_eq!(
            error.message(),
            "agent_chat intent_seq must be greater than zero"
        ),
        _ => panic!("unexpected work failure"),
    };
    drop(error);
    source.steps.set(0);
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
