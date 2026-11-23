#![cfg(all(unix, not(target_arch = "wasm32")))]

use oasis7_agent_api::world_action::ResourceKind;
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageIoBudget};
use oasis7_formal_protocol::incremental_state::{
    STATE_ROOT_BYTES, StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
};
use oasis7_formal_protocol::incremental_value::ValueIoBudget;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_inventory_genesis::*,
    types::MaterialLedgerId,
};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
pub(crate) struct Directory(pub(crate) std::path::PathBuf);
impl Directory {
    pub(crate) fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-inventory-genesis-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
pub(crate) fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}
pub(crate) fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
type WriteBatch = Vec<ReservedValue<StateWriteV1>>;
pub(crate) struct Writes {
    pub(crate) rows: ReservedValue<RefCell<WriteBatch>>,
    pub(crate) fail_after: usize,
    pub(crate) cancel: Option<Rc<Cell<bool>>>,
}
impl Writes {
    pub(crate) fn new(memory: &MemoryBudget) -> Self {
        Self {
            rows: memory
                .reserve(64 << 10, MemoryClass::Ordinary)
                .unwrap()
                .retain(RefCell::new(Vec::with_capacity(64))),
            fail_after: usize::MAX,
            cancel: None,
        }
    }
}
impl InventoryWriteSink for Writes {
    fn put_write(
        &mut self,
        write: ReservedValue<StateWriteV1>,
    ) -> Result<(), oasis7_formal_protocol::incremental_value::ValueTreeError> {
        let mut rows = self.rows.borrow_mut();
        assert!(rows.len() < 64);
        if rows.len() == self.fail_after {
            return Err(std::io::Error::other("injected sink failure").into());
        }
        rows.push(write);
        if let Some(flag) = &self.cancel {
            flag.set(true);
        }
        Ok(())
    }
}
pub(crate) fn source() -> WorldState {
    WorldState {
        time: 777,
        resources: BTreeMap::from([
            (ResourceKind::Electricity, i64::MIN),
            (ResourceKind::Data, 9),
        ]),
        materials: BTreeMap::from([(String::new(), 0), ("x:y".into(), i64::MAX)]),
        material_ledgers: BTreeMap::from([
            (MaterialLedgerId::World, BTreeMap::new()),
            (
                MaterialLedgerId::Agent("a".into()),
                BTreeMap::from([("x:y".into(), -3)]),
            ),
            (
                MaterialLedgerId::Factory("a".into()),
                BTreeMap::from([(String::new(), 42)]),
            ),
        ]),
        ..WorldState::default()
    }
}

pub(crate) fn build(
    source: &WorldState,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> (InventoryGenesisSummaryV1, [u8; STATE_ROOT_BYTES]) {
    let mut values = store.value_session(physical());
    let mut writes = Writes::new(memory);
    let summary = prepare_inventory_genesis_v1(
        source,
        &mut InventoryGenesisContext {
            values: &mut values,
            writes: &mut writes,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 256 << 20,
            },
            semantic_bytes: 128 << 20,
            record_bytes: 128 << 20,
            records_remaining: 64,
            cancelled: || false,
        },
    )
    .unwrap();
    let state_source = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let workspace = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let rows = writes.rows.borrow();
    let mut changes: Vec<_> = rows
        .iter()
        .map(|write| {
            assert_eq!(write.before, None);
            StateWriteV1 {
                key: write.key.clone(),
                before: None,
                after: write.after,
            }
        })
        .collect();
    changes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = workspace.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
            changes,
            &state_source,
            2 << 20,
        )
        .unwrap(),
    );
    assert_eq!(delta.after.leaf_count, rows.len() as u64);
    values.put_state_pages(&delta, memory).unwrap();
    (
        summary,
        delta.after.canonical_bytes().unwrap().try_into().unwrap(),
    )
}
pub(crate) fn text_token(text: &str) -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&(text.len() as u64).to_be_bytes());
    bytes.extend_from_slice(text.as_bytes());
    bytes
}
pub(crate) fn ledger_token(kind: u8, id: Option<&str>) -> Vec<u8> {
    let mut bytes = vec![4, kind];
    if let Some(id) = id {
        bytes.extend(text_token(id));
    }
    bytes
}
pub(crate) fn pair_token(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut bytes = vec![5];
    bytes.extend_from_slice(a);
    bytes.extend_from_slice(b);
    bytes
}
pub(crate) fn key(
    name: &'static str,
    shape: FieldShape,
    kind: WorldFieldRecordKindV1,
    token: Option<&[u8]>,
) -> (WorldFieldKeyV1, Vec<u8>) {
    let mut semantic = Vec::new();
    let digest = token.map(|token| {
        semantic.extend_from_slice(b"oasis7.iws.semantic-key.v1\0");
        semantic.extend_from_slice(token);
        *blake3::hash(&semantic).as_bytes()
    });
    (
        WorldFieldKeyV1::new(
            FieldDescriptor {
                owner: FieldOwner::WorldState,
                name,
                destination: FieldDestination::State(StateNamespaceV1::Business),
                shape,
            },
            kind,
            digest,
        )
        .unwrap(),
        semantic,
    )
}
