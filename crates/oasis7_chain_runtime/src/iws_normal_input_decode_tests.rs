use super::*;
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadBody, decode_consensus_action_payload_envelope,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use oasis7_proto::memory_budget::MemoryBudget;
fn limits() -> NormalInputDecodeLimitsV1 {
    NormalInputDecodeLimitsV1 {
        max_wire_bytes: 1 << 20,
        max_graph_bytes: 256 << 20,
        max_depth: 128,
    }
}
fn oracle(bytes: &[u8]) -> Result<ConsensusActionPayloadEnvelope, String> {
    let envelope = decode_consensus_action_payload_envelope(bytes)?;
    crate::committed_input_metadata::validate_committed_payload_metadata_v1(&envelope)?;
    Ok(envelope)
}
fn compare(bytes: &[u8]) {
    let memory = MemoryBudget::new(512 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(10_000_000, 1 << 32).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let decoded = decode_normal_payload_data_v1(bytes, &limits(), &mut common).unwrap();
    match oracle(bytes) {
        Ok(value) => assert_eq!(decoded.envelope().unwrap(), &value),
        Err(error) => assert_eq!(decoded.envelope().unwrap_err(), error),
    }
    assert!(memory.used() > 0);
    drop(decoded);
    assert_eq!(memory.used(), 0);
}
#[test]
fn paid_payload_matches_original_runtime_recipe_governance_and_fallback() {
    use oasis7_runtime::Action;
    let runtime = Action::RegisterAgent {
        agent_id: "玩家-α".into(),
        pos: oasis7_agent_api::world_action::GeoPos {
            x_cm: 1,
            y_cm: 2,
            z_cm: 3,
        },
    };
    let raw = serde_cbor::to_vec(&runtime).unwrap();
    compare(&raw);
    let envelope = ConsensusActionPayloadEnvelope::from_runtime_action(runtime);
    let raw = serde_cbor::to_vec(&envelope).unwrap();
    compare(&raw);
    // The original typed decoder also accepts indefinite containers and tags;
    // resource probing must not replace these with a definite-only grammar.
    assert!((0xa0..=0xb7).contains(&raw[0]));
    let mut indefinite = raw.clone();
    indefinite[0] = 0xbf;
    indefinite.push(0xff);
    compare(&indefinite);
    let mut tagged = vec![0xd9, 0xd9, 0xf7];
    tagged.extend_from_slice(&raw);
    compare(&tagged);
    let action = Action::ScheduleRecipe {
        requester_agent_id: "requester".into(),
        factory_id: "factory".into(),
        recipe_id: "recipe".into(),
        plan: oasis7_wasm_abi::RecipeExecutionPlan::accepted(1, vec![], vec![], vec![], 0, 1),
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
    };
    let origin = oasis7_proto::gameplay_submission_origin::GameplaySubmissionOrigin {
        verified_player_id: "player".into(),
        public_key: "ab".repeat(32),
        auth_nonce: 1,
        hosted_registration_nonce: None,
        requester_agent_id: "requester".into(),
        factory_id: "factory".into(),
        recipe_id: "recipe".into(),
    };
    compare(
        &serde_cbor::to_vec(&ConsensusActionPayloadEnvelope::from_recipe_submission(
            action, origin,
        ))
        .unwrap(),
    );
    // Missing governance auth is the original metadata refusal, not decoder support.
    compare(
        &serde_cbor::to_vec(&ConsensusActionPayloadEnvelope::from_runtime_action(
            Action::ApproveGovernanceValidatorAdmission {
                controller_account_id: "controller".into(),
                candidate_id: "candidate".into(),
            },
        ))
        .unwrap(),
    );
    compare(&[0xff]);
    compare(&[0xa1, 0x61, b'x']);
    let null = serde_cbor::Value::Map(
        [(
            serde_cbor::Value::Text("gameplay_submission_origin".into()),
            serde_cbor::Value::Null,
        )]
        .into_iter()
        .collect(),
    );
    compare(&serde_cbor::to_vec(&null).unwrap());
}
#[test]
fn resource_failure_is_fatal_and_never_becomes_legacy_fallback() {
    let memory = MemoryBudget::new(512 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(0, 1 << 20).unwrap();
    let mut calls = 0;
    let mut stop = || {
        calls += 1;
        false
    };
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    assert!(matches!(
        decode_normal_payload_data_v1(&[0], &limits(), &mut common),
        Err(NormalInputDecodeErrorV1::Common(_))
    ));
    assert_eq!(calls, 1);
    assert_eq!(memory.used(), 0);
    let work = FormalBoundedWorkV1::new(1_000_000, 1 << 30).unwrap();
    let mut calls = 0;
    let mut stop = || {
        calls += 1;
        true
    };
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    assert!(matches!(
        decode_normal_payload_data_v1(&[0], &limits(), &mut common),
        Err(NormalInputDecodeErrorV1::Common(_))
    ));
    assert_eq!(calls, 1);
    assert_eq!(memory.used(), 0);
    let work = FormalBoundedWorkV1::new(10_000_000, 1 << 32).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    // Full u64 sequence hint is never passed to serde without its actual layout admission.
    assert!(matches!(
        decode_normal_payload_data_v1(
            &[0x9b, 255, 255, 255, 255, 255, 255, 255, 255],
            &limits(),
            &mut common
        ),
        Err(NormalInputDecodeErrorV1::Limit(_) | NormalInputDecodeErrorV1::Memory(_))
    ));
    assert_eq!(memory.used(), 0);
}
#[test]
fn one_remaining_common_budget_continues_across_payloads() {
    let value = ConsensusActionPayloadEnvelope::from_runtime_action(
        oasis7_runtime::Action::GrantDataAccess {
            owner_agent_id: "owner".into(),
            grantee_agent_id: "grantee".into(),
        },
    );
    assert!(matches!(
        value.body,
        ConsensusActionPayloadBody::RuntimeAction { .. }
    ));
    let raw = serde_cbor::to_vec(&value).unwrap();
    let memory = MemoryBudget::new(512 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(10_000_000, 1 << 32).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let before = work.remaining_steps();
    let first = decode_normal_payload_data_v1(&raw, &limits(), &mut common).unwrap();
    let after = work.remaining_steps();
    assert!(after < before);
    drop(first);
    let second = decode_normal_payload_data_v1(&raw, &limits(), &mut common).unwrap();
    assert!(work.remaining_steps() < after);
    drop(second);
    assert_eq!(memory.used(), 0);
}
thread_local! {static POST_DECODE_CANCEL:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
struct CancelDuringDecode;
impl<'de> serde::Deserialize<'de> for CancelDuringDecode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let _ = <u8 as serde::Deserialize<'de>>::deserialize(d)?;
        POST_DECODE_CANCEL.with(|flag| flag.set(true));
        Ok(Self)
    }
}
#[test]
fn cancellation_after_real_from_slice_is_typed_before_the_value_can_be_observed() {
    POST_DECODE_CANCEL.with(|flag| flag.set(false));
    let memory = MemoryBudget::new(64 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(1_000_000, 1 << 30).unwrap();
    let mut stop = || POST_DECODE_CANCEL.with(|flag| flag.get());
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let limits = limits();
    let mut ops = PaidOps {
        common: &mut common,
        limits: &limits,
        layout: value_layout(),
        raw_len: 1,
        errors: std::array::from_fn(|_| None),
    };
    assert!(matches!(
        ops.decode::<CancelDuringDecode>(&[0], value_layout()),
        Err(NormalInputDecodeErrorV1::Common(_))
    ));
    drop(ops);
    assert_eq!(memory.used(), 0);
    POST_DECODE_CANCEL.with(|flag| flag.set(false));
}
#[test]
fn consuming_data_keeps_fees_through_error_and_panic_and_rejects_foreign_context() {
    let envelope = ConsensusActionPayloadEnvelope::from_runtime_action(
        oasis7_runtime::Action::GrantDataAccess {
            owner_agent_id: "owner".into(),
            grantee_agent_id: "grantee".into(),
        },
    );
    let raw = serde_cbor::to_vec(&envelope).unwrap();
    let memory = MemoryBudget::new(512 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(10_000_000, 1 << 32).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let decoded = decode_normal_payload_data_v1(&raw, &limits(), &mut common).unwrap();
    let foreign = FormalBoundedWorkV1::new(100, 1 << 20).unwrap();
    let mut foreign_stop = || false;
    let mut foreign_ctx = FormalBoundedContextV1::new(&memory, &foreign, &mut foreign_stop);
    assert!(matches!(
        decoded.validate_context(&mut foreign_ctx),
        Err(NormalInputDecodeErrorV1::Invalid(_))
    ));
    let foreign_memory = MemoryBudget::new(1 << 20, 0).unwrap();
    let mut pool_stop = || false;
    let mut pool_ctx = FormalBoundedContextV1::new(&foreign_memory, &work, &mut pool_stop);
    assert!(matches!(
        decoded.validate_context(&mut pool_ctx),
        Err(NormalInputDecodeErrorV1::Invalid(_))
    ));
    let result = decoded
        .with_envelope_data(&mut common, |value, _| {
            assert!(memory.used() > 0);
            assert_eq!(value, envelope);
            Err::<(), _>("consumer data error")
        })
        .unwrap();
    assert_eq!(result, Err("consumer data error"));
    assert_eq!(memory.used(), 0);
    let decoded = decode_normal_payload_data_v1(&raw, &limits(), &mut common).unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = decoded.with_envelope_data(&mut common, |_, _| -> () {
            assert!(memory.used() > 0);
            panic!("consumer unwind")
        });
    }));
    assert!(panic.is_err());
    assert_eq!(memory.used(), 0);
    let decoded = decode_normal_payload_data_v1(&[0xff], &limits(), &mut common).unwrap();
    let refused: Result<(), _> = decoded.with_envelope_data(&mut common, |_, _| {
        panic!("codec refusal must not call consumer")
    });
    assert!(
        matches!(&refused,Err(NormalPayloadConsumeErrorV1::Codec(source)) if source.envelope().is_err())
    );
    assert!(memory.used() > 0);
    drop(refused);
    assert_eq!(memory.used(), 0);
}
