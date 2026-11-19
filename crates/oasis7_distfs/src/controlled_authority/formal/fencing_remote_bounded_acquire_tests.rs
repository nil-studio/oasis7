use super::*;
use crate::controlled_authority::formal::endpoint::with_remote_acquire_fixture;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
fn memory() -> MemoryBudget {
    MemoryBudget::new(128 << 20, 0).unwrap()
}
fn work() -> FormalBoundedWorkV1 {
    FormalBoundedWorkV1::new(20_000_000, 128 << 20).unwrap()
}
#[test]
fn true_tcp_acquire_empty_and_pending_keep_paid_state_and_fd_after_context_drops() {
    for pending in [false, true] {
        with_remote_acquire_fixture(
            pending,
            |provider, instance, initial, policy, native, expected_pending| {
                let memory = memory();
                let meter = work();
                let mut stop = || false;
                let lease = {
                    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
                    acquire_provider_lease_bounded_v1(
                        provider.as_ref(),
                        instance,
                        initial,
                        policy,
                        &mut ctx,
                    )
                    .unwrap()
                };
                assert!(lease.uses_budget(&memory));
                assert_eq!(lease.reserved_bytes(), memory.used());
                assert_eq!(lease.checkpoint(), initial);
                assert_eq!(lease.pending(), expected_pending);
                assert_eq!(lease.native_checkpoint_bytes(), Some(native));
                assert_eq!(lease.pending().is_some(), pending);
                let retained = memory.used();
                assert!(retained > 0);
                // Read the genuinely retained response after the Context borrow ends.
                assert_eq!(lease.checkpoint().commitment, initial.commitment);
                drop(lease);
                assert_eq!(memory.used(), 0);
                // The first real connection has closed; a new Acquire must work.
                let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
                let second = acquire_provider_lease_bounded_v1(
                    provider.as_ref(),
                    instance,
                    initial,
                    policy,
                    &mut ctx,
                )
                .unwrap();
                drop(second);
                assert_eq!(memory.used(), 0);
            },
        );
    }
}
#[test]
fn default_bounded_provider_fails_closed_without_legacy_acquire() {
    struct Unsupported;
    impl BoundedMonotonicEndpointProvider for Unsupported {}
    with_remote_acquire_fixture(false, |_, instance, initial, policy, _, _| {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        assert!(matches!(
            acquire_provider_lease_bounded_v1(&Unsupported, instance, initial, policy, &mut ctx),
            Err(BoundedAcquireErrorV1::Provider(
                FencingError::FencingUnproven
            ))
        ));
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn genuine_forwarded_owner_with_foreign_core_or_pool_is_rejected_and_closed() {
    struct Forward(std::sync::Mutex<Option<PaidRemoteLeaseV1>>);
    impl BoundedMonotonicEndpointProvider for Forward {
        fn acquire_bounded(
            &self,
            _: &EndpointInstanceBindingV1,
            _: &EndpointCheckpointBindingV1,
            _: &mut FormalBoundedContextV1<'_>,
        ) -> ResultV<PaidRemoteLeaseV1> {
            Ok(self.0.lock().unwrap().take().unwrap())
        }
    }
    for foreign_pool in [false, true] {
        with_remote_acquire_fixture(false, |provider, instance, initial, policy, _, _| {
            let original_memory = memory();
            let original_meter = work();
            let mut stop = || false;
            let owner = {
                let mut ctx =
                    FormalBoundedContextV1::new(&original_memory, &original_meter, &mut stop);
                provider
                    .acquire_bounded(instance, initial, &mut ctx)
                    .unwrap()
            };
            let forward = Forward(std::sync::Mutex::new(Some(owner)));
            let other_memory = memory();
            let other_meter = work();
            let mut ctx = FormalBoundedContextV1::new(
                if foreign_pool {
                    &other_memory
                } else {
                    &original_memory
                },
                if foreign_pool {
                    &original_meter
                } else {
                    &other_meter
                },
                &mut stop,
            );
            assert!(matches!(
                acquire_provider_lease_bounded_v1(&forward, instance, initial, policy, &mut ctx),
                Err(BoundedAcquireErrorV1::Work(_))
            ));
            assert_eq!(original_memory.used(), 0);
            assert_eq!(other_memory.used(), 0);
        });
    }
}
#[test]
fn real_forwarded_lease_cannot_change_requested_instance_initial_or_policy() {
    struct Forward(std::sync::Mutex<Option<PaidRemoteLeaseV1>>);
    impl BoundedMonotonicEndpointProvider for Forward {
        fn acquire_bounded(
            &self,
            _: &EndpointInstanceBindingV1,
            _: &EndpointCheckpointBindingV1,
            _: &mut FormalBoundedContextV1<'_>,
        ) -> ResultV<PaidRemoteLeaseV1> {
            Ok(self.0.lock().unwrap().take().unwrap())
        }
    }
    for mode in 0..3 {
        with_remote_acquire_fixture(false, |provider, instance, initial, policy, _, _| {
            let memory = memory();
            let meter = work();
            let mut stop = || false;
            let owner = {
                let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
                provider
                    .acquire_bounded(instance, initial, &mut ctx)
                    .unwrap()
            };
            let forward = Forward(std::sync::Mutex::new(Some(owner)));
            let mut wrong_instance = instance.clone();
            wrong_instance.instance_id = "ab".repeat(32);
            let mut wrong_initial = initial.clone();
            wrong_initial.revision += 1;
            let mut wrong_policy = policy.clone();
            wrong_policy.authorized_resource_bindings = vec!["cd".repeat(32)];
            let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
            assert!(matches!(
                acquire_provider_lease_bounded_v1(
                    &forward,
                    if mode == 0 { &wrong_instance } else { instance },
                    if mode == 1 { &wrong_initial } else { initial },
                    if mode == 2 { &wrong_policy } else { policy },
                    &mut ctx
                ),
                Err(BoundedAcquireErrorV1::Provider(
                    FencingError::FencingUnproven
                ))
            ));
            assert_eq!(memory.used(), 0);
        });
    }
}
#[test]
fn caller_zero_limits_and_early_cancel_return_work_error_and_release_all_fees() {
    for mode in 0..4 {
        with_remote_acquire_fixture(false, |provider, instance, initial, policy, _, _| {
            let memory = MemoryBudget::new(if mode == 0 { 0 } else { 128 << 20 }, 0).unwrap();
            let meter = FormalBoundedWorkV1::new(
                if mode == 1 { 0 } else { 20_000_000 },
                if mode == 2 { 0 } else { 128 << 20 },
            )
            .unwrap();
            let mut stop = || mode == 3;
            let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
            assert!(matches!(
                acquire_provider_lease_bounded_v1(
                    provider.as_ref(),
                    instance,
                    initial,
                    policy,
                    &mut ctx
                ),
                Err(BoundedAcquireErrorV1::Work(_))
            ));
            assert_eq!(memory.used(), 0);
        });
    }
}
#[test]
fn original_cbor_and_domain_crypto_wire_are_preserved() {
    with_remote_acquire_fixture(false, |provider, instance, _, _, _, _| {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        let expected = serde_cbor::to_vec(instance).unwrap();
        let encoded = encode_paid(instance, usize::MAX, &mut ctx).unwrap();
        assert_eq!(encoded.as_ref(), expected);
        drop(encoded);
        assert_eq!(
            hash_paid(instance, &mut ctx).unwrap(),
            hash_value(instance).unwrap()
        );
        let signed = sign_paid(REQUEST, instance, &provider.key, &mut ctx).unwrap();
        assert_eq!(signed, sign(REQUEST, instance, &provider.key).unwrap());
        let key = hex::encode(provider.key.verifying_key().to_bytes());
        verify_paid(REQUEST, instance, &signed, &key, &mut ctx).unwrap();
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn actual_tcp_corrupt_provider_signature_is_rejected_and_fees_are_released() {
    with_remote_acquire_fixture(false, |provider, instance, initial, policy, _, _| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let upstream = provider.address;
        let proxy = std::thread::spawn(move || {
            let (mut client, _) = listener.accept().unwrap();
            let mut server = TcpStream::connect(upstream).unwrap();
            let mut header = [0; 4];
            client.read_exact(&mut header).unwrap();
            let mut request = vec![0; u32::from_be_bytes(header) as usize];
            client.read_exact(&mut request).unwrap();
            server.write_all(&header).unwrap();
            server.write_all(&request).unwrap();
            server.read_exact(&mut header).unwrap();
            let mut bytes = vec![0; u32::from_be_bytes(header) as usize];
            server.read_exact(&mut bytes).unwrap();
            let mut reply: SignedMonotonicOracleReplyV1 = serde_cbor::from_slice(&bytes).unwrap();
            let replacement = if reply.signature.starts_with("00") {
                "ff"
            } else {
                "00"
            };
            reply.signature.replace_range(..2, replacement);
            let bytes = serde_cbor::to_vec(&reply).unwrap();
            client
                .write_all(&(bytes.len() as u32).to_be_bytes())
                .unwrap();
            client.write_all(&bytes).unwrap();
        });
        let altered = RemoteMonotonicEndpointProvider::new(
            address,
            provider.timeout,
            provider.key.clone(),
            provider.policy.clone(),
        )
        .unwrap();
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        assert!(matches!(
            acquire_provider_lease_bounded_v1(&altered, instance, initial, policy, &mut ctx),
            Err(BoundedAcquireErrorV1::Provider(_))
        ));
        proxy.join().unwrap();
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn paid_decoder_keeps_first_work_cause_over_serde_io_and_rejects_original_bad_wire() {
    for bytes in [
        vec![0xff],
        vec![0xa0],
        vec![0xf6],
        vec![0x9a, 0xff, 0xff, 0xff, 0xff],
    ] {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        let fee = reserve(ctx.memory(), decode_phase(bytes.len()).unwrap()).unwrap();
        let mut reader = PaidRead {
            bytes: &bytes,
            ctx: &mut ctx,
            first: None,
        };
        assert!(serde_cbor::from_reader::<SignedMonotonicOracleReplyV1, _>(&mut reader).is_err());
        assert!(reader.first.is_none());
        drop(reader);
        drop(fee);
        assert_eq!(memory.used(), 0);
    }
    let memory = memory();
    let meter = FormalBoundedWorkV1::new(0, 100).unwrap();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let mut reader = PaidRead {
        bytes: &[0xa0],
        ctx: &mut ctx,
        first: None,
    };
    assert!(serde_cbor::from_reader::<SignedMonotonicOracleReplyV1, _>(&mut reader).is_err());
    assert!(
        matches!(reader.first.take(),Some(BoundedAcquireErrorV1::Work(FormalError::Invalid(s))) if s=="bounded archive read steps")
    );
}
#[test]
fn real_codec_and_io_private_adapters_handle_short_interrupted_zero_and_precise_budget() {
    struct Short {
        calls: usize,
        bytes: Vec<u8>,
        zero: bool,
    }
    impl Write for Short {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            if self.calls == 1 {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            if self.zero {
                return Ok(0);
            }
            let n = bytes.len().min(3);
            self.bytes.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for zero in [false, true] {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        let mut writer = Short {
            calls: 0,
            bytes: Vec::new(),
            zero,
        };
        let result = write_all_paid(&mut writer, b"abcdefgh", &mut ctx);
        if zero {
            assert!(matches!(
                result,
                Err(BoundedAcquireErrorV1::Provider(FencingError::Io(_)))
            ));
        } else {
            result.unwrap();
            assert_eq!(writer.bytes, b"abcdefgh");
            assert_eq!(writer.calls, 4);
        }
    }
    let memory = MemoryBudget::new(2 + std::mem::size_of::<Vec<u8>>() - 1, 0).unwrap();
    let meter = work();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    // Original CBOR u8(24) is two bytes. The measured payload reservation is one short.
    assert!(encode_paid(&24u8, LIMIT, &mut ctx).is_err());
    assert_eq!(memory.used(), 0);
    assert!(!meter.is_child_active());
}

// This test proxy always obtains a real upstream Oracle response first. It
// changes only the downstream test wire and observes the actual client EOF.
fn response_proxy(
    provider: &RemoteMonotonicEndpointProvider,
    transform: impl FnOnce(Vec<u8>) -> Vec<u8> + Send + 'static,
) -> (
    RemoteMonotonicEndpointProvider,
    std::thread::JoinHandle<(Vec<u8>, bool)>,
    std::sync::mpsc::Receiver<usize>,
) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let upstream = provider.address;
    let (sent, received) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let (mut client, _) = listener.accept().unwrap();
        let mut server = TcpStream::connect(upstream).unwrap();
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut header = [0; 4];
        client.read_exact(&mut header).unwrap();
        let mut request = vec![0; u32::from_be_bytes(header) as usize];
        client.read_exact(&mut request).unwrap();
        server.write_all(&header).unwrap();
        server.write_all(&request).unwrap();
        server.read_exact(&mut header).unwrap();
        let mut raw = vec![0; u32::from_be_bytes(header) as usize];
        server.read_exact(&mut raw).unwrap();
        let output = transform(raw);
        client
            .write_all(&(output.len() as u32).to_be_bytes())
            .unwrap();
        client.write_all(&output).unwrap();
        let _ = sent.send(output.len());
        let mut eof = [0; 1];
        let closed = matches!(client.read(&mut eof), Ok(0));
        (output, closed)
    });
    (
        RemoteMonotonicEndpointProvider::new(
            address,
            provider.timeout,
            provider.key.clone(),
            provider.policy.clone(),
        )
        .unwrap(),
        handle,
        received,
    )
}
#[test]
fn real_upstream_reply_resigned_semantic_mutations_are_rejected() {
    for mode in 0..4 {
        with_remote_acquire_fixture(false, |provider, instance, initial, policy, _, _| {
            let oracle_key = SigningKey::from_bytes(&[3; 32]);
            assert_eq!(
                hex::encode(oracle_key.verifying_key().to_bytes()),
                policy.provider_public_key
            );
            let (altered, proxy, _sent) = response_proxy(&provider, move |raw| {
                let mut reply: SignedMonotonicOracleReplyV1 = serde_cbor::from_slice(&raw).unwrap();
                match mode {
                    0 => reply.body.challenge = "d1".repeat(32),
                    1 => reply.body.request_digest = "d2".repeat(32),
                    2 => reply.body.resource_binding = "d3".repeat(32),
                    // Acquire has no previous lease to compare. Test its actual
                    // canonical-lease validation, not a fictitious old-lease rule.
                    _ => reply.body.lease = "not-a-canonical-lease".into(),
                }
                reply.signature = sign(RESPONSE, &reply.body, &oracle_key).unwrap();
                serde_cbor::to_vec(&reply).unwrap()
            });
            let memory = memory();
            let meter = work();
            let mut stop = || false;
            let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
            let error =
                acquire_provider_lease_bounded_v1(&altered, instance, initial, policy, &mut ctx)
                    .unwrap_err();
            if mode == 3 {
                assert!(matches!(
                    error,
                    BoundedAcquireErrorV1::Provider(FencingError::Invalid(_))
                ));
            } else {
                assert!(matches!(
                    error,
                    BoundedAcquireErrorV1::Provider(FencingError::FencingUnproven)
                ));
            }
            let (wire, closed) = proxy.join().unwrap();
            assert!(closed);
            assert!(!wire.is_empty());
            assert!(meter.remaining_read_bytes() < 128 << 20);
            assert_eq!(memory.used(), 0);
        });
    }
}
fn compare_full_reply_codec(bytes: &[u8], must_accept: bool) {
    let original = serde_cbor::from_slice::<SignedMonotonicOracleReplyV1>(bytes);
    assert_eq!(original.is_ok(), must_accept);
    let memory = memory();
    let meter = work();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let phase = reserve(ctx.memory(), decode_phase(bytes.len()).unwrap()).unwrap();
    let mut reader = PaidRead {
        bytes,
        ctx: &mut ctx,
        first: None,
    };
    let actual = serde_cbor::from_reader::<SignedMonotonicOracleReplyV1, _>(&mut reader);
    assert!(reader.first.is_none());
    assert_eq!(actual.is_ok(), original.is_ok());
    match (original, actual) {
        (Ok(a), Ok(b)) => assert_eq!(
            serde_cbor::to_vec(&a).unwrap(),
            serde_cbor::to_vec(&b).unwrap()
        ),
        (Err(a), Err(b)) => {
            assert_eq!(a.is_eof(), b.is_eof());
            assert_eq!(a.is_data(), b.is_data());
        }
        _ => unreachable!(),
    }
    drop(reader);
    drop(phase);
    assert_eq!(memory.used(), 0);
}
#[test]
fn complete_genuine_reply_codec_acceptance_and_hostile_vector_hints_match_original() {
    use serde_cbor::Value;
    with_remote_acquire_fixture(true, |provider, instance, initial, policy, _, _| {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let owner = {
            let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
            acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                &mut ctx,
            )
            .unwrap()
        };
        let raw = serde_cbor::to_vec(&*owner.reply).unwrap();
        let signature = owner.reply.signature.clone();
        drop(owner);
        assert_eq!(memory.used(), 0);
        compare_full_reply_codec(&raw, true);
        let mut trailing = raw.clone();
        trailing.push(0);
        compare_full_reply_codec(&trailing, false);
        assert_eq!(raw[0], 0xa2);
        let mut duplicate = vec![0xa3];
        duplicate.extend_from_slice(&raw[1..]);
        duplicate.extend(serde_cbor::to_vec(&"signature").unwrap());
        duplicate.extend(serde_cbor::to_vec(&signature).unwrap());
        compare_full_reply_codec(&duplicate, false);
        let mut unknown: Value = serde_cbor::from_slice(&raw).unwrap();
        let Value::Map(ref mut map) = unknown else {
            panic!("reply map")
        };
        map.insert(Value::Text("unknown-extra".into()), Value::Null);
        compare_full_reply_codec(&serde_cbor::to_vec(&unknown).unwrap(), false);
        let mut huge: Value = serde_cbor::from_slice(&raw).unwrap();
        let Value::Map(ref mut outer) = huge else {
            panic!("reply map")
        };
        let Value::Map(body) = outer.get_mut(&Value::Text("body".into())).unwrap() else {
            panic!("body map")
        };
        body.insert(
            Value::Text("native_checkpoint_bytes".into()),
            Value::Array(vec![]),
        );
        let mut huge = serde_cbor::to_vec(&huge).unwrap();
        let key = serde_cbor::to_vec(&"native_checkpoint_bytes").unwrap();
        let position = huge.windows(key.len()).position(|w| w == key).unwrap() + key.len();
        assert_eq!(huge[position], 0x80);
        huge.splice(position..position + 1, [0x9a, 0xff, 0xff, 0xff, 0xff]);
        compare_full_reply_codec(&huge, false);
    });
}
#[test]
fn genuine_full_reply_decode_phase_one_byte_short_is_rejected_before_deserializer() {
    with_remote_acquire_fixture(true, |provider, instance, initial, policy, _, _| {
        let source_memory = memory();
        let meter = work();
        let mut stop = || false;
        let owner = {
            let mut ctx = FormalBoundedContextV1::new(&source_memory, &meter, &mut stop);
            acquire_provider_lease_bounded_v1(
                provider.as_ref(),
                instance,
                initial,
                policy,
                &mut ctx,
            )
            .unwrap()
        };
        let raw = serde_cbor::to_vec(&*owner.reply).unwrap();
        drop(owner);
        assert_eq!(source_memory.used(), 0);
        // This is the concrete full-reply decoder admission boundary, not a
        // claim that the entire TCP operation needs only this phase budget.
        let required = decode_phase(raw.len()).unwrap();
        let memory = MemoryBudget::new(required - 1, 0).unwrap();
        assert!(reserve(&memory, required).is_err());
        assert_eq!(memory.used(), 0);
        let exact = MemoryBudget::new(required, 0).unwrap();
        let phase = reserve(&exact, required).unwrap();
        let mut ctx = FormalBoundedContextV1::new(&exact, &meter, &mut stop);
        let mut reader = PaidRead {
            bytes: &raw,
            ctx: &mut ctx,
            first: None,
        };
        let actual: SignedMonotonicOracleReplyV1 = serde_cbor::from_reader(&mut reader).unwrap();
        assert!(reader.first.is_none());
        assert_eq!(serde_cbor::to_vec(&actual).unwrap(), raw);
        drop(actual);
        drop(reader);
        drop(phase);
        assert_eq!(exact.used(), 0);
    });
}
#[test]
fn cancellation_after_full_tcp_response_in_paid_decoder_phase_closes_real_peer() {
    with_remote_acquire_fixture(true, |provider, instance, initial, policy, _, _| {
        let (proxied, proxy, sent) = response_proxy(&provider, |raw| raw);
        let memory = memory();
        let meter = work();
        let mut observed = None;
        let mut sent = Some(sent);
        // The request/raw phases are far smaller than this genuine decoder's
        // mandatory two cautious-Vec floor. Crossing it proves phase admission.
        let mut stop = || {
            if memory.used() >= 2 * (1 << 20) {
                let n = sent
                    .take()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(3))
                    .unwrap();
                assert!(n > 0 && n <= LIMIT);
                assert!(memory.used() >= decode_phase(n).unwrap());
                observed = Some((n, memory.used(), meter.remaining_read_bytes()));
                true
            } else {
                false
            }
        };
        let result = {
            let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
            acquire_provider_lease_bounded_v1(&proxied, instance, initial, policy, &mut ctx)
        };
        assert!(
            matches!(result,Err(BoundedAcquireErrorV1::Work(FormalError::Invalid(ref s)))if s=="bounded archive read cancelled")
        );
        let (wire, closed) = proxy.join().unwrap();
        assert!(closed);
        let (n, phase_used, remaining) = observed.unwrap();
        assert_eq!(wire.len(), n);
        assert!(phase_used >= decode_phase(n).unwrap());
        assert!(remaining < 128 << 20);
        assert_eq!(memory.used(), 0);
        let mut no_cancel = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut no_cancel);
        let retry = acquire_provider_lease_bounded_v1(
            provider.as_ref(),
            instance,
            initial,
            policy,
            &mut ctx,
        )
        .unwrap();
        drop(retry);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn private_read_adapter_short_interrupted_eof_and_source_error_keep_attempt_charges() {
    struct ShortRead {
        calls: usize,
        offset: usize,
        mode: u8,
    }
    impl Read for ShortRead {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            self.calls += 1;
            if self.calls == 1 {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            if self.mode == 1 {
                return Ok(0);
            }
            if self.mode == 2 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "remote read sentinel",
                ));
            }
            let input = b"abcdefgh";
            let n = out.len().min(3).min(input.len() - self.offset);
            out[..n].copy_from_slice(&input[self.offset..self.offset + n]);
            self.offset += n;
            Ok(n)
        }
    }
    for mode in 0..3 {
        let memory = memory();
        let meter = FormalBoundedWorkV1::new(100, 100).unwrap();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
        let mut source = ShortRead {
            calls: 0,
            offset: 0,
            mode,
        };
        let mut output = [0; 8];
        let result = read_exact_paid(
            &mut source,
            &mut output,
            "monotonic oracle response body read",
            &mut ctx,
        );
        match mode {
            0 => {
                result.unwrap();
                assert_eq!(&output, b"abcdefgh");
                assert_eq!(source.calls, 4);
                // Requested lengths: Interrupted8 + short8 + short5 + final2.
                assert_eq!(meter.remaining_read_bytes(), 77);
                assert_eq!(meter.remaining_steps(), 96);
            }
            1 => {
                let error = result.unwrap_err();
                assert!(matches!(
                    error,
                    BoundedAcquireErrorV1::Provider(FencingError::Io(_))
                ));
                assert_eq!(source.calls, 2);
                assert_eq!(meter.remaining_read_bytes(), 84);
                assert_eq!(meter.remaining_steps(), 98);
            }
            _ => {
                let error = result.unwrap_err();
                assert!(
                    matches!(&error,BoundedAcquireErrorV1::Provider(FencingError::Io(s))if s=="monotonic oracle response body read: remote read sentinel")
                );
                assert_eq!(source.calls, 2);
                assert_eq!(meter.remaining_read_bytes(), 84);
                assert_eq!(meter.remaining_steps(), 98);
            }
        }
        assert_eq!(memory.used(), 0);
    }
    let memory = memory();
    let meter = FormalBoundedWorkV1::new(100, 7).unwrap();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let mut source = ShortRead {
        calls: 0,
        offset: 0,
        mode: 0,
    };
    let error = read_exact_paid(
        &mut source,
        &mut [0; 8],
        "monotonic oracle response body read",
        &mut ctx,
    )
    .unwrap_err();
    assert!(
        matches!(error,BoundedAcquireErrorV1::Work(FormalError::Invalid(s))if s=="bounded archive read bytes")
    );
    assert_eq!(
        source.calls, 0,
        "work rejection precedes actual source operation"
    );
    assert_eq!(meter.remaining_steps(), 99);
    assert_eq!(meter.remaining_read_bytes(), 0);
}

#[test]
fn real_tcp_zero_oversized_and_truncated_response_frames_fail_and_close_peer() {
    for mode in 0..3 {
        with_remote_acquire_fixture(false, |provider, instance, initial, policy, _, _| {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let upstream = provider.address;
            let proxy = std::thread::spawn(move || {
                let (mut client, _) = listener.accept().unwrap();
                client
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut server = TcpStream::connect(upstream).unwrap();
                let mut header = [0; 4];
                client.read_exact(&mut header).unwrap();
                let mut request = vec![0; u32::from_be_bytes(header) as usize];
                client.read_exact(&mut request).unwrap();
                server.write_all(&header).unwrap();
                server.write_all(&request).unwrap();
                server.read_exact(&mut header).unwrap();
                let mut original = vec![0; u32::from_be_bytes(header) as usize];
                server.read_exact(&mut original).unwrap();
                drop(server);
                match mode {
                    0 => client.write_all(&0u32.to_be_bytes()).unwrap(),
                    1 => client
                        .write_all(&((LIMIT + 1) as u32).to_be_bytes())
                        .unwrap(),
                    _ => {
                        assert!(original.len() > 1);
                        client.write_all(&header).unwrap();
                        client.write_all(&original[..original.len() / 2]).unwrap();
                        // Only the response write half closes early. The read
                        // half must observe the client's own subsequent Drop.
                        client.shutdown(std::net::Shutdown::Write).unwrap();
                    }
                }
                let mut eof = [0; 1];
                matches!(client.read(&mut eof), Ok(0))
            });
            let altered = RemoteMonotonicEndpointProvider::new(
                address,
                provider.timeout,
                provider.key.clone(),
                provider.policy.clone(),
            )
            .unwrap();
            let memory = memory();
            let meter = work();
            let mut stop = || false;
            let mut ctx = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
            let error =
                acquire_provider_lease_bounded_v1(&altered, instance, initial, policy, &mut ctx)
                    .unwrap_err();
            if mode < 2 {
                assert!(
                    matches!(error,BoundedAcquireErrorV1::Provider(FencingError::Invalid(s))if s=="monotonic oracle frame bound")
                );
            } else {
                assert!(
                    matches!(error,BoundedAcquireErrorV1::Provider(FencingError::Io(s))if s.starts_with("monotonic oracle response body read:"))
                );
            }
            assert!(
                proxy.join().unwrap(),
                "client must close its real connection after Err"
            );
            assert!(meter.remaining_steps() < 20_000_000);
            assert!(meter.remaining_read_bytes() < 128 << 20);
            assert_eq!(memory.used(), 0);
        });
    }
}
