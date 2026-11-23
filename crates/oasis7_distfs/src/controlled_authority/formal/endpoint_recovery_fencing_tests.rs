//! CA-F1 exercises final signer and checkpoint-CAS entry points with an old
//! credential. The loopback oracle is a software fixture, not custody evidence.
use super::*;
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use std::io::{Read, Write};
use std::net::TcpStream;

const MAX_REPLY: usize = 1024 * 1024;

/// Original receipt bytes are captured before the recovery plan drains Ready
/// into Qualified. No old signature is manufactured after the plan is installed.
pub(super) struct OriginalReady {
    entries: [FormalRequestStateV1; 2],
    proof: FormalQualifiedCommitProofV1,
}

pub(super) fn capture_ready(fixture: &Fixture, cut: &VerifiedFormalCommitV1) -> OriginalReady {
    let mut entries = Vec::new();
    let mut decisions = Vec::new();
    let mut completions = Vec::new();
    for (dir, seed) in [(&fixture.pdir, 5), (&fixture.rdir, 6)] {
        let source = FileFormalArchiveSourceV1::new(dir.clone()).unwrap();
        let bytes = fs::read(dir.join("formal-checkpoint-v1.cbor")).unwrap();
        let body = Store::verify_public(&bytes, &public(seed)).unwrap();
        assert!(body.recovery_plan.is_none() && body.seal.is_none());
        let entry: FormalRequestStateV1 = serde_cbor::from_slice(
            &source
                .read_object(body.pending.as_ref().unwrap())
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let FormalRequestStateV1::Ready {
            decision,
            complete: Some(complete),
            ..
        } = &entry
        else {
            panic!("CA-F1 original Ready bytes unavailable before the recovery plan");
        };
        decisions.push(decision.clone());
        completions.push(complete.clone());
        entries.push(entry);
    }
    assert_eq!(decisions[0], decisions[1]);
    let proof = FormalQualifiedCommitProofV1 {
        schema_version: 1,
        decision: decisions.remove(0),
        primary_complete: completions.remove(0),
        replica_complete: completions.remove(0),
    };
    verify_formal_qualified(&proof, &fixture.authority, Some(cut), None).unwrap();
    OriginalReady {
        entries: entries.try_into().unwrap(),
        proof,
    }
}

/// Cold remounts retain the actual drained Qualified record. Replaying that
/// exact proof is an idempotent read; a fresh old-epoch Frozen request is refused.
pub(super) fn remount_sealed(
    binary: &std::ffi::OsStr,
    fixture: &Fixture,
    configs: &[PathBuf],
    cut: &VerifiedFormalCommitV1,
    original: &OriginalReady,
) {
    assert_eq!(cut.record(), &original.proof.decision.record);
    for ((config, role), dir) in configs
        .iter()
        .zip([EndpointRole::Primary, EndpointRole::Replica])
        .zip([&fixture.pdir, &fixture.rdir])
    {
        let file = dir.join("formal-checkpoint-v1.cbor");
        let original_bytes = fs::read(&file).unwrap();
        let (mut child, address) = start(binary, config);
        let mut endpoint = remote(address, &fixture.authority, role, 4, cut.head());
        assert_eq!(endpoint.head().unwrap(), cut.head());
        assert_eq!(
            endpoint.publish_qualified(original.proof.clone()).unwrap(),
            cut.head()
        );
        let (input, material) = fixture.frozen(cut, 47);
        endpoint.freeze(input, material).unwrap_err();
        assert_eq!(fs::read(&file).unwrap(), original_bytes);
        child.kill().unwrap();
        child.wait().unwrap();
        // Independently reopen the same native bytes after the refused write.
        let (mut reopened, address) = start(binary, config);
        let mut endpoint = remote(address, &fixture.authority, role, 4, cut.head());
        assert_eq!(endpoint.head().unwrap(), cut.head());
        let outcome = endpoint
            .query(&original.proof.decision.record.request_digest)
            .unwrap();
        let FormalQueryOutcomeV1::Recorded(state) = outcome else {
            panic!("CA-F1 original drained result disappeared after remount");
        };
        let FormalRequestStateV1::Qualified { proof, .. } = *state else {
            panic!("CA-F1 original Ready must have drained before sealing");
        };
        assert_eq!(proof, original.proof);
        assert_eq!(fs::read(&file).unwrap(), original_bytes);
        reopened.kill().unwrap();
        reopened.wait().unwrap();
        eprintln!(
            "CA-F1 role={role:?} old_native_remount=readable fresh_freeze=refused original_qualified_retry=idempotent cold_head=unchanged"
        );
    }
}

pub(super) fn final_entries(fixture: &Fixture, oracles: [&Oracle; 2], original: &OriginalReady) {
    for (((role, dir, signer), oracle), entry) in [
        (EndpointRole::Primary, &fixture.pdir, 5),
        (EndpointRole::Replica, &fixture.rdir, 6),
    ]
    .into_iter()
    .zip(oracles)
    .zip(&original.entries)
    {
        let instance = match role {
            EndpointRole::Primary => &fixture.authority.definition().primary.instance,
            EndpointRole::Replica => &fixture.authority.definition().replica.instance,
        };
        let file = dir.join("formal-checkpoint-v1.cbor");
        let bytes = fs::read(&file).unwrap();
        let body = Store::verify_public(&bytes, &instance.endpoint_public_key).unwrap();
        let checkpoint = Store::binding(&body, &bytes);
        assert!(
            body.recovery_plan.is_some(),
            "CA-F1 requires the actual durable recovery plan"
        );
        let FormalRequestStateV1::Ready {
            candidate,
            prepare,
            decision,
            complete: Some(complete),
        } = entry
        else {
            panic!("CA-F1 original Ready evidence changed");
        };
        let frozen = candidate.frozen.as_ref().unwrap();
        let frozen_receipt = match role {
            EndpointRole::Primary => &frozen.primary,
            EndpointRole::Replica => &frozen.replica,
        };
        let final_receipt = match role {
            EndpointRole::Primary => &decision.primary,
            EndpointRole::Replica => &decision.replica,
        };
        let mut requests = vec![
            (
                ProtectedEndpointSignaturePurposeV1::NativeCheckpoint,
                Store::checkpoint_signing_bytes(&body).unwrap(),
            ),
            (
                ProtectedEndpointSignaturePurposeV1::FrozenReceipt,
                frozen_receipt_signing_bytes(&frozen_receipt.body).unwrap(),
            ),
            (
                ProtectedEndpointSignaturePurposeV1::PrepareReceipt,
                formal_prepare_receipt_signing_bytes(&prepare.body).unwrap(),
            ),
            (
                ProtectedEndpointSignaturePurposeV1::FinalReceipt,
                formal_final_receipt_signing_bytes(&final_receipt.body).unwrap(),
            ),
            (
                ProtectedEndpointSignaturePurposeV1::ProofComplete,
                formal_proof_complete_signing_bytes(&complete.body).unwrap(),
            ),
        ];
        if let Some(seal) = &body.seal {
            requests.push((
                ProtectedEndpointSignaturePurposeV1::EpochSeal,
                fencing::epoch_seal_signing_bytes(seal).unwrap(),
            ));
        }
        for (purpose, preimage) in requests {
            let (mut connection, lease) =
                acquire(oracle, instance, &checkpoint, body.seal.is_some());
            let signing = ProtectedEndpointSigningRequestV1 {
                schema_version: 1,
                instance_digest: instance.digest().unwrap(),
                checkpoint: checkpoint.clone(),
                challenge: h(201),
                purpose: purpose.clone(),
                preimage,
            };
            signing.validate().unwrap();
            // Send the authenticated command directly. Calling the ordinary
            // adapter here would stop at its local seal guard and would not
            // prove the provider's final signing entry rejects the old owner.
            send(
                &mut connection,
                instance,
                Some(lease),
                MonotonicOracleCommandV1::ProtectedSign(Box::new(signing)),
            );
            assert_refused(&mut connection);
            assert_eq!(fs::read(&file).unwrap(), bytes);
            eprintln!(
                "CA-F1 role={role:?} final_signer={purpose:?} old_credential=refused checkpoint=unchanged"
            );
        }

        // Even a valid signature made with the old endpoint key cannot reset
        // the durable seal or append a fresh native checkpoint. The real
        // provider receives a complete, signed, one-revision CAS request.
        let mut next = body.clone();
        next.revision = next.revision.checked_add(1).unwrap();
        next.seal = None;
        #[derive(serde::Serialize)]
        struct SignedCheckpoint<'a> {
            body: &'a HotBody,
            signature: String,
        }
        let next_bytes = serde_cbor::to_vec(&SignedCheckpoint {
            body: &next,
            signature: hex::encode(
                key(signer)
                    .sign(&Store::checkpoint_signing_bytes(&next).unwrap())
                    .to_bytes(),
            ),
        })
        .unwrap();
        assert_eq!(
            Store::verify_public(&next_bytes, &instance.endpoint_public_key).unwrap(),
            next
        );
        let intent = CheckpointIntentV1 {
            previous: checkpoint.clone(),
            next: Store::binding(&next, &next_bytes),
            checkpoint_bytes: next_bytes,
            seal: None,
        };
        let (mut connection, lease) = acquire(oracle, instance, &checkpoint, body.seal.is_some());
        send(
            &mut connection,
            instance,
            Some(lease),
            MonotonicOracleCommandV1::Begin(Box::new(intent)),
        );
        assert_refused(&mut connection);
        assert_eq!(fs::read(&file).unwrap(), bytes);
        let (connection, _) = acquire(oracle, instance, &checkpoint, body.seal.is_some());
        drop(connection);
        eprintln!(
            "CA-F1 role={role:?} final_native_store=refused old_signed_CAS=valid checkpoint=unchanged"
        );
    }
}

fn acquire(
    oracle: &Oracle,
    instance: &EndpointInstanceBindingV1,
    checkpoint: &EndpointCheckpointBindingV1,
    sealed: bool,
) -> (TcpStream, String) {
    let mut connection =
        TcpStream::connect_timeout(&oracle.address, Duration::from_secs(3)).unwrap();
    connection.set_nodelay(true).unwrap();
    connection
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    connection
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let request = send(
        &mut connection,
        instance,
        None,
        MonotonicOracleCommandV1::Acquire(checkpoint.clone()),
    );
    let mut header = [0; 4];
    connection.read_exact(&mut header).unwrap();
    let length = u32::from_be_bytes(header) as usize;
    assert!((1..=MAX_REPLY).contains(&length));
    let mut bytes = vec![0; length];
    connection.read_exact(&mut bytes).unwrap();
    let reply: SignedMonotonicOracleReplyV1 = serde_cbor::from_slice(&bytes).unwrap();
    let signature = Signature::from_slice(&hex::decode(&reply.signature).unwrap()).unwrap();
    VerifyingKey::from_bytes(&key(3).verifying_key().to_bytes())
        .unwrap()
        .verify_strict(
            &monotonic_oracle_reply_signing_bytes(&reply.body).unwrap(),
            &signature,
        )
        .unwrap();
    assert_eq!(
        reply.body.request_digest,
        fencing::hash_value(&request).unwrap()
    );
    assert_eq!(reply.body.challenge, request.challenge);
    assert_eq!(&reply.body.checkpoint, checkpoint);
    assert!(reply.body.pending.is_none());
    assert_eq!(reply.body.seal.is_some(), sealed);
    assert!(reply.body.protected_signature.is_none());
    (connection, reply.body.lease)
}

fn send(
    connection: &mut TcpStream,
    instance: &EndpointInstanceBindingV1,
    lease: Option<String>,
    command: MonotonicOracleCommandV1,
) -> MonotonicOracleRequestBodyV1 {
    let body = MonotonicOracleRequestBodyV1 {
        schema_version: 1,
        instance: instance.clone(),
        authentication_public_key: public(17),
        lease,
        challenge: h(202),
        command,
    };
    let request = SignedMonotonicOracleRequestV1 {
        signature: hex::encode(
            key(17)
                .sign(&monotonic_oracle_request_signing_bytes(&body).unwrap())
                .to_bytes(),
        ),
        body: body.clone(),
    };
    let bytes = serde_cbor::to_vec(&request).unwrap();
    assert!(bytes.len() <= MAX_REPLY);
    connection
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    connection.write_all(&bytes).unwrap();
    body
}

fn assert_refused(connection: &mut TcpStream) {
    let mut header = [0; 4];
    let error = connection.read_exact(&mut header).unwrap_err();
    assert!(
        matches!(
            error.kind(),
            std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
        ),
        "CA-F1 final provider did not actively refuse: {error}"
    );
}
