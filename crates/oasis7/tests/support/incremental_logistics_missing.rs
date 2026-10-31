use super::*;
use std::{cell::RefCell, sync::Arc};
struct StateCopy<'a, 's, M, W> {
    source: &'a M,
    sink: &'a RefCell<&'s mut W>,
    memory: &'a MemoryBudget,
}
impl<M: MerkleNodeReader, W: ValueNodeWriter> MerkleNodeReader for StateCopy<'_, '_, M, W> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let page = self.source.read_node(id, max)?;
        if let Some(page) = &page {
            let bytes = page.as_ref();
            let hash = blake3::hash(bytes);
            if hash.to_hex().as_str() != id || bytes.len() > max {
                return Err("generic copy state integrity".into());
            }
            let charge = self
                .memory
                .reserve(bytes.len(), MemoryClass::Ordinary)
                .map_err(|e| format!("{e:?}"))?;
            self.sink
                .borrow_mut()
                .put_node(*hash.as_bytes(), charge.retain(bytes.to_vec()))
                .map_err(|e| format!("{e:?}"))?;
        }
        Ok(page)
    }
}
struct ValuesCopy<'a, 's, R, W> {
    source: &'a R,
    sink: &'a RefCell<&'s mut W>,
}
impl<R: ValueNodeReader, W: ValueNodeWriter> ValueNodeReader for ValuesCopy<'_, '_, R, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let page = self.source.read_node(id, max, memory, class)?;
        if !page.uses_budget(memory)
            || page.capacity() > page.reserved_bytes()
            || page.len() > max
            || blake3::hash(&page).as_bytes() != id
        {
            return Err(ValueTreeError::Invalid("generic copy value integrity"));
        }
        let charge = memory.reserve(page.len(), MemoryClass::Ordinary)?;
        self.sink
            .borrow_mut()
            .put_node(*id, charge.retain(page.to_vec()))?;
        Ok(page)
    }
}
fn copy_generic_only<W: ValueNodeWriter>(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    sink: &mut W,
) {
    let target = RefCell::new(sink);
    let state_source = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let value_source = store.value_session(physical_scope());
    let state = StateCopy {
        source: &state_source,
        sink: &target,
        memory,
    };
    let values = ValuesCopy {
        source: &value_source,
        sink: &target,
    };
    let mut scans = StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 10_000,
    };
    let mut io = logical();
    for root in &roots.state {
        StateScanContext {
            source: &state,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut scans,
        }
        .visit_entries(
            root,
            |entry| {
                ValueReadContext {
                    source: &values,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io,
                }
                .verify_closure(&entry.value, 8192, &mut || false)
                .map(|_| ())
            },
            || false,
        )
        .unwrap();
    }
}
pub(super) fn generic_complete_top_roots_do_not_prove_missing_typed_child() {
    let source_dir = Directory::new();
    let target_dir = Directory::new();
    let target_path = target_dir.0.join("pages");
    let (roots, child_id) = {
        let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
        let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
        let store = IncrementalPageStore::create(
            &source_dir.0.join("pages"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let roots = seed(&store, &memory, &source());
        let qualification = qualify(&store, &memory, &roots, MemoryClass::Ordinary);
        assert_eq!(qualification.record_counts(), [1, 2]);
        let (_, _, child) = malformed::original_child(&store, &memory, &roots);
        let target =
            IncrementalPageStore::create(&target_path, memory.clone(), PageCompression::Raw)
                .unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut io = physical_scope();
        let mut never = || false;
        let mut writer = target
            .buffered_page_writer(&mut io, &cancel, &mut never)
            .unwrap();
        copy_generic_only(&store, &memory, &roots, &mut writer);
        writer.finish().unwrap();
        (
            roots,
            *blake3::Hash::from_hex(&child.node_root).unwrap().as_bytes(),
        )
    };
    // The recovery path cannot fall back to the original store or memory pool.
    std::fs::remove_dir_all(&source_dir.0).unwrap();
    // Preserve only an empty directory for the fixture Drop contract.
    std::fs::create_dir(&source_dir.0).unwrap();
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let _caller = memory.reserve(16 << 10, MemoryClass::Recovery).unwrap();
    let store = IncrementalPageStore::open(
        &target_path,
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let missing = store.read_page(
        &child_id,
        STATE_MAX_PAGE_BYTES,
        MemoryClass::Recovery,
        &mut physical_scope(),
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert!(
        matches!(missing, Err(oasis7_distfs::incremental_page_store::PageStoreError::Missing(id)) if id == child_id)
    );
    let state = store
        .state_session(physical_scope(), MemoryClass::Recovery)
        .unwrap();
    let values = store.value_session(physical_scope());
    let mut scan = StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 10_000,
    };
    let mut io = logical();
    let mut entries = 0u64;
    // Real public generic verification of all six top state trees and every
    // reached ValueTree, not an assumed opaque CheckedStateValues marker.
    for root in &roots.state {
        StateScanContext {
            source: &state,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut scan,
        }
        .visit_entries(
            root,
            |entry| {
                entries += 1;
                ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Recovery,
                    io: &mut io,
                }
                .verify_closure(&entry.value, 8192, &mut || false)
                .map(|_| ())
            },
            || false,
        )
        .unwrap();
    }
    assert_eq!(entries, 7);
    let result = qualify_logistics_family_v1(
        &roots,
        &mut LogisticsFamilyReadContextV1 {
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 8192,
            max_payload_bytes: 4096,
            outer_scan: &mut StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 256 << 20,
                leaves_remaining: 10_000,
            },
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|_| ValueTreeError::Invalid("logistics missing child state"))
            },
            cancelled: || false,
        },
        &mut work(),
    );
    assert!(
        matches!(result,Err(ValueTreeError::Io(ref error)) if error.kind()==std::io::ErrorKind::NotFound)
    );
}
