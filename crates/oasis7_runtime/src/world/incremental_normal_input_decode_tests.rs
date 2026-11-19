use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use oasis7_proto::memory_budget::MemoryBudget;
fn limits() -> CborDataLimitsV1 {
    CborDataLimitsV1 {
        max_wire_bytes: 1 << 20,
        max_graph_bytes: 256 << 20,
        max_depth: 128,
    }
}
#[test]
fn embedded_service_packets_never_reach_the_runtime_queue_consumer() {
    let action = Action::WorldServiceIntent {
        request: serde_json::json!({"invalid_but_deferred":"the actual service consumer must validate this"}),
    };
    for raw in [
        serde_cbor::to_vec(&action).unwrap(),
        serde_cbor::to_vec(&Envelope::from_runtime_action(action)).unwrap(),
    ] {
        let memory = MemoryBudget::new(512 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(10_000_000, 1 << 32).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let payload = decode_runtime_input_payload_data_v1(&raw, &limits(), &mut common).unwrap();
        let retained = memory.used();
        let result: Result<(), _> = payload.with_runtime_data(&mut common, |_, _| {
            panic!("service input entered Runtime queue")
        });
        assert!(matches!(
            &result,
            Err(RuntimePayloadConsumeErrorV1::Side(_))
        ));
        assert!(retained > 0);
        assert_eq!(memory.used(), retained, "deferred owner retains every fee");
        drop(result);
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn runtime_projection_consumes_the_real_graph_under_its_original_fee() {
    let action = Action::GrantDataAccess {
        owner_agent_id: "owner".into(),
        grantee_agent_id: "grantee".into(),
    };
    let raw = serde_cbor::to_vec(&Envelope::from_runtime_action(action.clone())).unwrap();
    let memory = MemoryBudget::new(512 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(10_000_000, 1 << 32).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let payload = decode_runtime_input_payload_data_v1(&raw, &limits(), &mut common).unwrap();
    let result = payload
        .with_runtime_data(&mut common, |envelope, _| {
            assert!(memory.used() > 0);
            assert_eq!(envelope.body.runtime_action(), Some(&action));
            Err::<(), _>("queue error")
        })
        .ok()
        .expect("valid Runtime data");
    assert_eq!(result, Err("queue error"));
    assert_eq!(memory.used(), 0);
}

#[test]
fn refused_foreign_common_keeps_the_original_payload_for_its_actual_consumer() {
    let action = Action::GrantDataAccess {
        owner_agent_id: "owner".into(),
        grantee_agent_id: "grantee".into(),
    };
    let raw = serde_cbor::to_vec(&Envelope::from_runtime_action(action.clone())).unwrap();
    let memory = MemoryBudget::new(512 << 20, 0).unwrap();
    let work = FormalBoundedWorkV1::new(10_000_000, 1 << 32).unwrap();
    let foreign = FormalBoundedWorkV1::new(10_000_000, 1 << 32).unwrap();
    let mut stop = || false;
    let mut foreign_stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let mut foreign_common = FormalBoundedContextV1::new(&memory, &foreign, &mut foreign_stop);
    let payload = decode_runtime_input_payload_data_v1(&raw, &limits(), &mut common).unwrap();
    let retained = memory.used();
    let remaining = work.remaining_steps();
    let refused: Result<(), _> = payload.with_runtime_data(&mut foreign_common, |_, _| {
        panic!("foreign Common entered the original Runtime consumer")
    });
    assert_eq!(
        memory.used(),
        retained,
        "context refusal keeps the original owner"
    );
    assert_eq!(work.remaining_steps(), remaining);
    let source = match refused {
        Err(RuntimePayloadConsumeErrorV1::Context {
            error: BoundedCborDataErrorV1::Invalid("normal Runtime payload context"),
            source,
        }) => source,
        other => panic!("expected actual context refusal, got {other:?}"),
    };
    source
        .with_runtime_data(&mut common, |envelope, _| {
            assert!(memory.used() > 0);
            assert_eq!(envelope.body.runtime_action(), Some(&action));
        })
        .expect("the same source remains consumable by its original Common");
    assert_eq!(memory.used(), 0);
}
