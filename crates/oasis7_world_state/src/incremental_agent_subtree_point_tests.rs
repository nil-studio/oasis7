use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_value_mutation::ValueMutationContextV1,
};
use oasis7_formal_protocol::incremental_execution::*;
use std::cell::{Cell, RefCell};
struct Values<'a> {
    base: &'a Pages,
    extra: &'a RefCell<Pages>,
}
impl ValueNodeReader for Values<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if let Some(page) = self.extra.borrow().0.get(id) {
            if page.len() > max {
                return Err(ValueTreeError::Invalid("point fixture bound"));
            }
            return Ok(page.clone());
        }
        self.base.read_node(id, max, memory, class)
    }
}
struct Sink<'a>(&'a RefCell<Pages>);
impl ValueNodeWriter for Sink<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.borrow_mut().put_node(id, bytes)
    }
}
macro_rules! overlay {
    ($tx:expr,$state:expr,$values:expr,$memory:expr,$io:expr,$keys:expr,$cancel:expr) => {
        WorldFieldOverlayContextV1 {
            transaction: $tx,
            values: ValueReadContext {
                source: $values,
                memory: $memory,
                class: MemoryClass::Ordinary,
                io: $io,
            },
            key_work: $keys,
            semantic_bytes: 1024,
            record_bytes: 1 << 20,
            take_state_error: || $state.take(),
            cancelled: $cancel,
        }
    };
}
fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 64,
        max_writes: 16,
        max_facts: 4,
        max_pending_node_bytes: 8 << 20,
    }
}
fn request(agent: &str, field: AgentFieldV1, ordinal: u64) -> AgentSubtreeScalarRequestV1<'_> {
    AgentSubtreeScalarRequestV1 {
        agent,
        field,
        ordinal,
    }
}
#[test]
fn ordinary_scalars_preserve_same_transaction_optional_members_and_original_groups() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
    let (pages, roots) = fixture::seed(&memory);
    let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
    let extra = RefCell::new(Pages::default());
    let values = Values {
        base: &pages,
        extra: &extra,
    };
    let state = fixture::State::new(&pages, &memory);
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &state, &memory, limits()).unwrap();
    let mut read_io = io();
    let mut write_io = io();
    let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut sink = Sink(&extra);
    use AgentFieldV1 as F;
    use AgentScalarRefV1 as V;
    use AgentSubtreeValueV1 as O;
    macro_rules! edit {
        ($agent:expr,$field:expr,$ordinal:expr,$expected:expr,$after:expr) => {{
            let guard = tx.guard();
            change_agent_subtree_scalar_v1(
                AgentSubtreeCellChangeV1 {
                    query: AgentSubtreeQualifiedRequestV1 {
                        cell: request($agent, $field, $ordinal),
                        qualification: &proof,
                    },
                    expected: $expected,
                    after: $after,
                    max_scalar_bytes: 1 << 20,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || false
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || false,
                },
                &guard,
                &live,
            )
            .unwrap()
        }};
    }
    macro_rules! get {
        ($agent:expr,$field:expr,$ordinal:expr) => {{
            let guard = tx.guard();
            read_agent_subtree_scalar_overlay_v1(
                AgentSubtreeQualifiedRequestV1 {
                    cell: request($agent, $field, $ordinal),
                    qualification: &proof,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || false
                ),
                1 << 20,
                &mut io::sink(),
                &guard,
                &live,
            )
            .unwrap()
        }};
    }
    assert!(edit!(
        "agent β",
        F::IntentEventSeq,
        0,
        Some(V::U64(u64::MAX)),
        Some(V::U64(7))
    ));
    assert_eq!(get!("agent β", F::IntentEventSeq, 0), Some(O::U64(7)));
    assert_eq!(get!("agent α", F::ResourceAmount, 1), None);
    assert!(edit!(
        "agent α",
        F::ResourceAmount,
        1,
        None,
        Some(V::I64(0))
    ));
    assert_eq!(get!("agent α", F::ResourceAmount, 1), Some(O::I64(0)));
    assert!(edit!(
        "agent α",
        F::ResourceAmount,
        1,
        Some(V::I64(0)),
        None
    ));
    assert_eq!(get!("agent α", F::ResourceAmount, 1), None);
    assert_eq!(
        get!("agent β", F::IntentTarget, 0),
        Some(O::OptionalText(Some(0)))
    );
    assert!(edit!(
        "agent β",
        F::IntentTarget,
        0,
        Some(V::OptionalText(Some(""))),
        Some(V::OptionalText(None))
    ));
    assert_eq!(
        get!("agent β", F::IntentTarget, 0),
        Some(O::OptionalText(None))
    );
    assert!(edit!(
        "agent α",
        F::SlotLocked,
        0,
        Some(V::Bool(false)),
        Some(V::Bool(true))
    ));
    assert_eq!(get!("agent α", F::SlotLocked, 0), Some(O::Bool(true)));
    assert!(edit!(
        "agent α",
        F::ActivityReasonSummary,
        0,
        Some(V::OptionalText(None)),
        Some(V::OptionalText(Some("世界🙂")))
    ));
    assert_eq!(
        get!("agent α", F::ActivityReasonSummary, 0),
        Some(O::OptionalText(Some("世界🙂".len() as u64)))
    );
    assert!(!edit!("agent α", F::IntentSummary, 0, None, None));
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verified = TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    }
    .verify(&sealed.record)
    .unwrap();
    assert_eq!(verified.record.after, sealed.record.after);
    let accepted = accept_agent_subtree_transition_v1(
        &proof,
        &TransitionVerifier {
            parent: &parent,
            operation: [4; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        },
        &sealed.record,
        &mut crate::incremental_field_read::WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 1024,
            record_bytes: 1 << 20,
            take_state_error: || state.take(),
            cancelled: || false,
        },
        &mut AgentSubtreeTransitionWorkV1 {
            cells_remaining: 100,
            max_scalar_bytes: 1 << 20,
        },
    )
    .unwrap();
    accepted.validate_binding(&sealed.record.after).unwrap();
    assert_eq!(accepted.agent_count(), 2);
    assert_eq!(accepted.mailbox_reference_count(), 0);
}
#[test]
fn public_getter_n_then_noop_n_plus_one_cancellation_poison_successful_prefix() {
    for write_channel in [false, true] {
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
        let (pages, roots) = fixture::seed(&memory);
        let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
        let extra = RefCell::new(Pages::default());
        let values = Values {
            base: &pages,
            extra: &extra,
        };
        let state = fixture::State::new(&pages, &memory);
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent, [5; 32], &state, &memory, limits()).unwrap();
        let mut read_io = io();
        let mut write_io = io();
        let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut sink = Sink(&extra);
        let guard = tx.guard();
        assert!(
            change_agent_subtree_scalar_v1(
                AgentSubtreeCellChangeV1 {
                    query: AgentSubtreeQualifiedRequestV1 {
                        cell: request("agent β", AgentFieldV1::IntentEventSeq, 0),
                        qualification: &proof
                    },
                    expected: Some(AgentScalarRefV1::U64(u64::MAX)),
                    after: Some(AgentScalarRefV1::U64(7)),
                    max_scalar_bytes: 128
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || false
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || false
                },
                &guard,
                &live
            )
            .unwrap()
        );
        let n = Cell::new(0usize);
        let guard = tx.guard();
        let got = read_agent_subtree_scalar_overlay_v1(
            AgentSubtreeQualifiedRequestV1 {
                cell: request("agent β", AgentFieldV1::IntentLogicalTime, 0),
                qualification: &proof,
            },
            &mut overlay!(
                &mut tx,
                &state,
                &values,
                &memory,
                &mut read_io,
                &mut keys,
                || {
                    n.set(n.get() + 1);
                    false
                }
            ),
            128,
            &mut io::sink(),
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(got, Some(AgentSubtreeValueV1::U64(u64::MAX)));
        let count = Cell::new(0usize);
        let guard = tx.guard();
        let result = change_agent_subtree_scalar_v1(
            AgentSubtreeCellChangeV1 {
                query: AgentSubtreeQualifiedRequestV1 {
                    cell: request("agent β", AgentFieldV1::IntentLogicalTime, 0),
                    qualification: &proof,
                },
                expected: Some(AgentScalarRefV1::U64(u64::MAX)),
                after: Some(AgentScalarRefV1::U64(u64::MAX)),
                max_scalar_bytes: 128,
            },
            &mut overlay!(
                &mut tx,
                &state,
                &values,
                &memory,
                &mut read_io,
                &mut keys,
                || {
                    if write_channel {
                        false
                    } else {
                        count.set(count.get() + 1);
                        count.get() == n.get() + 1
                    }
                }
            ),
            &mut ValueMutationContextV1 {
                sink: &mut sink,
                memory: &memory,
                io: &mut write_io,
                cancelled: || {
                    if write_channel {
                        count.set(count.get() + 1);
                        count.get() == n.get() + 1
                    } else {
                        false
                    }
                },
            },
            &guard,
            &live,
        );
        assert!(matches!(
            result,
            Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
        ));
        assert_eq!(count.get(), n.get() + 1);
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
        eprintln!(
            "agent getterN={} noopN+1={} writer={write_channel} prefixAborted",
            n.get(),
            count.get()
        );
    }
}

#[test]
fn receiver_requires_option_flag_and_complete_original_group_delta() {
    for removed in [0usize, 1, 7] {
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
        let (pages, roots) = fixture::seed(&memory);
        let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
        let extra = RefCell::new(Pages::default());
        let values = Values {
            base: &pages,
            extra: &extra,
        };
        let state = fixture::State::new(&pages, &memory);
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [8; 32], &state, &memory, limits())
                .unwrap();
        let mut sink = Sink(&extra);
        let false_value = write_agent_scalar_stream_v1(
            AgentScalarRefV1::Bool(false),
            &mut ValueMutationContextV1 {
                sink: &mut sink,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
        )
        .unwrap();
        let flag = AgentFieldKeyV1::new("agent α", AgentFieldV1::ActivityPresent, 0).unwrap();
        tx.write(
            AgentFieldV1::ActivityPresent.namespace(),
            flag.as_bytes(),
            Some(false_value.digest().unwrap()),
        )
        .unwrap();
        for field in AgentFieldV1::ALL
            .iter()
            .copied()
            .filter(|field| schema::group(*field) == schema::Group::Activity)
            .take(removed)
        {
            let key = AgentFieldKeyV1::new("agent α", field, 0).unwrap();
            tx.write(field.namespace(), key.as_bytes(), None).unwrap();
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [8; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        verifier.verify(&sealed.record).unwrap();
        let accepted = accept_agent_subtree_transition_v1(
            &proof,
            &verifier,
            &sealed.record,
            &mut crate::incremental_field_read::WorldFieldReadContextV1 {
                root: &sealed.record.after.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io(),
                },
                key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 1 << 20,
                take_state_error: || state.take(),
                cancelled: || false,
            },
            &mut AgentSubtreeTransitionWorkV1 {
                cells_remaining: 100,
                max_scalar_bytes: 1 << 20,
            },
        );
        if removed == 7 {
            accepted
                .unwrap()
                .validate_binding(&sealed.record.after)
                .unwrap();
        } else {
            assert!(matches!(
                accepted,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ));
        }
    }
}

#[test]
fn explicit_optional_replacement_supports_same_transaction_remove_create_and_scalar_followup() {
    let memory = MemoryBudget::new(96 << 20, 8 << 20).unwrap();
    let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
    let (pages, roots) = fixture::seed(&memory);
    let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
    let extra = RefCell::new(Pages::default());
    let values = Values {
        base: &pages,
        extra: &extra,
    };
    let state = fixture::State::new(&pages, &memory);
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [9; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 64,
            ..limits()
        },
    )
    .unwrap();
    let mut read_io = io();
    let mut write_io = io();
    let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut sink = Sink(&extra);
    let original = crate::agent_cell::AgentActivityV1::idle(u64::MAX);
    let mut replacement = crate::agent_cell::AgentActivityV1::idle(0);
    replacement.reason_summary = Some("".into());
    let intent: crate::AgentIntentV2 = serde_json::from_value(serde_json::json!({
        "schema_version":u32::MAX,"agent_id":"agent α","intent_id":"same-TX original", "kind":"raw kind",
        "summary":"界", "status":"raw status", "source":"raw source", "logical_time":u64::MAX,
        "event_seq":0,"updated_at":0
    })).unwrap();
    use AgentSubtreeOptionalRefV1 as O;
    for (expected, after) in [
        (O::Activity(Some(&original)), O::Activity(None)),
        (O::Activity(None), O::Activity(Some(&replacement))),
        (O::Intent(None), O::Intent(Some(&intent))),
    ] {
        let guard = tx.guard();
        assert!(
            change_agent_subtree_optional_v1(
                AgentSubtreeOptionalChangeV1 {
                    agent: "agent α",
                    qualification: &proof,
                    expected,
                    after,
                    max_scalar_bytes: 1 << 20,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || false
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || false
                },
                &guard,
                &live
            )
            .unwrap()
        );
    }
    let guard = tx.guard();
    assert!(
        change_agent_subtree_scalar_v1(
            AgentSubtreeCellChangeV1 {
                query: AgentSubtreeQualifiedRequestV1 {
                    cell: request("agent α", AgentFieldV1::IntentEventSeq, 0),
                    qualification: &proof
                },
                expected: Some(AgentScalarRefV1::U64(0)),
                after: Some(AgentScalarRefV1::U64(u64::MAX)),
                max_scalar_bytes: 64,
            },
            &mut overlay!(
                &mut tx,
                &state,
                &values,
                &memory,
                &mut read_io,
                &mut keys,
                || false
            ),
            &mut ValueMutationContextV1 {
                sink: &mut sink,
                memory: &memory,
                io: &mut write_io,
                cancelled: || false
            },
            &guard,
            &live
        )
        .unwrap()
    );
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [9; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let accepted = accept_agent_subtree_transition_v1(
        &proof,
        &verifier,
        &sealed.record,
        &mut crate::incremental_field_read::WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 1024,
            record_bytes: 1 << 20,
            take_state_error: || state.take(),
            cancelled: || false,
        },
        &mut AgentSubtreeTransitionWorkV1 {
            cells_remaining: 100,
            max_scalar_bytes: 1 << 20,
        },
    )
    .unwrap();
    accepted.validate_binding(&sealed.record.after).unwrap();
}

#[test]
fn receiver_checks_row_peer_presence_count_and_monotonic_allocation() {
    for member_count in [0usize, 3, 4] {
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
        let (pages, roots) = fixture::seed(&memory);
        let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
        let extra = RefCell::new(Pages::default());
        let values = Values {
            base: &pages,
            extra: &extra,
        };
        let state = fixture::State::new(&pages, &memory);
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [10; 32], &state, &memory, limits())
                .unwrap();
        let mut sink = Sink(&extra);
        let mut write_io = io();
        use AgentFieldV1 as F;
        use AgentScalarRefV1 as V;
        for (field, ordinal, value) in [(F::SlotCount, 0, V::U64(3)), (F::SlotNext, 0, V::U64(3))]
            .into_iter()
            .chain(
                [
                    (F::SlotId, 2, V::Text(" repeated original ")),
                    (F::SlotType, 2, V::U8(5)),
                    (F::SlotInstalled, 2, V::OptionalText(Some(""))),
                    (F::SlotLocked, 2, V::Bool(true)),
                ]
                .into_iter()
                .take(member_count),
            )
        {
            let value = write_agent_scalar_stream_v1(
                value,
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || false,
                },
            )
            .unwrap();
            let key = AgentFieldKeyV1::new("agent α", field, ordinal).unwrap();
            tx.write(
                field.namespace(),
                key.as_bytes(),
                Some(value.digest().unwrap()),
            )
            .unwrap();
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [10; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        verifier.verify(&sealed.record).unwrap();
        let accepted = accept_agent_subtree_transition_v1(
            &proof,
            &verifier,
            &sealed.record,
            &mut crate::incremental_field_read::WorldFieldReadContextV1 {
                root: &sealed.record.after.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io(),
                },
                key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 1 << 20,
                take_state_error: || state.take(),
                cancelled: || false,
            },
            &mut AgentSubtreeTransitionWorkV1 {
                cells_remaining: 100,
                max_scalar_bytes: 1 << 20,
            },
        );
        if member_count == 4 {
            accepted
                .unwrap()
                .validate_binding(&sealed.record.after)
                .unwrap();
        } else {
            assert!(matches!(
                accepted,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ));
        }
    }
}

#[test]
fn normal_rows_preserve_burned_ordinals_duplicate_ids_and_cargo_holes() {
    let memory = MemoryBudget::new(96 << 20, 8 << 20).unwrap();
    let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
    let (pages, roots) = fixture::seed(&memory);
    let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
    let extra = RefCell::new(Pages::default());
    let values = Values {
        base: &pages,
        extra: &extra,
    };
    let state = fixture::State::new(&pages, &memory);
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [11; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 64,
            ..limits()
        },
    )
    .unwrap();
    let mut read_io = io();
    let mut write_io = io();
    let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut sink = Sink(&extra);
    use crate::agent_state::{BodyModuleSlot, BodySlotType, CargoEntityEntry, CargoEntityKind};
    let slot = BodyModuleSlot {
        slot_id: " repeated original ".into(),
        slot_type: BodySlotType::Universal,
        installed_module: Some("".into()),
        locked: true,
    };
    let original_cargo = CargoEntityEntry {
        entity_id: " repeated original ".into(),
        entity_kind: CargoEntityKind::Mineral,
        quantity: i64::MIN,
        size_per_unit: 0,
    };
    let cargo = CargoEntityEntry {
        entity_id: " repeated original ".into(),
        entity_kind: CargoEntityKind::InterfaceModuleItem,
        quantity: i64::MAX,
        size_per_unit: i64::MIN,
    };
    use AgentSubtreeRowRefV1 as R;
    for (ordinal, count, next, expected, after) in [
        (2, 2, 2, None, Some(R::Slot(&slot))),
        (2, 3, 3, Some(R::Slot(&slot)), None),
        (3, 2, 3, None, Some(R::Slot(&slot))),
        (1, 1, 1, None, Some(R::Cargo(&cargo))),
        (0, 2, 2, Some(R::Cargo(&original_cargo)), None),
        (2, 1, 2, None, Some(R::Cargo(&cargo))),
    ] {
        let guard = tx.guard();
        assert!(
            change_agent_subtree_row_v1(
                AgentSubtreeRowChangeV1 {
                    agent: "agent α",
                    qualification: &proof,
                    ordinal,
                    expected_count: count,
                    expected_next: next,
                    expected,
                    after,
                    max_scalar_bytes: 1 << 20,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || false
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || false
                },
                &guard,
                &live
            )
            .unwrap()
        );
    }
    for (field, ordinal, expected) in [
        (
            AgentFieldV1::SlotCount,
            0,
            Some(AgentSubtreeValueV1::U64(3)),
        ),
        (AgentFieldV1::SlotNext, 0, Some(AgentSubtreeValueV1::U64(4))),
        (AgentFieldV1::SlotId, 2, None),
        (AgentFieldV1::CargoId, 0, None),
        (
            AgentFieldV1::CargoCount,
            0,
            Some(AgentSubtreeValueV1::U64(2)),
        ),
        (
            AgentFieldV1::CargoNext,
            0,
            Some(AgentSubtreeValueV1::U64(3)),
        ),
    ] {
        let guard = tx.guard();
        assert_eq!(
            read_agent_subtree_scalar_overlay_v1(
                AgentSubtreeQualifiedRequestV1 {
                    cell: request("agent α", field, ordinal),
                    qualification: &proof,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || false
                ),
                1 << 20,
                &mut io::sink(),
                &guard,
                &live
            )
            .unwrap(),
            expected
        );
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [11; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let accepted = accept_agent_subtree_transition_v1(
        &proof,
        &verifier,
        &sealed.record,
        &mut crate::incremental_field_read::WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 1024,
            record_bytes: 1 << 20,
            take_state_error: || state.take(),
            cancelled: || false,
        },
        &mut AgentSubtreeTransitionWorkV1 {
            cells_remaining: 100,
            max_scalar_bytes: 1 << 20,
        },
    )
    .unwrap();
    accepted.validate_binding(&sealed.record.after).unwrap();
}

#[path = "incremental_agent_subtree_row_noop_tests.rs"]
mod row_noop;

#[path = "incremental_agent_subtree_create_tests.rs"]
mod create_tests;

#[path = "incremental_agent_subtree_delete_tests.rs"]
mod delete_tests;

#[path = "incremental_agent_subtree_mailbox_tests.rs"]
mod mailbox_tests;

#[path = "incremental_agent_subtree_source_tests.rs"]
mod source_tests;
