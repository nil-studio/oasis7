//! Real Store draft and original fixed-key TCP oracle; no custody qualification.
use super::*;

pub(in crate::controlled_authority) fn with_remote_sign_fixture(
    pending: bool,
    ctx: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        Arc<RemoteMonotonicEndpointProvider>,
        &EndpointInstanceBindingV1,
        &EndpointCheckpointBindingV1,
        &TrustedFencingPolicyV1,
        crate::controlled_authority::formal::PaidNativeCheckpointDraftV1<'_>,
        &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ),
) {
    with_signed_checkpoint_fixture(pending, None, ctx, run);
}

// Boundary metadata is issued by the ORIGINAL signed publication protocol.
// It is unused base-parent data, not a valid parent or execution qualification.
pub(in crate::controlled_authority) fn with_signed_checkpoint_fixture(
    pending: bool,
    boundary_revision: Option<u64>,
    ctx: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        Arc<RemoteMonotonicEndpointProvider>,
        &EndpointInstanceBindingV1,
        &EndpointCheckpointBindingV1,
        &TrustedFencingPolicyV1,
        crate::controlled_authority::formal::PaidNativeCheckpointDraftV1<'_>,
        &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ),
) {
    let fixture = Fixture::new();
    let oracle = Oracle::start(&fixture, EndpointRole::Primary);
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: public(3),
        authorized_resource_bindings: fixture
            .authority
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let provider = Arc::new(
        RemoteMonotonicEndpointProvider::new(
            oracle.address,
            Duration::from_secs(3),
            key(17),
            policy.clone(),
        )
        .unwrap(),
    );
    let mut endpoint = FileFormalEndpoint::open_protected(
        &fixture.pdir,
        FormalEndpointAuthorizationV1 {
            authority: fixture.authority.clone(),
            original: fixture.authority.clone(),
            recovery: None,
            parent: None,
        },
        EndpointRole::Primary,
        provider.clone(),
        None,
    )
    .unwrap();
    if let Some(revision) = boundary_revision {
        assert!(matches!(revision, 22 | 23));
        while endpoint.store.body.revision < revision - 1 {
            let mut guard = endpoint.guard().unwrap();
            let next = endpoint.store.body.clone();
            endpoint.publish_native(next, guard.as_mut()).unwrap();
        }
        let mut next = endpoint.store.body.clone();
        let mut parent = fixture.initial.candidate.record.clone();
        parent.request_digest = "a".repeat(60_000);
        next.base_parent = Some(store::CheckpointParent {
            record: parent,
            finalizing_authority_id: fixture.authority.authority_id().unwrap(),
        });
        next.revision = revision;
        let n = Store::signed_hot_size_for_test(&next);
        let text = &mut next.base_parent.as_mut().unwrap().record.request_digest;
        if n < 65536 {
            text.extend(std::iter::repeat_n('a', 65536 - n));
        } else {
            text.truncate(text.len() - (n - 65536));
        }
        assert_eq!(Store::signed_hot_size_for_test(&next), 65536);
        let mut guard = endpoint.guard().unwrap();
        endpoint.publish_native(next, guard.as_mut()).unwrap();
        assert_eq!(endpoint.store.body.revision, revision);
        assert_eq!(endpoint.store.checkpoint_bytes().unwrap().len(), 65536);
    }
    let instance = endpoint.instance().clone();
    let initial = endpoint.store.checkpoint.clone();
    let native = endpoint.store.checkpoint_bytes().unwrap();
    let expected_pending = if pending {
        let mut guard = endpoint.guard().unwrap();
        let next = endpoint.store.body.clone();
        endpoint.store.fault = Some(1);
        let error = endpoint.publish_native(next, guard.as_mut()).unwrap_err();
        assert!(
            matches!(error, FormalError::Io(ref text) if text == "injected after provider intent")
        );
        let intent = guard
            .pending()
            .expect("real Begin installed pending intent")
            .clone();
        assert!(!intent.checkpoint_bytes.is_empty());
        assert_eq!(intent.previous, initial);
        assert_eq!(
            Store::verify_public(&intent.checkpoint_bytes, &instance.endpoint_public_key)
                .unwrap()
                .revision,
            initial.revision + 1
        );
        drop(guard);
        Some(intent)
    } else {
        None
    };
    let draft = endpoint
        .store
        .prepare_native_checkpoint_draft_bounded(ctx)
        .unwrap();
    let _ = &expected_pending;
    run(provider, &instance, &initial, &policy, draft, ctx);
    // ProtectedSign alone must not publish a Hot checkpoint or alter native data.
    assert_eq!(endpoint.store.checkpoint, initial);
    assert_eq!(endpoint.store.checkpoint_bytes().unwrap(), native);
    assert_eq!(
        Store::verify_public(&native, &instance.endpoint_public_key)
            .unwrap()
            .revision,
        initial.revision
    );
}

pub(in crate::controlled_authority) fn with_bounded_transaction_fixture(
    run: impl FnOnce(
        &mut Store,
        &RemoteMonotonicEndpointProvider,
        &EndpointInstanceBindingV1,
        &TrustedFencingPolicyV1,
    ),
) {
    let fixture = Fixture::new();
    let oracle = Oracle::start(&fixture, EndpointRole::Primary);
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: public(3),
        authorized_resource_bindings: fixture
            .authority
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let provider = Arc::new(
        RemoteMonotonicEndpointProvider::new(
            oracle.address,
            Duration::from_secs(3),
            key(17),
            policy.clone(),
        )
        .unwrap(),
    );
    let mut endpoint = FileFormalEndpoint::open_protected(
        &fixture.pdir,
        FormalEndpointAuthorizationV1 {
            authority: fixture.authority.clone(),
            original: fixture.authority.clone(),
            recovery: None,
            parent: None,
        },
        EndpointRole::Primary,
        provider.clone(),
        None,
    )
    .unwrap();
    let instance = endpoint.instance().clone();
    run(&mut endpoint.store, provider.as_ref(), &instance, &policy);
}
