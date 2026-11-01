use super::*;
use crate::{
    AgentCell, DomainEvent,
    agent_state::{AgentState, CargoEntityEntry, CargoEntityKind},
    incremental_agent_body::{AgentBodyKeyV1, AgentBodyValueV1},
    incremental_event_fields::{MailboxFrontierV1, MailboxKeyV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_semantic_key::SemanticKeyBudget,
};
use oasis7_formal_protocol::{incremental_execution::RootSetV1, incremental_state::*};
use std::cell::{Cell, RefCell};
pub(super) struct State<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
    error: RefCell<Option<ValueTreeError>>,
    reads: Cell<u64>,
    bytes: Cell<u64>,
}
impl<'a> State<'a> {
    pub(super) fn new(pages: &'a Pages, memory: &'a MemoryBudget) -> Self {
        Self {
            pages,
            memory,
            error: RefCell::new(None),
            reads: Cell::new(1_000_000),
            bytes: Cell::new(64 << 20),
        }
    }
    pub(super) fn take(&self) -> Option<ValueTreeError> {
        self.error.borrow_mut().take()
    }
}
pub(super) struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl MerkleNodeReader for State<'_> {
    type Node = Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        let Some(page) = self.pages.0.get(&id) else {
            return Ok(None);
        };
        if self.reads.get() == 0 || self.bytes.get() < page.len() as u64 {
            self.error
                .borrow_mut()
                .get_or_insert(ValueTreeError::ResourceLimited("agent fixture state IO"));
            return Err("state IO".into());
        }
        self.reads.set(self.reads.get() - 1);
        self.bytes.set(self.bytes.get() - page.len() as u64);
        if page.len() > max {
            return Err("state bound".into());
        }
        Ok(Some(Node(page.clone())))
    }
}
fn value(
    bytes: &[u8],
    pages: &mut Pages,
    memory: &MemoryBudget,
) -> Result<[u8; 32], ValueTreeError> {
    let length = bytes.len() as u64;
    let mut input = bytes;
    write_value(&mut input, length, pages, memory, &mut io(), || false)?.digest()
}
struct Gen<'a> {
    agent: &'a str,
    pages: &'a mut Pages,
    memory: &'a MemoryBudget,
    rows: Vec<(StateNamespaceV1, Vec<u8>, [u8; 32])>,
}
impl Gen<'_> {
    fn emit(
        &mut self,
        ns: StateNamespaceV1,
        key: &[u8],
        id: [u8; 32],
    ) -> Result<(), ValueTreeError> {
        if self.rows.len() >= 512 {
            return Err(ValueTreeError::ResourceLimited("fixture rows"));
        }
        self.rows.push((ns, key.to_vec(), id));
        Ok(())
    }
}
impl AgentCellFieldVisitor for Gen<'_> {
    type Error = ValueTreeError;
    fn scalar(
        &mut self,
        field: AgentFieldV1,
        ordinal: u64,
        v: AgentScalarRefV1<'_>,
    ) -> Result<(), Self::Error> {
        let k =
            AgentFieldKeyV1::new(self.agent, field, ordinal).map_err(ValueTreeError::Invalid)?;
        let id = tree(v, self.memory, self.pages);
        self.emit(field.namespace(), k.as_bytes(), id)
    }
    fn body(&mut self, v: &AgentBodyValueV1) -> Result<(), Self::Error> {
        let k = AgentBodyKeyV1::new(self.agent).map_err(ValueTreeError::Invalid)?;
        let id = value(&v.canonical_bytes(), self.pages, self.memory)?;
        self.emit(StateNamespaceV1::Business, k.as_bytes(), id)
    }
    fn mailbox_frontier(&mut self, count: u64) -> Result<(), Self::Error> {
        let k = MailboxKeyV1::new(self.agent, None).map_err(ValueTreeError::Invalid)?;
        let bytes = MailboxFrontierV1 {
            head: 0,
            tail: count,
        }
        .canonical_bytes()
        .map_err(ValueTreeError::Invalid)?;
        let id = value(&bytes, self.pages, self.memory)?;
        self.emit(StateNamespaceV1::Scheduler, k.as_bytes(), id)
    }
    fn mailbox(&mut self, _: u64, _: &DomainEvent) -> Result<(), Self::Error> {
        Err(ValueTreeError::Invalid(
            "fixture requires genuine history original",
        ))
    }
}
pub(super) fn source_cell(i: usize, agent: &str) -> AgentCell {
    let mut cell = AgentCell::new(
        AgentState::new(
            agent,
            oasis7_agent_api::world_action::GeoPos {
                x_cm: i64::MIN,
                y_cm: 0,
                z_cm: i64::MAX,
            },
        ),
        u64::MAX,
    );
    cell.state.resources.amounts.clear();
    cell.state.resources.amounts.insert(
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        77,
    );
    cell.state.body_state.slots.truncate(2);
    cell.state.body_state.cargo_entries.push(CargoEntityEntry {
        entity_id: " repeated original ".into(),
        entity_kind: CargoEntityKind::Mineral,
        quantity: i64::MIN,
        size_per_unit: 0,
    });
    if i == 0 {
        cell.activity = Some(crate::agent_cell::AgentActivityV1::idle(u64::MAX));
    } else {
        cell.intent=Some(serde_json::from_value(serde_json::json!({"schema_version":u32::MAX,"agent_id":agent,"intent_id":"original intent","kind":"unclamped original kind","summary":"unicode 界","target_id":"","status":"unclamped original status","source":"raw original source","logical_time":u64::MAX,"event_seq":u64::MAX,"updated_at":0,"receipt_ref":"","reason_code":null,"reason_summary":null,"replaced_by":null})).unwrap());
    }
    cell
}
pub(super) fn seed(memory: &MemoryBudget) -> (Pages, RootSetV1) {
    let _rows = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let _inputs = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::default();
    let mut rows = Vec::with_capacity(512);
    for (i, agent) in ["agent α", "agent β"].into_iter().enumerate() {
        let cell = source_cell(i, agent);
        let mut producer = Gen {
            agent,
            pages: &mut pages,
            memory,
            rows,
        };
        visit_agent_cell(agent, &cell, &mut producer).unwrap();
        rows = producer.rows;
    }
    let mut roots = RootSetV1::empty("agent-subtree-unit".into(), [0x52; 32]).unwrap();
    for ns in [
        StateNamespaceV1::Business,
        StateNamespaceV1::Scheduler,
        StateNamespaceV1::Authorization,
    ] {
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.0 == ns)
            .map(|(_, key, value)| StateWriteV1 {
                key: key.clone(),
                before: None,
                after: Some(*value),
            })
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let charge = memory
            .reserve(
                writes.len() * (512 << 10) + (128 << 10),
                MemoryClass::Ordinary,
            )
            .unwrap();
        let delta = {
            let source = State::new(&pages, memory);
            charge.retain(
                StateDeltaV1::build(StateRootV1::empty(ns).unwrap(), writes, &source, 8 << 20)
                    .unwrap(),
            )
        };
        persist(&delta, &mut pages, memory);
        *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after.clone();
    }
    (pages, roots)
}
fn persist(delta: &StateDeltaV1, pages: &mut Pages, memory: &MemoryBudget) {
    for node in &delta.new_nodes {
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        pages
            .put_node(id, charge.retain(node.bytes.clone()))
            .unwrap();
    }
}
pub(super) fn qualify(
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
) -> Result<ReservedValue<AgentSubtreeStructureQualificationV1>, ValueTreeError> {
    qualify_max_agents(roots, pages, memory, 2)
}
pub(super) fn qualify_max_agents(
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
    max_agents: usize,
) -> Result<ReservedValue<AgentSubtreeStructureQualificationV1>, ValueTreeError> {
    let source = State::new(pages, memory);
    qualify_agent_subtree_structure_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &source,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 1024,
            record_bytes: 1 << 20,
            take_state_error: || source.take(),
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 1_000_000,
            leaves_remaining: 100_000,
            read_bytes_remaining: 64 << 20,
        },
        &mut AgentSubtreeScanWorkV1 {
            max_agents,
            cells_remaining: 100_000,
            max_scalar_bytes: 1 << 20,
        },
        &mut |_| Err(ValueTreeError::Invalid("unexpected mailbox dependency")),
    )
}
pub(super) fn false_cross_agent_counts(
    roots: &RootSetV1,
    pages: &mut Pages,
    memory: &MemoryBudget,
) -> RootSetV1 {
    let ns = StateNamespaceV1::Business;
    let before = roots.state_root(ns);
    let mut writes = Vec::new();
    for (agent, field, n) in [
        ("agent α", AgentFieldV1::SlotCount, 3),
        ("agent α", AgentFieldV1::SlotNext, 4),
        ("agent β", AgentFieldV1::SlotCount, 1),
    ] {
        let key = AgentFieldKeyV1::new(agent, field, 0).unwrap();
        let after = tree(AgentScalarRefV1::U64(n), memory, pages);
        let source = State::new(pages, memory);
        writes.push(StateWriteV1 {
            key: key.as_bytes().to_vec(),
            before: before.get(key.as_bytes(), &source).unwrap(),
            after: Some(after),
        });
    }
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let charge = memory
        .reserve(3 * (512 << 10) + (128 << 10), MemoryClass::Ordinary)
        .unwrap();
    let delta = {
        let source = State::new(pages, memory);
        charge.retain(StateDeltaV1::build(before.clone(), writes, &source, 8 << 20).unwrap())
    };
    persist(&delta, pages, memory);
    let mut roots = roots.clone();
    *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after.clone();
    roots
}
