use super::*;
use crate::{
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer, io, source_admission},
    incremental_world_rule_composite_family::test_support as fx,
    incremental_world_rule_receipts::{
        WorldRuleRequestEditV1, WorldRuleRequestsQualificationV1,
        accept_world_rule_requests_transition_v1, patch_world_rule_requests_v1,
        qualify_world_rule_requests_v1, recover_world_rule_requests_v1,
    },
    incremental_world_rule_requests_current::{
        insert_current_observation_registry_v2, insert_current_world_rule_request_v1,
        read_current_observation_registry_v2, read_current_world_rule_request_v1,
    },
    world_rules::{WorldRuleReceiptV1, current_observation_key_v2, request_key},
};
use oasis7_agent_api::world_action::{EmergencySupplyCommandV1, WorldRuleCommandV1};
use oasis7_formal_protocol::{
    incremental_execution::TransitionVerifier, incremental_value::ValueIoBudget,
};

fn receipt(at: u64, result: &str) -> WorldRuleReceiptV1 {
    WorldRuleReceiptV1 {
        request_id: "request".into(),
        actor_id: "actor".into(),
        command: WorldRuleCommandV1::EmergencySupply {
            command: EmergencySupplyCommandV1::Observe {
                region_id: "region".into(),
            },
        },
        committed_at: at,
        result_id: result.into(),
    }
}
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    }
}
fn no_error() -> Option<ValueTreeError> {
    None
}
fn context<'t, 's, 'v, C: FnMut() -> bool>(
    tx: &'t mut ExecutionTransaction<'s, Pages>,
    pages: &'v Pages,
    memory: &'v MemoryBudget,
    io: &'v mut ValueIoBudget,
    keys: &'v mut SemanticKeyBudget,
    cancelled: C,
) -> WorldFieldOverlayContextV1<'t, 's, 'v, Pages, Pages, fn() -> Option<ValueTreeError>, C> {
    WorldFieldOverlayContextV1 {
        transaction: tx,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work: keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: no_error,
        cancelled,
    }
}
fn qualify(
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
) -> ReservedValue<WorldRuleRequestsQualificationV1> {
    qualify_world_rule_requests_v1(
        roots,
        &mut fx::context(pages, roots, memory, &mut io(), &mut keys()),
        &mut fx::scan(),
        &mut fx::read_work(),
    )
    .unwrap()
}

#[test]
fn selected_results_reopen_original_outers_with_large_unselected_text() {
    let _source = source_admission();
    let first = receipt(60, "selected");
    let canonical = request_key(&first.actor_id, &first.request_id);
    let mut unreadable = receipt(180, &"unchanged".repeat(32_768));
    unreadable.request_id = "other".into();
    let original = Continuity::from([
        ("_length".into(), first),
        (canonical.clone(), receipt(120, "selected")),
        ("unselected".into(), unreadable),
    ]);
    let (memory, pages, roots) = fx::seed(&original);
    let page_memory = memory.used();
    {
        let proof = qualify(&roots, &pages, &memory);
        let parent = fx::parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent, [41; 32], &pages, &memory, fx::limits()).unwrap();
        let mut read_io = io();
        let mut key_work = keys();
        let mut scan = fx::scan();
        let mut work = fx::read_work();
        let mut ctx = context(
            &mut tx,
            &pages,
            &memory,
            &mut read_io,
            &mut key_work,
            || false,
        );
        let guard = ctx.transaction.guard();
        let selected = observe_current_requests_result_sources_v1(
            "selected", &proof, &roots, &pages, &mut ctx, &mut scan, &mut work, &guard, &live,
        )
        .unwrap();
        assert_eq!(selected.requests().count(), 2);
        for source in selected.requests() {
            // A new authenticated read extends the read-set without changing
            // the original selected requests body or refreshing its guard.
            ctx.transaction
                .read(StateNamespaceV1::Scheduler, b"unrelated-read")
                .unwrap();
            let guard = ctx.transaction.guard();
            let value = read_current_requests_result_receipt_v1(
                &source, &proof, &roots, &pages, &mut ctx, &mut scan, &mut work, &guard, &live,
            )
            .unwrap();
            assert_eq!(Some(value.value()), original.get(source.outer_key()));
            assert!(value.uses_budget(&memory));
        }
        // The ordinary actor/request reader keeps its canonical key domain.
        let guard = ctx.transaction.guard();
        let normal = read_current_world_rule_request_v1(
            "actor", "request", &proof, &roots, &pages, &mut ctx, &mut scan, &mut work, &guard,
            &live,
        )
        .unwrap();
        assert_eq!(normal.value(), original.get(&canonical));
        assert!(ctx.values.io.nodes_remaining < io().nodes_remaining);
        assert!(scan.nodes_remaining < fx::scan().nodes_remaining);
        assert_eq!(work.max_tree_heap, 64 << 10);
    }
    assert_eq!(memory.used(), page_memory);
    drop(pages);
    assert_eq!(memory.used(), 0);
}

#[test]
fn selected_result_sources_reject_changed_current_foreign_pool_and_cancel() {
    let _source = source_admission();
    let original = Continuity::from([("external-key".into(), receipt(60, "selected"))]);
    let (memory, pages, roots) = fx::seed(&original);
    let proof = qualify(&roots, &pages, &memory);
    let parent = fx::parent(&roots);
    let live = parent.fence().unwrap();
    for case in 0..5 {
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [42; 32], &pages, &memory, fx::limits())
                .unwrap();
        let mut read_io = io();
        let mut key_work = keys();
        let mut scan = fx::scan();
        let mut work = fx::read_work();
        let selected = {
            let guard = tx.guard();
            observe_current_requests_result_sources_v1(
                "selected",
                &proof,
                &roots,
                &pages,
                &mut context(
                    &mut tx,
                    &pages,
                    &memory,
                    &mut read_io,
                    &mut key_work,
                    || false,
                ),
                &mut scan,
                &mut work,
                &guard,
                &live,
            )
            .unwrap()
        };
        let source = selected.requests().next().unwrap();
        if case == 0 {
            let changed = receipt(120, "selected");
            let guard = tx.guard();
            patch_world_rule_requests_v1(
                &proof,
                &roots,
                None,
                &[WorldRuleRequestEditV1::Put {
                    key: source.outer_key(),
                    expected: Some(source.row.value()),
                    receipt: &changed,
                }],
                &pages,
                &mut context(
                    &mut tx,
                    &pages,
                    &memory,
                    &mut read_io,
                    &mut key_work,
                    || false,
                ),
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
                &mut scan,
                &mut work,
                &mut fx::write_work(),
                &guard,
                &live,
            )
            .unwrap();
        }
        if case == 4 {
            tx.write(StateNamespaceV1::Scheduler, b"unrelated-same-value", None)
                .unwrap();
        }
        let foreign = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
        let mut other =
            ExecutionTransaction::begin(parent.clone(), [43; 32], &pages, &memory, fx::limits())
                .unwrap();
        let active = if case == 2 { &mut other } else { &mut tx };
        let guard = active.guard();
        let result = read_current_requests_result_receipt_v1(
            &source,
            &proof,
            &roots,
            &pages,
            &mut context(
                active,
                &pages,
                if case == 1 { &foreign } else { &memory },
                &mut read_io,
                &mut key_work,
                || case == 3,
            ),
            &mut scan,
            &mut work,
            &guard,
            &live,
        );
        match case {
            0 | 4 => assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Execution(
                    ExecutionError::StaleGuard
                ))
            )),
            3 => assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
            )),
            _ => assert!(result.is_err()),
        }
        let guard = active.guard();
        if case == 2 {
            assert!(other.seal(&guard, &live, 1).is_err());
        } else {
            assert!(tx.seal(&guard, &live, 1).is_err());
        }
        assert_eq!(foreign.used(), 0);
    }
    drop(proof);
    drop(pages);
    assert_eq!(memory.used(), 0);
}

#[test]
fn observation_registry_result_source_survives_real_count_put_and_receiver() {
    let _source = source_admission();
    let (memory, pages, roots) = fx::seed(&Continuity::new());
    let proof = qualify(&roots, &pages, &memory);
    let parent = fx::parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [44; 32], &pages, &memory, fx::limits())
            .unwrap();
    let mut read_io = io();
    let mut key_work = keys();
    let mut scan = fx::scan();
    let mut work = fx::read_work();
    let original = receipt(
        120,
        &format!("current-observation-original-v2:{}", "a".repeat(64)),
    );
    let registry_key = current_observation_key_v2("region", 60);
    let current = {
        let mut ctx = context(
            &mut tx,
            &pages,
            &memory,
            &mut read_io,
            &mut key_work,
            || false,
        );
        let guard = ctx.transaction.guard();
        let absent = read_current_observation_registry_v2(
            "region", 60, &proof, &roots, &pages, &mut ctx, &mut scan, &mut work, &guard, &live,
        )
        .unwrap();
        assert!(absent.registered().is_none());
        let guard = ctx.transaction.guard();
        let current = insert_current_observation_registry_v2(
            &absent,
            &original,
            &proof,
            &roots,
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
            &mut scan,
            &mut work,
            &mut fx::write_work(),
            &guard,
            &live,
        )
        .unwrap()
        .unwrap();
        let guard = ctx.transaction.guard();
        let selected = observe_current_requests_result_sources_v1(
            &original.result_id,
            &proof,
            &roots,
            &pages,
            &mut ctx,
            &mut scan,
            &mut work,
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(selected.requests().count(), 1);
        let source = selected.requests().next().unwrap();
        assert_eq!(source.outer_key(), registry_key);
        let guard = ctx.transaction.guard();
        let value = read_current_requests_result_receipt_v1(
            &source, &proof, &roots, &pages, &mut ctx, &mut scan, &mut work, &guard, &live,
        )
        .unwrap();
        assert_eq!(value.value(), &original);
        let guard = ctx.transaction.guard();
        let normal = read_current_world_rule_request_v1(
            &original.actor_id,
            &original.request_id,
            &proof,
            &roots,
            &pages,
            &mut ctx,
            &mut scan,
            &mut work,
            &guard,
            &live,
        )
        .unwrap();
        assert!(normal.value().is_none());
        let guard = ctx.transaction.guard();
        let registered = read_current_observation_registry_v2(
            "region", 60, &proof, &roots, &pages, &mut ctx, &mut scan, &mut work, &guard, &live,
        )
        .unwrap();
        assert_eq!(registered.registered(), Some(&original));
        current
    };
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    fx::persist(&pages, &memory, &sealed.new_nodes);
    let accepted = accept_world_rule_requests_transition_v1(
        &proof,
        &TransitionVerifier {
            parent: &parent,
            operation: [44; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        },
        &sealed.record,
        &mut fx::context(
            &pages,
            &sealed.record.after,
            &memory,
            &mut io(),
            &mut keys(),
        ),
        &mut fx::scan(),
        &mut fx::read_work(),
    )
    .unwrap();
    assert_eq!(accepted.body(), Some(current.body()));
    let cold = qualify(&sealed.record.after, &pages, &memory);
    assert_eq!(cold.body(), accepted.body());
    let recovered = recover_world_rule_requests_v1(
        &mut fx::context(
            &pages,
            &sealed.record.after,
            &memory,
            &mut io(),
            &mut keys(),
        ),
        &mut fx::scan(),
        &mut fx::read_work(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        recovered.value(),
        &Continuity::from([(registry_key, original)])
    );
}

#[test]
fn request_and_registry_inserts_keep_their_original_binding_gates() {
    let _source = source_admission();
    let (memory, pages, roots) = fx::seed(&Continuity::new());
    let proof = qualify(&roots, &pages, &memory);
    let parent = fx::parent(&roots);
    let live = parent.fence().unwrap();
    for case in 0..3 {
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [45; 32], &pages, &memory, fx::limits())
                .unwrap();
        let mut read_io = io();
        let mut key_work = keys();
        let mut scan = fx::scan();
        let mut work = fx::read_work();
        let mut value = receipt(
            120,
            &format!("current-observation-original-v2:{}", "a".repeat(64)),
        );
        let result = {
            let mut ctx = context(
                &mut tx,
                &pages,
                &memory,
                &mut read_io,
                &mut key_work,
                || false,
            );
            let mut writer = Writer(&pages);
            let mut write_io = io();
            let mut write_keys = keys();
            let mut output = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 2 << 20,
                    record_bytes: 3 << 20,
                },
                cancelled: || false,
            };
            let guard = ctx.transaction.guard();
            if case == 0 {
                let absent = read_current_world_rule_request_v1(
                    "actor", "request", &proof, &roots, &pages, &mut ctx, &mut scan, &mut work,
                    &guard, &live,
                )
                .unwrap();
                value.actor_id = "other-actor".into();
                let guard = ctx.transaction.guard();
                insert_current_world_rule_request_v1(
                    &absent,
                    &value,
                    &proof,
                    &roots,
                    &pages,
                    &mut ctx,
                    &mut output,
                    &mut scan,
                    &mut work,
                    &mut fx::write_work(),
                    &guard,
                    &live,
                )
            } else {
                let absent = read_current_observation_registry_v2(
                    "region", 60, &proof, &roots, &pages, &mut ctx, &mut scan, &mut work, &guard,
                    &live,
                )
                .unwrap();
                if case == 1 {
                    value.committed_at = 119;
                } else {
                    value.result_id = format!("current-observation-original-v2:{}", "A".repeat(64));
                }
                let guard = ctx.transaction.guard();
                insert_current_observation_registry_v2(
                    &absent,
                    &value,
                    &proof,
                    &roots,
                    &pages,
                    &mut ctx,
                    &mut output,
                    &mut scan,
                    &mut work,
                    &mut fx::write_work(),
                    &guard,
                    &live,
                )
            }
        };
        assert!(result.is_err());
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
    }
}
