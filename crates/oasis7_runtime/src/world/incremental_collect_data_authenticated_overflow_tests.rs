//! Actual authenticated business overflow preserves the legacy typed error and aborts.
use super::*;
use crate::world::incremental_action_data::{
    CollectionResourceStateErrorV1, auth_current::CollectDataAuthorizationWorkV1,
    auth_signature::CollectDataSignatureWorkV1,
};
use crate::world::incremental_body_event::materials::test_support::MaterialFixtureV1;
use oasis7_world_state::incremental_world_native::StarterBindingIndexWorkV1;
#[test]
fn authenticated_data_overflow_preserves_original_stock_error_and_aborts_without_result() {
    use oasis7_agent_api::world_action::ResourceKind;
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = collect_data_world();
    world
        .state
        .agents
        .get_mut("collector")
        .unwrap()
        .state
        .resources
        .set(ResourceKind::Data, i64::MAX)
        .unwrap();
    let key = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&[7; 32])
            .verifying_key()
            .to_bytes(),
    );
    world.state.starter_oc_claims.insert(
        "outer".into(),
        oasis7_world_state::gameplay_state::StarterOcClaimState {
            player_id: "player".into(),
            public_key: Some(key.clone()),
            agent_id: "collector".into(),
            ..Default::default()
        },
    );
    let signature = oasis7_proto::collect_data_auth::sign_authorization(
        oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
        3,
        4,
        "player",
        &key,
        1,
        &hex::encode([7; 32]),
    )
    .unwrap();
    world.submit_action(crate::Action::CollectDataAuthenticated {
        collector_agent_id: "collector".into(),
        electricity_cost: 3,
        data_amount: 4,
        player_id: "player".into(),
        public_key: key,
        nonce: 1,
        signature,
    });
    let mut oracle = World::new_with_state(world.state.clone());
    oracle.state.time = 1;
    let envelope = world.pending_actions.front().unwrap();
    let body = oracle.action_to_event(envelope).unwrap();
    let error = oracle
        .append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)))
        .unwrap_err();
    let crate::WorldError::ResourceBalanceInvalid { reason: expected } = error else {
        panic!("legacy error kind changed")
    };
    assert!(expected.starts_with("data collection data credit failed:"));
    let material_limits = MaterialFixtureLimitsV1::for_state(&world.state);
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    drop(oracle);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut originals = Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [141; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: oasis7_formal_protocol::incremental_execution::EXECUTION_MAX_READS,
            max_writes: 1024,
            max_facts: 64,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &tx, &memory).unwrap();
    let directory = {
        let mut io = genesis::io();
        let mut keys = genesis::keys();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        build_accepted_invocation_directory_v1(
            &fixture.runtime,
            &fixture.head,
            fixture.current.raw().instances(),
            fixture.current.runtime().config(),
            &reader,
            &mut overlay,
            &mut rules::directory_work(),
            &live,
        )
        .unwrap()
    };
    let mut pending_work = genesis::current_work(&fixture.roots).runtime.pending;
    let guard = tx.guard();
    let mut pending =
        oasis7_world_state::incremental_pending_action_overlay::begin_pending_action_current_v1(
            fixture.current.runtime().pending(),
            &fixture.roots,
            &mut tx,
            &memory,
            &guard,
            &live,
            &mut pending_work.index,
            || false,
        )
        .unwrap();
    let mut read_io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut auth_io = genesis::io();
    let mut auth_keys = genesis::keys();
    let mut auth_read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut auth_io,
        },
        key_work: &mut auth_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut parent_io = genesis::io();
    let mut parent_keys = genesis::keys();
    let mut parent_read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut parent_io,
        },
        key_work: &mut parent_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut auth_work = CollectDataAuthorizationWorkV1 {
        signature: CollectDataSignatureWorkV1 {
            checks_remaining: 2,
            bytes_remaining: 1 << 20,
        },
        claims: StarterBindingIndexWorkV1 {
            max_rows: 256,
            max_row_text_bytes: 64 << 10,
            max_pending_rows: 1024,
            steps_remaining: 100_000,
            bytes_remaining: 64 << 20,
            comparison_bytes_remaining: 64 << 20,
            moved_slots_remaining: 1_000_000,
            pending_entries_remaining: 100_000,
        },
        native: genesis::current_work(&fixture.roots).native.native,
        scan: genesis::scan(),
    };
    let mut backing = MaterialFixtureV1::build(
        &fixture.roots,
        &reader,
        &reader,
        &memory,
        material_limits,
        || reader.cas.error(),
        || false,
    );
    let mut materials = backing.context(&fixture.roots, &reader, || reader.cas.error());
    let mut sink = fixture.cas.clone();
    let mut output_io = genesis::io();
    let mut canonical = genesis::canonical();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut output_io,
        cancelled: || false,
    };
    let mut callbacks = 0;
    {
        let guard = tx.guard();
        let mut step = IwsStepContext::begin_counted(
            &mut tx,
            0,
            &guard,
            &live,
            &reader,
            &mut mutation,
            &mut canonical,
            &counter,
        )
        .unwrap();
        let selected = take_pending_action(
            &mut pending,
            step.transaction,
            &mut read,
            &mut pending_work,
            &mut originals,
            &live,
        )
        .unwrap()
        .unwrap();
        let outcome = rules::outcome(
            &fixture,
            &reader,
            step.transaction,
            &selected,
            &directory,
            &counter,
            &mut originals,
            &memory,
            step.time(),
            &live,
        );
        let guard = step.transaction.guard();
        let error=crate::world::incremental_action_data::authenticated::execute_selected_authenticated_collect_data_v1(
            &mut step,&mut materials,&selected,&outcome,fixture.current.raw().agents(),&fixture.roots,
            &mut read,&mut parent_read,fixture.current.native().native().starter_binding_index(),fixture.current.native().controls(),
            &mut auth_read,&mut auth_work,&mut collect_data_work(),&mut collect_data_audit_work(),64<<10,&guard,&live,
            |_:&mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {callbacks+=1;Err(crate::world::incremental_body::BodyMutationError::Invalid("overflow publication"))});
        let error = match error {
            Err(crate::world::incremental_body::BodyMutationError::Original(error)) => error,
            Err(other) => panic!("expected retained original stock error, got {other:?}"),
            Ok(_) => panic!("overflow unexpectedly succeeded"),
        };
        let original = error
            .downcast_ref::<CollectionResourceStateErrorV1>()
            .expect("closed typed collection stock error");
        let crate::WorldError::ResourceBalanceInvalid { reason } = &original.cause else {
            panic!("typed error kind changed")
        };
        assert_eq!(reason, &expected);
        assert_eq!(
            step.transaction.ordered_fact_count(),
            1,
            "overflow cannot publish a result Event"
        );
    }
    assert_eq!(callbacks, 0);
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
