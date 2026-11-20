use super::*;
use crate::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use crate::viewer::{CollectDataCommand, CollectDataRequest};
use oasis7_wasm_abi::{CanonicalCborAllocator, CanonicalCborWorkV1};
use std::cell::{Cell, RefCell, RefMut};

struct Meter {
    memory: MemoryBudget,
    calls: Cell<usize>,
    fail_at: usize,
    cancelled: bool,
    work: RefCell<CanonicalCborWorkV1>,
}
impl Meter {
    fn new(fail_at: usize, cancelled: bool) -> Self {
        Self {
            memory: MemoryBudget::new(1 << 20, 0).unwrap(),
            calls: Cell::new(0),
            fail_at,
            cancelled,
            work: RefCell::new(CanonicalCborWorkV1 {
                visits_remaining: 100_000,
                bytes_remaining: 1 << 20,
                max_depth: 64,
                max_map_keys: 1024,
                max_key_bytes: 1 << 16,
            }),
        }
    }
}
impl CanonicalCborAllocator for Meter {
    type Charge = MemoryReservation;
    fn reserve(&self, n: usize) -> Result<Self::Charge, CanonicalCborStreamError> {
        self.memory
            .reserve(n, MemoryClass::Ordinary)
            .map_err(|_| CanonicalCborStreamError::ResourceLimited("player test memory"))
    }
    fn charge_work(&self, _: u64, _: u64) -> Result<(), CanonicalCborStreamError> {
        let n = self.calls.get() + 1;
        self.calls.set(n);
        if n >= self.fail_at {
            return Err(if self.cancelled {
                CanonicalCborStreamError::Cancelled
            } else {
                CanonicalCborStreamError::ResourceLimited("player test debit")
            });
        }
        Ok(())
    }
}
impl CapabilityCanonicalWorkV1 for Meter {
    type Loan<'a> = RefMut<'a, CanonicalCborWorkV1>;
    fn is_metered(&self) -> bool {
        true
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        self.work.borrow_mut()
    }
}
fn credentials() -> (String, String) {
    let key = SigningKey::from_bytes(&[31; 32]);
    (
        hex::encode(key.verifying_key().to_bytes()),
        hex::encode(key.to_bytes()),
    )
}
fn gameplay(public: &str) -> GameplayActionRequest {
    GameplayActionRequest {
        action_id: " collect_data ".into(),
        target_agent_id: " agent-中文 ".into(),
        actor_agent_id: Some(" actor ".into()),
        player_id: " player ".into(),
        public_key: Some(public.into()),
        auth: None,
    }
}

#[test]
fn paid_auth_verifies_original_signed_gameplay_chat_and_collect_contracts() {
    let (public, private) = credentials();
    let request = gameplay(&public);
    let proof = sign_gameplay_action_auth_proof(&request, 9, &public, &private).unwrap();
    // An independent original serde-CBOR encoder authenticates the same signed preimage.
    let original = serde_cbor::to_vec(&ViewerPlayerAuthSigningEnvelope {
        version: 1,
        payload: GameplayActionSigningPayload {
            operation: "gameplay_action",
            action_id: "collect_data",
            target_agent_id: "agent-中文",
            actor_agent_id: Some("actor"),
            player_id: "player",
            public_key: &public,
            nonce: 9,
        },
        actor: None,
    })
    .unwrap();
    let signing = SigningKey::from_bytes(&[31; 32]);
    assert_eq!(
        proof.signature,
        format!(
            "awviewauth:v1:{}",
            hex::encode(signing.sign(&original).to_bytes())
        )
    );
    let meter = Meter::new(usize::MAX, false);
    let paid = verify_gameplay_action_auth_proof_with_work_v1(&request, &proof, &meter).unwrap();
    assert_eq!(
        paid.verified(),
        &verify_gameplay_action_auth_proof(&request, &proof).unwrap()
    );
    assert!(meter.memory.used() > 0);
    drop(paid);
    assert_eq!(meter.memory.used(), 0);

    let chat: AgentChatRequest = serde_json::from_value(serde_json::json!({
        "agent_id":"agent-中文", "message":"带\u{0}原文", "player_id":"player", "public_key":public,
        "intent_tick":7, "intent_seq":8, "world_id":"world", "reorg_epoch":2,
        "authority_scope":AGENT_CHAT_AUTHORITY_SCOPE, "replaces_intent_id":"old-intent"
    }))
    .unwrap();
    let proof = sign_agent_chat_auth_proof(&chat, 11, &public, &private).unwrap();
    let paid = verify_agent_chat_auth_proof_with_work_v1(&chat, &proof, &meter).unwrap();
    assert_eq!(
        paid.verified(),
        &verify_agent_chat_auth_proof(&chat, &proof).unwrap()
    );
    drop(paid);
    assert_eq!(meter.memory.used(), 0);
    let mut changed = chat.clone();
    changed.reorg_epoch = Some(3);
    let refusal = verify_agent_chat_auth_proof_with_work_v1(&changed, &proof, &meter)
        .err()
        .unwrap();
    assert!(
        matches!(&refusal, PlayerAuthWorkErrorV1::Protocol(e) if e.message.starts_with("verify auth signature failed:"))
    );
    drop(refusal);
    assert_eq!(meter.memory.used(), 0);

    let collect = CollectDataRequest {
        electricity_cost: 10,
        data_amount: 2,
        player_id: "player".into(),
        public_key: Some(public.clone()),
        auth: None,
    };
    for command in [
        CollectDataCommand::Preflight {
            request: collect.clone(),
        },
        CollectDataCommand::Submit { request: collect },
    ] {
        let proof = sign_collect_data_auth_proof(&command, 13, &public, &private).unwrap();
        let paid = verify_collect_data_auth_proof_with_work_v1(&command, &proof, &meter).unwrap();
        assert_eq!(
            paid.verified(),
            &verify_collect_data_auth_proof(&command, &proof).unwrap()
        );
        drop(paid);
        assert_eq!(meter.memory.used(), 0);
    }
}

#[test]
fn every_player_auth_debit_stops_without_refund_or_live_reservations() {
    let (public, private) = credentials();
    let request = gameplay(&public);
    let proof = sign_gameplay_action_auth_proof(&request, 9, &public, &private).unwrap();
    let complete = Meter::new(usize::MAX, false);
    drop(verify_gameplay_action_auth_proof_with_work_v1(&request, &proof, &complete).unwrap());
    for cancelled in [false, true] {
        for fail_at in 1..=complete.calls.get() {
            let meter = Meter::new(fail_at, cancelled);
            let error = verify_gameplay_action_auth_proof_with_work_v1(&request, &proof, &meter)
                .err()
                .unwrap();
            if cancelled {
                assert!(matches!(
                    error,
                    PlayerAuthWorkErrorV1::Work(CanonicalCborStreamError::Cancelled)
                ));
            } else {
                assert!(matches!(
                    error,
                    PlayerAuthWorkErrorV1::Work(CanonicalCborStreamError::ResourceLimited(
                        "player test debit"
                    ))
                ));
            }
            assert_eq!(meter.calls.get(), fail_at);
            assert_eq!(meter.memory.used(), 0);
        }
    }
}

#[test]
fn paid_protocol_refusal_retains_its_original_pool_until_drop() {
    let (public, private) = credentials();
    let request = gameplay(&public);
    let mut proof = sign_gameplay_action_auth_proof(&request, 9, &public, &private).unwrap();
    proof.player_id = "someone-else".into();
    proof.signature = "also-invalid".into();
    let meter = Meter::new(usize::MAX, false);
    let error = verify_gameplay_action_auth_proof_with_work_v1(&request, &proof, &meter)
        .err()
        .unwrap();
    match &error {
        PlayerAuthWorkErrorV1::Protocol(refusal) => {
            assert_eq!(
                refusal.message,
                "auth proof player_id does not match request player_id"
            );
            assert!(refusal._fee.as_ref().unwrap().uses_budget(&meter.memory));
            assert!(
                !refusal
                    ._fee
                    .as_ref()
                    .unwrap()
                    .uses_budget(&MemoryBudget::new(1 << 20, 0).unwrap())
            );
        }
        error => panic!("unexpected error {error:?}"),
    }
    assert!(meter.memory.used() > 0);
    drop(error);
    assert_eq!(meter.memory.used(), 0);
}
