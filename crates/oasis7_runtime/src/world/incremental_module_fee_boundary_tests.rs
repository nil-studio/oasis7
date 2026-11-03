use super::*;
use crate::world::incremental_module_fee_contribution::{
    RootModuleFeeWriteContextV1, apply_charge, contribute_root_module_fee_v1,
};
use oasis7_world_state::{
    incremental_agent_fields::{AgentFieldKeyV1, AgentScalarRefV1},
    incremental_agent_subtree::{AgentSubtreeCellChangeV1, change_agent_subtree_scalar_v1},
};
fn event(mode: u8) -> ModuleRuntimeChargeEvent {
    ModuleRuntimeChargeEvent {
        module_id: "fee-module".into(),
        trace_id: "fee-trace".into(),
        payer_agent_id: "payer".into(),
        compute_fee_kind: ResourceKind::Data,
        compute_fee_amount: if mode == 1 {
            0
        } else if mode == 3 {
            -1
        } else {
            3
        },
        electricity_fee_kind: ResourceKind::Data,
        electricity_fee_amount: if mode == 1 {
            0
        } else if mode == 2 {
            18
        } else {
            4
        },
        input_bytes: 0,
        output_bytes: 0,
        effect_count: 0,
        emit_count: 0,
    }
}
fn assert_body<M: MerkleNodeReader, E: FnMut() -> Option<ValueTreeError>, C: FnMut() -> bool>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, Values, E, C>,
    want: u64,
) {
    use oasis7_world_state::incremental_agent_body::{AgentBodyKeyV1, AgentBodyValueV1};
    let key = AgentBodyKeyV1::new("payer").unwrap();
    let id = ctx
        .transaction
        .read(StateNamespaceV1::Business, key.as_bytes())
        .unwrap()
        .unwrap();
    let root = ctx.values.load_root(&id).unwrap();
    let mut bytes = Vec::new();
    ctx.values
        .read_range(&root, 0, root.0.logical_len, &mut bytes, &mut ctx.cancelled)
        .unwrap();
    assert_eq!(
        AgentBodyValueV1::from_canonical_bytes(&bytes)
            .unwrap()
            .last_active,
        want
    );
}
#[test]
fn typed_same_kind_zero_and_rejected_event_keep_original_world_results() {
    for mode in 0..4 {
        let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
        let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let mut original = world();
        if mode == 1 {
            original.state.resources.clear();
            original
                .state
                .agents
                .get_mut("payer")
                .unwrap()
                .state
                .resources
                .amounts
                .clear();
        }
        let charge = event(mode);
        let oracle = original.prepare_module_runtime_charge_event(&charge, 12);
        let (parent, pages, values) = seed(&original, &memory);
        let agents = qualify(&parent, &pages, &values, &memory);
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [20 + mode; 32],
            &pages,
            &memory,
            txn_limits(),
        )
        .unwrap();
        let mut sink = values.clone();
        let mut read_io = io();
        let mut read_keys = keys();
        let mut write_io = io();
        let mut write_keys = keys();
        if mode >= 2 {
            let guard = tx.guard();
            change_agent_subtree_scalar_v1(
                AgentSubtreeCellChangeV1 {
                    query: AgentSubtreeQualifiedRequestV1 {
                        cell: AgentSubtreeScalarRequestV1 {
                            agent: "payer",
                            field: AgentFieldV1::ResourceAmount,
                            ordinal: 0,
                        },
                        qualification: &agents,
                    },
                    expected: Some(AgentScalarRefV1::I64(20)),
                    after: Some(AgentScalarRefV1::I64(19)),
                    max_scalar_bytes: 128,
                },
                &mut overlay!(
                    &mut tx,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut read_keys,
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
            .unwrap();
        }
        let result = with_overlay(
            &mut overlay!(
                &mut tx,
                &values,
                &memory,
                &mut read_io,
                &mut read_keys,
                || false
            ),
            |ctx| {
                apply_charge(
                    &charge,
                    &agents,
                    12,
                    ctx,
                    &mut ValueMutationContextV1 {
                        sink: &mut sink,
                        memory: &memory,
                        io: &mut write_io,
                        cancelled: || false,
                    },
                    &mut write_keys,
                    member_limits(),
                )
            },
        );
        if mode >= 2 {
            assert!(matches!(
                result,
                Err(RootModuleFeeErrorV1::World(
                    crate::WorldError::ResourceBalanceInvalid { .. }
                ))
            ));
            assert!(oracle.is_err());
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 0),
                Err(ExecutionError::Aborted)
            ));
        } else {
            result.unwrap();
            oracle.unwrap().install_infallible(&mut original);
            let mut ctx = overlay!(
                &mut tx,
                &values,
                &memory,
                &mut read_io,
                &mut read_keys,
                || false
            );
            assert_eq!(
                resource("payer", ResourceKind::Data, &agents, &mut ctx).unwrap(),
                if mode == 0 { Some(13) } else { None }
            );
            assert_eq!(
                ctx.resource(ResourceKind::Data).unwrap(),
                if mode == 0 { Some(i64::MAX) } else { None }
            );
            assert_eq!(
                ctx.resource(ResourceKind::Electricity).unwrap(),
                if mode == 0 { Some(-2) } else { None }
            );
            assert_body(&mut ctx, original.state().agents["payer"].last_active);
            let guard = tx.guard();
            tx.seal(&guard, &live, 0).unwrap();
        }
    }
}
struct ClassState<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
    class: MemoryClass,
}
impl MerkleNodeReader for ClassState<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        self.pages
            .nodes
            .get(id)
            .map(|bytes| {
                if bytes.len() > max {
                    return Err("class state bound".into());
                }
                Ok(Node(
                    self.memory
                        .reserve(bytes.len(), self.class)
                        .map_err(|e| e.to_string())?
                        .retain(bytes.clone()),
                ))
            })
            .transpose()
    }
}
struct ClassValues<'a>(&'a Values);
impl ValueNodeReader for ClassValues<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let bytes = self
            .0
            .pages
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("class value missing"))?;
        if bytes.len() > max {
            return Err(ValueTreeError::Invalid("class value bound"));
        }
        Ok(Arc::new(
            memory.reserve(bytes.len(), class)?.retain(bytes.to_vec()),
        ))
    }
}
fn class_proof(
    parent: &ExecutionParent,
    pages: &Pages,
    values: &Values,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> ReservedValue<AgentSubtreeStructureQualificationV1> {
    let state = ClassState {
        pages,
        memory,
        class,
    };
    let values = ClassValues(values);
    qualify_agent_subtree_structure_v1(
        &parent.roots,
        &mut WorldFieldReadContextV1 {
            root: &parent.roots.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut io(),
            },
            key_work: &mut keys(),
            semantic_bytes: 1024,
            record_bytes: 1 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 128 << 20,
            leaves_remaining: 10000,
        },
        &mut AgentSubtreeScanWorkV1 {
            max_agents: 4,
            cells_remaining: 10000,
            max_scalar_bytes: 64 << 20,
        },
        &mut |_| Err(ValueTreeError::Invalid("no mailbox")),
    )
    .unwrap()
}
#[test]
fn fee_rejects_recovery_and_foreign_agent_qualifications_after_real_prefix() {
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let original = world();
    let manifest = manifest();
    let output = output();
    let (parent, pages, values) = seed(&original, &memory);
    let agents = qualify(&parent, &pages, &values, &memory);
    let recovery = class_proof(&parent, &pages, &values, &memory, MemoryClass::Recovery);
    let foreign_memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let foreign = class_proof(
        &parent,
        &pages,
        &values,
        &foreign_memory,
        MemoryClass::Ordinary,
    );
    let live = parent.fence().unwrap();
    for proof in [&recovery, &foreign] {
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [30; 32], &pages, &memory, txn_limits())
                .unwrap();
        let mut sink = values.clone();
        let mut read_io = io();
        let mut read_keys = keys();
        prefix(
            &agents,
            &mut overlay!(
                &mut tx,
                &values,
                &memory,
                &mut read_io,
                &mut read_keys,
                || false
            ),
            &mut sink,
            &memory,
        );
        let guard = tx.guard();
        let result = prepare_root_module_fee_v1(
            request(&parent, proof, &guard, &live, &manifest),
            &output,
            &mut overlay!(
                &mut tx,
                &values,
                &memory,
                &mut read_io,
                &mut read_keys,
                || false
            ),
        );
        assert!(matches!(
            result,
            Err(RootModuleFeeErrorV1::Overlay(
                WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_))
            ))
        ));
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
}
#[test]
fn zero_compute_fee_preserves_present_zero_and_absent_treasury() {
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let mut state = world().state().clone();
    state.resources.clear();
    state
        .agents
        .get_mut("payer")
        .unwrap()
        .state
        .resources
        .amounts
        .insert(ResourceKind::Data, 0);
    state
        .agents
        .get_mut("payer")
        .unwrap()
        .state
        .resources
        .amounts
        .insert(ResourceKind::Electricity, 1);
    let original = World::new_with_state(state);
    let manifest = manifest();
    let output = ModuleOutput {
        new_state: None,
        effects: vec![],
        emits: vec![],
        tick_lifecycle: None,
        output_bytes: 0,
    };
    let (parent, pages, values) = seed(&original, &memory);
    let agents = qualify(&parent, &pages, &values, &memory);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [31; 32], &pages, &memory, txn_limits())
            .unwrap();
    let mut read_io = io();
    let mut read_keys = keys();
    let guard = tx.guard();
    let mut query = request(&parent, &agents, &guard, &live, &manifest);
    query.input_bytes = 0;
    let RootModuleFeeOutcomeV1::Prepared(prepared) = prepare_root_module_fee_v1(
        query,
        &output,
        &mut overlay!(
            &mut tx,
            &values,
            &memory,
            &mut read_io,
            &mut read_keys,
            || false
        ),
    )
    .unwrap() else {
        panic!("charged")
    };
    assert_eq!(prepared.event().compute_fee_amount, 0);
    assert_eq!(prepared.event().electricity_fee_amount, 1);
    let mut sink = values.clone();
    contribute_root_module_fee_v1(
        &prepared,
        &agents,
        12,
        &mut overlay!(
            &mut tx,
            &values,
            &memory,
            &mut read_io,
            &mut read_keys,
            || false
        ),
        &mut RootModuleFeeWriteContextV1 {
            values: ValueMutationContextV1 {
                sink: &mut sink,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
            key_work: &mut keys(),
            member_limits: member_limits(),
        },
        &live,
    )
    .unwrap();
    let mut ctx = overlay!(
        &mut tx,
        &values,
        &memory,
        &mut read_io,
        &mut read_keys,
        || false
    );
    assert_eq!(
        resource("payer", ResourceKind::Data, &agents, &mut ctx).unwrap(),
        Some(0)
    );
    assert_eq!(
        resource("payer", ResourceKind::Electricity, &agents, &mut ctx).unwrap(),
        None
    );
    assert_eq!(ctx.resource(ResourceKind::Data).unwrap(), None);
    assert_eq!(ctx.resource(ResourceKind::Electricity).unwrap(), Some(1));
    assert_body(&mut ctx, 12);
}
#[test]
fn ordinary_fee_scope_does_not_read_unmodified_body_kind_at_three_sizes() {
    let mut expected = None;
    for length in [64usize, 1 << 20, 32 << 20] {
        let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
        let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let input_memory = MemoryBudget::new(64 << 20, 0).unwrap();
        let input = input_memory
            .reserve(length + 4096, MemoryClass::Ordinary)
            .unwrap();
        let mut original = world();
        original
            .state
            .agents
            .get_mut("payer")
            .unwrap()
            .state
            .body
            .kind = "x".repeat(length);
        let (parent, pages, values) = seed(&original, &memory);
        let agents = qualify(&parent, &pages, &values, &memory);
        let sibling = AgentFieldKeyV1::new("payer", AgentFieldV1::BodyKind, 0).unwrap();
        let sibling = parent
            .roots
            .state_root(StateNamespaceV1::Business)
            .get(sibling.as_bytes(), &pages)
            .unwrap()
            .unwrap();
        drop(original);
        drop(input);
        values.forbidden.set(Some(sibling));
        values.reads.set(0);
        pages.reads.set(0);
        let manifest = manifest();
        let output = output();
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [32; 32], &pages, &memory, txn_limits())
                .unwrap();
        let mut read_io = io();
        let mut read_keys = keys();
        let guard = tx.guard();
        let RootModuleFeeOutcomeV1::Prepared(prepared) = prepare_root_module_fee_v1(
            request(&parent, &agents, &guard, &live, &manifest),
            &output,
            &mut overlay!(
                &mut tx,
                &values,
                &memory,
                &mut read_io,
                &mut read_keys,
                || false
            ),
        )
        .unwrap() else {
            panic!("charged")
        };
        let mut sink = values.clone();
        let mut write_io = io();
        contribute_root_module_fee_v1(
            &prepared,
            &agents,
            12,
            &mut overlay!(
                &mut tx,
                &values,
                &memory,
                &mut read_io,
                &mut read_keys,
                || false
            ),
            &mut RootModuleFeeWriteContextV1 {
                values: ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || false,
                },
                key_work: &mut keys(),
                member_limits: member_limits(),
            },
            &live,
        )
        .unwrap();
        let counts = (
            pages.reads.get(),
            values.reads.get(),
            read_io.read_bytes_remaining,
            write_io.read_bytes_remaining,
            write_io.write_bytes_remaining,
        );
        if let Some(expected) = expected {
            assert_eq!(counts, expected);
        } else {
            expected = Some(counts);
        }
        let guard = tx.guard();
        tx.seal(&guard, &live, 0).unwrap();
    }
}
