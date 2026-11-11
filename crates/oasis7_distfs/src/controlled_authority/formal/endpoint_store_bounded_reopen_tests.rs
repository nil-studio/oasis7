use super::super::super::tests::with_signed_request_states_fixture;
use super::*;
use crate::controlled_authority::formal::endpoint::{
    with_cold_transaction_fixture, with_finish_loss_transaction_fixture,
};
fn memory() -> MemoryBudget {
    MemoryBudget::new(512 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(300_000_000, 8u64 << 30).unwrap()
}
#[test]
fn all_store_owners_drop_then_cold_reopen_and_advance_frozen_to_real_prepared_entry() {
    with_cold_transaction_fixture(|path, authority, provider, instance, policy| {
        let memory = memory();
        let work = work();
        let mut revisions = Vec::new();
        let mut entries = Vec::new();
        let mut requests = Vec::new();
        with_signed_request_states_fixture(|stage, source_endpoint, request, original| {
            if !matches!(stage, "Frozen" | "Prepared") {
                return;
            }
            let source = source_endpoint
                .load_bounded(request, &memory, &work, &mut || false)
                .unwrap()
                .unwrap();
            let mut store = Store::reopen_existing_bounded(
                path,
                authority,
                EndpointRole::Primary,
                provider,
                policy,
                &memory,
                &work,
                &mut || false,
            )
            .unwrap();
            let before = store.checkpoint.revision;
            let update = store
                .prepare_request_update_bounded(&source, &memory, &work, &mut || false)
                .unwrap();
            let entry = update.entry_id().to_owned();
            store
                .publish_pending_update_bounded(
                    update,
                    provider,
                    instance,
                    policy,
                    &memory,
                    &work,
                    &mut || false,
                )
                .unwrap();
            assert_eq!(store.checkpoint.revision, before + 1);
            assert_eq!(
                store.lookup(request).unwrap().as_deref(),
                Some(entry.as_str())
            );
            assert_eq!(
                &store.get::<FormalRequestStateV1>(&entry).unwrap(),
                original
            );
            let wire = store.checkpoint_bytes().unwrap();
            assert_eq!(
                Store::verify_public(&wire, &instance.endpoint_public_key).unwrap(),
                store.body
            );
            assert_eq!(Store::binding(&store.body, &wire), store.checkpoint);
            revisions.push(store.checkpoint.revision);
            entries.push(entry);
            requests.push(request.to_owned());
            drop(store);
            assert_eq!(memory.used(), source.reserved_bytes());
            drop(source);
            assert_eq!(memory.used(), 0);
        });
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[1], revisions[0] + 1);
        assert_eq!(requests[0], requests[1]);
        assert_ne!(entries[0], entries[1]);
        let store = Store::reopen_existing_bounded(
            path,
            authority,
            EndpointRole::Primary,
            provider,
            policy,
            &memory,
            &work,
            &mut || false,
        )
        .unwrap();
        assert_eq!(
            store.lookup(&requests[0]).unwrap().as_deref(),
            Some(entries[1].as_str())
        );
        drop(store);
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn server_committed_finish_reply_loss_all_drop_cold_reconcile_then_prepared_continues() {
    with_finish_loss_transaction_fixture(|path, authority, via, direct, instance, policy| {
        let memory = memory();
        let work = work();
        let mut first = None;
        let mut frozen_entry = None;
        let mut final_revision = None;
        with_signed_request_states_fixture(|stage, source_endpoint, request, original| {
            if !matches!(stage, "Frozen" | "Prepared") {
                return;
            }
            let source = source_endpoint
                .load_bounded(request, &memory, &work, &mut || false)
                .unwrap()
                .unwrap();
            let mut store = Store::reopen_existing_bounded(
                path,
                authority,
                EndpointRole::Primary,
                direct,
                policy,
                &memory,
                &work,
                &mut || false,
            )
            .unwrap();
            let update = store
                .prepare_request_update_bounded(&source, &memory, &work, &mut || false)
                .unwrap();
            let entry = update.entry_id().to_owned();
            if stage == "Frozen" {
                first = Some(store.checkpoint.revision);
                frozen_entry = Some(entry);
                let error = store
                    .publish_pending_update_bounded(
                        update,
                        via,
                        instance,
                        policy,
                        &memory,
                        &work,
                        &mut || false,
                    )
                    .unwrap_err();
                assert!(matches!(
                    error,
                    BoundedAcquireErrorV1::Work(FormalError::Io(_))
                        | BoundedAcquireErrorV1::Provider(fencing::FencingError::Io(_))
                ));
                assert_eq!(store.checkpoint.revision, first.unwrap());
            } else {
                // New Store constructor must recover the signed disk CPP after
                // the REAL oracle committed Finish but its reply never arrived.
                assert_eq!(store.checkpoint.revision, first.unwrap() + 1);
                assert_eq!(
                    store.lookup(request).unwrap().as_deref(),
                    frozen_entry.as_deref()
                );
                assert_ne!(Some(entry.as_str()), frozen_entry.as_deref());
                store
                    .publish_pending_update_bounded(
                        update,
                        direct,
                        instance,
                        policy,
                        &memory,
                        &work,
                        &mut || false,
                    )
                    .unwrap();
                assert_eq!(store.checkpoint.revision, first.unwrap() + 2);
                assert_eq!(
                    &store.get::<FormalRequestStateV1>(&entry).unwrap(),
                    original
                );
                final_revision = Some(store.checkpoint.revision);
            }
            drop(store);
            assert_eq!(memory.used(), source.reserved_bytes());
            drop(source);
            assert_eq!(memory.used(), 0);
        });
        assert_eq!(final_revision, Some(first.unwrap() + 2));
    });
}
#[test]
fn cold_wrong_role_policy_missing_hot_cancel_and_zero_heap_fail_closed_release_lock() {
    with_cold_transaction_fixture(|path, authority, provider, _, policy| {
        let memory = memory();
        let work = work();
        let before = fs::read(path.join(HOT)).unwrap();
        assert!(
            Store::reopen_existing_bounded(
                path,
                authority,
                EndpointRole::Replica,
                provider,
                policy,
                &memory,
                &work,
                &mut || false
            )
            .is_err()
        );
        assert_eq!(memory.used(), 0);
        let mut wrong = policy.clone();
        wrong.provider_public_key = "0".repeat(64);
        assert!(
            Store::reopen_existing_bounded(
                path,
                authority,
                EndpointRole::Primary,
                provider,
                &wrong,
                &memory,
                &work,
                &mut || false
            )
            .is_err()
        );
        assert_eq!(memory.used(), 0);
        assert!(
            Store::reopen_existing_bounded(
                path,
                authority,
                EndpointRole::Primary,
                provider,
                policy,
                &memory,
                &work,
                &mut || true
            )
            .is_err()
        );
        assert_eq!(memory.used(), 0);
        let zero = MemoryBudget::new(0, 0).unwrap();
        assert!(
            Store::reopen_existing_bounded(
                path,
                authority,
                EndpointRole::Primary,
                provider,
                policy,
                &zero,
                &work,
                &mut || false
            )
            .is_err()
        );
        assert_eq!(zero.used(), 0);
        fs::rename(path.join(HOT), path.join("saved-native.cbor")).unwrap();
        assert!(
            Store::reopen_existing_bounded(
                path,
                authority,
                EndpointRole::Primary,
                provider,
                policy,
                &memory,
                &work,
                &mut || false
            )
            .is_err()
        );
        assert!(!path.join(HOT).exists());
        assert_eq!(memory.used(), 0);
        fs::rename(path.join("saved-native.cbor"), path.join(HOT)).unwrap();
        assert_eq!(fs::read(path.join(HOT)).unwrap(), before);
        let store = Store::reopen_existing_bounded(
            path,
            authority,
            EndpointRole::Primary,
            provider,
            policy,
            &memory,
            &work,
            &mut || false,
        )
        .unwrap();
        assert!(
            Store::reopen_existing_bounded(
                path,
                authority,
                EndpointRole::Primary,
                provider,
                policy,
                &memory,
                &work,
                &mut || false
            )
            .is_err()
        );
        drop(store);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn genuine_frozen_c_prepared_t_restore_same_epoch_suffix_then_decided_changes_entry() {
    with_cold_transaction_fixture(|path, authority, provider, instance, policy| {
        let memory = memory();
        let work = work();
        let mut c = None;
        let mut t = None;
        let mut entry_t = None;
        let mut base = None;
        let mut seen = Vec::new();
        with_signed_request_states_fixture(|stage, source_endpoint, request, original| {
            if !matches!(stage, "Frozen" | "Prepared" | "Decided") {
                return;
            }
            let source = source_endpoint
                .load_bounded(request, &memory, &work, &mut || false)
                .unwrap()
                .unwrap();
            let mut store = Store::reopen_existing_bounded(
                path,
                authority,
                EndpointRole::Primary,
                provider,
                policy,
                &memory,
                &work,
                &mut || false,
            )
            .unwrap();
            if stage == "Frozen" {
                base = Some(store.checkpoint.revision);
            }
            if stage == "Decided" {
                assert_eq!(store.checkpoint.revision, base.unwrap() + 2);
                assert_eq!(store.checkpoint_bytes().unwrap(), *t.as_ref().unwrap());
                assert_eq!(
                    store.lookup(request).unwrap().as_deref(),
                    entry_t.as_deref()
                );
            }
            let update = store
                .prepare_request_update_bounded(&source, &memory, &work, &mut || false)
                .unwrap();
            let entry = update.entry_id().to_owned();
            if stage == "Decided" {
                assert_ne!(Some(entry.as_str()), entry_t.as_deref());
            }
            store
                .publish_pending_update_bounded(
                    update,
                    provider,
                    instance,
                    policy,
                    &memory,
                    &work,
                    &mut || false,
                )
                .unwrap();
            assert_eq!(
                &store.get::<FormalRequestStateV1>(&entry).unwrap(),
                original
            );
            match stage {
                "Frozen" => {
                    c = Some(store.checkpoint_bytes().unwrap());
                }
                "Prepared" => {
                    t = Some(store.checkpoint_bytes().unwrap());
                    entry_t = Some(entry);
                }
                "Decided" => {
                    assert_eq!(store.checkpoint.revision, base.unwrap() + 3);
                }
                _ => unreachable!(),
            }
            drop(store);
            assert_eq!(memory.used(), source.reserved_bytes());
            if stage == "Prepared" {
                // Fresh fixture only: replay the genuine earlier signed C bytes.
                // T and its pending=None floor remain in the actual live oracle.
                fs::write(path.join(HOT), c.as_ref().unwrap()).unwrap();
                File::open(path.join(HOT)).unwrap().sync_all().unwrap();
                File::open(path).unwrap().sync_all().unwrap();
                assert_eq!(fs::read(path.join(HOT)).unwrap(), *c.as_ref().unwrap());
            }
            drop(source);
            assert_eq!(memory.used(), 0);
            seen.push(stage.to_owned());
        });
        assert_eq!(seen, ["Frozen", "Prepared", "Decided"]);
    });
}
#[test]
fn correctly_signed_durable_marker_is_rejected_by_plain_verified_authority_before_reconcile() {
    with_cold_transaction_fixture(|path, authority, provider, instance, policy| {
        let memory = memory();
        let work = work();
        assert!(authority.durable_set_activation_digest().is_none());
        let original = fs::read(path.join(HOT)).unwrap();
        let mut body = Store::verify_public(&original, &instance.endpoint_public_key).unwrap();
        body.durable_set = Some(fencing::DurableSetNativeBindingV1 {
            transition_digest: "1".repeat(64),
            independent_inventory_digest: "2".repeat(64),
            activation_digest: Some("3".repeat(64)),
        });
        body.durable_set.as_ref().unwrap().validate().unwrap();
        // Original unique Hot codec + the actual fixed primary fixture key.
        // This is a hostile signed DATA record, never a paid owner or authority.
        let key = ed25519_dalek::SigningKey::from_bytes(&[5; 32]);
        assert_eq!(
            hex::encode(key.verifying_key().to_bytes()),
            instance.endpoint_public_key
        );
        let wire = Store::signed(&body, &key).unwrap();
        assert_eq!(
            Store::verify_public(&wire, &instance.endpoint_public_key).unwrap(),
            body
        );
        fs::write(path.join(HOT), &wire).unwrap();
        let error = Store::reopen_existing_bounded(
            path,
            authority,
            EndpointRole::Primary,
            provider,
            policy,
            &memory,
            &work,
            &mut || false,
        )
        .err()
        .unwrap();
        assert!(
            matches!(error,BoundedAcquireErrorV1::Work(FormalError::Invalid(ref s)) if s=="bounded cold imported/recovery/durable checkpoint unsupported")
        );
        assert_eq!(fs::read(path.join(HOT)).unwrap(), wire);
        assert_eq!(memory.used(), 0);
        // Independent fresh acquisition confirms the provider floor was unchanged.
        let initial = Store::binding(
            &Store::verify_public(&original, &instance.endpoint_public_key).unwrap(),
            &original,
        );
        let mut no = || false;
        let mut ctx = work.bounded_context(&memory, &mut no);
        let lease = crate::controlled_authority::fencing::acquire_provider_lease_bounded_v1(
            provider, instance, &initial, policy, &mut ctx,
        )
        .unwrap();
        assert_eq!(lease.checkpoint(), &initial);
        assert!(lease.pending().is_none());
        drop(lease);
        drop(ctx);
        assert_eq!(memory.used(), 0);
        fs::write(path.join(HOT), &original).unwrap();
    });
}

#[test]
fn authority_derived_registration_preimage_matches_original_real_initial_hot() {
    with_cold_transaction_fixture(|path, authority, _, instance, _| {
        let memory = memory();
        let work = work();
        let actual = fs::read(path.join(HOT)).unwrap();
        let body = Store::verify_public(&actual, &instance.endpoint_public_key).unwrap();
        assert_eq!(body.revision, 0);
        let source = cold_registration(
            authority,
            EndpointRole::Primary,
            &memory,
            &work,
            &mut || false,
        )
        .unwrap();
        assert_eq!(
            source.value().unsigned_checkpoint_preimage,
            Store::checkpoint_signing_bytes(&body).unwrap()
        );
        assert_eq!(
            source.value().authority_id,
            authority.authority_id().unwrap()
        );
        assert_eq!(source.value().activation_ref, authority.activation_ref());
        assert!(source.value().existing_checkpoint.is_none());
        assert!(memory.used() > 0);
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}
