use super::*;
fn context<'a>(
    store: &'a Store,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
) -> ValueReadContext<'a, Store> {
    ValueReadContext {
        source: store,
        memory,
        class: MemoryClass::Ordinary,
        io,
    }
}
#[test]
fn full_closure_checks_each_occurrence_and_empty_leaf_without_output() {
    for size in [0, 1, VALUE_CHUNK_BYTES + 7, VALUE_CHUNK_BYTES * 129 + 7] {
        let data = vec![0x73; size];
        let (root, store, memory) = build(&data);
        let id = root.digest().unwrap();
        let mut quota = io();
        quota.write_bytes_remaining = 0;
        let result = context(&store, &memory, &mut quota)
            .verify_closure(&id, size as u64, || false)
            .unwrap();
        let mut level = size.div_ceil(VALUE_CHUNK_BYTES).max(1) as u64;
        let mut expected = level + 1;
        while level > 1 {
            level = level.div_ceil(VALUE_FANOUT as u64);
            expected += level;
        }
        assert_eq!(result.root(), root);
        assert_eq!(result.node_occurrences(), expected);
        assert_eq!(store.reads.get() as u64, expected);
        assert_eq!(quota.write_bytes_remaining, 0);
    }
}
#[test]
fn empty_or_unselected_missing_leaf_cannot_pass_complete_closure() {
    for size in [0, VALUE_CHUNK_BYTES + 1] {
        let (root, mut store, memory) = build(&vec![1; size]);
        let missing = if size == 0 {
            root.0.id
        } else {
            let page = store.nodes.get(&root.0.id).unwrap();
            match decode_node(page, &root.0).unwrap() {
                Node::Index(children) => children[1].id,
                _ => panic!("fixture index"),
            }
        };
        store.nodes.remove(&missing);
        // Narrow authorized ranges do not need the removed descendant.
        range(&root, 0, u64::from(size != 0), &store, &mut vec![], &memory).unwrap();
        assert!(
            context(&store, &memory, &mut io())
                .verify_closure(&root.digest().unwrap(), size as u64, || false)
                .is_err()
        );
    }
}
#[test]
fn exact_node_and_read_allowances_pass_one_less_or_logical_limit_fail() {
    let (root, store, memory) = build(&vec![2; VALUE_CHUNK_BYTES + 1]);
    let id = root.digest().unwrap();
    let mut generous = io();
    context(&store, &memory, &mut generous)
        .verify_closure(&id, u64::MAX, || false)
        .unwrap();
    let nodes = u64::MAX - generous.nodes_remaining;
    let bytes = u64::MAX - generous.read_bytes_remaining;
    for mode in 0..4 {
        let mut quota = ValueIoBudget {
            nodes_remaining: nodes - u64::from(mode == 1),
            read_bytes_remaining: bytes - u64::from(mode == 2),
            write_bytes_remaining: 0,
        };
        let result = context(&store, &memory, &mut quota).verify_closure(
            &id,
            if mode == 3 { 1 } else { u64::MAX },
            || false,
        );
        if mode == 0 {
            result.unwrap();
            assert_eq!((quota.nodes_remaining, quota.read_bytes_remaining), (0, 0));
        } else {
            assert!(matches!(result, Err(ValueTreeError::ResourceLimited(_))));
        }
    }
}
#[test]
fn cancellation_after_last_node_never_returns_partial_success() {
    let (root, store, memory) = build(&[]);
    let mut calls = 0;
    let mut quota = io();
    let before = memory.used();
    assert!(matches!(
        context(&store, &memory, &mut quota).verify_closure(&root.digest().unwrap(), 0, || {
            calls += 1;
            calls == 3
        }),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(store.reads.get(), 2);
    assert_eq!(memory.used(), before);
    assert_eq!(quota.nodes_remaining, u64::MAX - 2);
}

#[test]
fn caught_source_unwind_spends_node_allowance_and_releases_workspace() {
    struct Panics;
    impl ValueNodeReader for Panics {
        fn read_node(
            &self,
            _: &[u8; 32],
            _: usize,
            _: &MemoryBudget,
            _: MemoryClass,
        ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
            panic!("source unwind")
        }
    }
    let memory = MemoryBudget::new(1 << 20, 0).unwrap();
    let mut quota = io();
    let before = memory.used();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ValueReadContext {
                source: &Panics,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            }
            .verify_closure(&[0; 32], 0, || false)
        }))
        .is_err()
    );
    assert_eq!(quota.nodes_remaining, u64::MAX - 1);
    assert!(quota.read_bytes_remaining < u64::MAX);
    assert_eq!(memory.used(), before);
}
