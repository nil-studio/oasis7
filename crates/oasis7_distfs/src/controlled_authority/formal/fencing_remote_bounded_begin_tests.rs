use super::*;
use crate::controlled_authority::formal::{
    prepare_signed_native_checkpoint_bounded_v1, with_signed_checkpoint_fixture,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalBoundedWorkV1 {
    FormalBoundedWorkV1::new(100_000_000, 512 << 20).unwrap()
}
#[test]
fn genuine_begin_records_pending_on_same_fd_and_fresh_acquire_recovers_exact_intent() {
    let memory = memory();
    let meter = work();
    let mut no = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut no);
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
            let checkpoint = prepare_signed_native_checkpoint_bounded_v1(signed, ctx).unwrap();
            let expected_next = checkpoint.binding().clone();
            let expected_wire = checkpoint.bytes().to_vec();
            let begun =
                begin_signed_native_checkpoint_bounded_v1(provider.as_ref(), checkpoint, ctx)
                    .unwrap();
            assert_eq!(begun.checkpoint(), initial);
            assert_eq!(begun.pending(), begun.intent());
            assert_eq!(&begun.intent().previous, initial);
            assert_eq!(begun.intent().next, expected_next);
            assert_eq!(begun.intent().checkpoint_bytes, expected_wire);
            assert_eq!(begun.intent().next.revision, initial.revision + 1);
            assert!(begun.uses_budget(&memory));
            assert_eq!(memory.used(), begun.reserved_bytes());
            let expected = begun.intent().clone();
            drop(begun);
            assert_eq!(memory.used(), 0);
            let recovered = acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                ctx,
            )
            .unwrap();
            assert_eq!(recovered.checkpoint(), initial);
            assert_eq!(recovered.pending(), Some(&expected));
            drop(recovered);
            assert_eq!(memory.used(), 0);
        },
    );
}
#[test]
fn foreign_pool_or_core_consumes_checkpoint_without_recording_pending() {
    for wrong_pool in [false, true] {
        let memory = memory();
        let meter = work();
        let mut no = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut no);
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
                let checkpoint = prepare_signed_native_checkpoint_bounded_v1(signed, ctx).unwrap();
                let other_memory = MemoryBudget::new(256 << 20, 0).unwrap();
                let other_work = work();
                let mut no = || false;
                let mut foreign = FormalBoundedContextV1::new(
                    if wrong_pool { &other_memory } else { &memory },
                    if wrong_pool { &meter } else { &other_work },
                    &mut no,
                );
                assert!(matches!(
                    begin_signed_native_checkpoint_bounded_v1(
                        provider.as_ref(),
                        checkpoint,
                        &mut foreign
                    ),
                    Err(BoundedAcquireErrorV1::Work(_))
                ));
                assert_eq!(memory.used(), 0);
                assert_eq!(other_memory.used(), 0);
                let recovered = acquire_provider_lease_bounded_v1(
                    provider.as_ref(),
                    instance,
                    initial,
                    policy,
                    ctx,
                )
                .unwrap();
                assert!(recovered.pending().is_none());
                drop(recovered);
                assert_eq!(memory.used(), 0);
            },
        );
    }
}
#[test]
fn cancellation_and_zero_available_heap_close_fd_before_begin() {
    for heap in [false, true] {
        let memory = memory();
        let meter = work();
        let cancelled = std::cell::Cell::new(false);
        let mut stop = || cancelled.get();
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
                let checkpoint = prepare_signed_native_checkpoint_bounded_v1(signed, ctx).unwrap();
                let pressure = if heap {
                    Some(
                        memory
                            .reserve((256 << 20) - memory.used(), MemoryClass::Ordinary)
                            .unwrap(),
                    )
                } else {
                    cancelled.set(true);
                    None
                };
                let error =
                    begin_signed_native_checkpoint_bounded_v1(provider.as_ref(), checkpoint, ctx)
                        .unwrap_err();
                assert!(matches!(error, BoundedAcquireErrorV1::Work(_)));
                if !heap {
                    assert!(error.to_string().contains("bounded archive read cancelled"));
                }
                assert_eq!(memory.used(), pressure.as_ref().map_or(0, |p| p.bytes()));
                drop(pressure);
                cancelled.set(false);
                assert_eq!(memory.used(), 0);
                let recovered = acquire_provider_lease_bounded_v1(
                    provider.as_ref(),
                    instance,
                    initial,
                    policy,
                    ctx,
                )
                .unwrap();
                assert!(recovered.pending().is_none());
                drop(recovered);
                assert_eq!(memory.used(), 0);
            },
        );
    }
}
fn read_frame(stream: &mut TcpStream) -> Vec<u8> {
    let mut header = [0; 4];
    stream.read_exact(&mut header).unwrap();
    let mut bytes = vec![0; u32::from_be_bytes(header) as usize];
    stream.read_exact(&mut bytes).unwrap();
    bytes
}
fn write_frame(stream: &mut TcpStream, bytes: &[u8]) {
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(bytes).unwrap();
}
#[test]
fn server_recorded_begin_then_eof_or_bad_reply_is_unknown_and_acquire_recovers_pending() {
    for mode in 0..3 {
        let memory = memory();
        let meter = work();
        let mut no = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut no);
        with_signed_checkpoint_fixture(
            false,
            None,
            &mut ctx,
            |provider, instance, initial, policy, draft, ctx| {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let address = listener.local_addr().unwrap();
                let upstream = provider.address;
                let proxy = std::thread::spawn(move || {
                    let (mut client, _) = listener.accept().unwrap();
                    client
                        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                        .unwrap();
                    let mut server = TcpStream::connect(upstream).unwrap();
                    server
                        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                        .unwrap();
                    let mut expected = None;
                    for phase in 0..3 {
                        let request = read_frame(&mut client);
                        let decoded: SignedMonotonicOracleRequestV1 =
                            serde_cbor::from_slice(&request).unwrap();
                        match (&decoded.body.command, phase) {
                            (MonotonicOracleCommandV1::Acquire(_), 0)
                            | (MonotonicOracleCommandV1::ProtectedSign(_), 1) => {}
                            (MonotonicOracleCommandV1::Begin(intent), 2) => {
                                expected = Some((**intent).clone())
                            }
                            _ => panic!("three actual same-FD operations"),
                        }
                        write_frame(&mut server, &request);
                        let response = read_frame(&mut server);
                        if phase < 2 {
                            write_frame(&mut client, &response);
                            continue;
                        }
                        let mut reply: SignedMonotonicOracleReplyV1 =
                            serde_cbor::from_slice(&response).unwrap();
                        assert_eq!(reply.body.pending.as_ref(), expected.as_ref());
                        // An original, signed server reply proves Begin happened
                        // before the proxy introduces the observable failure.
                        verify(
                            RESPONSE,
                            &reply.body,
                            &reply.signature,
                            &public_key_for_test(),
                        )
                        .unwrap();
                        if mode == 0 {
                            client
                                .write_all(&(response.len() as u32).to_be_bytes())
                                .unwrap();
                            client.write_all(&response[..response.len() / 2]).unwrap();
                            client.shutdown(std::net::Shutdown::Write).unwrap();
                        } else {
                            if mode == 1 {
                                reply.signature.replace_range(..2, "zz");
                            } else {
                                reply.body.pending.as_mut().unwrap().next.commitment =
                                    "af".repeat(32);
                                reply.signature =
                                    sign(RESPONSE, &reply.body, &SigningKey::from_bytes(&[3; 32]))
                                        .unwrap();
                            }
                            write_frame(&mut client, &serde_cbor::to_vec(&reply).unwrap());
                        }
                    }
                    drop(server);
                    let mut byte = [0; 1];
                    let eof = matches!(client.read(&mut byte), Ok(0));
                    (expected.unwrap(), eof)
                });
                let via = RemoteMonotonicEndpointProvider::new(
                    address,
                    provider.timeout,
                    provider.key.clone(),
                    provider.policy.clone(),
                )
                .unwrap();
                let lease = acquire_provider_lease_bounded_v1(&via, instance, initial, policy, ctx)
                    .unwrap();
                let signed = sign_native_checkpoint_bounded_v1(&via, lease, draft, ctx).unwrap();
                let checkpoint = prepare_signed_native_checkpoint_bounded_v1(signed, ctx).unwrap();
                let error =
                    begin_signed_native_checkpoint_bounded_v1(&via, checkpoint, ctx).unwrap_err();
                if mode == 0 {
                    assert!(matches!(
                        error,
                        BoundedAcquireErrorV1::Provider(FencingError::Io(_))
                    ));
                } else {
                    assert!(matches!(error, BoundedAcquireErrorV1::Provider(_)));
                }
                assert_eq!(memory.used(), 0);
                let (expected, eof) = proxy.join().unwrap();
                assert!(eof);
                let recovered = acquire_provider_lease_bounded_v1(
                    provider.as_ref(),
                    instance,
                    initial,
                    policy,
                    ctx,
                )
                .unwrap();
                assert_eq!(recovered.checkpoint(), initial);
                assert_eq!(recovered.pending(), Some(&expected));
                drop(recovered);
                assert_eq!(memory.used(), 0);
            },
        );
    }
}
fn public_key_for_test() -> String {
    hex::encode(SigningKey::from_bytes(&[3; 32]).verifying_key().to_bytes())
}
