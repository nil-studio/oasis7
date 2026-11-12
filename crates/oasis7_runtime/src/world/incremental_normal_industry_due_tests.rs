//! Component codec/diagnostic admission tests. These DTO inputs are not VM,
//! Current, source, GEN acceptance or whole-due execution evidence.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, FormalBoundedWorkV1,
};
use oasis7_wasm_abi::{ModuleEmit, ModuleOutput, ProductValidationDecision};
fn output(payload: serde_json::Value) -> ModuleOutput {
    ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: crate::world::economy::PRODUCT_VALIDATION_EMIT_KIND.into(),
            payload,
        }],
        tick_lifecycle: None,
        output_bytes: 0,
    }
}
#[test]
fn paid_product_decision_uses_original_borrowed_acceptance_and_keeps_fees() {
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(1_000_000, 16 << 20).unwrap();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let notes = vec!["α\"\\\n"; 16];
    let payloads = [
        serde_json::json!({"product_id":"iron_ingot","accepted":true,"stack_limit":100,"tradable":true,"notes":notes,"quality_levels":["base","fine"],"unknown":{"retained":"ignored"}}),
        serde_json::json!({"product_id":"iron_ingot","accepted":true,"stack_limit":100,"tradable":true}),
        serde_json::json!({"product_id":"iron_ingot","accepted":true,"stack_limit":100,"tradable":true,"notes":[123]}),
    ];
    for payload in payloads {
        let input = output(payload.clone());
        let bounded = decision::decode_actual_product_decision_v1(&input, &mut ctx).unwrap();
        let expected = serde_json::from_value::<ProductValidationDecision>(payload);
        match bounded {
            decision::ProductDecisionPayloadV1::Decoded(actual) => {
                assert!(actual.uses_budget(&memory));
                assert!(memory.used() > 0);
                match (&*actual, expected) {
                    (Ok(actual), Ok(expected)) => assert_eq!(actual, &expected),
                    (Err(actual), Err(expected)) => {
                        assert_eq!(actual.to_string(), expected.to_string())
                    }
                    _ => panic!("original decision acceptance changed"),
                }
            }
            _ => panic!("single selected emit must reach original codec"),
        }
        assert_eq!(memory.used(), 0);
    }
    assert!(work.remaining_steps() < 1_000_000);
    assert!(work.remaining_read_bytes() < 16 << 20);
}
#[test]
fn product_decoder_keeps_typed_budget_cancel_and_memory_first_causes() {
    let input = output(
        serde_json::json!({"product_id":"x","accepted":true,"stack_limit":1,"tradable":true}),
    );
    for mode in 0..3 {
        let memory = MemoryBudget::new(if mode == 2 { 0 } else { 1 << 20 }, 0).unwrap();
        let work = FormalBoundedWorkV1::new(if mode == 0 { 1 } else { 10000 }, 1 << 20).unwrap();
        let mut stop = || mode == 1;
        let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let error = match decision::decode_actual_product_decision_v1(&input, &mut ctx) {
            Err(e) => e,
            Ok(_) => panic!("resource condition must fail"),
        };
        match mode {
            0 => assert!(error.to_string().contains("bounded archive read steps")),
            1 => assert!(error.to_string().contains("bounded archive read cancelled")),
            _ => assert!(matches!(
                error,
                BodyMutationError::Value(
                    oasis7_formal_protocol::incremental_value::ValueTreeError::Memory(_)
                )
            )),
        }
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn paid_failclosed_diagnostic_matches_original_exact_text_and_payload() {
    let memory = MemoryBudget::new(1 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(10000, 1 << 20).unwrap();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let stack = crate::MaterialStack::new("sensor_pack", 7);
    let reason = "module refused \"payload\" / α";
    let paid =
        product_values::failed_decision_v1("module.product.sensor", &stack, reason, &mut ctx)
            .unwrap();
    let (decision, detail) = crate::world::World::fail_closed_product_validation(
        "module.product.sensor",
        &stack,
        reason,
    );
    assert_eq!(paid.decision, decision);
    assert_eq!(paid.detail.as_deref(), Some(detail.as_str()));
    assert!(memory.used() > 0);
    drop(paid);
    assert_eq!(memory.used(), 0);
}
