use super::*;
use crate::agent_state::AgentState;
use crate::world::{
    incremental_agent_genesis::AgentGenesisContext, incremental_body::BodyMutationContext,
};
use crate::{AgentCell, World, WorldState};
use oasis7_agent_api::world_action::GeoPos;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
        TransitionVerifier,
    },
    incremental_state::{StateDeltaV1, StateScanBudget, StateWriteV1},
    incremental_value::{ValueIoBudget, ValueNodeWriter},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_world_state::{
    incremental_agent_subtree::{AgentSubtreeScanWorkV1, qualify_agent_subtree_structure_v1},
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_genesis::{
        InventoryGenesisContext, InventoryWriteSink, prepare_inventory_genesis_v1,
    },
    incremental_semantic_key::SemanticKeyBudget,
    incremental_value_mutation::ValueMutationContextV1,
    incremental_world_text_maps::prepare_world_text_maps_genesis_v1,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};
type ValuePages = Rc<RefCell<BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>>>;
#[derive(Clone)]
struct Values {
    pages: ValuePages,
    forbidden: Rc<Cell<Option<[u8; 32]>>>,
    reads: Rc<Cell<u64>>,
}
impl Values {
    fn new() -> Self {
        Self {
            pages: Rc::new(RefCell::new(BTreeMap::new())),
            forbidden: Rc::new(Cell::new(None)),
            reads: Rc::new(Cell::new(0)),
        }
    }
}
impl ValueNodeWriter for Values {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.pages.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for Values {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        _: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.reads.set(self.reads.get() + 1);
        if self.forbidden.get() == Some(*id) {
            return Err(ValueTreeError::Invalid("unmodified fee sibling touched"));
        }
        let bytes = self
            .pages
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("fee test missing value"))?;
        if bytes.len() > max || !bytes.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("fee test value bound/pool"));
        }
        Ok(bytes)
    }
}
struct Pages {
    memory: MemoryBudget,
    nodes: BTreeMap<String, Vec<u8>>,
    reads: Cell<u64>,
    cancel_after: Cell<Option<u64>>,
    cancelled: Cell<bool>,
    fail_after: Cell<Option<u64>>,
    error: RefCell<Option<ValueTreeError>>,
}
struct Node(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl MerkleNodeReader for Pages {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let count = self.reads.get() + 1;
        self.reads.set(count);
        if self.fail_after.get() == Some(count) {
            *self.error.borrow_mut() = Some(ValueTreeError::Io(std::io::Error::other(
                "actual fee state read",
            )));
            return Err("actual fee state read".into());
        }
        let result = self
            .nodes
            .get(id)
            .map(|bytes| {
                if bytes.len() > max {
                    return Err("fee state bound".into());
                }
                Ok(Node(
                    self.memory
                        .reserve(bytes.len(), MemoryClass::Ordinary)
                        .map_err(|e| e.to_string())?
                        .retain(bytes.clone()),
                ))
            })
            .transpose();
        if self.cancel_after.get() == Some(count) {
            self.cancelled.set(true);
        }
        result
    }
}
struct Rows<'a>(&'a mut Vec<(StateNamespaceV1, Vec<u8>, [u8; 32])>);
impl InventoryWriteSink for Rows<'_> {
    fn put_write(&mut self, write: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        self.0.push((
            StateNamespaceV1::Business,
            write.key.clone(),
            write.after.unwrap(),
        ));
        Ok(())
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 128 << 20,
        write_bytes_remaining: 128 << 20,
    }
}
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    }
}
fn member_limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 1024,
        payload_bytes: 1 << 20,
        record_bytes: 1 << 20,
    }
}
fn txn_limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 256,
        max_writes: 64,
        max_facts: 16,
        max_pending_node_bytes: 8 << 20,
    }
}
fn manifest() -> ModuleManifest {
    serde_json::from_value(serde_json::json!({"module_id":"fee-module","name":"fee","version":"1","kind":"reducer","wasm_hash":"fee-hash","interface_version":"1","limits":{"max_output_bytes":1025,"max_effects":2,"max_emits":1}})).unwrap()
}
fn output() -> ModuleOutput {
    ModuleOutput {
        new_state: Some(vec![1]),
        effects: vec![],
        emits: vec![],
        tick_lifecycle: None,
        output_bytes: 1025,
    }
}
fn world() -> World {
    let mut state = WorldState {
        time: 12,
        ..Default::default()
    };
    let mut cell = AgentCell::new(
        AgentState::new(
            "payer",
            GeoPos {
                x_cm: 0,
                y_cm: 0,
                z_cm: 0,
            },
        ),
        1,
    );
    cell.state.resources.amounts.insert(ResourceKind::Data, 20);
    cell.state
        .resources
        .amounts
        .insert(ResourceKind::Electricity, 20);
    state.agents.insert("payer".into(), cell);
    state
        .module_artifact_owners
        .insert("fee-hash".into(), "payer".into());
    state.resources.insert(ResourceKind::Data, i64::MAX - 1);
    state.resources.insert(ResourceKind::Electricity, -2);
    World::new_with_state(state)
}
fn seed(world: &World, memory: &MemoryBudget) -> (ExecutionParent, Pages, Values) {
    let mut values = Values::new();
    let mut rows = vec![];
    for agent in world.state().agents.keys() {
        world
            .write_iws_agent_genesis_fields(
                agent,
                &mut AgentGenesisContext {
                    mutation: BodyMutationContext {
                        sink: &mut values,
                        memory,
                        io: &mut io(),
                        cancelled: || false,
                    },
                    emit: |ns, key: &[u8], id| {
                        rows.push((ns, key.to_vec(), id));
                        Ok(())
                    },
                    mailbox_original: |_: u64, _: &oasis7_world_state::DomainEvent| {
                        Err(crate::BodyMutationError::Invalid("no mailbox expected"))
                    },
                    max_scalar_bytes: 64 << 20,
                },
            )
            .unwrap();
    }
    world
        .write_iws_clock_genesis(
            &mut BodyMutationContext {
                sink: &mut values,
                memory,
                io: &mut io(),
                cancelled: || false,
            },
            |key, id| {
                rows.push((StateNamespaceV1::Allocator, key.to_vec(), id));
                Ok(())
            },
        )
        .unwrap();
    prepare_inventory_genesis_v1(
        world.state(),
        &mut InventoryGenesisContext {
            values: &mut values,
            writes: &mut Rows(&mut rows),
            memory,
            io: &mut io(),
            key_work: &mut keys(),
            semantic_bytes: 1024,
            record_bytes: 1 << 20,
            records_remaining: 1000,
            cancelled: || false,
        },
    )
    .unwrap();
    prepare_world_text_maps_genesis_v1(
        world.state(),
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut io(),
            key_work: &mut keys(),
            limits: member_limits(),
            cancelled: || false,
        },
        &mut 1000,
        &mut |write| {
            rows.push((
                write.namespace,
                write.mutation.key.clone(),
                write.mutation.after.unwrap(),
            ));
            Ok(())
        },
    )
    .unwrap();
    let mut roots = RootSetV1::empty("fee-world".into(), [2; 32]).unwrap();
    let mut pages = Pages {
        memory: memory.clone(),
        nodes: BTreeMap::new(),
        reads: Cell::new(0),
        cancel_after: Cell::new(None),
        cancelled: Cell::new(false),
        fail_after: Cell::new(None),
        error: RefCell::new(None),
    };
    for root in &mut roots.state {
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|(ns, _, _)| *ns == root.namespace)
            .map(|(_, key, id)| StateWriteV1 {
                key: key.clone(),
                before: None,
                after: Some(*id),
            })
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = StateDeltaV1::build(root.clone(), writes, &pages, 8 << 20).unwrap();
        *root = delta.after;
        for node in delta.new_nodes {
            pages.nodes.insert(node.digest, node.bytes);
        }
    }
    (
        ExecutionParent {
            roots,
            commit: [1; 32],
            authority: AuthorityBinding {
                token: [3; 32],
                epoch: 0,
            },
        },
        pages,
        values,
    )
}
fn qualify(
    parent: &ExecutionParent,
    pages: &Pages,
    values: &Values,
    memory: &MemoryBudget,
) -> ReservedValue<AgentSubtreeStructureQualificationV1> {
    qualify_agent_subtree_structure_v1(
        &parent.roots,
        &mut WorldFieldReadContextV1 {
            root: &parent.roots.state[0],
            state: pages,
            values: ValueReadContext {
                source: values,
                memory,
                class: MemoryClass::Ordinary,
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
macro_rules! overlay {
    ($tx:expr,$values:expr,$memory:expr,$io:expr,$keys:expr,$cancel:expr) => {
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
            take_state_error: || None,
            cancelled: $cancel,
        }
    };
}
fn request<'a>(
    parent: &'a ExecutionParent,
    agents: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    guard: &'a PreparedGuard,
    live: &'a ExecutionFence,
    manifest: &'a ModuleManifest,
) -> RootModuleFeeRequestV1<'a> {
    RootModuleFeeRequestV1 {
        parent_roots: &parent.roots,
        agents,
        guard,
        live,
        module_id: "fee-module",
        trace_id: "fee-trace",
        manifest,
        input_bytes: 1,
        max_owner_bytes: 1024,
    }
}
#[test]
fn root_fee_matches_literal_amounts_and_original_world_result() {
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let world = world();
    let manifest = manifest();
    let output = output();
    let (parent, pages, values) = seed(&world, &memory);
    let agents = qualify(&parent, &pages, &values, &memory);
    let live = parent.fence().unwrap();
    let operation = [7; 32];
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), operation, &pages, &memory, txn_limits())
            .unwrap();
    let mut read_io = io();
    let mut read_keys = keys();
    let guard = tx.guard();
    let prepared = prepare_root_module_fee_v1(
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
    .unwrap();
    let RootModuleFeeOutcomeV1::Prepared(prepared) = prepared else {
        panic!("expected charged")
    };
    assert_eq!(prepared.event().compute_fee_amount, 3);
    assert_eq!(prepared.event().electricity_fee_amount, 2);
    let mut oracle = World::new_with_state(world.state().clone());
    oracle
        .apply_module_runtime_charge_event(prepared.event(), 12)
        .unwrap();
    assert_eq!(
        oracle.state().agents["payer"]
            .state
            .resources
            .get(ResourceKind::Data),
        17
    );
    assert_eq!(
        oracle.state().agents["payer"]
            .state
            .resources
            .get(ResourceKind::Electricity),
        18
    );
    assert_eq!(oracle.state().resources[&ResourceKind::Data], i64::MAX);
    assert_eq!(oracle.state().resources[&ResourceKind::Electricity], 0);
    let mut sink = values.clone();
    let mut write_io = io();
    let mut write_keys = keys();
    super::super::incremental_module_fee_contribution::contribute_root_module_fee_v1(
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
        &mut super::super::incremental_module_fee_contribution::RootModuleFeeWriteContextV1 {
            values: ValueMutationContextV1 {
                sink: &mut sink,
                memory: &memory,
                io: &mut write_io,
                cancelled: || false,
            },
            key_work: &mut write_keys,
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
        Some(17)
    );
    assert_eq!(
        resource("payer", ResourceKind::Electricity, &agents, &mut ctx).unwrap(),
        Some(18)
    );
    assert_eq!(ctx.resource(ResourceKind::Data).unwrap(), Some(i64::MAX));
    assert_eq!(ctx.resource(ResourceKind::Electricity).unwrap(), Some(0));
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 0).unwrap();
    TransitionVerifier {
        parent: &parent,
        operation,
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    }
    .verify(&sealed.record)
    .unwrap();
}

fn prefix<M: MerkleNodeReader, E: FnMut() -> Option<ValueTreeError>, C: FnMut() -> bool>(
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, Values, E, C>,
    sink: &mut Values,
    memory: &MemoryBudget,
) {
    use oasis7_world_state::incremental_agent_fields::AgentScalarRefV1;
    use oasis7_world_state::incremental_agent_subtree::{
        AgentSubtreeCellChangeV1, change_agent_subtree_scalar_v1,
    };
    let guard = ctx.transaction.guard();
    let live = ctx.transaction.parent_fence();
    assert!(
        change_agent_subtree_scalar_v1(
            AgentSubtreeCellChangeV1 {
                query: AgentSubtreeQualifiedRequestV1 {
                    cell: AgentSubtreeScalarRequestV1 {
                        agent: "payer",
                        field: AgentFieldV1::ResourceAmount,
                        ordinal: 1
                    },
                    qualification: agents
                },
                expected: Some(AgentScalarRefV1::I64(20)),
                after: Some(AgentScalarRefV1::I64(19)),
                max_scalar_bytes: 128
            },
            ctx,
            &mut ValueMutationContextV1 {
                sink,
                memory,
                io: &mut io(),
                cancelled: || false
            },
            &guard,
            &live
        )
        .unwrap()
    );
}
#[test]
fn free_and_denied_outcomes_preserve_original_world_semantics() {
    for mode in 0..4 {
        let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
        let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let mut state = world().state().clone();
        match mode {
            0 => {
                state.module_artifact_owners.clear();
            }
            1 => {
                state.agents.clear();
            }
            2 => {
                state
                    .agents
                    .get_mut("payer")
                    .unwrap()
                    .state
                    .resources
                    .amounts
                    .insert(ResourceKind::Data, 2);
            }
            _ => {
                state
                    .agents
                    .get_mut("payer")
                    .unwrap()
                    .state
                    .resources
                    .amounts
                    .insert(ResourceKind::Data, 7);
            }
        }
        let world = World::new_with_state(state);
        let manifest = manifest();
        let output = output();
        let (parent, pages, values) = seed(&world, &memory);
        let agents = qualify(&parent, &pages, &values, &memory);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [8; 32], &pages, &memory, txn_limits())
                .unwrap();
        let mut read_io = io();
        let mut read_keys = keys();
        if mode == 3 {
            let guard = tx.guard();
            let result = check_root_module_fee_resources_v1(
                request(&parent, &agents, &guard, &live, &manifest),
                &mut overlay!(
                    &mut tx,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut read_keys,
                    || false
                ),
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                &*result,
                &world
                    .check_module_runtime_resources("fee-module", "fee-trace", &manifest, 1)
                    .unwrap_err()
            );
            assert_eq!(
                result.detail,
                "module runtime upper-bound fee insufficient payer=payer data 7/8 electricity 20/5"
            );
        } else {
            let guard = tx.guard();
            let result = prepare_root_module_fee_v1(
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
            .unwrap();
            if mode < 2 {
                assert!(matches!(result, RootModuleFeeOutcomeV1::Free));
                assert!(
                    World::new_with_state(world.state().clone())
                        .try_charge_module_runtime("fee-module", "fee-trace", &manifest, 1, &output)
                        .is_ok()
                );
            } else {
                let RootModuleFeeOutcomeV1::Denied(failure) = result else {
                    panic!("expected denied")
                };
                assert_eq!(
                    &*failure,
                    &World::new_with_state(world.state().clone())
                        .try_charge_module_runtime("fee-module", "fee-trace", &manifest, 1, &output)
                        .unwrap_err()
                );
                assert_eq!(
                    failure.detail,
                    "module runtime fee insufficient payer=payer data 2/3 electricity 20/2"
                );
            }
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 0).unwrap();
        assert!(sealed.record.writes.is_empty());
    }
}
#[test]
fn absent_owner_last_real_read_cancellation_aborts_a_successful_prefix() {
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let mut state = world().state().clone();
    state.module_artifact_owners.clear();
    let world = World::new_with_state(state);
    let manifest = manifest();
    let output = output();
    let (parent, pages, values) = seed(&world, &memory);
    let agents = qualify(&parent, &pages, &values, &memory);
    let live = parent.fence().unwrap();
    let mut baseline = 0;
    for cancel in [false, true] {
        pages.cancelled.set(false);
        pages.cancel_after.set(None);
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [9; 32], &pages, &memory, txn_limits())
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
        pages.reads.set(0);
        if cancel {
            pages.cancel_after.set(Some(baseline));
        }
        let guard = tx.guard();
        let result = prepare_root_module_fee_v1(
            request(&parent, &agents, &guard, &live, &manifest),
            &output,
            &mut overlay!(
                &mut tx,
                &values,
                &memory,
                &mut read_io,
                &mut read_keys,
                || pages.cancelled.get()
            ),
        );
        if cancel {
            assert!(pages.cancelled.get());
            assert!(matches!(
                result,
                Err(RootModuleFeeErrorV1::Overlay(
                    WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled)
                ))
            ));
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 0),
                Err(ExecutionError::Aborted)
            ));
        } else {
            assert!(matches!(result, Ok(RootModuleFeeOutcomeV1::Free)));
            baseline = pages.reads.get();
            assert!(baseline > 0);
        }
    }
}
#[test]
fn private_prepared_guard_cannot_be_replaced_with_a_fresh_caller_guard() {
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let world = world();
    let manifest = manifest();
    let output = output();
    let (parent, pages, values) = seed(&world, &memory);
    let agents = qualify(&parent, &pages, &values, &memory);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [10; 32], &pages, &memory, txn_limits())
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
    assert_eq!(
        overlay!(
            &mut tx,
            &values,
            &memory,
            &mut read_io,
            &mut read_keys,
            || false
        )
        .resource(ResourceKind::Data)
        .unwrap(),
        Some(i64::MAX - 1)
    );
    let mut sink = values.clone();
    let result = super::super::incremental_module_fee_contribution::contribute_root_module_fee_v1(
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
        &mut super::super::incremental_module_fee_contribution::RootModuleFeeWriteContextV1 {
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
    );
    assert!(matches!(
        result,
        Err(RootModuleFeeErrorV1::Overlay(
            WorldFieldOverlayErrorV1::Execution(_)
        ))
    ));
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 0),
        Err(ExecutionError::Aborted)
    ));
}
struct FailWriter {
    inner: Values,
    puts: usize,
}
impl ValueNodeWriter for FailWriter {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.puts += 1;
        if self.puts == 2 {
            return Err(ValueTreeError::Io(std::io::Error::other(
                "actual fee page put",
            )));
        }
        self.inner.put_node(id, value)
    }
}
#[test]
fn actual_source_and_put_io_after_successful_prefix_remain_storage_failures() {
    for source_fault in [false, true] {
        let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
        let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let world = world();
        let manifest = manifest();
        let output = output();
        let (parent, pages, values) = seed(&world, &memory);
        let agents = qualify(&parent, &pages, &values, &memory);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [11; 32], &pages, &memory, txn_limits())
                .unwrap();
        let mut read_io = io();
        let mut read_keys = keys();
        let mut sink = values.clone();
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
        // Preload treasury so the next actual Merkle read belongs to clock
        // checking, whose adapter preserves its typed source error separately.
        if source_fault {
            let mut ctx = overlay!(
                &mut tx,
                &values,
                &memory,
                &mut read_io,
                &mut read_keys,
                || false
            );
            ctx.resource(ResourceKind::Data).unwrap();
            ctx.resource(ResourceKind::Electricity).unwrap();
        }
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
        if source_fault {
            pages.fail_after.set(Some(pages.reads.get() + 1));
        }
        let mut writer = FailWriter {
            inner: values.clone(),
            puts: 0,
        };
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 1024,
            record_bytes: 1 << 20,
            take_state_error: || pages.error.borrow_mut().take(),
            cancelled: || false,
        };
        let result = if source_fault {
            super::super::incremental_module_fee_contribution::contribute_root_module_fee_v1(&prepared,&agents,12,&mut ctx,&mut super::super::incremental_module_fee_contribution::RootModuleFeeWriteContextV1{values:ValueMutationContextV1{sink:&mut sink,memory:&memory,io:&mut io(),cancelled:||false},key_work:&mut keys(),member_limits:member_limits()},&live)
        } else {
            super::super::incremental_module_fee_contribution::contribute_root_module_fee_v1(&prepared,&agents,12,&mut ctx,&mut super::super::incremental_module_fee_contribution::RootModuleFeeWriteContextV1{values:ValueMutationContextV1{sink:&mut writer,memory:&memory,io:&mut io(),cancelled:||false},key_work:&mut keys(),member_limits:member_limits()},&live)
        };
        assert!(
            matches!(
                result,
                Err(RootModuleFeeErrorV1::Overlay(
                    WorldFieldOverlayErrorV1::Value(ValueTreeError::Io(_))
                ))
            ),
            "{result:?}"
        );
        if !source_fault {
            assert_eq!(writer.puts, 2);
        }
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
}
#[test]
fn shared_fee_core_keeps_literal_boundaries_and_original_saturating_ceil() {
    for (bytes, want) in [
        (0, 0),
        (1, 1),
        (1023, 1),
        (1024, 1),
        (1025, 2),
        (u64::MAX, 18_014_398_509_481_983),
    ] {
        let out = ModuleOutput {
            new_state: None,
            effects: vec![],
            emits: vec![],
            tick_lifecycle: None,
            output_bytes: 0,
        };
        let got = core::actual(bytes, &out);
        assert_eq!(got.compute, want);
        assert_eq!(got.electricity, 1);
    }
    let limits = crate::ModuleLimits {
        max_output_bytes: u64::MAX,
        max_effects: u32::MAX,
        max_emits: u32::MAX,
        ..Default::default()
    };
    let got = core::upper_bound(&limits, u64::MAX);
    assert_eq!(got.compute, 36_028_809_903_865_851);
    assert_eq!(got.electricity, 8_589_934_592);
}
#[test]
fn same_kind_sequential_fees_zero_missing_and_second_debit_failure_match_world() {
    for mode in 0..4 {
        let mut world = world();
        let mut charge = crate::ModuleRuntimeChargeEvent {
            module_id: "fee-module".into(),
            trace_id: "fee-trace".into(),
            payer_agent_id: "payer".into(),
            compute_fee_kind: ResourceKind::Data,
            compute_fee_amount: 3,
            electricity_fee_kind: ResourceKind::Data,
            electricity_fee_amount: 4,
            input_bytes: 0,
            output_bytes: 0,
            effect_count: 0,
            emit_count: 0,
        };
        if mode == 1 {
            charge.compute_fee_amount = 0;
            charge.electricity_fee_amount = 0;
            world.state.resources.clear();
            world
                .state
                .agents
                .get_mut("payer")
                .unwrap()
                .state
                .resources
                .amounts
                .clear();
        }
        if mode == 2 {
            charge.electricity_fee_amount = 18;
        }
        if mode == 3 {
            charge.compute_fee_amount = -1;
        }
        let expected = world.prepare_module_runtime_charge_event(&charge, 12);
        let mut stock = world.state().agents["payer"].state.resources.clone();
        let result = core::prepare_balances(&charge, &mut stock, |kind| {
            world.state().resources.get(&kind).copied()
        });
        if mode >= 2 {
            assert!(result.is_err());
            assert!(expected.is_err());
            if mode == 2 {
                assert_eq!(stock.get(ResourceKind::Data), 17);
            } else {
                assert_eq!(stock.get(ResourceKind::Data), 20);
            }
        } else {
            let treasury = result.unwrap();
            expected.unwrap().install_infallible(&mut world);
            assert_eq!(world.state().agents["payer"].state.resources, stock);
            assert_eq!(world.state().agents["payer"].last_active, 12);
            if mode == 0 {
                assert_eq!(stock.get(ResourceKind::Data), 13);
                assert_eq!(treasury[&ResourceKind::Data], i64::MAX);
            } else {
                assert!(treasury.is_empty());
                assert!(stock.amounts.is_empty());
                assert!(world.state().resources.is_empty());
            }
        }
    }
}

#[path = "incremental_module_fee_boundary_tests.rs"]
mod boundaries;
