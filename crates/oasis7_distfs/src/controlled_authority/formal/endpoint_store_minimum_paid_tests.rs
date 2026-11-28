#![cfg(unix)]
use super::super::super::{FormalArchiveReadWorkV1 as Work, tests::with_minimum_history_fixture};
use super::*;
use std::cell::Cell;

const POOL: usize = 256 << 20;
fn resources() -> (MemoryBudget, Work) {
    let work = Work::new(4_000_000, 1 << 32).unwrap();
    (MemoryBudget::new(POOL, 0).unwrap(), work)
}
fn leave(pool: &MemoryBudget, available: usize) -> MemoryReservation {
    pool.reserve(POOL - pool.used() - available, MemoryClass::Ordinary)
        .unwrap()
}
fn refused<T>(result: Result<T, FormalError>, label: &str) {
    assert_eq!(result.err().expect("the phase must refuse"), invalid(label));
}
fn release(owner: ReservedValue<Vec<PaidRaw>>, pool: &MemoryBudget) {
    assert!(owner.uses_budget(pool));
    assert!(owner.iter().all(|r| r.0.uses_budget(pool)));
    let raw = owner.iter().map(|r| r.0.reserved_bytes()).sum::<usize>();
    assert_eq!(pool.used(), owner.reserved_bytes() + raw);
    drop(owner);
    assert_eq!(pool.used(), 0);
}
fn child_active(work: &Work, pool: &MemoryBudget) -> bool {
    work.bounded_context(pool, &mut || false)
        .work()
        .is_child_active()
}

#[test]
fn paid_minimum_keeps_real_skip_bytes_owners_and_continuing_work() {
    with_minimum_history_fixture(|ep, min| {
        let store = &ep.store;
        let expected = store.minimum_witness(min).unwrap();
        assert_eq!(expected.len(), 2);
        let node: HistoryNode = decode(&expected[0]).unwrap();
        assert_eq!(node.ancestors[1].head.position, 2);
        let (pool, work) = resources();
        let before = work.remaining_read_bytes();
        let run = || store.minimum_witness_bounded(min, &pool, &work, &mut || false);
        let paid = run().unwrap();
        assert_eq!(paid.len(), expected.len());
        for (raw, bytes) in paid.iter().zip(&expected) {
            assert_eq!(raw.as_ref(), bytes);
        }
        release(paid, &pool);
        let spent = before - work.remaining_read_bytes();
        assert!(spent > 0);
        let debit = work.remaining_read_bytes() - (spent - 1);
        work.consume_read_bytes(debit, &mut || false).unwrap();
        assert!(run().is_err());
        assert_eq!(work.remaining_read_bytes(), 0);
        assert!(!child_active(&work, &pool));
        assert_eq!(pool.used(), 0);
    });
}

#[test]
fn actual_minimum_ops_admit_raw_typed_clone_and_overlapping_outer_buffers() {
    with_minimum_history_fixture(|ep, _| {
        let store = &ep.store;
        let id = store.body.head_node.as_deref().unwrap();
        let (pool, work) = resources();
        let peak = Cell::new(0);
        let mut stop = || {
            peak.set(peak.get().max(pool.used()));
            false
        };
        let mut ops = PaidOps::new(store, &pool, &work, &mut stop).unwrap();
        let gate = leave(&pool, 0);
        refused(ops.begin(&store.body), "formal minimum clone admission");
        drop(gate);
        ops.begin(&store.body).unwrap();
        let gate = leave(&pool, 0);
        refused(ops.load(id), "bounded Store path admission");
        drop(gate);
        let raw = ops.load(id).unwrap();
        let baseline = pool.used();
        let gate = leave(&pool, 0);
        refused(ops.decode(&raw), "formal minimum decoder admission");
        drop(gate);
        assert_eq!(pool.used(), baseline);
        peak.set(baseline);
        let typed = ops.decode(&raw).unwrap();
        let typed_peak = peak.get() - baseline;
        assert!(peak.get() > pool.used(), "typed output and scratch overlap");
        drop(typed);
        let gate = leave(&pool, typed_peak - 1);
        refused(ops.decode(&raw), "formal minimum decoder admission");
        drop(gate);
        assert_eq!(pool.used(), baseline);
        let typed = ops.decode(&raw).unwrap();
        let node: &HistoryNode = typed.borrow();
        let link = &node.ancestors[1];
        let gate = leave(&pool, 0);
        refused(ops.advance(link), "formal minimum clone admission");
        drop(gate);
        let old_clones = ops.clones.bytes();
        ops.advance(link).unwrap();
        assert!(ops.clones.bytes() > old_clones);
        let mut witness = Vec::new();
        ops.push(&mut witness, raw).unwrap();
        assert_eq!(witness.capacity(), 1);
        let old_outer = ops.outer.bytes();
        let next = ops.load(&link.node).unwrap();
        let gate = leave(&pool, 2 * size_of::<PaidRaw>() - 1);
        refused(
            ops.push(&mut witness, next),
            "formal minimum outer admission",
        );
        drop(gate);
        assert_eq!((witness.len(), witness.capacity()), (1, 1));
        assert_eq!(ops.outer.bytes(), old_outer);
        let next = ops.load(&link.node).unwrap();
        ops.push(&mut witness, next).unwrap();
        assert_eq!(witness.capacity(), 2);
        let outer = size_of::<Vec<PaidRaw>>() + 2 * size_of::<PaidRaw>();
        assert_eq!(ops.outer.bytes(), outer);
        drop(typed);
        release(ops.finish(witness).unwrap(), &pool);
    });
}

#[test]
fn paid_history_decoder_preserves_actual_named_packed_indefinite_and_error_wires() {
    with_minimum_history_fixture(|ep, _| {
        let store = &ep.store;
        let node: HistoryNode = store.get(store.body.head_node.as_deref().unwrap()).unwrap();
        let named = encode(&node).unwrap();
        let mut packed = Vec::new();
        let mut encoder = serde_cbor::Serializer::new(&mut packed).packed_format();
        node.serialize(&mut encoder).unwrap();
        let mut indefinite = named.clone();
        assert_eq!(indefinite[0], 0xa4);
        indefinite[0] = 0xbf;
        indefinite.push(0xff);
        let mut trailing = named.clone();
        trailing.push(0);
        let mut unknown = named.clone();
        unknown[0] = 0xa5;
        unknown.extend_from_slice(&[0x61, b'x', 0]);
        assert!(decode::<HistoryNode>(&unknown).is_ok());
        let mut huge = vec![0x9b];
        huge.extend_from_slice(&u64::MAX.to_be_bytes());
        let (pool, work) = resources();
        for bytes in [named, packed, indefinite, trailing, unknown, huge] {
            // Alternate CAS DATA never replaces the signed checkpoint's root.
            let id = blake3::hash(&bytes).to_hex().to_string();
            fs::write(store.path.join("formal-objects-v1").join(&id), &bytes).unwrap();
            let mut stop = || false;
            let mut ops = PaidOps::new(store, &pool, &work, &mut stop).unwrap();
            let raw = ops.load(&id).unwrap();
            let before = work.remaining_steps();
            if bytes[0] == 0x9b {
                let error = ops.decode(&raw).err().unwrap();
                assert!(error.to_string().contains("layout/declared hint"));
            } else {
                match (decode::<HistoryNode>(&bytes), ops.decode(&raw)) {
                    (Ok(old), Ok(paid)) => {
                        let actual: &HistoryNode = paid.borrow();
                        assert_eq!(encode(actual).unwrap(), encode(&old).unwrap());
                    }
                    (Err(old), Err(paid)) => assert_eq!(paid, old),
                    _ => panic!("original HistoryNode codec verdict changed"),
                }
            }
            assert!(work.remaining_steps() < before);
            drop(raw);
            drop(ops);
            assert_eq!(pool.used(), 0);
        }
    });
}

#[test]
fn paid_predispatch_preserves_fork_and_nonroot_failure_poisoning() {
    for corrupt in [false, true] {
        with_minimum_history_fixture(|ep, min| {
            let (pool, work) = resources();
            let mut fork = min.clone();
            fork.record_id = "ff".repeat(32);
            let result = ep.verify_minimum_bounded(&fork, &pool, &work, &mut || false);
            let error = result.unwrap_err();
            assert!(matches!(error, FormalError::Rollback(_)));
            assert_eq!(ep.head().unwrap().position, 4);
            let store = &ep.store;
            let node: HistoryNode = store.get(store.body.head_node.as_deref().unwrap()).unwrap();
            let path = store
                .path
                .join("formal-objects-v1")
                .join(&node.ancestors[1].node);
            let original = fs::read(&path).unwrap();
            let permissions = fs::metadata(&path).unwrap().permissions();
            if corrupt {
                fs::write(&path, b"corrupt").unwrap();
            } else {
                fs::remove_file(&path).unwrap();
            }
            let old = ep.store.minimum_witness(min).unwrap_err();
            let result = ep.verify_minimum_bounded(min, &pool, &work, &mut || false);
            let error = result.unwrap_err();
            fs::write(&path, original).unwrap();
            fs::set_permissions(&path, permissions).unwrap();
            assert_eq!(std::mem::discriminant(&error), std::mem::discriminant(&old));
            assert!(matches!(
                error,
                FormalError::Invalid(_) | FormalError::Io(_)
            ));
            let poisoned =
                FormalError::Io("formal endpoint outcome unknown; reopen required".into());
            assert_eq!(ep.head().unwrap_err(), poisoned);
            assert!(!child_active(&work, &pool));
            assert_eq!(pool.used(), 0);
        });
    }
}

#[test]
fn paid_minimum_entry_and_child_cancel_preserve_cause_and_release_every_owner() {
    with_minimum_history_fixture(|ep, min| {
        for inside in [false, true] {
            let (pool, work) = resources();
            let before = work.remaining_steps();
            let mut stop = || !inside || child_active(&work, &pool);
            let result = ep
                .store
                .minimum_witness_bounded(min, &pool, &work, &mut stop);
            let label = if inside {
                "bounded generation selection cancelled"
            } else {
                "bounded archive read cancelled"
            };
            refused(result, label);
            assert_eq!(work.remaining_steps() < before, inside);
            assert!(!child_active(&work, &pool));
            assert_eq!(pool.used(), 0);
        }
    });
}
