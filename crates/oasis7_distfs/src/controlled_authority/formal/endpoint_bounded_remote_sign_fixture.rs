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

pub(in crate::controlled_authority) fn with_cold_transaction_fixture(
    run: impl FnOnce(
        &Path,
        &VerifiedAuthorityV1,
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
    drop(endpoint);
    run(
        &fixture.pdir,
        &fixture.authority,
        provider.as_ref(),
        &instance,
        &policy,
    );
}

/// Use `via` for exactly one publication; cold open/reconcile uses `direct`.
/// The proxy verifies the original server Finish reply before dropping it.
pub(in crate::controlled_authority) fn with_finish_loss_transaction_fixture(
    run: impl FnOnce(
        &Path,
        &VerifiedAuthorityV1,
        &RemoteMonotonicEndpointProvider,
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

    use std::io::{Read, Write};
    let instance = endpoint.instance().clone();
    drop(endpoint);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let upstream = oracle.address;
    let proxy = std::thread::spawn(move || {
        fn frame(stream: &mut std::net::TcpStream) -> Vec<u8> {
            let mut header = [0; 4];
            stream.read_exact(&mut header).unwrap();
            let n = u32::from_be_bytes(header) as usize;
            assert!((1..=1024 * 1024).contains(&n));
            let mut bytes = vec![0; n];
            stream.read_exact(&mut bytes).unwrap();
            bytes
        }
        fn send(stream: &mut std::net::TcpStream, bytes: &[u8]) {
            stream
                .write_all(&(bytes.len() as u32).to_be_bytes())
                .unwrap();
            stream.write_all(bytes).unwrap();
        }
        let (mut client, _) = listener.accept().unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut server = std::net::TcpStream::connect(upstream).unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut expected = None;
        for phase in 0..4 {
            let request = frame(&mut client);
            let decoded: SignedMonotonicOracleRequestV1 = serde_cbor::from_slice(&request).unwrap();
            match (&decoded.body.command, phase) {
                (MonotonicOracleCommandV1::Acquire(_), 0)
                | (MonotonicOracleCommandV1::ProtectedSign(_), 1) => {}
                (MonotonicOracleCommandV1::Begin(intent), 2) => expected = Some((**intent).clone()),
                (MonotonicOracleCommandV1::Finish, 3) => {}
                _ => panic!("actual Acquire/Sign/Begin/Finish sequence"),
            }
            send(&mut server, &request);
            let response = frame(&mut server);
            if phase < 3 {
                send(&mut client, &response);
                continue;
            }
            let reply: SignedMonotonicOracleReplyV1 = serde_cbor::from_slice(&response).unwrap();
            fencing::verify(
                "oasis7.formal.monotonic_oracle.response.v1",
                &reply.body,
                &reply.signature,
                &public(3),
            )
            .unwrap();
            let expected = expected.as_ref().unwrap();
            assert_eq!(reply.body.checkpoint, expected.next);
            assert!(reply.body.pending.is_none());
            assert_eq!(
                reply.body.native_checkpoint_bytes.as_ref(),
                Some(&expected.checkpoint_bytes)
            );
            // Real server commit has completed. Client observes only EOF.
            client.shutdown(std::net::Shutdown::Write).unwrap();
        }
        drop(server);
        let mut byte = [0];
        assert!(matches!(client.read(&mut byte), Ok(0)));
    });
    let via = RemoteMonotonicEndpointProvider::new(
        address,
        Duration::from_secs(3),
        key(17),
        policy.clone(),
    )
    .unwrap();
    run(
        &fixture.pdir,
        &fixture.authority,
        &via,
        provider.as_ref(),
        &instance,
        &policy,
    );
    proxy.join().unwrap();
}
