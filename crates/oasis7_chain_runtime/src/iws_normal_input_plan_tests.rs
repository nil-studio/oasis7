use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use oasis7_proto::memory_budget::MemoryBudget;
fn limits() -> NormalInputDecodeLimitsV1 {
    NormalInputDecodeLimitsV1 {
        max_wire_bytes: 1 << 20,
        max_graph_bytes: 128 << 20,
        max_depth: 128,
    }
}
#[test]
fn whole_list_reserved_validation_precedes_any_ordinary_decode() {
    let actions = vec![
        NodeConsensusAction {
            action_id: 1,
            submitter_player_id: "player".into(),
            payload_cbor: vec![0xff],
            payload_hash: "opaque".into(),
        },
        NodeConsensusAction {
            action_id: replicated::REPLICATED_EXECUTION_INPUT_ACTION_ID,
            submitter_player_id: "reserved-wrong-submitter".into(),
            payload_cbor: vec![0xff],
            payload_hash: "opaque".into(),
        },
    ];
    let memory = MemoryBudget::new(64 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(1_000_000, 1 << 30).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let mut scope = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
    let paid = replicated::validate_replicated_execution_input_actions_with(
        &actions,
        9,
        &mut ReservedValidation {
            common: &mut common,
            limits: &limits(),
            scope: &mut scope,
        },
    )
    .unwrap();
    let ordinary = replicated::validate_replicated_execution_input_actions(&actions, 9);
    assert_eq!(paid, ordinary);
    assert!(
        paid.unwrap_err()
            .contains("reserved replicated execution input identity is malformed")
    );
    drop(scope);
    assert_eq!(memory.used(), 0);
}
#[test]
fn reserved_predicate_duplicate_and_height_keep_original_order() {
    fn action(height: u64) -> NodeConsensusAction {
        let value = NodeReplicatedExecutionInputV1::new(
            replicated::PROVIDER_BACKED_BOOTSTRAP_EXECUTION_INPUT_KIND,
            height,
            vec![0x80],
        );
        NodeConsensusAction {
            action_id: replicated::REPLICATED_EXECUTION_INPUT_ACTION_ID,
            submitter_player_id: replicated::REPLICATED_EXECUTION_INPUT_SUBMITTER.into(),
            payload_cbor: serde_cbor::to_vec(&value).unwrap(),
            payload_hash: "data".into(),
        }
    }
    for actions in [
        vec![action(0), action(9)],
        vec![action(8), action(9)],
        vec![action(9), action(9)],
    ] {
        let memory = MemoryBudget::new(64 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(1_000_000, 1 << 30).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut scope = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
        let result = replicated::validate_replicated_execution_input_actions_with(
            &actions,
            9,
            &mut ReservedValidation {
                common: &mut common,
                limits: &limits(),
                scope: &mut scope,
            },
        )
        .unwrap();
        assert_eq!(
            result,
            replicated::validate_replicated_execution_input_actions(&actions, 9)
        );
        drop(scope);
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn fatal_reserved_budget_is_not_a_codec_refusal() {
    let action = NodeConsensusAction {
        action_id: replicated::REPLICATED_EXECUTION_INPUT_ACTION_ID,
        submitter_player_id: replicated::REPLICATED_EXECUTION_INPUT_SUBMITTER.into(),
        payload_cbor: vec![0xff],
        payload_hash: "data".into(),
    };
    let memory = MemoryBudget::new(1 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(0, 0).unwrap();
    let mut callbacks = 0;
    let mut stop = || {
        callbacks += 1;
        false
    };
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let mut scope = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
    let result = replicated::validate_replicated_execution_input_actions_with(
        &[action],
        9,
        &mut ReservedValidation {
            common: &mut common,
            limits: &limits(),
            scope: &mut scope,
        },
    );
    assert!(matches!(result, Err(NormalInputDecodeErrorV1::Common(_))));
    drop(scope);
    assert_eq!(memory.used(), 0);
}
#[test]
fn embedded_json_uses_from_value_acceptance_and_paid_output() {
    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Target {
        number: u64,
    }
    let layout = cbor::CborDataLayoutV1 {
        root: std::mem::size_of::<Target>(),
        sequence_element: 0,
        map_pair: 0,
        set_element: 0,
        box_pointee: 0,
        default_heap_per_value: 0,
        symbols_bytes: 0,
        visits: 1,
        missing: None,
    };
    for value in [
        serde_json::json!({"number":7}),
        serde_json::json!({"number":7.0}),
        serde_json::json!({"number":"7"}),
    ] {
        let expected = serde_json::from_value::<Target>(value.clone()).map_err(|e| e.to_string());
        let memory = MemoryBudget::new(64 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(1_000_000, 1 << 30).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let result =
            cbor::decode_closed_json_value_data_v1::<Target>(value, layout, &limits(), &mut common)
                .unwrap();
        assert!(memory.used() > 0);
        match (result, expected) {
            (Ok(owner), Ok(expected)) => assert_eq!(Borrow::<Target>::borrow(&owner), &expected),
            (Err(note), Err(expected)) => assert_eq!(note.message(), expected),
            _ => panic!("JSON accepted domain changed"),
        };
        assert_eq!(memory.used(), 0);
    }
}
