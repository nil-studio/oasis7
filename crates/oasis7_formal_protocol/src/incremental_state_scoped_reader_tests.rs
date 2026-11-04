use super::*;
#[test]
fn scoped_reader_has_no_implicit_traversal_and_point_work_shares_allowances() {
    let (root, source) = source(&[b"a", b"b"]);
    let memory = &source.memory;
    let mut allowance = io();
    let initial = (
        allowance.nodes_remaining,
        allowance.read_bytes_remaining,
        allowance.leaves_remaining,
    );
    let reads = source.reads.get();
    let result = StateScanContext {
        source: &source,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut allowance,
    }
    .with_reader(|| false, |_| Ok::<_, String>(42))
    .unwrap();
    assert_eq!(result, 42);
    assert_eq!(source.reads.get(), reads);
    assert_eq!(
        (
            allowance.nodes_remaining,
            allowance.read_bytes_remaining,
            allowance.leaves_remaining
        ),
        initial
    );
    let result = StateScanContext {
        source: &source,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut allowance,
    }
    .with_reader(|| false, |reader| reader.get(&root, b"a"))
    .unwrap()
    .unwrap();
    assert_eq!(result, [3; 32]);
    assert!(allowance.nodes_remaining < initial.0);
    assert!(allowance.read_bytes_remaining < initial.1);
    assert_eq!(allowance.leaves_remaining, initial.2);
}
#[test]
fn scoped_reader_rejects_swallowed_read_failure_and_preserves_explicit_visitor_error() {
    let (root, mut source) = source(&[b"a"]);
    source.nodes.remove(&root.node_root);
    let mut allowance = io();
    let result = StateScanContext {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io: &mut allowance,
    }
    .with_reader(
        || false,
        |reader| {
            assert!(reader.get(&root, b"a").is_err());
            Ok::<_, String>(())
        },
    );
    assert!(matches!(
        result,
        Err(StateScanError::Invalid(
            "scoped state reader failure ignored"
        ))
    ));
    let result = StateScanContext {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io: &mut allowance,
    }
    .with_reader(|| false, |_| Err::<(), _>("actual typed visitor error"));
    assert!(matches!(
        result,
        Err(StateScanError::Visitor("actual typed visitor error"))
    ));
}
#[test]
fn scoped_reader_budget_cancel_pool_class_and_admission_fail_closed() {
    let (root, source) = source(&[b"a"]);
    let mut allowance = io();
    allowance.nodes_remaining = 0;
    let result = StateScanContext {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io: &mut allowance,
    }
    .with_reader(
        || false,
        |reader| {
            let _ = reader.get(&root, b"a");
            Ok::<_, String>(())
        },
    );
    assert!(matches!(result, Err(StateScanError::ResourceLimited)));
    let cancelled = Cell::new(false);
    let mut allowance = io();
    let result = StateScanContext {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io: &mut allowance,
    }
    .with_reader(
        || cancelled.get(),
        |reader| {
            cancelled.set(true);
            let _ = reader.get(&root, b"a");
            Err::<(), _>("ignored on cancellation")
        },
    );
    assert!(matches!(result, Err(StateScanError::Cancelled)));
    let foreign = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    let called = Cell::new(false);
    for (memory, class) in [
        (&foreign, MemoryClass::Ordinary),
        (&source.memory, MemoryClass::Recovery),
    ] {
        let result = StateScanContext {
            source: &source,
            memory,
            class,
            io: &mut allowance,
        }
        .with_reader(
            || false,
            |_| {
                called.set(true);
                Ok::<_, String>(())
            },
        );
        assert!(matches!(
            result,
            Err(StateScanError::Invalid("state scan source pool/class"))
        ));
    }
    assert!(!called.get());
    let held = source
        .memory
        .reserve((15 << 20) - source.memory.used(), MemoryClass::Ordinary)
        .unwrap();
    let exhausted = StateScanContext {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io: &mut allowance,
    }
    .with_reader(
        || false,
        |_| {
            called.set(true);
            Ok::<_, String>(())
        },
    );
    assert!(matches!(exhausted, Err(StateScanError::Memory(_))));
    assert!(!called.get());
    let result = StateScanContext {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io: &mut allowance,
    }
    .with_reader(
        || true,
        |_| {
            called.set(true);
            Ok::<_, String>(())
        },
    );
    assert!(matches!(result, Err(StateScanError::Cancelled)));
    assert!(!called.get());
    drop(held);
}
