use super::*;
use crate::controlled_authority::formal::transport::Value;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1 as Context, FormalBoundedWorkV1 as Work,
};
use oasis7_proto::memory_budget::MemoryBudget;
use std::{cell::Cell, mem::size_of};

fn with_checkpoint_endpoint(
    run: impl FnOnce(&Fixture, &mut FileFormalEndpoint, &AtomicUsize, &AtomicUsize),
) {
    let fixture = Fixture::new();
    let oracle = Oracle::start(&fixture, EndpointRole::Primary);
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let signatures = Arc::new(AtomicUsize::new(0));
    let mut endpoint = FileFormalEndpoint::open_protected(
        &fixture.pdir,
        FormalEndpointAuthorizationV1 {
            authority: fixture.authority.clone(),
            original: fixture.authority.clone(),
            recovery: None,
            parent: None,
        },
        EndpointRole::Primary,
        Arc::new(CountingProvider {
            inner: oracle.provider(&fixture),
            acquisitions: Arc::clone(&acquisitions),
            signatures: Arc::clone(&signatures),
        }),
        None,
    )
    .unwrap();
    acquisitions.store(0, Ordering::SeqCst);
    signatures.store(0, Ordering::SeqCst);
    run(&fixture, &mut endpoint, &acquisitions, &signatures);
}

#[test]
fn checkpoint_original_is_paid_before_read_and_released_with_its_owner() {
    with_checkpoint_endpoint(|_, endpoint, _, _| {
        let original = endpoint.store.checkpoint_bytes().unwrap();
        assert!(!original.is_empty());
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let work = Work::new(1_000, 1 << 20).unwrap();
        let peak = Cell::new(0);
        let mut stop = || {
            peak.set(peak.get().max(memory.used()));
            false
        };
        let paid = endpoint
            .store
            .checkpoint_bytes_bounded(&mut Context::new(&memory, &work, &mut stop))
            .unwrap();
        assert_eq!(paid.as_slice(), original.as_slice());
        assert!(paid.uses_budget(&memory));
        assert_eq!(paid.reserved_bytes(), original.len() + size_of::<Vec<u8>>());
        assert_eq!(memory.used(), paid.reserved_bytes());
        assert!(peak.get() > memory.used(), "path and original overlap");
        assert!(work.remaining_steps() < 1_000);
        assert!(work.remaining_read_bytes() < (1 << 20) - original.len() as u64);
        drop(paid);
        assert_eq!(memory.used(), 0);
        let tight = MemoryBudget::new(peak.get() - 1, 0).unwrap();
        let error = endpoint
            .store
            .checkpoint_bytes_bounded(&mut Context::new(&tight, &work, &mut || false))
            .unwrap_err();
        assert_eq!(error, invalid("bounded generation metadata admission"));
        assert_eq!(tight.used(), 0);
        assert!(!work.is_child_active());
    });
}

#[test]
fn checkpoint_copy_refusal_precedes_signing_and_success_retains_both_allocations() {
    with_checkpoint_endpoint(|fixture, endpoint, acquisitions, signatures| {
        let original = endpoint.store.checkpoint_bytes().unwrap();
        let fee = original.len() + size_of::<Vec<u8>>();
        let work = Work::new(1_000_000, 1 << 30).unwrap();
        for limit in [2 * fee - 1, 1 << 20] {
            let memory = MemoryBudget::new(limit, 0).unwrap();
            let mut stop = || false;
            let mut ctx = Context::new(&memory, &work, &mut stop);
            // The one-byte-short pool admits the actual HOT original first.
            let raw = endpoint.store.checkpoint_bytes_bounded(&mut ctx).unwrap();
            assert_eq!(raw.reserved_bytes(), fee);
            drop(raw);
            acquisitions.store(0, Ordering::SeqCst);
            signatures.store(0, Ordering::SeqCst);
            let result = endpoint.sign_formal_rpc_response(
                h(21),
                h(22),
                fixture.authority.definition().world.genesis_head().unwrap(),
                Ok(Value::Complete),
                Some(&mut ctx),
            );
            assert_eq!(acquisitions.load(Ordering::SeqCst), 1);
            if limit == 2 * fee - 1 {
                let error = result
                    .err()
                    .expect("original plus copy must be admitted together");
                assert_eq!(error, invalid("formal RPC checkpoint copy admission"));
                assert_eq!(signatures.load(Ordering::SeqCst), 0);
            } else {
                let owner = result.unwrap();
                assert_eq!(owner.response().body.checkpoint_bytes, original);
                assert!(owner.response().protected_signature.is_some());
                assert_eq!(signatures.load(Ordering::SeqCst), 1);
                assert_eq!(memory.used(), 2 * fee);
                drop(owner);
            }
            assert_eq!(memory.used(), 0);
            assert!(!work.is_child_active());
        }
    });
}

#[cfg(unix)]
#[test]
fn checkpoint_paid_read_preserves_hot_group_write_permission_policy() {
    use std::os::unix::fs::PermissionsExt;
    with_checkpoint_endpoint(|fixture, endpoint, _, _| {
        let path = fixture.pdir.join("formal-checkpoint-v1.cbor");
        let original = fs::metadata(&path).unwrap().permissions();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o660)).unwrap();
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let work = Work::new(1_000, 1 << 20).unwrap();
        let legacy = endpoint.store.checkpoint_bytes();
        let paid =
            endpoint
                .store
                .checkpoint_bytes_bounded(&mut Context::new(&memory, &work, &mut || false));
        fs::set_permissions(&path, original).unwrap();
        assert_eq!(
            legacy.unwrap().as_slice(),
            paid.as_ref().unwrap().as_slice()
        );
        drop(paid);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn checkpoint_child_cancellation_keeps_first_cause_and_continuing_work() {
    with_checkpoint_endpoint(|_, endpoint, _, _| {
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let work = Work::new(1_000, 1 << 20).unwrap();
        let mut child_calls = 0;
        let mut stop = || {
            if work.is_child_active() {
                child_calls += 1;
            }
            child_calls == 2
        };
        let error = endpoint
            .store
            .checkpoint_bytes_bounded(&mut Context::new(&memory, &work, &mut stop))
            .unwrap_err();
        assert_eq!(child_calls, 2, "cancel after the first physical read");
        assert_eq!(error, invalid("bounded generation selection cancelled"));
        assert!(work.remaining_steps() < 1_000);
        assert!(work.remaining_read_bytes() < 1 << 20);
        assert!(!work.is_child_active());
        assert_eq!(memory.used(), 0);
        work.consume_steps(work.remaining_steps(), &mut || false)
            .unwrap();
        for _ in 0..2 {
            let error = endpoint
                .store
                .checkpoint_bytes_bounded(&mut Context::new(&memory, &work, &mut || false))
                .unwrap_err();
            assert_eq!(error, invalid("bounded archive read steps"));
            assert_eq!(work.remaining_steps(), 0);
            assert!(!work.is_child_active());
            assert_eq!(memory.used(), 0);
        }
        let error = endpoint
            .store
            .checkpoint_bytes_bounded(&mut Context::new(
                &MemoryBudget::new(0, 0).unwrap(),
                &work,
                &mut || true,
            ))
            .unwrap_err();
        assert_eq!(error, invalid("bounded archive read cancelled"));
    });
}
