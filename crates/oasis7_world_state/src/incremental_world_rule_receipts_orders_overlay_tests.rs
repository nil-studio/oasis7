use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_test_support::{Pages, Writer, io, source_admission};
use crate::incremental_world_rule_composite_family::test_support as fx;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionTransaction, RootSetV1, TransitionVerifier,
};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
use oasis7_formal_protocol::incremental_value::ValueReadContext;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
fn apply(
    parent: &ReservedValue<AgentOrdersQualificationV1>,
    roots: &RootSetV1,
    current: Option<&ReservedValue<AgentOrdersOverlayV1>>,
    edits: &[AgentOrderEditV1<'_>],
    pages: &Pages,
    memory: &MemoryBudget,
    transaction: &mut ExecutionTransaction<'_, Pages>,
    cancel: bool,
) -> Result<Option<ReservedValue<AgentOrdersOverlayV1>>, WorldFieldOverlayErrorV1> {
    let guard = transaction.guard();
    let live = transaction.parent_fence();
    let mut read_work = fx::read_work();
    read_work.max_tree_heap = 16 << 10;
    patch_world_rule_agent_orders_v1(
        parent,
        roots,
        current,
        edits,
        pages,
        &mut WorldFieldOverlayContextV1 {
            transaction,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || cancel,
        },
        &mut MemberRecordWriteContext {
            sink: &mut Writer(pages),
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut fx::scan(),
        &mut read_work,
        &mut fx::write_work(),
        &guard,
        &live,
    )
}
fn selected(
    body: &oasis7_formal_protocol::incremental_state::StateRootV1,
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
    path: &[fx::Key<'_>],
) -> [u8; 32] {
    fx::selected(
        body,
        &WorldRuleCompositeBranchV1::AgentOrders.profile(),
        path,
        &mut fx::context(
            pages,
            roots,
            memory,
            &mut io(),
            &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
        ),
    )
}

#[test]
fn orders_progress_leaves_replay_without_hydrating_unchanged_quote() {
    let _source = source_admission();
    use crate::world_rules::AgentIndustrialPhaseV1 as Phase;
    for case in 0..4 {
        let mut order = data::fixture_AgentIndustrialOrderV1();
        order.quote.dependency_digest = "large-quote".repeat(32_768);
        let source = BTreeMap::from([("external-order-key".to_owned(), order)]);
        let (memory, pages, roots) = fx::seed(&source);
        let proof = qualify_world_rule_agent_orders_v1(
            &roots,
            &mut fx::context(
                &pages,
                &roots,
                &memory,
                &mut io(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 32 << 20,
                },
            ),
            &mut fx::scan(),
            &mut fx::read_work(),
        )
        .unwrap();
        let phase = selected(
            proof.body().unwrap(),
            &roots,
            &pages,
            &memory,
            &[
                fx::Key::Entry("external-order-key"),
                fx::Key::Field("phase"),
                fx::Key::Field("tag"),
            ],
        );
        let progress = selected(
            proof.body().unwrap(),
            &roots,
            &pages,
            &memory,
            &[
                fx::Key::Entry("external-order-key"),
                fx::Key::Field("completed_work_ticks"),
            ],
        );
        let reason = selected(
            proof.body().unwrap(),
            &roots,
            &pages,
            &memory,
            &[
                fx::Key::Entry("external-order-key"),
                fx::Key::Field("terminal_reason"),
            ],
        );
        if case == 1 {
            let old_quote_text = selected(
                proof.body().unwrap(),
                &roots,
                &pages,
                &memory,
                &[
                    fx::Key::Entry("external-order-key"),
                    fx::Key::Field("quote"),
                    fx::Key::Field("dependency_digest"),
                ],
            );
            // Deliberately absent unchanged payload proves component inheritance.
            // This does not mint the separate physical lease or WorldReady.
            pages.remove(&old_quote_text);
        }
        let parent = fx::parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, fx::limits())
                .unwrap();
        tx.write(
            StateNamespaceV1::Scheduler,
            b"unrelated-prefix",
            Some([1; 32]),
        )
        .unwrap();
        let edits = [
            AgentOrderEditV1::Phase {
                key: "external-order-key",
                expected: if case == 2 { [255; 32] } else { phase },
                after: Phase::Producing,
            },
            AgentOrderEditV1::CompletedWorkTicks {
                key: "external-order-key",
                expected: progress,
                after: 1,
            },
        ];
        let result = apply(
            &proof,
            &roots,
            None,
            &edits,
            &pages,
            &memory,
            &mut tx,
            case == 3,
        );
        if case >= 2 {
            assert!(result.is_err(), "case {case}");
            let guard = tx.guard();
            assert!(tx.seal(&guard, &live, 1).is_err());
            continue;
        }
        let current = result.unwrap().unwrap();
        let second = [AgentOrderEditV1::TerminalReason {
            key: "external-order-key",
            expected: reason,
            after: None,
        }];
        let next = apply(
            &proof,
            &roots,
            Some(&current),
            &second,
            &pages,
            &memory,
            &mut tx,
            false,
        )
        .unwrap()
        .unwrap();
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        for node in &sealed.new_nodes {
            let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
            Writer(&pages)
                .put_node(
                    id,
                    memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [4; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        let mut work = fx::read_work();
        work.max_tree_heap = 16 << 10;
        let accepted = accept_world_rule_agent_orders_transition_v1(
            &proof,
            &verifier,
            &sealed.record,
            &mut fx::context(
                &pages,
                &sealed.record.after,
                &memory,
                &mut io(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 32 << 20,
                },
            ),
            &mut fx::scan(),
            &mut work,
        )
        .unwrap();
        assert_eq!(accepted.body(), Some(next.body()));
        if case == 0 {
            let mut cold = fx::read_work();
            cold.max_tree_heap = 2 << 20;
            let recovered = recover_world_rule_agent_orders_v1(
                &mut fx::context(
                    &pages,
                    &sealed.record.after,
                    &memory,
                    &mut io(),
                    &mut SemanticKeyBudget {
                        bytes_remaining: 32 << 20,
                    },
                ),
                &mut fx::scan(),
                &mut cold,
            )
            .unwrap()
            .unwrap();
            let order = &recovered.value()["external-order-key"];
            assert_eq!(order.phase, Phase::Producing);
            assert_eq!(order.completed_work_ticks, 1);
            assert_eq!(order.terminal_reason, None);
            assert_eq!(order.quote, source["external-order-key"].quote);
        }
    }
}
