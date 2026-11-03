use super::body_tests::*;
use super::*;
use crate::{
    incremental_field_member::SemanticKeyBudget, incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::JsonReadWorkV1, incremental_record_tree as tree,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
fn transfer<W: ValueNodeWriter>(
    roots: &RootSetV1,
    source: &Pages,
    memory: &MemoryBudget,
    max: usize,
    cancelled: impl FnMut() -> bool,
    sink: &mut W,
) -> Result<ArtifactConfigPagesV1, ValueTreeError> {
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut state_cancel = || false;
    let state_cancel = RefCell::new(&mut state_cancel);
    let error = RefCell::new(None);
    let state = tree::Source {
        source,
        memory,
        class: MemoryClass::Ordinary,
        io: &state_io,
        cancelled: &state_cancel,
        error: &error,
        labels: genesis::LABELS,
    };
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut context = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Artifacts),
        state: &state,
        values: ValueReadContext {
            source,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        },
        key_work: &mut keys,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        take_state_error: || error.borrow_mut().take(),
        cancelled,
    };
    transfer_artifact_config_pages_v1(
        roots,
        &mut context,
        &mut ArtifactConfigPagesWorkV1 {
            outer_scan: &mut StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 256 << 20,
                leaves_remaining: 10_000,
            },
            inner_scan: &mut StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 256 << 20,
                leaves_remaining: 10_000,
            },
            body: &mut ArtifactConfigReadWorkV1 {
                cells_remaining: 100_000,
                max_payload_bytes: 1 << 20,
                json: JsonReadWorkV1 {
                    cells_remaining: 10_000,
                    json_nodes_remaining: 1000,
                    max_depth: 16,
                    max_payload_bytes: 1 << 20,
                },
            },
            max_pages: max,
        },
        sink,
    )
}
struct Counted<'a> {
    pages: &'a mut Pages,
    count: &'a Cell<usize>,
    fail: Option<usize>,
}
impl ValueNodeWriter for Counted<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.pages.put_node(id, bytes)?;
        self.count.set(self.count.get() + 1);
        if self.fail == Some(self.count.get()) {
            return Err(ValueTreeError::Io(std::io::Error::other(
                "artifact config actual put",
            )));
        }
        Ok(())
    }
}
#[test]
fn typed_pages_restore_fresh_read_pool_and_fail_after_actual_final_unique_put() {
    let memory = MemoryBudget::new(192 << 20, 0).unwrap();
    let _source_index = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let _target_index = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let _bookkeeping = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let mut source = Pages {
        nodes: Rc::new(RefCell::new(BTreeMap::new())),
    };
    let (roots, _) = seed(&memory, &mut source);
    let mut target = Pages {
        nodes: Rc::new(RefCell::new(BTreeMap::new())),
    };
    let marked = transfer(&roots, &source, &memory, 32768, || false, &mut target).unwrap();
    marked
        .qualification()
        .validate_binding(&roots.digest().unwrap())
        .unwrap();
    let limit = marked.page_count();
    assert!(limit > 0);
    assert_eq!(target.nodes.borrow().len(), limit);
    marked
        .visit_pages::<ValueTreeError>(|id| {
            let nodes = target.nodes.borrow();
            let value = nodes.get(id).unwrap();
            assert_eq!(blake3::hash(value.as_slice()).as_bytes(), id);
            assert_eq!(
                value.as_slice(),
                source.nodes.borrow().get(id).unwrap().as_slice()
            );
            Ok(())
        })
        .unwrap();
    // Source DTOs were dropped by seed; this is a fresh small read allocation
    // pool over copied in-memory pages, not a DistFS/RSS durability claim.
    qualify_family(&roots, &target).unwrap();
    drop(marked);
    for mode in 0..3 {
        let mut target = Pages {
            nodes: Rc::new(RefCell::new(BTreeMap::new())),
        };
        let count = Cell::new(0);
        let result = transfer(
            &roots,
            &source,
            &memory,
            if mode == 0 { 0 } else { 32768 },
            || mode == 2 && count.get() >= limit,
            &mut Counted {
                pages: &mut target,
                count: &count,
                fail: (mode == 1).then_some(limit),
            },
        );
        match mode {
            0 => {
                assert!(matches!(result, Err(ValueTreeError::ResourceLimited(_))));
                assert_eq!(count.get(), 0);
            }
            1 => {
                assert!(matches!(result, Err(ValueTreeError::Io(_))));
                assert_eq!(count.get(), limit);
                assert_eq!(target.nodes.borrow().len(), limit);
            }
            _ => {
                assert!(matches!(result, Err(ValueTreeError::Cancelled)));
                assert_eq!(count.get(), limit);
                assert_eq!(target.nodes.borrow().len(), limit);
            }
        }
    }
}
