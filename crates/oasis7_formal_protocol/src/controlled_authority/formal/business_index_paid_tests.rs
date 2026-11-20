use super::*;
use crate::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use std::cell::Cell;

struct Nodes {
    values: BTreeMap<String, Vec<u8>>,
    pool: MemoryBudget,
    reads: Cell<usize>,
    failure: bool,
}
impl Nodes {
    fn new(pool: &MemoryBudget) -> Self {
        Self {
            values: BTreeMap::new(),
            pool: pool.clone(),
            reads: Cell::new(0),
            failure: false,
        }
    }
    fn persist(&mut self, delta: &BusinessIndexDeltaV1) {
        for node in &delta.node_writes {
            self.values.insert(node.digest.clone(), node.bytes.clone());
        }
    }
}
impl BusinessIndexNodeReader for Nodes {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.values.get(digest).cloned())
    }
}
impl PaidBusinessIndexNodeReaderV1 for Nodes {
    fn read_node_bounded(
        &self,
        digest: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, FormalError> {
        self.reads.set(self.reads.get() + 1);
        common.consume_steps(1)?;
        if self.failure {
            return Err(bad("original reader failure"));
        }
        let Some(raw) = self.values.get(digest) else {
            return Ok(None);
        };
        common.consume_read_bytes(raw.len() as u64)?;
        let fee = self
            .pool
            .reserve(raw.len() + size_of::<Vec<u8>>(), MemoryClass::Ordinary)
            .map_err(|_| bad("fixture node admission"))?;
        Ok(Some(fee.retain(raw.to_vec())))
    }
}
fn updates(metadata: u8, key: u8, value: u8) -> Vec<BusinessIndexLeafV1> {
    super::super::tests::updates(metadata, &[(hex::encode([key; 32]), value)])
}
fn allowance() -> FormalBoundedWorkV1 {
    FormalBoundedWorkV1::new(100_000_000, 1 << 30).unwrap()
}

#[test]
fn paid_index_preserves_real_v2_delta_lookup_and_verification() {
    let memory = MemoryBudget::new(256 << 20, 0).unwrap();
    let work = allowance();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let mut nodes = Nodes::new(&memory);
    let empty = BusinessIndexRootV1::empty().unwrap();
    let first = BusinessIndexDeltaV1::build_v2(empty.clone(), updates(1, 7, 2), &nodes).unwrap();
    let paid =
        build_business_index_v2_bounded_v1(empty, updates(1, 7, 2), &nodes, &mut common).unwrap();
    assert_eq!(paid.delta(), &first);
    assert_eq!(paid.delta().new.leaf_count, 2);
    assert!(paid.uses_budget(&memory));
    assert_eq!(memory.used(), paid.reserved_bytes());
    paid.validate_context(&mut common).unwrap();
    nodes.persist(&first);
    drop(paid);
    assert_eq!(memory.used(), 0);

    let second =
        BusinessIndexDeltaV1::build_v2(first.new.clone(), updates(3, 9, 4), &nodes).unwrap();
    let paid = build_business_index_v2_bounded_v1(
        first.new.clone(),
        updates(3, 9, 4),
        &nodes,
        &mut common,
    )
    .unwrap();
    assert_eq!(paid.delta(), &second);
    assert!(
        nodes.reads.get() > 0,
        "the paid path must read the actual parent nodes"
    );
    let raw = serde_cbor::to_vec(paid.delta()).unwrap();
    let decoded = decode_business_index_delta_bounded_v1(&raw, &mut common).unwrap();
    let root =
        verify_business_index_delta_bounded_v1(&decoded, &first.new_root, &nodes, &mut common)
            .unwrap();
    assert_eq!(*root, second.new);
    assert_eq!(root.leaf_count, 3);
    nodes.persist(&second);
    let found =
        get_business_index_leaf_bounded_v1(&root, &hex::encode([7; 32]), &nodes, &mut common)
            .unwrap()
            .unwrap();
    assert_eq!(found.leaf().canonical_value, [2]);
    assert!(
        get_business_index_leaf_bounded_v1(&root, &hex::encode([8; 32]), &nodes, &mut common,)
            .unwrap()
            .is_none()
    );
    let id = canonical_business_index_root_bounded_v1(&root, &mut common).unwrap();
    assert_eq!(*id, second.new_root);
    validate_business_index_root_bounded_v1(&root, &mut common).unwrap();
    let paid_empty = empty_business_index_root_bounded_v1(&mut common).unwrap();
    assert_eq!(*paid_empty, BusinessIndexRootV1::empty().unwrap());
    assert!(work.remaining_steps() < 100_000_000);
    assert!(work.remaining_read_bytes() < 1 << 30);
    drop((paid, decoded, root, found, id, paid_empty));
    assert_eq!(memory.used(), 0);
}

#[test]
fn paid_index_owners_reject_a_new_core_or_another_pool() {
    let memory = MemoryBudget::new(64 << 20, 0).unwrap();
    let work = allowance();
    let mut stop = || false;
    let nodes = Nodes::new(&memory);
    let paid = build_business_index_v2_bounded_v1(
        BusinessIndexRootV1::empty().unwrap(),
        updates(1, 7, 2),
        &nodes,
        &mut FormalBoundedContextV1::new(&memory, &work, &mut stop),
    )
    .unwrap();
    let foreign_work = allowance();
    let foreign_pool = MemoryBudget::new(64 << 20, 0).unwrap();
    for (pool, core) in [(&memory, &foreign_work), (&foreign_pool, &work)] {
        assert_eq!(
            paid.validate_context(&mut FormalBoundedContextV1::new(pool, core, &mut stop)),
            Err(bad("business index foreign paid owner"))
        );
    }
    assert_eq!(foreign_pool.used(), 0);
    drop(paid);
    assert_eq!(memory.used(), 0);
}

#[test]
fn paid_index_preserves_append_only_errors_and_first_reader_failure() {
    let memory = MemoryBudget::new(128 << 20, 0).unwrap();
    let mut nodes = Nodes::new(&memory);
    let first = BusinessIndexDeltaV1::build_v2(
        BusinessIndexRootV1::empty().unwrap(),
        updates(1, 7, 2),
        &nodes,
    )
    .unwrap();
    nodes.persist(&first);
    let legacy =
        BusinessIndexDeltaV1::build_v2(first.new.clone(), updates(3, 7, 99), &nodes).unwrap_err();
    let work = allowance();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let error = build_business_index_v2_bounded_v1(
        first.new.clone(),
        updates(3, 7, 99),
        &nodes,
        &mut common,
    )
    .unwrap_err();
    assert_eq!(error.error(), &FormalError::Invalid(legacy));
    assert!(
        memory.used() > 0,
        "the failure retains its admitted graph phase"
    );
    drop(error);
    assert_eq!(memory.used(), 0);

    nodes.failure = true;
    nodes.reads.set(0);
    let error =
        get_business_index_leaf_bounded_v1(&first.new, &hex::encode([7; 32]), &nodes, &mut common)
            .unwrap_err();
    assert_eq!(error.error(), &bad("original reader failure"));
    assert_eq!(nodes.reads.get(), 1);
    drop(error);
    assert_eq!(memory.used(), 0);

    nodes.failure = false;
    nodes.pool = MemoryBudget::new(64 << 20, 0).unwrap();
    nodes.reads.set(0);
    let error =
        get_business_index_leaf_bounded_v1(&first.new, &hex::encode([7; 32]), &nodes, &mut common)
            .unwrap_err();
    assert_eq!(error.error(), &bad("business index raw reader pool/graph"));
    assert_eq!(nodes.reads.get(), 1);
    assert_eq!(nodes.pool.used(), 0);
    drop(error);
    assert_eq!(memory.used(), 0);
}

#[test]
fn paid_index_limits_and_cancellation_stop_before_reader_io() {
    for mode in 0..3 {
        let memory = MemoryBudget::new(if mode == 2 { 1 } else { 64 << 20 }, 0).unwrap();
        let work = FormalBoundedWorkV1::new(if mode == 1 { 0 } else { 10_000 }, 1 << 20).unwrap();
        let mut stop = || mode == 0;
        let nodes = Nodes::new(&memory);
        let error = get_business_index_leaf_bounded_v1(
            &BusinessIndexRootV1 {
                node_root: hex::encode([4; 32]),
                leaf_count: 1,
            },
            &hex::encode([7; 32]),
            &nodes,
            &mut FormalBoundedContextV1::new(&memory, &work, &mut stop),
        )
        .unwrap_err();
        let expected = match mode {
            0 => "bounded archive read cancelled",
            1 => "bounded archive read steps",
            _ => "business index complete leaf phase admission",
        };
        assert_eq!(error.error(), &bad(expected));
        assert_eq!(nodes.reads.get(), 0);
        drop(error);
        assert_eq!(memory.used(), 0);
    }
}
