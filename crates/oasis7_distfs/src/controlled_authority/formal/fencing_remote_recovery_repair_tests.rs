//! Transport acceptance only: the separately checked repair grant is fixed in
//! this guard. The peer independently checks every signed frame and full entry
//! digest; protocol tests and the actual R-only fixture verify grant authority.
use super::*;
use crate::controlled_authority::replicated_protocol::HeadAnchor;
use ed25519_dalek::Signer;
use std::net::{Shutdown, TcpListener};

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
fn public(seed: u8) -> String {
    hex::encode(key(seed).verifying_key().to_bytes())
}
fn hash(seed: u8) -> String {
    format!("{seed:02x}").repeat(32)
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    EarlySignature,
    ChangedGrant,
    Disconnect,
    OriginalDeadline,
}

fn receive(stream: &mut TcpStream) -> Option<SignedMonotonicOracleRequestV1> {
    let mut length = [0; 4];
    if stream.read_exact(&mut length).is_err() {
        return None;
    }
    let length = u32::from_be_bytes(length) as usize;
    assert!(length > 0 && length <= LIMIT, "original RPC frame limit");
    let mut encoded = vec![0; length];
    stream.read_exact(&mut encoded).unwrap();
    let request: SignedMonotonicOracleRequestV1 = serde_cbor::from_slice(&encoded).unwrap();
    verify(REQUEST, &request.body, &request.signature, &public(1)).unwrap();
    assert_eq!(request.body.authentication_public_key, public(1));
    assert_eq!(request.body.lease.as_deref(), Some(hash(40).as_str()));
    Some(request)
}

fn reply(
    stream: &mut TcpStream,
    request: &SignedMonotonicOracleRequestV1,
    original: &MonotonicOracleReplyBodyV1,
    signature: Option<ProtectedEndpointSignatureV1>,
    corrupt_grant: bool,
) -> std::io::Result<()> {
    let mut body = original.clone();
    body.request_digest = hash_value(&request.body).unwrap();
    body.challenge = request.body.challenge.clone();
    body.protected_signature = signature;
    if corrupt_grant {
        body.recovery_repair.as_mut().unwrap().directory_fingerprint = hash(99);
    }
    let signed = SignedMonotonicOracleReplyV1 {
        signature: sign(RESPONSE, &body, &key(3)).unwrap(),
        body,
    };
    let encoded = serde_cbor::to_vec(&signed).unwrap();
    assert!(encoded.len() <= LIMIT);
    stream.write_all(&(encoded.len() as u32).to_be_bytes())?;
    stream.write_all(&encoded)
}

fn exercise(fault: Fault) {
    let instance = EndpointInstanceBindingV1 {
        schema_version: 1,
        world_id: "repair-world".into(),
        chain_id: "repair-chain".into(),
        genesis_digest: hash(4),
        authority_epoch: 1,
        endpoint_role: EndpointRole::Primary,
        endpoint_id: "original-primary".into(),
        endpoint_public_key: public(2),
        instance_id: hash(5),
        storage_id: hash(6),
        storage_fingerprint: hash(7),
    };
    let native = b"fixed original signed native bytes".to_vec();
    let checkpoint = EndpointCheckpointBindingV1 {
        revision: 31,
        commitment: blake3::hash(&native).to_hex().to_string(),
        head: HeadAnchor {
            position: 9,
            decision_hash: hash(8),
            qualified: true,
        },
    };
    let signing = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: instance.digest().unwrap(),
        checkpoint: checkpoint.clone(),
        challenge: hash(9),
        purpose: ProtectedEndpointSignaturePurposeV1::PrepareReceipt,
        preimage: bytes("oasis7.formal.prepare_receipt.sign.v1", &hash(10)).unwrap(),
    };
    signing.validate().unwrap();
    let endpoint_signature = hex::encode(key(2).sign(&signing.preimage).to_bytes());
    let request = ProtectedRecoveryReceiptReplayV1 {
        signing: signing.clone(),
        entry_prefix: vec![255; 2 * 1024 * 1024 + 17],
        entry_suffix: vec![71; 192 * 1024 + 3],
    };
    assert!(serde_cbor::to_vec(&request).unwrap().len() > LIMIT);
    let mut digest = blake3::Hasher::new();
    digest.update(&request.entry_prefix);
    digest.update(endpoint_signature.as_bytes());
    digest.update(&request.entry_suffix);
    let original_pending = digest.finalize().to_hex().to_string();
    let statement = MonotonicProviderStatementV1 {
        schema_version: 1,
        capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
        resource_binding: hash(11),
        instance_digest: instance.digest().unwrap(),
        checkpoint: checkpoint.clone(),
        epoch_floor: 1,
        seal_digest: None,
    };
    let evidence = MonotonicProviderEvidenceV1 {
        signature: sign("oasis7.endpoint.monotonic-provider.v1", &statement, &key(3)).unwrap(),
        statement,
    };
    let state = MonotonicOracleReplyBodyV1 {
        schema_version: 1,
        capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
        resource_binding: hash(11),
        request_digest: hash(12),
        challenge: hash(13),
        lease: hash(40),
        checkpoint: checkpoint.clone(),
        pending: None,
        seal: None,
        evidence: Some(evidence),
        native_checkpoint_bytes: Some(native.clone()),
        protected_custody: Some(ProtectedSignerCustodyV1::NonexportableLeaseBoundV1),
        durable_set: None,
        recovery_repair: Some(ProtectedRecoveryRepairGrantV1 {
            schema_version: 1,
            request_digest: hash(14),
            instance_digest: instance.digest().unwrap(),
            plan_digest: hash(15),
            directory_fingerprint: hash(16),
            recovery_credential_public_key: public(1),
            original_checkpoint: checkpoint.clone(),
            original_native_checkpoint: native,
            original_pending: Some(original_pending.clone()),
        }),
        protected_signature: None,
    };
    let body = ProtectedEndpointSignatureBodyV1 {
        schema_version: 1,
        custody: ProtectedSignerCustodyV1::NonexportableLeaseBoundV1,
        instance_digest: instance.digest().unwrap(),
        resource_binding: hash(11),
        checkpoint,
        request_digest: signing.digest().unwrap(),
        challenge: signing.challenge.clone(),
        endpoint_signature,
    };
    let signature = ProtectedEndpointSignatureV1 {
        provider_signature: hex::encode(
            key(3)
                .sign(&protected_endpoint_signature_signing_bytes(&body).unwrap())
                .to_bytes(),
        ),
        body,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer_state = state.clone();
    let expected = request.clone();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_nodelay(true).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let first = receive(&mut stream).unwrap();
        assert_eq!(first.body.instance, instance);
        let MonotonicOracleCommandV1::BeginReplay(header) = &first.body.command else {
            panic!("expected bounded header")
        };
        assert_eq!(header.signing, expected.signing);
        assert_eq!(header.prefix_bytes, expected.entry_prefix.len() as u64);
        assert_eq!(header.suffix_bytes, expected.entry_suffix.len() as u64);
        if matches!(fault, Fault::OriginalDeadline) {
            std::thread::sleep(Duration::from_millis(300));
        }
        reply(
            &mut stream,
            &first,
            &peer_state,
            matches!(fault, Fault::EarlySignature).then(|| signature.clone()),
            false,
        )
        .unwrap();
        if matches!(fault, Fault::EarlySignature) {
            assert!(receive(&mut stream).is_none());
            return;
        }
        let mut offset = 0usize;
        let mut received_digest = blake3::Hasher::new();
        for (part, bytes) in [&expected.entry_prefix, &expected.entry_suffix]
            .into_iter()
            .enumerate()
        {
            for expected_chunk in bytes.chunks(64 * 1024) {
                let frame = receive(&mut stream).unwrap();
                assert_eq!(frame.body.instance, instance);
                let MonotonicOracleCommandV1::ReplayChunk {
                    offset: actual,
                    bytes,
                } = &frame.body.command
                else {
                    panic!("expected original byte chunk")
                };
                assert_eq!(*actual, offset as u64);
                assert_eq!(bytes, expected_chunk);
                assert!(bytes.len() <= 64 * 1024);
                if matches!(fault, Fault::Disconnect) {
                    return;
                }
                if matches!(fault, Fault::OriginalDeadline) {
                    std::thread::sleep(Duration::from_millis(400));
                    let _ = reply(&mut stream, &frame, &peer_state, None, false);
                    assert!(receive(&mut stream).is_none());
                    return;
                }
                reply(
                    &mut stream,
                    &frame,
                    &peer_state,
                    None,
                    matches!(fault, Fault::ChangedGrant),
                )
                .unwrap();
                if matches!(fault, Fault::ChangedGrant) {
                    assert!(receive(&mut stream).is_none());
                    return;
                }
                received_digest.update(bytes);
                offset += bytes.len();
            }
            if part == 0 {
                received_digest.update(signature.body.endpoint_signature.as_bytes());
            }
        }
        assert_eq!(
            offset,
            expected.entry_prefix.len() + expected.entry_suffix.len()
        );
        assert_eq!(
            received_digest.finalize().to_hex().as_str(),
            original_pending
        );
        let last = receive(&mut stream).unwrap();
        assert!(matches!(
            last.body.command,
            MonotonicOracleCommandV1::FinishReplay
        ));
        reply(&mut stream, &last, &peer_state, Some(signature), false).unwrap();
    });
    let timeout = if matches!(fault, Fault::OriginalDeadline) {
        Duration::from_millis(600)
    } else {
        Duration::from_secs(10)
    };
    let stream = connect_oracle(&address, timeout).unwrap();
    let mut guard = RemoteGuard {
        failed: false,
        stream: Mutex::new(stream),
        instance: EndpointInstanceBindingV1 {
            schema_version: 1,
            world_id: "repair-world".into(),
            chain_id: "repair-chain".into(),
            genesis_digest: hash(4),
            authority_epoch: 1,
            endpoint_role: EndpointRole::Primary,
            endpoint_id: "original-primary".into(),
            endpoint_public_key: public(2),
            instance_id: hash(5),
            storage_id: hash(6),
            storage_fingerprint: hash(7),
        },
        key: key(1),
        policy: TrustedFencingPolicyV1 {
            provider_public_key: public(3),
            authorized_resource_bindings: vec![hash(11)],
        },
        state: state.clone(),
    };
    let result = guard.replay_recovery_receipt(request);
    if matches!(fault, Fault::None) {
        let actual = result.unwrap();
        verify_protected_endpoint_signature(&actual, &signing, &guard.instance, &guard.policy)
            .unwrap();
        assert_eq!(guard.state.recovery_repair, state.recovery_repair);
        assert_eq!(
            guard.stream.get_mut().unwrap().read_timeout().unwrap(),
            Some(timeout)
        );
        assert_eq!(
            guard.stream.get_mut().unwrap().write_timeout().unwrap(),
            Some(timeout)
        );
    } else {
        assert!(result.is_err(), "{fault:?}");
        assert!(guard.failed);
        assert!(guard.evidence().is_err());
        assert!(guard.sign_endpoint(signing).is_err());
        assert_eq!(guard.state.recovery_repair, state.recovery_repair);
        assert_eq!(guard.state.checkpoint, state.checkpoint);
    }
    let _ = guard.stream.get_mut().unwrap().shutdown(Shutdown::Both);
    worker.join().unwrap();
}

#[test]
fn original_receipt_larger_than_rpc_frame_streams_one_lease_and_preserves_grant() {
    exercise(Fault::None);
}

#[test]
fn replay_never_accepts_early_signature_changed_grant_or_partial_connection() {
    for fault in [
        Fault::EarlySignature,
        Fault::ChangedGrant,
        Fault::Disconnect,
    ] {
        exercise(fault);
    }
}

#[test]
fn replay_chunks_share_original_absolute_deadline() {
    exercise(Fault::OriginalDeadline);
}
