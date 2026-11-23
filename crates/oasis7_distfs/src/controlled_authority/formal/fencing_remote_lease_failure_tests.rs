//! Real signed TCP protocol tests. The peer is an in-memory simulated oracle,
//! not evidence of physical nonrollback storage or a two-endpoint write lease.
use super::*;
use crate::controlled_authority::replicated_protocol::HeadAnchor;
use ed25519_dalek::Signer;
use std::net::TcpListener;

#[derive(Clone, Copy)]
enum Fault {
    LostBegin,
    LostFinish,
    ChangedBeginChallenge,
}
fn key(value: u8) -> SigningKey {
    SigningKey::from_bytes(&[value; 32])
}
fn public(value: u8) -> String {
    hex::encode(key(value).verifying_key().to_bytes())
}
fn checkpoint(revision: u64, body: &[u8]) -> EndpointCheckpointBindingV1 {
    EndpointCheckpointBindingV1 {
        revision,
        commitment: blake3::hash(body).to_hex().to_string(),
        head: HeadAnchor {
            position: 0,
            decision_hash: "d".repeat(64),
            qualified: false,
        },
    }
}
fn receive(stream: &mut TcpStream) -> SignedMonotonicOracleRequestV1 {
    let mut header = [0; 4];
    stream.read_exact(&mut header).unwrap();
    let len = u32::from_be_bytes(header) as usize;
    assert!(len <= LIMIT);
    let mut body = vec![0; len];
    stream.read_exact(&mut body).unwrap();
    let request: SignedMonotonicOracleRequestV1 = serde_cbor::from_slice(&body).unwrap();
    assert_eq!(request.body.authentication_public_key, public(1));
    verify(REQUEST, &request.body, &request.signature, &public(1)).unwrap();
    request
}
fn reply(
    stream: &mut TcpStream,
    request: &SignedMonotonicOracleRequestV1,
    current: &EndpointCheckpointBindingV1,
    pending: Option<&CheckpointIntentV1>,
    changed_challenge: bool,
) {
    let statement = MonotonicProviderStatementV1 {
        schema_version: 1,
        capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
        resource_binding: "e".repeat(64),
        instance_digest: request.body.instance.digest().unwrap(),
        checkpoint: current.clone(),
        epoch_floor: request.body.instance.authority_epoch,
        seal_digest: None,
    };
    let evidence = pending.is_none().then(|| MonotonicProviderEvidenceV1 {
        signature: hex::encode(
            key(3)
                .sign(&bytes("oasis7.endpoint.monotonic-provider.v1", &statement).unwrap())
                .to_bytes(),
        ),
        statement,
    });
    let body = MonotonicOracleReplyBodyV1 {
        schema_version: 1,
        capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
        resource_binding: "e".repeat(64),
        request_digest: hash_value(&request.body).unwrap(),
        challenge: if changed_challenge {
            "0".repeat(64)
        } else {
            request.body.challenge.clone()
        },
        lease: "f".repeat(64),
        checkpoint: current.clone(),
        pending: pending.cloned(),
        seal: None,
        evidence,
        native_checkpoint_bytes: None,
        protected_custody: None,
        durable_set: None,
        recovery_repair: None,
        protected_signature: None,
    };
    let reply = SignedMonotonicOracleReplyV1 {
        signature: hex::encode(key(3).sign(&bytes(RESPONSE, &body).unwrap()).to_bytes()),
        body,
    };
    let body = serde_cbor::to_vec(&reply).unwrap();
    stream
        .write_all(&(body.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&body).unwrap();
}
fn exercise(fault: Fault) {
    let instance = EndpointInstanceBindingV1 {
        schema_version: 1,
        world_id: "world".into(),
        chain_id: "chain".into(),
        genesis_digest: "a".repeat(64),
        authority_epoch: 4,
        endpoint_role: EndpointRole::Primary,
        endpoint_id: "primary".into(),
        endpoint_public_key: public(2),
        instance_id: "b".repeat(64),
        storage_id: "c".repeat(64),
        storage_fingerprint: "d".repeat(64),
    };
    let initial = checkpoint(1, b"initial");
    let intent = CheckpointIntentV1 {
        previous: initial.clone(),
        next: checkpoint(2, b"next"),
        checkpoint_bytes: b"next".to_vec(),
        seal: None,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let provider = RemoteMonotonicEndpointProvider::new(
        listener.local_addr().unwrap(),
        Duration::from_secs(3),
        key(1),
        TrustedFencingPolicyV1 {
            provider_public_key: public(3),
            authorized_resource_bindings: vec!["e".repeat(64)],
        },
    )
    .unwrap();
    let expected_instance = instance.clone();
    let expected_intent = intent.clone();
    let worker = std::thread::spawn(move || {
        let accept = || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_nodelay(true).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream
        };
        let mut stream = accept();
        let acquire = receive(&mut stream);
        assert_eq!(acquire.body.instance, expected_instance);
        assert!(matches!(
            acquire.body.command,
            MonotonicOracleCommandV1::Acquire(_)
        ));
        assert!(acquire.body.lease.is_none());
        let mut current = expected_intent.previous.clone();
        reply(&mut stream, &acquire, &current, None, false);
        let begin = receive(&mut stream);
        assert_eq!(begin.body.lease.as_deref(), Some("f".repeat(64).as_str()));
        match &begin.body.command {
            MonotonicOracleCommandV1::Begin(actual) => {
                assert_eq!(actual.as_ref(), &expected_intent)
            }
            _ => panic!("expected Begin"),
        }
        // Apply before fault: a client-side error cannot roll this back.
        let mut pending = Some(expected_intent.clone());
        match fault {
            Fault::LostBegin => {}
            Fault::ChangedBeginChallenge => {
                reply(&mut stream, &begin, &current, pending.as_ref(), true);
                let mut byte = [0];
                assert_eq!(
                    stream.read(&mut byte).unwrap(),
                    0,
                    "failed guard must close its lease"
                );
            }
            Fault::LostFinish => {
                reply(&mut stream, &begin, &current, pending.as_ref(), false);
                let finish = receive(&mut stream);
                assert!(matches!(
                    finish.body.command,
                    MonotonicOracleCommandV1::Finish
                ));
                current = pending.take().unwrap().next;
            }
        }
        drop(stream);
        let mut recovered = accept();
        let acquire = receive(&mut recovered);
        assert!(matches!(
            acquire.body.command,
            MonotonicOracleCommandV1::Acquire(_)
        ));
        assert!(acquire.body.lease.is_none());
        reply(&mut recovered, &acquire, &current, pending.as_ref(), false);
        if let Some(intent) = pending {
            let finish = receive(&mut recovered);
            assert!(matches!(
                finish.body.command,
                MonotonicOracleCommandV1::Finish
            ));
            current = intent.next;
            reply(&mut recovered, &finish, &current, None, false);
        }
        assert_eq!(current, expected_intent.next);
    });
    let mut guard = provider.acquire(&instance, &initial).unwrap();
    assert!(guard.evidence().is_ok());
    if matches!(fault, Fault::LostFinish) {
        guard.begin(intent.clone()).unwrap();
        assert!(guard.finish().is_err());
    } else {
        assert!(guard.begin(intent.clone()).is_err());
    }
    assert!(
        guard.evidence().is_err(),
        "old signed evidence must not survive a failed lease"
    );
    assert!(guard.begin(intent.clone()).is_err());
    assert!(guard.finish().is_err());
    let signing = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: instance.digest().unwrap(),
        checkpoint: initial.clone(),
        challenge: "9".repeat(64),
        purpose: ProtectedEndpointSignaturePurposeV1::RpcResponse,
        preimage: bytes("oasis7.formal.endpoint.rpc.response.v1", &"8".repeat(64)).unwrap(),
    };
    assert!(guard.sign_endpoint(signing).is_err());
    // Only a new authenticated connection may recover the actual committed or
    // pending state. No old request is replayed and no state is reset to initial.
    let mut recovered = provider.acquire(&instance, &initial).unwrap();
    if matches!(fault, Fault::LostFinish) {
        assert!(recovered.pending().is_none());
    } else {
        assert_eq!(recovered.pending(), Some(&intent));
        recovered.finish().unwrap();
    }
    assert_eq!(recovered.checkpoint(), &intent.next);
    assert!(recovered.evidence().is_ok());
    worker.join().unwrap();
}
#[test]
fn lost_begin_reply_retires_lease_and_fresh_acquire_recovers_pending() {
    exercise(Fault::LostBegin);
}
#[test]
fn lost_finish_reply_retires_lease_and_fresh_acquire_observes_commit() {
    exercise(Fault::LostFinish);
}
#[test]
fn signed_wrong_challenge_retires_lease_without_reusing_cached_evidence() {
    exercise(Fault::ChangedBeginChallenge);
}
