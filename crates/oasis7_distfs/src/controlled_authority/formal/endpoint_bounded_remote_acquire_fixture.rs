//! Fixed-key loopback registry only: real client protocol, no durable custody proof.
use super::*;

pub(in crate::controlled_authority) fn with_remote_acquire_fixture(
    pending: bool,
    run: impl FnOnce(
        Arc<RemoteMonotonicEndpointProvider>,
        &EndpointInstanceBindingV1,
        &EndpointCheckpointBindingV1,
        &TrustedFencingPolicyV1,
        &[u8],
        Option<&CheckpointIntentV1>,
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
    run(
        provider,
        &instance,
        &initial,
        &policy,
        &native,
        expected_pending.as_ref(),
    );
}
