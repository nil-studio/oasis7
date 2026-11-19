use super::super::super::tests::{
    with_signed_request_states_fixture, with_store_for_bounded_index_fixture,
};
use super::*;
use ed25519_dalek::Signer;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(50_000_000, 1 << 30).unwrap()
}
fn key() -> String {
    hex::encode(SigningKey::from_bytes(&[5; 32]).verifying_key().to_bytes())
}

#[test]
fn verified_disk_owner_lives_with_real_pending_draft() {
    with_signed_request_states_fixture(|stage, endpoint, request, _| {
        if stage != "Prepared" {
            return;
        }
        let memory = memory();
        let work = work();
        let source = endpoint
            .load_bounded(request, &memory, &work, &mut || false)
            .unwrap()
            .unwrap();
        let source_fee = source.reserved_bytes();
        with_store_for_bounded_index_fixture(|store, _| {
            let before = store.checkpoint_bytes().unwrap();
            let update = store
                .prepare_request_update_bounded(&source, &memory, &work, &mut || false)
                .unwrap();
            let index = update.index_root().to_owned();
            let entry = update.entry_id().to_owned();
            let draft = store
                .prepare_pending_native_checkpoint_draft_from_disk_bounded(
                    update,
                    &key(),
                    &memory,
                    &work,
                    &mut || false,
                )
                .unwrap();
            assert_eq!(draft.body().pending.as_deref(), Some(entry.as_str()));
            assert_eq!(draft.body().index.as_deref(), Some(index.as_str()));
            assert_eq!(
                draft.preimage(),
                Store::checkpoint_signing_bytes(draft.body()).unwrap()
            );
            assert!(draft.uses_budget(&memory));
            assert_eq!(memory.used(), source_fee + draft.reserved_bytes());
            assert_eq!(store.checkpoint_bytes().unwrap(), before);
            drop(draft);
            assert_eq!(memory.used(), source_fee);
        });
        drop(source);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn valid_signed_disk_checkpoint_changed_from_cache_is_rejected() {
    with_store_for_bounded_index_fixture(|store, _| {
        let before = store.checkpoint_bytes().unwrap();
        let cached = store.body.clone();
        let mut hot: Hot = decode(&before).unwrap();
        hot.body.revision += 1;
        let preimage = Store::checkpoint_signing_bytes(&hot.body).unwrap();
        hot.signature = hex::encode(SigningKey::from_bytes(&[5; 32]).sign(&preimage).to_bytes());
        let changed = encode(&hot).unwrap();
        Store::verify_public(&changed, &key()).unwrap();
        fs::write(store.path.join(HOT), &changed).unwrap();
        let memory = memory();
        let error = store
            .prepare_native_checkpoint_draft_from_disk_bounded(
                &key(),
                &memory,
                &work(),
                &mut || false,
            )
            .unwrap_err();
        assert!(matches!(error, FormalError::Conflict(_)));
        assert_eq!(memory.used(), 0);
        assert_eq!(store.body, cached);
        assert_eq!(store.checkpoint_bytes().unwrap(), changed);
        fs::write(store.path.join(HOT), before).unwrap();
    });
}

#[test]
fn fresh_noop_draft_wrong_key_cancel_and_admission_release_all_fees() {
    with_store_for_bounded_index_fixture(|store, _| {
        let before = store.checkpoint_bytes().unwrap();
        let memory = memory();
        let draft = store
            .prepare_native_checkpoint_draft_from_disk_bounded(
                &key(),
                &memory,
                &work(),
                &mut || false,
            )
            .unwrap();
        assert_eq!(draft.body().revision, store.body.revision + 1);
        assert_eq!(memory.used(), draft.reserved_bytes());
        drop(draft);
        assert_eq!(memory.used(), 0);
        let wrong = hex::encode(SigningKey::from_bytes(&[6; 32]).verifying_key().to_bytes());
        assert!(
            store
                .prepare_native_checkpoint_draft_from_disk_bounded(
                    &wrong,
                    &memory,
                    &work(),
                    &mut || false,
                )
                .is_err()
        );
        let error = store
            .prepare_native_checkpoint_draft_from_disk_bounded(
                &key(),
                &memory,
                &work(),
                &mut || true,
            )
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert_eq!(memory.used(), 0);
        let zero = MemoryBudget::new(0, 0).unwrap();
        assert!(
            store
                .prepare_native_checkpoint_draft_from_disk_bounded(
                    &key(),
                    &zero,
                    &work(),
                    &mut || false,
                )
                .is_err()
        );
        assert_eq!(zero.used(), 0);
        assert_eq!(store.checkpoint_bytes().unwrap(), before);
    });
}
