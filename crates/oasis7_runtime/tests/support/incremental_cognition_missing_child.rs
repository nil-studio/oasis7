//! Generic top ValueTree closure does not authenticate a hidden typed StateRoot.
use super::*;
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{incremental_cognition_family::*, incremental_json_records::JsonReadWorkV1};
use std::{cell::Cell, sync::Arc};
struct Withheld<'a, R> {
    source: &'a R,
    id: [u8; 32],
    hits: Cell<usize>,
}
impl<R: ValueNodeReader> ValueNodeReader for Withheld<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if id == &self.id {
            self.hits.set(self.hits.get() + 1);
            return Err(ValueTreeError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "withheld Cognition typed child page",
            )));
        }
        self.source.read_node(id, max, memory, class)
    }
}
pub fn exercise(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    child: &str,
) {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let original = store.value_session(physical());
    let values = Withheld {
        source: &original,
        id: *blake3::Hash::from_hex(child).unwrap().as_bytes(),
        hits: Cell::new(0),
    };
    let mut io = fixture::logical();
    let mut value_context = ValueReadContext {
        source: &values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut io,
    };
    let mut scan = StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 1000,
    };
    // Explicit integrity traversal, not an opaque accepted-head or owner token.
    for root in &roots.state {
        StateScanContext {
            source: &state,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut scan,
        }
        .visit_entries(
            root,
            |entry| {
                value_context
                    .verify_closure(&entry.value, 8192, &mut || false)
                    .map(|_| ())
            },
            || false,
        )
        .unwrap();
    }
    assert_eq!(
        values.hits.get(),
        0,
        "generic top closure must not follow typed RootRef bytes"
    );
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut context = WorldFieldReadContextV1 {
        root: &roots.state[0],
        state: &state,
        values: value_context,
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let result = qualify_cognition_current_state_v1(
        roots,
        &mut context,
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        },
        &mut JsonReadWorkV1 {
            cells_remaining: 4096,
            json_nodes_remaining: 512,
            max_depth: 4,
            max_payload_bytes: 4096,
        },
        &mut 1000,
    );
    assert!(
        matches!(result,Err(ValueTreeError::Io(ref error)) if error.kind()==std::io::ErrorKind::NotFound),
        "{result:?}"
    );
    assert!(values.hits.get() > 0);
}
