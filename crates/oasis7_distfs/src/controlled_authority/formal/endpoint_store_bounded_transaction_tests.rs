use super::super::super::tests::with_signed_request_states_fixture;
use super::*;
use crate::controlled_authority::formal::endpoint::with_bounded_transaction_fixture;
fn memory() -> MemoryBudget {
    MemoryBudget::new(512 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(200_000_000, 4u64 << 30).unwrap()
}
fn frozen(
    mut run: impl FnMut(
        &PaidRequestStateV1,
        &str,
        &FormalRequestStateV1,
        &MemoryBudget,
        &FormalArchiveReadWorkV1,
    ),
) {
    with_signed_request_states_fixture(|stage, endpoint, request, original| {
        if stage != "Frozen" {
            return;
        }
        let memory = memory();
        let work = work();
        let source = endpoint
            .load_bounded(request, &memory, &work, &mut || false)
            .unwrap()
            .unwrap();
        run(&source, request, original, &memory, &work);
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn real_pending_index_transaction_writes_hot_finishes_same_fd_and_promotes_paid_cache() {
    frozen(|source, request, original, memory, work| {
        with_bounded_transaction_fixture(|store, provider, instance, policy| {
            let before = store.checkpoint.clone();
            let update = store
                .prepare_request_update_bounded(source, memory, work, &mut || false)
                .unwrap();
            let entry = update.entry_id().to_owned();
            store
                .publish_pending_update_bounded(
                    update,
                    provider,
                    instance,
                    policy,
                    memory,
                    work,
                    &mut || false,
                )
                .unwrap();
            assert_eq!(store.body.revision, before.revision + 1);
            assert_eq!(store.body.pending.as_deref(), Some(entry.as_str()));
            assert_eq!(
                store.lookup(request).unwrap().as_deref(),
                Some(entry.as_str())
            );
            assert_eq!(
                &store.get::<FormalRequestStateV1>(&entry).unwrap(),
                original
            );
            let bytes = store.checkpoint_bytes().unwrap();
            assert_eq!(
                Store::verify_public(&bytes, &instance.endpoint_public_key).unwrap(),
                store.body
            );
            assert_eq!(Store::binding(&store.body, &bytes), store.checkpoint);
            let mut no = || false;
            let mut ctx = work.bounded_context(memory, &mut no);
            let lease = acquire_provider_lease_bounded_v1(
                provider,
                instance,
                &store.checkpoint,
                policy,
                &mut ctx,
            )
            .unwrap();
            assert_eq!(lease.checkpoint(), &store.checkpoint);
            assert!(lease.pending().is_none());
            assert_eq!(lease.native_checkpoint_bytes(), Some(bytes.as_slice()));
            drop(lease);
            assert_eq!(
                memory.used(),
                source.reserved_bytes()
                    + store._bounded_cache_fee.as_ref().unwrap().reserved_bytes()
                    + store
                        ._bounded_checkpoint_fee
                        .as_ref()
                        .unwrap()
                        .reserved_bytes()
            );
        });
        assert_eq!(memory.used(), source.reserved_bytes());
    });
}
#[test]
fn after_begin_and_after_disk_errors_close_fd_and_fresh_acquire_reconciles_actual_hot() {
    frozen(|source, request, _, memory, work| {
        for fault in [1, 2] {
            with_bounded_transaction_fixture(|store, provider, instance, policy| {
                let before = store.checkpoint.clone();
                let old = store.checkpoint_bytes().unwrap();
                let update = store
                    .prepare_request_update_bounded(source, memory, work, &mut || false)
                    .unwrap();
                store.fault = Some(fault);
                let error = store
                    .publish_pending_update_bounded(
                        update,
                        provider,
                        instance,
                        policy,
                        memory,
                        work,
                        &mut || false,
                    )
                    .unwrap_err();
                assert!(matches!(
                    error,
                    BoundedAcquireErrorV1::Work(FormalError::Io(_))
                ));
                assert_eq!(store.checkpoint, before);
                if fault == 1 {
                    assert_eq!(store.checkpoint_bytes().unwrap(), old);
                } else {
                    assert_ne!(store.checkpoint_bytes().unwrap(), old);
                }
                assert_eq!(memory.used(), source.reserved_bytes());
                store.fault = None;
                store
                    .reconcile_pending_update_bounded(
                        provider,
                        instance,
                        policy,
                        memory,
                        work,
                        &mut || false,
                    )
                    .unwrap();
                assert_eq!(store.checkpoint.revision, before.revision + 1);
                assert!(store.lookup(request).unwrap().is_some());
                let bytes = store.checkpoint_bytes().unwrap();
                assert_eq!(
                    Store::binding(
                        &Store::verify_public(&bytes, &instance.endpoint_public_key).unwrap(),
                        &bytes
                    ),
                    store.checkpoint
                );
                let mut no = || false;
                let mut ctx = work.bounded_context(memory, &mut no);
                let lease = acquire_provider_lease_bounded_v1(
                    provider,
                    instance,
                    &store.checkpoint,
                    policy,
                    &mut ctx,
                )
                .unwrap();
                assert!(lease.pending().is_none());
                assert_eq!(lease.checkpoint(), &store.checkpoint);
                drop(lease);
            });
            assert_eq!(memory.used(), source.reserved_bytes());
        }
    });
}
#[test]
fn cancellation_and_foreign_meter_consume_update_without_changing_hot_or_provider() {
    frozen(|source, _, _, memory, work| {
        for mode in [0, 1] {
            with_bounded_transaction_fixture(|store, provider, instance, policy| {
                let before = store.checkpoint_bytes().unwrap();
                let update = store
                    .prepare_request_update_bounded(source, memory, work, &mut || false)
                    .unwrap();
                let other = super::tests::work();
                assert!(
                    store
                        .publish_pending_update_bounded(
                            update,
                            provider,
                            instance,
                            policy,
                            memory,
                            if mode == 1 { &other } else { work },
                            &mut || mode == 0
                        )
                        .is_err()
                );
                assert_eq!(store.checkpoint_bytes().unwrap(), before);
                assert_eq!(memory.used(), source.reserved_bytes());
                let mut no = || false;
                let mut ctx = work.bounded_context(memory, &mut no);
                let lease = acquire_provider_lease_bounded_v1(
                    provider,
                    instance,
                    &store.checkpoint,
                    policy,
                    &mut ctx,
                )
                .unwrap();
                assert!(lease.pending().is_none());
                drop(lease);
            });
        }
    });
}

#[test]
fn cancelled_atomic_write_removes_only_its_created_temporary() {
    with_bounded_transaction_fixture(|store, _, _, _| {
        let memory = memory();
        let work = work();
        let before = store.checkpoint_bytes().unwrap();
        let mut calls = 0;
        let result = store.atomic_hot_bounded(&before, &memory, &work, &mut || {
            calls += 1;
            calls == 6
        });
        assert!(result.is_err());
        assert_eq!(calls, 6);
        assert_eq!(store.checkpoint_bytes().unwrap(), before);
        assert_eq!(memory.used(), 0);
        assert!(!fs::read_dir(&store.path).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".formal-temp-")
        }));
    });
}
