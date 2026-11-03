use super::*;
use crate::incremental_value::{
    ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueRootV1, ValueTreeError,
    write_value,
};
struct Values {
    nodes: BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
    reads: Cell<usize>,
    _metadata: MemoryReservation,
}
impl ValueNodeWriter for Values {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.nodes.insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for Values {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        _class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.reads.set(self.reads.get() + 1);
        let node = self
            .nodes
            .get(id)
            .ok_or(ValueTreeError::Invalid("missing test value"))?;
        if node.len() > max || !node.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("test value pool/size"));
        }
        Ok(node.clone())
    }
}
fn value(s: &Setup, bytes: &[u8]) -> (ValueRootV1, Values) {
    let mut values = Values {
        nodes: BTreeMap::new(),
        reads: Cell::new(0),
        _metadata: s.memory.reserve(64 * 1024, MemoryClass::Ordinary).unwrap(),
    };
    let mut input = bytes;
    let root = write_value(
        &mut input,
        bytes.len() as u64,
        &mut values,
        &s.memory,
        &mut io(),
        || false,
    )
    .unwrap();
    (root, values)
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 10000,
        read_bytes_remaining: 1 << 25,
        write_bytes_remaining: 1 << 25,
    }
}
fn allowance() -> WrittenValueBudget {
    WrittenValueBudget {
        values_remaining: 7,
        logical_bytes_remaining: 1000000,
        max_value_bytes: 1000000,
    }
}
fn core(s: &Setup, id: [u8; 32]) -> ReservedValue<CheckedCommitCore> {
    let (mut objects, pages, candidate, _) = execution_sources(s, id);
    verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: &s.authority,
            parent: Some(&s.commit),
            max_pending_node_bytes: 4 << 20,
        },
        &mut objects,
        &pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap()
}
#[test]
fn actual_transition_value_reference_requires_all_data_including_empty_leaf() {
    let s = setup();
    for bytes in [vec![], vec![7; 65537]] {
        let (root, mut values) = value(&s, &bytes);
        let checked = core(&s, root.digest().unwrap());
        let mut io = io();
        io.write_bytes_remaining = 0;
        let mut context = ValueReadContext {
            source: &values,
            memory: &s.memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        };
        let mut quota = allowance();
        let result = verify_written_values(&checked, &mut context, &mut quota, || false).unwrap();
        assert_eq!(result.body_id(), checked.catalog().entry.commit);
        assert_eq!(result.values(), 7);
        assert_eq!(result.logical_bytes(), 7 * bytes.len() as u64);
        assert_eq!(
            result.node_occurrences(),
            if bytes.is_empty() { 14 } else { 28 }
        );
        assert_eq!(
            quota.logical_bytes_remaining,
            1000000 - 7 * bytes.len() as u64
        );
        drop(result);
        values.nodes.remove(&root.0.id);
        let mut context = ValueReadContext {
            source: &values,
            memory: &s.memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        };
        let mut quota = allowance();
        assert!(verify_written_values(&checked, &mut context, &mut quota, || false).is_err());
        assert_eq!(
            (quota.values_remaining, quota.logical_bytes_remaining),
            (6, 0)
        );
    }
}
#[test]
fn genesis_cannot_claim_delta_closure_and_bounds_class_cancel_precede_reads() {
    let s = setup();
    let (root, values) = value(&s, b"current");
    let delta = core(&s, root.digest().unwrap());
    let (mut objects, pages) = sources(&s, &s.history);
    let candidate = candidate(&s, s.commit.record().clone());
    let genesis = verify_commit_core(
        &request(&s, &candidate),
        &mut objects,
        &pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    {
        use crate::incremental_state::StateScanBudget;
        let mut io = io();
        let mut context = ValueReadContext {
            source: &values,
            memory: &s.memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        };
        let mut state = StateScanBudget {
            nodes_remaining: 0,
            read_bytes_remaining: 0,
            leaves_remaining: 0,
        };
        let mut budget = WrittenValueBudget {
            values_remaining: 0,
            logical_bytes_remaining: 0,
            max_value_bytes: 0,
        };
        let checked = verify_state_values(
            &genesis,
            &pages,
            &mut context,
            &mut state,
            &mut budget,
            || false,
        )
        .unwrap();
        assert_eq!(checked.leaf_counts(), [0; 6]);
        assert_eq!(checked.logical_bytes(), 0);
        assert_eq!(values.reads.get(), 0);
    }
    for mode in 0..5 {
        let mut io = io();
        let mut context = ValueReadContext {
            source: &values,
            memory: &s.memory,
            class: if mode == 1 {
                MemoryClass::Recovery
            } else {
                MemoryClass::Ordinary
            },
            io: &mut io,
        };
        let mut quota = allowance();
        if mode == 2 {
            quota.values_remaining = 0;
        }
        if mode == 3 {
            quota.max_value_bytes = 1;
        }
        let checked = if mode == 0 { &genesis } else { &delta };
        let before = values.reads.get();
        assert!(verify_written_values(checked, &mut context, &mut quota, || mode == 4).is_err());
        assert_eq!(values.reads.get() - before, usize::from(mode == 3));
    }
}

#[test]
fn full_state_checks_all_six_namespaces_and_each_repeated_value() {
    use crate::incremental_state::StateScanBudget;
    let s = setup();
    let (root, mut values) = value(&s, b"now");
    let (mut objects, mut pages, candidate, _) = execution_sources(&s, root.digest().unwrap());
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: &s.authority,
            parent: Some(&s.commit),
            max_pending_node_bytes: 4 << 20,
        },
        &mut objects,
        &pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
        pages.class = class;
        let mut io = io();
        let mut context = ValueReadContext {
            source: &values,
            memory: &s.memory,
            class,
            io: &mut io,
        };
        let mut state = StateScanBudget {
            nodes_remaining: 10000,
            read_bytes_remaining: 1 << 24,
            leaves_remaining: 7,
        };
        let mut allowance = WrittenValueBudget {
            values_remaining: 7,
            logical_bytes_remaining: 21,
            max_value_bytes: 3,
        };
        let checked = verify_state_values(
            &core,
            &pages,
            &mut context,
            &mut state,
            &mut allowance,
            || false,
        )
        .unwrap();
        assert_eq!(checked.body_id(), core.catalog().entry.commit);
        assert_eq!(checked.leaf_counts(), [2, 1, 1, 1, 1, 1]);
        assert_eq!(checked.logical_bytes(), 21);
        assert_eq!(checked.value_node_occurrences(), 14);
        assert_eq!(
            (
                state.leaves_remaining,
                allowance.values_remaining,
                allowance.logical_bytes_remaining
            ),
            (0, 0, 0)
        );
    }
    pages.class = MemoryClass::Ordinary;
    values.nodes.remove(&root.0.id);
    let mut io = io();
    let mut context = ValueReadContext {
        source: &values,
        memory: &s.memory,
        class: MemoryClass::Ordinary,
        io: &mut io,
    };
    let mut state = StateScanBudget {
        nodes_remaining: 10000,
        read_bytes_remaining: 1 << 24,
        leaves_remaining: 7,
    };
    assert!(
        verify_state_values(
            &core,
            &pages,
            &mut context,
            &mut state,
            &mut allowance(),
            || false
        )
        .is_err()
    );
}
