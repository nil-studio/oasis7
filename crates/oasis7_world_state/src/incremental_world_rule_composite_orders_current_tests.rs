use super::*;
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_test_support::{Writer, io, source_admission};
use crate::incremental_world_rule_composite_family::test_support as fx;
use crate::incremental_world_rule_receipts::qualify_world_rule_agent_orders_v1;
use crate::incremental_world_rule_receipts::{AgentOrderEditV1, patch_world_rule_agent_orders_v1};
use oasis7_formal_protocol::incremental_execution::ExecutionTransaction;
#[path = "incremental_world_rule_receipts_fixture.rs"]
mod data;
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    }
}
fn directory_work() -> AgentOrderDirectoryWorkV1 {
    AgentOrderDirectoryWorkV1 {
        max_ids: 16,
        max_id_bytes: 128,
        comparison_bytes_remaining: 1 << 20,
    }
}

#[test]
fn actual_current_orders_bind_diff_and_selected_decode_without_removed_quote_hydration() {
    let _input = source_admission();
    let mut small = data::fixture_AgentIndustrialOrderV1();
    small.phase = crate::world_rules::AgentIndustrialPhaseV1::Reserved;
    let mut large = data::fixture_AgentIndustrialOrderV1();
    large.quote.dependency_digest = "large-unchanged-quote".repeat(32768);
    let mut new = data::fixture_AgentIndustrialOrderV1();
    new.order_id = "new-order".into();
    new.phase = crate::world_rules::AgentIndustrialPhaseV1::Producing;
    let expected_old = small.clone();
    let expected_new = new.clone();
    let source = BTreeMap::from([("a".into(), small), ("large".into(), large)]);
    let (memory, pages, roots) = fx::seed(&source);
    let proof = qualify_world_rule_agent_orders_v1(
        &roots,
        &mut fx::context(&pages, &roots, &memory, &mut io(), &mut keys()),
        &mut fx::scan(),
        &mut fx::read_work(),
    )
    .unwrap();
    let mut tx =
        ExecutionTransaction::begin(fx::parent(&roots), [41; 32], &pages, &memory, fx::limits())
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
    let baseline = bind_current_agent_orders_v1(
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
    let mut typed = fx::read_work();
    let mut work = directory_work();
    let guard = ctx.transaction.guard();
    let ids = collect_parent_agent_order_ids_v1(
        &baseline, &pages, &mut ctx, &mut scan, &mut typed, &mut work, &guard, &live,
    )
    .unwrap();
    assert_eq!(&**ids, &["a", "large"]);
    let guard = ctx.transaction.guard();
    let old = read_current_agent_order_v1(
        &baseline, "a", &pages, &mut ctx, &mut scan, &mut typed, &guard, &live,
    )
    .unwrap()
    .unwrap();
    assert_eq!(old.value(), &expected_old);
    drop(old);
    let phase = fx::selected(
        proof.body().unwrap(),
        &profile(),
        &[
            fx::Key::Entry("a"),
            fx::Key::Field("phase"),
            fx::Key::Field("tag"),
        ],
        &mut fx::context(&pages, &roots, &memory, &mut io(), &mut keys()),
    );
    let removed = fx::selected(
        proof.body().unwrap(),
        &profile(),
        &[fx::Key::Entry("large")],
        &mut fx::context(&pages, &roots, &memory, &mut io(), &mut keys()),
    );
    let guard = ctx.transaction.guard();
    let _updated = patch_world_rule_agent_orders_v1(
        &proof,
        &roots,
        None,
        &[
            AgentOrderEditV1::Phase {
                key: "a",
                expected: phase,
                after: crate::world_rules::AgentIndustrialPhaseV1::Producing,
            },
            AgentOrderEditV1::Remove {
                key: "large",
                expected: removed,
            },
            AgentOrderEditV1::Put {
                key: "new",
                expected: None,
                order: &new,
            },
        ],
        &pages,
        &mut ctx,
        &mut MemberRecordWriteContext {
            sink: &mut Writer(&pages),
            memory: &memory,
            io: &mut io(),
            key_work: &mut keys(),
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut fx::scan(),
        &mut fx::read_work(),
        &mut fx::write_work(),
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    let guard = ctx.transaction.guard();
    let mut narrow = fx::read_work();
    narrow.max_tree_heap = 16 << 10;
    let current = bind_current_agent_orders_v1(
        &proof,
        &roots,
        &pages,
        &mut ctx,
        &mut scan,
        &mut narrow,
        &guard,
        &live,
    )
    .unwrap();
    let guard = ctx.transaction.guard();
    let changes = collect_changed_agent_order_ids_v1(
        &current,
        &pages,
        &mut ctx,
        &mut scan,
        &mut narrow,
        &mut work,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(&**changes, &["a", "large", "new"]);
    let guard = ctx.transaction.guard();
    assert!(
        read_current_agent_order_v1(
            &current, "large", &pages, &mut ctx, &mut scan, &mut typed, &guard, &live
        )
        .unwrap()
        .is_none()
    );
    let guard = ctx.transaction.guard();
    let actual = read_current_agent_order_v1(
        &current, "new", &pages, &mut ctx, &mut scan, &mut typed, &guard, &live,
    )
    .unwrap()
    .unwrap();
    assert_eq!(actual.value(), &expected_new);
    let guard = ctx.transaction.guard();
    let changed = read_current_agent_order_v1(
        &current, "a", &pages, &mut ctx, &mut scan, &mut typed, &guard, &live,
    )
    .unwrap()
    .unwrap();
    let mut expected = expected_old;
    expected.phase = crate::world_rules::AgentIndustrialPhaseV1::Producing;
    assert_eq!(changed.value(), &expected);
    let guard = ctx.transaction.guard();
    assert!(
        read_current_agent_order_v1(
            &baseline, "a", &pages, &mut ctx, &mut scan, &mut typed, &guard, &live
        )
        .is_err(),
        "old private source must not fall back to parent after actual root replacement"
    );
    let guard = ctx.transaction.guard();
    drop(ctx);
    assert!(tx.seal(&guard, &live, 1).is_err());
}

#[test]
fn actual_current_orders_reject_foreign_transaction_and_exhausted_directory_budget() {
    let _input = source_admission();
    for foreign in [false, true] {
        let source = BTreeMap::from([("a".into(), data::fixture_AgentIndustrialOrderV1())]);
        let (memory, pages, roots) = fx::seed(&source);
        let proof = qualify_world_rule_agent_orders_v1(
            &roots,
            &mut fx::context(&pages, &roots, &memory, &mut io(), &mut keys()),
            &mut fx::scan(),
            &mut fx::read_work(),
        )
        .unwrap();
        let mut tx = ExecutionTransaction::begin(
            fx::parent(&roots),
            [42; 32],
            &pages,
            &memory,
            fx::limits(),
        )
        .unwrap();
        let live = tx.parent_fence();
        let guard = tx.guard();
        let current = bind_current_agent_orders_v1(
            &proof,
            &roots,
            &pages,
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io(),
                },
                key_work: &mut keys(),
                semantic_bytes: 4096,
                record_bytes: 3 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
            &mut fx::scan(),
            &mut fx::read_work(),
            &guard,
            &live,
        )
        .unwrap();
        let mut transaction = if foreign {
            ExecutionTransaction::begin(fx::parent(&roots), [43; 32], &pages, &memory, fx::limits())
                .unwrap()
        } else {
            tx
        };
        let guard = transaction.guard();
        let mut work = directory_work();
        if !foreign {
            work.comparison_bytes_remaining = 0;
        }
        let result = collect_parent_agent_order_ids_v1(
            &current,
            &pages,
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut transaction,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io(),
                },
                key_work: &mut keys(),
                semantic_bytes: 4096,
                record_bytes: 3 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
            &mut fx::scan(),
            &mut fx::read_work(),
            &mut work,
            &guard,
            &live,
        );
        if foreign {
            assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Execution(
                    oasis7_formal_protocol::incremental_execution::ExecutionError::StaleGuard
                ))
            ));
        } else {
            assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(
                    ValueTreeError::ResourceLimited("order directory comparisons")
                ))
            ));
        }
        let guard = transaction.guard();
        assert!(transaction.seal(&guard, &live, 1).is_err());
    }
}

#[test]
fn actual_current_orders_cancellation_preserves_first_error_and_poison() {
    let _input = source_admission();
    let source = BTreeMap::from([("a".into(), data::fixture_AgentIndustrialOrderV1())]);
    let (memory, pages, roots) = fx::seed(&source);
    let proof = qualify_world_rule_agent_orders_v1(
        &roots,
        &mut fx::context(&pages, &roots, &memory, &mut io(), &mut keys()),
        &mut fx::scan(),
        &mut fx::read_work(),
    )
    .unwrap();
    let mut tx =
        ExecutionTransaction::begin(fx::parent(&roots), [44; 32], &pages, &memory, fx::limits())
            .unwrap();
    let live = tx.parent_fence();
    let polls = std::cell::Cell::new(0usize);
    let limit = std::cell::Cell::new(usize::MAX);
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
        cancelled: || {
            let count = polls.get().saturating_add(1);
            polls.set(count);
            count >= limit.get()
        },
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
    limit.set(polls.get().saturating_add(20));
    let guard = ctx.transaction.guard();
    let result = collect_parent_agent_order_ids_v1(
        &current,
        &pages,
        &mut ctx,
        &mut fx::scan(),
        &mut fx::read_work(),
        &mut directory_work(),
        &guard,
        &live,
    );
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
    ));
    assert!(polls.get() >= limit.get());
    let guard = ctx.transaction.guard();
    drop(ctx);
    assert!(tx.seal(&guard, &live, 1).is_err());
}
