#![cfg(not(target_arch = "wasm32"))]
//! Original State reader failures at the public inventory qualification boundary.
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{
        MerkleNodeReader, StateDeltaV1, StateNamespaceV1, StateScanBudget, StateWriteV1,
    },
    incremental_value::{ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    MaterialLedgerId, WorldState,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory::{InventoryFamilyWorkV1, qualify_inventory_family_v1},
    incremental_inventory_genesis::{
        InventoryGenesisContext, InventoryWriteSink, prepare_inventory_genesis_v1,
    },
    incremental_semantic_key::SemanticKeyBudget,
};
use std::cell::{Cell, RefCell};

#[expect(
    dead_code,
    reason = "The shared page fixture also contains helpers for other integration tests"
)]
#[path = "support/crisis_current_pages.rs"]
mod pages;
use pages::{Pages, Writer, io};

fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 100000,
    }
}
fn work() -> InventoryFamilyWorkV1 {
    InventoryFamilyWorkV1 {
        records_remaining: 100000,
        index_steps_remaining: 1_000_000,
        max_semantic_bytes: 4096,
    }
}
struct Rows(Vec<ReservedValue<StateWriteV1>>);
impl InventoryWriteSink for Rows {
    fn put_write(&mut self, write: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        self.0.push(write);
        Ok(())
    }
}
fn source() -> WorldState {
    let mut source = WorldState::default();
    source.resources.clear();
    source.materials.clear();
    source.material_ledgers.clear();
    source.resources.insert(ResourceKind::Electricity, i64::MIN);
    source.materials.insert(" ore/界 ".into(), i64::MAX);
    source
        .material_ledgers
        .insert(MaterialLedgerId::world(), [(" ore/界 ".into(), -7)].into());
    source
}
fn install(memory: &MemoryBudget, pages: &Pages) -> RootSetV1 {
    let mut rows = Rows(Vec::new());
    let mut writer = Writer(pages);
    prepare_inventory_genesis_v1(
        &source(),
        &mut InventoryGenesisContext {
            values: &mut writer,
            writes: &mut rows,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            records_remaining: 10000,
            cancelled: || false,
        },
    )
    .unwrap();
    let mut writes: Vec<_> = rows.0.iter().map(|row| (**row).clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let mut roots = RootSetV1::empty("inventory-family".into(), [8; 32]).unwrap();
    let namespace = StateNamespaceV1::Business;
    let delta =
        StateDeltaV1::build(roots.state_root(namespace).clone(), writes, pages, 8 << 20).unwrap();
    for node in &delta.new_nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    *roots
        .state
        .iter_mut()
        .find(|root| root.namespace == namespace)
        .unwrap() = delta.after;
    roots
}

struct StateReadFault<'a> {
    pages: &'a Pages,
    original: RefCell<Option<ValueTreeError>>,
    read_seen: Cell<bool>,
    cancelled: Cell<bool>,
    cancel_on_fault: bool,
}
impl MerkleNodeReader for StateReadFault<'_> {
    type Node = <Pages as MerkleNodeReader>::Node;

    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        MerkleNodeReader::allocation_budget(self.pages)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        MerkleNodeReader::allocation_class(self.pages)
    }
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Self::Node>, String> {
        let page = MerkleNodeReader::read_node(self.pages, digest, max_bytes)?;
        assert!(
            page.is_some(),
            "fault must follow a real installed page read"
        );
        self.read_seen.set(true);
        if self.original.borrow().is_none() {
            *self.original.borrow_mut() = Some(ValueTreeError::ResourceLimited(
                "original inventory State read allowance",
            ));
        }
        self.cancelled.set(self.cancel_on_fault);
        Err("Merkle reader erased the typed State failure".into())
    }
}

fn qualify_with_state_read_fault(cancel_on_fault: bool) -> ValueTreeError {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let roots = install(&memory, &pages);
    let source = StateReadFault {
        pages: &pages,
        original: RefCell::new(None),
        read_seen: Cell::new(false),
        cancelled: Cell::new(false),
        cancel_on_fault,
    };
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let result = qualify_inventory_family_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state: &source,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || source.original.borrow_mut().take(),
            cancelled: || source.cancelled.get(),
        },
        &mut scan(),
        &mut work(),
    );
    assert!(source.read_seen.get());
    assert!(source.original.borrow().is_none());
    match result {
        Err(error) => error,
        Ok(_) => panic!("a failed State reader cannot qualify inventory"),
    }
}

#[test]
fn inventory_qualification_preserves_original_state_read_failure() {
    assert!(matches!(
        qualify_with_state_read_fault(false),
        ValueTreeError::ResourceLimited("original inventory State read allowance")
    ));
}

#[test]
fn inventory_qualification_prioritizes_cancellation_after_state_read_failure() {
    assert!(matches!(
        qualify_with_state_read_fault(true),
        ValueTreeError::Cancelled
    ));
}
