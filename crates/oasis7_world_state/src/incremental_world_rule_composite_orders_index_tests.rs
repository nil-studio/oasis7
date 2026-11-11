use super::*;
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_test_support::{io, source_admission};
use crate::incremental_world_rule_composite_family::test_support as fx;
use crate::incremental_world_rule_receipts::qualify_world_rule_agent_orders_v1;
use oasis7_formal_protocol::incremental_execution::ExecutionTransaction;
#[path = "incremental_world_rule_receipts_fixture.rs"]
mod data;
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    }
}

#[test]
fn actual_order_index_reads_only_phase_factory_target_and_delivery_without_quote() {
    let _input = source_admission();
    let mut order = data::fixture_AgentIndustrialOrderV1();
    order.phase = crate::world_rules::AgentIndustrialPhaseV1::Producing;
    order.delivered_at = None;
    order.quote.dependency_digest = "large-unread-quote".repeat(32768);
    let expected_factory = order.factory_id.clone();
    let expected_target = order.target_agent_id.clone();
    let source = std::collections::BTreeMap::from([("a".into(), order)]);
    let (memory, pages, roots) = fx::seed(&source);
    let proof = qualify_world_rule_agent_orders_v1(
        &roots,
        &mut fx::context(&pages, &roots, &memory, &mut io(), &mut keys()),
        &mut fx::scan(),
        &mut fx::read_work(),
    )
    .unwrap();
    let quote_payload = fx::selected(
        proof.body().unwrap(),
        &profile(),
        &[
            fx::Key::Entry("a"),
            fx::Key::Field("quote"),
            fx::Key::Field("dependency_digest"),
        ],
        &mut fx::context(&pages, &roots, &memory, &mut io(), &mut keys()),
    );
    // Existing qualified component inheritance must not read this unrelated
    // payload. This test does not grant a physical lease or World readiness.
    pages.remove(&quote_payload);
    let mut tx =
        ExecutionTransaction::begin(fx::parent(&roots), [51; 32], &pages, &memory, fx::limits())
            .unwrap();
    let live = tx.parent_fence();
    let mut value_io = io();
    let mut semantic = keys();
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        },
        key_work: &mut semantic,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let guard = ctx.transaction.guard();
    let current = bind_current_agent_orders_v1(
        &proof,
        &roots,
        &pages,
        &mut ctx,
        &mut fx::scan(),
        &mut fx::read_work(),
        &guard,
        &live,
    )
    .unwrap();
    let mut scan = fx::scan();
    let mut work = fx::read_work();
    work.max_tree_heap = 16 << 10;
    let guard = ctx.transaction.guard();
    let index = read_current_agent_order_index_v1(
        &current, "a", &pages, &mut ctx, &mut scan, &mut work, 128, &guard, &live,
    )
    .unwrap()
    .unwrap();
    assert!(index.uses_budget(&memory));
    assert_eq!(
        index.phase(),
        crate::world_rules::AgentIndustrialPhaseV1::Producing
    );
    assert_eq!(index.factory_id(), expected_factory);
    assert_eq!(index.target_agent_id(), expected_target);
    assert_eq!(index.delivered_at(), None);
    let guard = ctx.transaction.guard();
    assert!(
        read_current_agent_order_index_v1(
            &current, "missing", &pages, &mut ctx, &mut scan, &mut work, 128, &guard, &live
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn actual_order_index_missing_required_phase_payload_is_error_not_absent_order() {
    let _input = source_admission();
    let source =
        std::collections::BTreeMap::from([("a".into(), data::fixture_AgentIndustrialOrderV1())]);
    let (memory, pages, roots) = fx::seed(&source);
    let proof = qualify_world_rule_agent_orders_v1(
        &roots,
        &mut fx::context(&pages, &roots, &memory, &mut io(), &mut keys()),
        &mut fx::scan(),
        &mut fx::read_work(),
    )
    .unwrap();
    let phase_payload = fx::selected(
        proof.body().unwrap(),
        &profile(),
        &[
            fx::Key::Entry("a"),
            fx::Key::Field("phase"),
            fx::Key::Field("tag"),
        ],
        &mut fx::context(&pages, &roots, &memory, &mut io(), &mut keys()),
    );
    let mut tx =
        ExecutionTransaction::begin(fx::parent(&roots), [52; 32], &pages, &memory, fx::limits())
            .unwrap();
    let live = tx.parent_fence();
    let mut value_io = io();
    let mut semantic = keys();
    let guard = {
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut value_io,
            },
            key_work: &mut semantic,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let guard = ctx.transaction.guard();
        let current = bind_current_agent_orders_v1(
            &proof,
            &roots,
            &pages,
            &mut ctx,
            &mut fx::scan(),
            &mut fx::read_work(),
            &guard,
            &live,
        )
        .unwrap();
        pages.remove(&phase_payload);
        let guard = ctx.transaction.guard();
        assert!(
            read_current_agent_order_index_v1(
                &current,
                "a",
                &pages,
                &mut ctx,
                &mut fx::scan(),
                &mut fx::read_work(),
                128,
                &guard,
                &live
            )
            .is_err()
        );
        ctx.transaction.guard()
    };
    assert!(tx.seal(&guard, &live, 1).is_err());
}
