use super::*;
use crate::controlled_authority::fencing::{
    acquire_provider_lease_bounded_v1, sign_native_checkpoint_bounded_v1,
};
use crate::controlled_authority::formal::with_signed_checkpoint_fixture;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalBoundedWorkV1 {
    FormalBoundedWorkV1::new(100_000_000, 512 << 20).unwrap()
}
#[test]
fn genuine_signed_checkpoint_matches_original_codec_verifier_binding_and_releases_fd() {
    let memory = memory();
    let meter = work();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    with_signed_checkpoint_fixture(
        false,
        None,
        &mut ctx,
        |provider, instance, initial, policy, draft, ctx| {
            let lease = acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                ctx,
            )
            .unwrap();
            let signed =
                sign_native_checkpoint_bounded_v1(provider.as_ref(), lease, draft, ctx).unwrap();
            let expected_wire = encode(&Hot {
                body: signed.draft().body(),
                signature: signed.endpoint_signature_hex(),
            })
            .unwrap();
            let output = prepare_signed_native_checkpoint_bounded_v1(signed, ctx).unwrap();
            assert_eq!(output.bytes(), expected_wire);
            let decoded =
                Store::verify_public(output.bytes(), &instance.endpoint_public_key).unwrap();
            assert_eq!(&decoded, output.source().draft().body());
            assert_eq!(Store::binding(&decoded, output.bytes()), *output.binding());
            assert_eq!(output.source().checkpoint(), initial);
            assert!(output.uses_budget(&memory));
            assert_eq!(output.reserved_bytes(), memory.used());
            drop(output);
            assert_eq!(memory.used(), 0);
            let reopened = acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                ctx,
            )
            .unwrap();
            assert_eq!(reopened.checkpoint(), initial);
            assert!(reopened.pending().is_none());
            drop(reopened);
            assert_eq!(memory.used(), 0);
        },
    );
}
#[test]
fn final_signed_wire_has_an_independent_exact_64k_limit() {
    for revision in [22, 23] {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        with_signed_checkpoint_fixture(
            false,
            Some(revision),
            &mut ctx,
            |provider, instance, initial, policy, draft, ctx| {
                assert_eq!(initial.revision, revision);
                let lease = acquire_provider_lease_bounded_v1(
                    provider.as_ref(),
                    instance,
                    initial,
                    policy,
                    ctx,
                )
                .unwrap();
                let signed =
                    sign_native_checkpoint_bounded_v1(provider.as_ref(), lease, draft, ctx)
                        .unwrap();
                let oldwire = encode(&Hot {
                    body: signed.draft().body(),
                    signature: signed.endpoint_signature_hex(),
                })
                .unwrap();
                assert_eq!(oldwire.len(), 65536 + usize::from(revision == 23));
                assert!(signed.draft().preimage().len() <= 65536);
                let result = prepare_signed_native_checkpoint_bounded_v1(signed, ctx);
                if revision == 22 {
                    let paid = result.unwrap();
                    assert_eq!(paid.bytes(), oldwire);
                    Store::verify_public(paid.bytes(), &instance.endpoint_public_key).unwrap();
                    assert_eq!(memory.used(), paid.reserved_bytes());
                    drop(paid);
                } else {
                    assert!(
                        result
                            .unwrap_err()
                            .to_string()
                            .contains("signed native checkpoint byte bound")
                    );
                }
                assert_eq!(memory.used(), 0);
                let reopened = acquire_provider_lease_bounded_v1(
                    provider.as_ref(),
                    instance,
                    initial,
                    policy,
                    ctx,
                )
                .unwrap();
                assert_eq!(reopened.checkpoint(), initial);
                drop(reopened);
                assert_eq!(memory.used(), 0);
            },
        );
    }
}
#[test]
fn foreign_pool_or_core_consumes_signature_and_closes_original_fd() {
    for foreign_pool in [true, false] {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        with_signed_checkpoint_fixture(
            false,
            None,
            &mut ctx,
            |provider, instance, initial, policy, draft, ctx| {
                let lease = acquire_provider_lease_bounded_v1(
                    provider.as_ref(),
                    instance,
                    initial,
                    policy,
                    ctx,
                )
                .unwrap();
                let signed =
                    sign_native_checkpoint_bounded_v1(provider.as_ref(), lease, draft, ctx)
                        .unwrap();
                let other_memory = super::tests::memory();
                let other_meter = work();
                let mut no = || false;
                let mut foreign = FormalBoundedContextV1::new(
                    if foreign_pool { &other_memory } else { &memory },
                    if foreign_pool { &meter } else { &other_meter },
                    &mut no,
                );
                assert!(prepare_signed_native_checkpoint_bounded_v1(signed, &mut foreign).is_err());
                assert_eq!(memory.used(), 0);
                assert_eq!(other_memory.used(), 0);
                let reopened = acquire_provider_lease_bounded_v1(
                    provider.as_ref(),
                    instance,
                    initial,
                    policy,
                    ctx,
                )
                .unwrap();
                drop(reopened);
                assert_eq!(memory.used(), 0);
            },
        );
    }
}
#[test]
fn cancelled_consumer_preserves_typed_cause_drops_owner_and_allows_reacquire() {
    let memory = memory();
    let meter = work();
    let cancel = std::cell::Cell::new(false);
    let mut stop = || cancel.get();
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    with_signed_checkpoint_fixture(
        false,
        None,
        &mut ctx,
        |provider, instance, initial, policy, draft, ctx| {
            let lease = acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                ctx,
            )
            .unwrap();
            let signed =
                sign_native_checkpoint_bounded_v1(provider.as_ref(), lease, draft, ctx).unwrap();
            cancel.set(true);
            let error = prepare_signed_native_checkpoint_bounded_v1(signed, ctx).unwrap_err();
            assert!(matches!(
                error,
                BoundedAcquireErrorV1::Work(FormalError::Invalid(_))
            ));
            assert!(error.to_string().contains("bounded archive read cancelled"));
            assert_eq!(memory.used(), 0);
            cancel.set(false);
            let reopened = acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                ctx,
            )
            .unwrap();
            drop(reopened);
            assert_eq!(memory.used(), 0);
        },
    );
}

#[test]
fn same_pool_exhausted_admission_returns_no_wire_owner_and_releases_signature() {
    let memory = memory();
    let meter = work();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    with_signed_checkpoint_fixture(
        false,
        None,
        &mut ctx,
        |provider, instance, initial, policy, draft, ctx| {
            let lease = acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                ctx,
            )
            .unwrap();
            let signed =
                sign_native_checkpoint_bounded_v1(provider.as_ref(), lease, draft, ctx).unwrap();
            let pressure = memory
                .reserve((256 << 20) - memory.used(), MemoryClass::Ordinary)
                .unwrap();
            let pressure_bytes = pressure.bytes();
            assert!(prepare_signed_native_checkpoint_bounded_v1(signed, ctx).is_err());
            assert_eq!(memory.used(), pressure_bytes);
            drop(pressure);
            assert_eq!(memory.used(), 0);
            let reopened = acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                ctx,
            )
            .unwrap();
            drop(reopened);
            assert_eq!(memory.used(), 0);
        },
    );
}
