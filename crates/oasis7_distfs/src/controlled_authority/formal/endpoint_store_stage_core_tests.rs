//! Regression for the original concrete Leaf producer's paid path allowance.
use super::super::super::tests::with_store_for_bounded_index_fixture;
use super::*;
#[cfg(unix)]
#[test]
fn equivalent_trailing_separator_keeps_paid_leaf_stage_compatible() {
    with_store_for_bounded_index_fixture(|store, _| {
        // Same actual initialized namespace and live endpoint lock; only the
        // lexical directory spelling has an equivalent trailing separator.
        store.path.push("");
        assert_eq!(
            store.path.as_os_str().as_encoded_bytes().last(),
            Some(&b'/')
        );
        let memory = MemoryBudget::new(256 << 20, 0).unwrap();
        let work = FormalArchiveReadWorkV1::new(2_000_000, 256 << 20).unwrap();
        let owner = store
            .stage_index_leaf_bounded(
                &"11".repeat(32),
                &"22".repeat(32),
                &memory,
                &work,
                &mut || false,
            )
            .unwrap();
        assert_eq!(blake3::hash(owner.bytes()).to_hex().as_str(), owner.id());
        assert!(owner.uses_budget(&memory));
        drop(owner);
        assert_eq!(memory.used(), 0);
    });
}
