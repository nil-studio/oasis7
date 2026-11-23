use super::*;
use crate::controlled_authority::replicated_protocol::HeadAnchor;
use ed25519_dalek::Signer;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn root() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "oasis7-fencing-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    path
}
fn instance(path: &Path) -> EndpointInstanceBindingV1 {
    EndpointInstanceBindingV1 {
        schema_version: 1,
        world_id: "world".into(),
        chain_id: "chain".into(),
        genesis_digest: "a".repeat(64),
        authority_epoch: 4,
        endpoint_role: EndpointRole::Primary,
        endpoint_id: "primary".into(),
        endpoint_public_key: hex::encode(
            SigningKey::from_bytes(&[2; 32]).verifying_key().to_bytes(),
        ),
        instance_id: "b".repeat(64),
        storage_id: "c".repeat(64),
        storage_fingerprint: directory_fingerprint(path).unwrap(),
    }
}
fn checkpoint(revision: u64, bytes: &[u8]) -> EndpointCheckpointBindingV1 {
    EndpointCheckpointBindingV1 {
        revision,
        commitment: blake3::hash(bytes).to_hex().to_string(),
        head: HeadAnchor {
            position: 0,
            decision_hash: "d".repeat(64),
            qualified: false,
        },
    }
}
#[test]
#[cfg(unix)]
fn provider_retains_intent_progress_and_unique_instance_but_never_claims_formal_fencing() {
    let root = root();
    let path = root.join("endpoint");
    std::fs::create_dir(&path).unwrap();
    let binding = instance(&path);
    let provider =
        ControlledFileProvider::new(root.join("provider"), SigningKey::from_bytes(&[4; 32]))
            .unwrap();
    let initial = checkpoint(1, b"initial");
    let next = checkpoint(2, b"next");
    let mut guard = provider.acquire(&binding, &initial).unwrap();
    guard
        .begin(CheckpointIntentV1 {
            previous: initial.clone(),
            next: next.clone(),
            checkpoint_bytes: b"next".to_vec(),
            seal: None,
        })
        .unwrap();
    drop(guard);
    let mut guard = provider.acquire(&binding, &initial).unwrap();
    assert_eq!(guard.pending().unwrap().next, next);
    guard.finish().unwrap();
    assert_eq!(guard.checkpoint(), &next);
    let body = EpochSealBodyV1 {
        recovery_plan: None,
        writer_seal: None,
        schema_version: 1,
        instance: binding.clone(),
        recovery_operation: "e".repeat(64),
        highest_complete: next.head.clone(),
        checkpoint_before: next.clone(),
        tail: SealedTailV1::empty(),
        next_epoch_floor: 5,
    };
    let sealed = checkpoint(3, b"sealed");
    guard
        .begin(CheckpointIntentV1 {
            previous: next,
            next: sealed.clone(),
            checkpoint_bytes: b"sealed".to_vec(),
            seal: Some(body.clone()),
        })
        .unwrap();
    guard.finish().unwrap();
    let receipt = EpochSealReceiptV1::sign(
        body,
        guard.evidence().unwrap(),
        &SigningKey::from_bytes(&[2; 32]),
    )
    .unwrap();
    assert_eq!(
        verify_epoch_seal(
            &receipt,
            &TrustedFencingPolicyV1 {
                provider_public_key: provider.public_key(),
                authorized_resource_bindings: vec![provider.resource_binding().into()]
            }
        )
        .unwrap_err(),
        FencingError::FencingUnproven
    );
    assert_eq!(
        guard
            .begin(CheckpointIntentV1 {
                previous: sealed,
                next: checkpoint(4, b"forbidden"),
                checkpoint_bytes: b"forbidden".to_vec(),
                seal: None
            })
            .unwrap_err(),
        FencingError::EpochSealed
    );
    drop(guard);
    let mut copied = binding;
    copied.instance_id = "f".repeat(64);
    assert!(provider.acquire(&copied, &initial).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn protected_signature_rejects_changed_purpose_floor_nonce_resource_and_key() {
    // Simulated external signer fixture; this test establishes cryptographic
    // binding only, not physical key custody or a real nonrollback deployment.
    let path = root();
    let binding = instance(&path);
    let endpoint_key = SigningKey::from_bytes(&[2; 32]);
    let provider_key = SigningKey::from_bytes(&[4; 32]);
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: hex::encode(provider_key.verifying_key().to_bytes()),
        authorized_resource_bindings: vec!["resource".into()],
    };
    let request = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: binding.digest().unwrap(),
        checkpoint: checkpoint(4, b"current"),
        challenge: "e".repeat(64),
        purpose: ProtectedEndpointSignaturePurposeV1::RpcResponse,
        preimage: bytes("oasis7.formal.endpoint.rpc.response.v1", &"f".repeat(64)).unwrap(),
    };
    let body = ProtectedEndpointSignatureBodyV1 {
        schema_version: 1,
        custody: ProtectedSignerCustodyV1::NonexportableLeaseBoundV1,
        instance_digest: binding.digest().unwrap(),
        resource_binding: "resource".into(),
        checkpoint: request.checkpoint.clone(),
        request_digest: request.digest().unwrap(),
        challenge: request.challenge.clone(),
        endpoint_signature: hex::encode(endpoint_key.sign(&request.preimage).to_bytes()),
    };
    let signature = ProtectedEndpointSignatureV1 {
        provider_signature: hex::encode(
            provider_key
                .sign(&protected_endpoint_signature_signing_bytes(&body).unwrap())
                .to_bytes(),
        ),
        body,
    };
    assert!(verify_protected_endpoint_signature(&signature, &request, &binding, &policy).is_ok());
    let mut changed = request.clone();
    changed.challenge = "a".repeat(64);
    assert!(verify_protected_endpoint_signature(&signature, &changed, &binding, &policy).is_err());
    changed = request.clone();
    changed.checkpoint.revision += 1;
    assert!(verify_protected_endpoint_signature(&signature, &changed, &binding, &policy).is_err());
    changed = request.clone();
    changed.purpose = ProtectedEndpointSignaturePurposeV1::FinalReceipt;
    assert!(verify_protected_endpoint_signature(&signature, &changed, &binding, &policy).is_err());
    let mut changed_policy = policy.clone();
    changed_policy.authorized_resource_bindings = vec!["other".into()];
    assert!(
        verify_protected_endpoint_signature(&signature, &request, &binding, &changed_policy)
            .is_err()
    );
    changed_policy = policy;
    changed_policy.provider_public_key =
        hex::encode(SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes());
    assert!(
        verify_protected_endpoint_signature(&signature, &request, &binding, &changed_policy)
            .is_err()
    );
    std::fs::remove_dir_all(path).unwrap();
}
