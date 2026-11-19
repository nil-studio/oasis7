use super::*;
use crate::controlled_authority::formal::endpoint::with_remote_sign_fixture;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalBoundedWorkV1 {
    FormalBoundedWorkV1::new(50_000_000, 256 << 20).unwrap()
}
#[test]
fn real_native_sign_consumes_draft_and_same_fd_without_advancing_checkpoint() {
    let memory = memory();
    let meter = work();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    with_remote_sign_fixture(
        false,
        &mut ctx,
        |provider, instance, initial, policy, draft, ctx| {
            let before = draft.before_checkpoint().clone();
            let preimage = draft.preimage().to_vec();
            let lease = acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                ctx,
            )
            .unwrap();
            let native = lease.native_checkpoint_bytes().unwrap().to_vec();
            let output =
                sign_native_checkpoint_bounded_v1(provider.as_ref(), lease, draft, ctx).unwrap();
            assert_eq!(output.checkpoint(), &before);
            assert_eq!(output.native_checkpoint_bytes(), Some(native.as_slice()));
            assert_eq!(output.draft().preimage(), preimage);
            assert_eq!(output.request().preimage, preimage);
            assert!(matches!(
                output.request().purpose,
                ProtectedEndpointSignaturePurposeV1::NativeCheckpoint
            ));
            assert_eq!(
                verify_protected_endpoint_signature(
                    output.signature(),
                    output.request(),
                    instance,
                    policy
                )
                .unwrap(),
                output.endpoint_signature_hex()
            );
            crate::controlled_authority::formal::verify_native_draft_signature_for_test(
                output.draft().body(),
                output.endpoint_signature_hex(),
                &instance.endpoint_public_key,
            )
            .unwrap();
            assert!(output.uses_budget(&memory));
            assert_eq!(output.reserved_bytes(), memory.used());
            assert_eq!(output.draft().identity(), meter.identity());
            drop(output);
            assert_eq!(memory.used(), 0);
            // The sole previous FD is now closed. Acquire can recover the same floor.
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
#[test]
fn real_unresolved_pending_is_denied_and_consumed_connection_closes() {
    let memory = memory();
    let meter = work();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    with_remote_sign_fixture(
        true,
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
            assert!(lease.pending().is_some());
            assert!(matches!(
                sign_native_checkpoint_bounded_v1(provider.as_ref(), lease, draft, ctx),
                Err(BoundedAcquireErrorV1::Provider(
                    FencingError::FencingUnproven
                ))
            ));
            assert_eq!(memory.used(), 0);
            let reopened = acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                ctx,
            )
            .unwrap();
            assert!(reopened.pending().is_some());
            drop(reopened);
            assert_eq!(memory.used(), 0);
        },
    );
}
#[test]
fn wrong_authentication_key_never_signs_or_preserves_a_reusable_old_owner() {
    let memory = memory();
    let meter = work();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    with_remote_sign_fixture(
        false,
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
            let wrong = RemoteMonotonicEndpointProvider::new(
                provider.address,
                provider.timeout,
                SigningKey::from_bytes(&[11; 32]),
                provider.policy.clone(),
            )
            .unwrap();
            assert!(matches!(
                sign_native_checkpoint_bounded_v1(&wrong, lease, draft, ctx),
                Err(BoundedAcquireErrorV1::Provider(
                    FencingError::FencingUnproven
                ))
            ));
            assert_eq!(memory.used(), 0);
            drop(
                acquire_provider_lease_bounded_v1(
                    provider.as_ref(),
                    instance,
                    initial,
                    policy,
                    ctx,
                )
                .unwrap(),
            );
            assert_eq!(memory.used(), 0);
        },
    );
}
#[test]
fn switching_pool_or_core_after_real_acquire_consumes_both_sources_on_error() {
    for wrong_pool in [false, true] {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        with_remote_sign_fixture(
            false,
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
                let other_memory = MemoryBudget::new(256 << 20, 0).unwrap();
                let other_work = work();
                let mut no_cancel = || false;
                let mut foreign = FormalBoundedContextV1::new(
                    if wrong_pool { &other_memory } else { &memory },
                    if wrong_pool { &meter } else { &other_work },
                    &mut no_cancel,
                );
                assert!(matches!(
                    sign_native_checkpoint_bounded_v1(
                        provider.as_ref(),
                        lease,
                        draft,
                        &mut foreign
                    ),
                    Err(BoundedAcquireErrorV1::Work(_))
                ));
                assert_eq!(memory.used(), 0);
                assert_eq!(other_memory.used(), 0);
                drop(
                    acquire_provider_lease_bounded_v1(
                        provider.as_ref(),
                        instance,
                        initial,
                        policy,
                        ctx,
                    )
                    .unwrap(),
                );
                assert_eq!(memory.used(), 0);
            },
        );
    }
}
#[test]
fn actual_draft_and_lease_are_released_on_continuous_budget_or_cancellation_failure() {
    for mode in 0..3 {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        with_remote_sign_fixture(
            false,
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
                let pressure = if mode == 0 {
                    Some(
                        memory
                            .reserve((256 << 20) - memory.used() - 1, MemoryClass::Ordinary)
                            .unwrap(),
                    )
                } else {
                    None
                };
                if mode == 1 {
                    ctx.work()
                        .consume_read_bytes(ctx.work().remaining_read_bytes(), &mut || false)
                        .unwrap();
                }
                let result = if mode == 2 {
                    let mut cancel = || true;
                    let mut stopped = FormalBoundedContextV1::new(&memory, &meter, &mut cancel);
                    sign_native_checkpoint_bounded_v1(provider.as_ref(), lease, draft, &mut stopped)
                } else {
                    sign_native_checkpoint_bounded_v1(provider.as_ref(), lease, draft, ctx)
                };
                assert!(matches!(result, Err(BoundedAcquireErrorV1::Work(_))));
                drop(result);
                assert_eq!(memory.used(), pressure.as_ref().map_or(0, |p| p.bytes()));
                drop(pressure);
                assert_eq!(memory.used(), 0);
            },
        );
    }
}

#[test]
fn actual_server_signed_then_bad_reply_closes_consumed_fd_without_old_state_return() {
    let memory = memory();
    let meter = work();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    with_remote_sign_fixture(
        false,
        &mut ctx,
        |provider, instance, initial, policy, draft, ctx| {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let upstream = provider.address;
            let proxy = std::thread::spawn(move || {
                let (mut client, _) = listener.accept().unwrap();
                client
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut server = TcpStream::connect(upstream).unwrap();
                let mut signed = false;
                for phase in 0..2 {
                    let mut header = [0; 4];
                    client.read_exact(&mut header).unwrap();
                    let mut request = vec![0; u32::from_be_bytes(header) as usize];
                    client.read_exact(&mut request).unwrap();
                    let decoded: SignedMonotonicOracleRequestV1 =
                        serde_cbor::from_slice(&request).unwrap();
                    if phase == 0 {
                        assert!(matches!(
                            decoded.body.command,
                            MonotonicOracleCommandV1::Acquire(_)
                        ));
                    } else {
                        assert!(matches!(
                            decoded.body.command,
                            MonotonicOracleCommandV1::ProtectedSign(_)
                        ));
                    }
                    server.write_all(&header).unwrap();
                    server.write_all(&request).unwrap();
                    server.read_exact(&mut header).unwrap();
                    let mut response = vec![0; u32::from_be_bytes(header) as usize];
                    server.read_exact(&mut response).unwrap();
                    if phase == 1 {
                        let mut reply: SignedMonotonicOracleReplyV1 =
                            serde_cbor::from_slice(&response).unwrap();
                        signed = reply.body.protected_signature.is_some();
                        assert!(signed);
                        let replacement = if reply.signature.starts_with("00") {
                            "ff"
                        } else {
                            "00"
                        };
                        reply.signature.replace_range(..2, replacement);
                        response = serde_cbor::to_vec(&reply).unwrap();
                    }
                    client
                        .write_all(&(response.len() as u32).to_be_bytes())
                        .unwrap();
                    client.write_all(&response).unwrap();
                }
                let mut eof = [0; 1];
                (signed, matches!(client.read(&mut eof), Ok(0)))
            });
            let via = RemoteMonotonicEndpointProvider::new(
                address,
                provider.timeout,
                provider.key.clone(),
                provider.policy.clone(),
            )
            .unwrap();
            let lease =
                acquire_provider_lease_bounded_v1(&via, instance, initial, policy, ctx).unwrap();
            assert!(matches!(
                sign_native_checkpoint_bounded_v1(&via, lease, draft, ctx),
                Err(BoundedAcquireErrorV1::Provider(_))
            ));
            let (signed, closed) = proxy.join().unwrap();
            assert!(signed && closed);
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

#[test]
fn actual_server_signed_then_caller_cancel_closes_fd_without_a_signature_owner() {
    let memory = memory();
    let meter = work();
    let mut no_cancel = || false;
    let mut initial_ctx = FormalBoundedContextV1::new(&memory, &meter, &mut no_cancel);
    with_remote_sign_fixture(
        false,
        &mut initial_ctx,
        |provider, instance, initial, policy, draft, ctx| {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let upstream = provider.address;
            let (signed_send, signed_receive) = std::sync::mpsc::channel();
            let signed_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let peer_flag = signed_flag.clone();
            let proxy = std::thread::spawn(move || {
                let (mut client, _) = listener.accept().unwrap();
                client
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut server = TcpStream::connect(upstream).unwrap();
                for phase in 0..2 {
                    let mut header = [0; 4];
                    client.read_exact(&mut header).unwrap();
                    let mut request = vec![0; u32::from_be_bytes(header) as usize];
                    client.read_exact(&mut request).unwrap();
                    let decoded: SignedMonotonicOracleRequestV1 =
                        serde_cbor::from_slice(&request).unwrap();
                    if phase == 1 {
                        assert!(matches!(
                            decoded.body.command,
                            MonotonicOracleCommandV1::ProtectedSign(_)
                        ));
                    }
                    server.write_all(&header).unwrap();
                    server.write_all(&request).unwrap();
                    server.read_exact(&mut header).unwrap();
                    let mut reply = vec![0; u32::from_be_bytes(header) as usize];
                    server.read_exact(&mut reply).unwrap();
                    if phase == 1 {
                        let actual: SignedMonotonicOracleReplyV1 =
                            serde_cbor::from_slice(&reply).unwrap();
                        assert!(actual.body.protected_signature.is_some());
                        peer_flag.store(true, std::sync::atomic::Ordering::Release);
                    }
                    // Send the original complete valid response, not a forged
                    // failure. The caller's cancellation is the sole error source.
                    client.write_all(&header).unwrap();
                    client.write_all(&reply).unwrap();
                    if phase == 1 {
                        signed_send.send(reply.len()).unwrap();
                    }
                }
                let mut eof = [0; 1];
                matches!(client.read(&mut eof), Ok(0))
            });
            let via = RemoteMonotonicEndpointProvider::new(
                address,
                provider.timeout,
                provider.key.clone(),
                provider.policy.clone(),
            )
            .unwrap();
            let lease =
                acquire_provider_lease_bounded_v1(&via, instance, initial, policy, ctx).unwrap();
            let baseline = memory.used();
            let p = draft.preimage().len();
            assert!(p < 16 * 1024);
            // Original typed serde supplies the exact outgoing wire length for
            // this fixed shape: all generated digests/nonces/signatures have fixed
            // hex lengths. This sizing DTO is never signed/sent or minted as an owner.
            let sizing = SignedMonotonicOracleRequestV1 {
                body: MonotonicOracleRequestBodyV1 {
                    schema_version: 1,
                    instance: instance.clone(),
                    authentication_public_key: lease.request.body.authentication_public_key.clone(),
                    lease: Some(lease.reply.body.lease.clone()),
                    challenge: "00".repeat(32),
                    command: MonotonicOracleCommandV1::ProtectedSign(Box::new(
                        ProtectedEndpointSigningRequestV1 {
                            schema_version: 1,
                            instance_digest: "00".repeat(32),
                            checkpoint: lease.checkpoint().clone(),
                            challenge: "00".repeat(32),
                            purpose: ProtectedEndpointSignaturePurposeV1::NativeCheckpoint,
                            preimage: draft.preimage().to_vec(),
                        },
                    )),
                },
                signature: "00".repeat(64),
            };
            let w = serde_cbor::to_vec(&sizing).unwrap().len();
            let vec_header = std::mem::size_of::<Vec<u8>>();
            let digest_phase = 4 * (64 + std::mem::size_of::<String>());
            let extra = std::mem::size_of::<PaidNativeCheckpointSignatureV1<'_>>()
                - std::mem::size_of::<PaidNativeCheckpointDraftV1<'_>>()
                - std::mem::size_of::<PaidRemoteLeaseV1>()
                - std::mem::size_of::<SignedMonotonicOracleRequestV1>();
            let q = std::mem::size_of::<SignedMonotonicOracleRequestV1>()
                + std::mem::size_of::<ProtectedEndpointSigningRequestV1>()
                + extra
                + instance_payload(instance).unwrap()
                + lease.reply.body.lease.len()
                + 64
                + 64
                + 128
                + 64
                + 64
                + checkpoint_payload(lease.checkpoint()).unwrap()
                + p;
            // The request's unsigned body B is smaller than its signed envelope W.
            // Its measured body+domain frame peak is <=2W+8+domain+two Vec headers.
            let outgoing_peak = digest_phase + q + 2 * w + 8 + REQUEST.len() + 2 * vec_header;
            let preflight_pair = 2 * [
                serde_cbor::to_vec(&policy.provider_public_key)
                    .unwrap()
                    .len(),
                serde_cbor::to_vec(&policy.authorized_resource_bindings)
                    .unwrap()
                    .len(),
                serde_cbor::to_vec(lease.checkpoint()).unwrap().len(),
                serde_cbor::to_vec(instance).unwrap().len(),
            ]
            .into_iter()
            .max()
            .unwrap()
                + 2 * vec_header
                + digest_phase;
            let raw_before_phase = digest_phase + q + LIMIT + vec_header;
            assert!(outgoing_peak < 2 * (1 << 20));
            assert!(preflight_pair < 2 * (1 << 20));
            assert!(raw_before_phase < 2 * (1 << 20));
            let mut signed_receive = Some(signed_receive);
            let mut observed = None;
            let mut cancel = || {
                if memory.used() >= baseline + 2 * (1 << 20) {
                    let n = signed_receive
                        .take()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(3))
                        .unwrap();
                    assert!(signed_flag.load(std::sync::atomic::Ordering::Acquire));
                    assert!(memory.used() >= baseline + decode_phase(n).unwrap());
                    observed = Some((n, memory.used()));
                    true
                } else {
                    false
                }
            };
            let mut signing_ctx = FormalBoundedContextV1::new(&memory, &meter, &mut cancel);
            let error = sign_native_checkpoint_bounded_v1(&via, lease, draft, &mut signing_ctx)
                .unwrap_err();
            assert!(
                matches!(error,BoundedAcquireErrorV1::Work(FormalError::Invalid(s))if s=="bounded archive read cancelled")
            );
            assert!(proxy.join().unwrap());
            let (n, paid) = observed.unwrap();
            assert!(n > 0);
            assert!(paid >= baseline + decode_phase(n).unwrap());
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
