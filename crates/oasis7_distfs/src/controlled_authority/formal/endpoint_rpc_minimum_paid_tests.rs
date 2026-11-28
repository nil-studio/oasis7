#![cfg(unix)]
use super::super::super::{
    FormalArchiveReadWorkV1 as Work,
    store::{HistoryNode, paid_minimum_read_count},
    tests::with_minimum_history_fixture,
};
use super::*;
use std::{cell::Cell, fs, mem::size_of};

const POOL: usize = 256 << 20;
fn resources() -> (MemoryBudget, Work) {
    (
        MemoryBudget::new(POOL, 0).unwrap(),
        Work::new(4_000_000, 1 << 32).unwrap(),
    )
}
fn child_active(work: &Work, pool: &MemoryBudget) -> bool {
    work.bounded_context(pool, &mut || false)
        .work()
        .is_child_active()
}
fn copy_fee(wire: &[Vec<u8>]) -> usize {
    size_of::<Vec<Vec<u8>>>()
        + wire.len() * size_of::<Vec<u8>>()
        + wire.iter().map(Vec::len).sum::<usize>()
}
fn respond(
    endpoint: &mut FileFormalEndpoint,
    minimum: &FormalHeadRefV1,
    guarded: bool,
    budget: RpcReadBudget<'_>,
) -> Result<RpcResponseOwner, FormalError> {
    if guarded {
        endpoint.sign_formal_rpc_read_response(
            "21".repeat(32),
            "22".repeat(32),
            minimum.clone(),
            &Command::Head,
            Some(budget),
        )
    } else {
        endpoint.sign_formal_rpc_response(
            "21".repeat(32),
            "22".repeat(32),
            minimum.clone(),
            Ok(Value::Complete),
            Some(budget),
        )
    }
}

#[test]
fn response_ancestry_admission_and_cancel_do_not_poison_or_leak() {
    with_minimum_history_fixture(|endpoint, minimum| {
        let expected = endpoint.store.minimum_witness(minimum).unwrap();
        let bytes = copy_fee(&expected);
        let (pool, work) = resources();
        for cancel in [false, true] {
            let mut guard = endpoint.guard().unwrap();
            let source = endpoint
                .minimum_witness_under_guard(
                    minimum,
                    Some(&mut RpcReadBudget::new(&pool, &work, &mut || false)),
                )
                .unwrap();
            let original_fee = pool.used();
            let gate = (!cancel).then(|| {
                pool.reserve(POOL - original_fee - bytes + 1, MemoryClass::Ordinary)
                    .unwrap()
            });
            let before = work.remaining_steps();
            let mut stop = || cancel && pool.used() == original_fee + bytes;
            let result = endpoint.sign_formal_rpc_response_with_guard(
                ("21".repeat(32), "22".repeat(32), minimum.clone()),
                Ok(Value::Complete),
                source,
                guard.as_mut(),
                Some(RpcReadBudget::new(&pool, &work, &mut stop)),
            );
            let label = if cancel {
                "bounded archive read cancelled"
            } else {
                "formal minimum response admission"
            };
            assert_eq!(result.err().unwrap(), invalid(label));
            assert!(work.remaining_steps() < before);
            drop(guard);
            drop(gate);
            assert_eq!(pool.used(), 0);
            assert!(!child_active(&work, &pool));
            assert_eq!(endpoint.head().unwrap().position, 4);
        }
    });
}

#[test]
fn response_ancestry_keeps_original_and_copy_fees_until_owner_drop() {
    with_minimum_history_fixture(|endpoint, minimum| {
        let expected = endpoint.store.minimum_witness(minimum).unwrap();
        assert_eq!(expected.len(), 2);
        let checkpoint = endpoint.store.checkpoint_bytes().unwrap();
        let (pool, work) = resources();
        let original = endpoint
            .store
            .minimum_witness_bounded(minimum, &pool, &work, &mut || false)
            .unwrap();
        assert!(original.uses_budget(&pool));
        let original_fee = pool.used();
        assert!(
            original_fee > original.reserved_bytes(),
            "raw element owners remain live"
        );
        drop(original);
        let retained =
            original_fee + copy_fee(&expected) + 2 * (checkpoint.len() + size_of::<Vec<u8>>());
        for guarded in [false, true] {
            let before = work.remaining_read_bytes();
            let owner = respond(
                endpoint,
                minimum,
                guarded,
                RpcReadBudget::new(&pool, &work, &mut || false),
            )
            .unwrap();
            let response = owner.response();
            assert_eq!(response.body.ancestry, expected);
            verify_formal_rpc_checkpoint(&response.body, &endpoint.authority, endpoint.role)
                .unwrap();
            assert_eq!(pool.used(), retained);
            assert!(work.remaining_read_bytes() < before);
            assert!(!child_active(&work, &pool));
            drop(owner);
            assert_eq!(pool.used(), 0);
        }
        let alien = MemoryBudget::new(POOL, 0).unwrap();
        let other_work = Work::new(4_000_000, 1 << 32).unwrap();
        for (memory, meter) in [(&alien, &work), (&pool, &other_work)] {
            let source = MinimumSource::load(
                &endpoint.store,
                minimum,
                Some(&mut RpcReadBudget::new(&pool, &work, &mut || false)),
            )
            .unwrap();
            let result =
                source.into_wire(Some(&mut RpcReadBudget::new(memory, meter, &mut || false)));
            assert_eq!(
                result.err().unwrap(),
                invalid("formal minimum response budget")
            );
            assert_eq!((pool.used(), alien.used()), (0, 0));
        }
    });
}

#[test]
fn response_minimum_nonroot_invalid_and_io_keep_original_poison_boundaries() {
    for io in [false, true] {
        for consumer in 0..3 {
            with_minimum_history_fixture(|endpoint, minimum| {
                let (pool, work) = resources();
                let expected = endpoint.store.minimum_witness(minimum).unwrap();
                let head: HistoryNode = serde_cbor::from_slice(&expected[0]).unwrap();
                let directory = endpoint.store.path.join("formal-objects-v1");
                let id = &head.ancestors[1].node;
                let path = directory.join(id);
                let original = fs::read(&path).unwrap();
                let permissions = fs::metadata(&path).unwrap().permissions();
                assert_eq!(original, expected[1]);
                assert!(!original.is_empty() && original.len() < 4096);
                let peak = Cell::new(0);
                if io {
                    for absent in [
                        directory.join(".generation-current-v1"),
                        directory.join(".representation-v1").join(id),
                    ] {
                        assert_eq!(
                            fs::symlink_metadata(absent).unwrap_err().kind(),
                            std::io::ErrorKind::NotFound
                        );
                    }
                    let start = paid_minimum_read_count();
                    let mut observe = || {
                        if paid_minimum_read_count() == start + 1 && child_active(&work, &pool) {
                            peak.set(peak.get().max(pool.used()));
                        }
                        false
                    };
                    drop(
                        endpoint
                            .store
                            .minimum_witness_bounded(minimum, &pool, &work, &mut observe)
                            .unwrap(),
                    );
                    assert!(peak.get() > 0);
                    assert_eq!(pool.used(), 0);
                } else {
                    fs::write(&path, b"corrupt").unwrap();
                }
                let start = paid_minimum_read_count();
                let fired = Cell::new(false);
                let mut stop = || {
                    // Raw output makes a strict peak above legacy control alone.
                    // Its first callback is seek's Work step after open/len/admission,
                    // before exact() reads the body; no callback ordinal is used.
                    if io
                        && !fired.get()
                        && paid_minimum_read_count() == start + 1
                        && child_active(&work, &pool)
                        && pool.used() == peak.get()
                    {
                        fs::OpenOptions::new()
                            .write(true)
                            .open(&path)
                            .unwrap()
                            .set_len(0)
                            .unwrap();
                        fired.set(true);
                    }
                    false
                };
                let before = work.remaining_read_bytes();
                let result = if consumer == 0 {
                    endpoint.verify_minimum_bounded(minimum, &pool, &work, &mut stop)
                } else {
                    respond(
                        endpoint,
                        minimum,
                        consumer == 1,
                        RpcReadBudget::new(&pool, &work, &mut stop),
                    )
                    .map(drop)
                };
                fs::write(&path, original).unwrap();
                fs::set_permissions(&path, permissions).unwrap();
                let error = result.unwrap_err();
                if io {
                    assert!(
                        fired.get(),
                        "truncate only after the opened output is admitted"
                    );
                    assert!(matches!(error, FormalError::Io(_)), "{error:?}");
                } else {
                    assert!(matches!(error, FormalError::Invalid(_)), "{error:?}");
                }
                if consumer == 2 {
                    assert_eq!(endpoint.head().unwrap().position, 4);
                } else {
                    assert_eq!(
                        endpoint.head().unwrap_err(),
                        FormalError::Io("formal endpoint outcome unknown; reopen required".into())
                    );
                }
                assert!(work.remaining_read_bytes() < before);
                assert!(!child_active(&work, &pool));
                assert_eq!(pool.used(), 0);
            });
        }
    }
}
