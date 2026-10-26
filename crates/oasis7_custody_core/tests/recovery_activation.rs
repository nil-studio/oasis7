use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use oasis7_custody_core::{DecimalU64, recovery::*};
struct Strict;
impl RecoveryActivationVerifier for Strict {
    fn verify(
        &mut self,
        public: &str,
        signature: &str,
        bytes: &[u8],
    ) -> Result<(), InvalidRecoveryActivation> {
        let mut key = [0u8; 32];
        for (i, byte) in key.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&public[i * 2..i * 2 + 2], 16)
                .map_err(|_| InvalidRecoveryActivation)?;
        }
        let raw = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| InvalidRecoveryActivation)?;
        if URL_SAFE_NO_PAD.encode(&raw) != signature {
            return Err(InvalidRecoveryActivation);
        }
        VerifyingKey::from_bytes(&key)
            .map_err(|_| InvalidRecoveryActivation)?
            .verify_strict(
                bytes,
                &Signature::from_slice(&raw).map_err(|_| InvalidRecoveryActivation)?,
            )
            .map_err(|_| InvalidRecoveryActivation)
    }
}
fn fixture() -> (RecoveryAuthorityPolicyV1, SignedRecoveryActivationV1) {
    let keys = [
        SigningKey::from_bytes(&[81; 32]),
        SigningKey::from_bytes(&[82; 32]),
    ];
    let policy = RecoveryAuthorityPolicyV1 {
        api_version: 1,
        environment: "test".into(),
        authority_id: "independent-recovery-one".into(),
        quorum: 2,
        keys: keys
            .iter()
            .enumerate()
            .map(|(i, key)| RecoveryAuthorityKeyV1 {
                key_id: format!("offline-{i}"),
                public_key: key
                    .verifying_key()
                    .to_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
            })
            .collect(),
    };
    let manifest = RecoveryActivationManifestV1 {
        api_version: 1,
        environment: policy.environment.clone(),
        authority_id: policy.authority_id.clone(),
        object_id: "physical-fence-one".into(),
        operation_id: "bootstrap-one".into(),
        policy_digest: policy.digest().unwrap(),
        previous_state_digest: "ab".repeat(32),
        evidence_digest: "cd".repeat(32),
        from_epoch: DecimalU64(0),
        to_epoch: DecimalU64(1),
        issued_at: DecimalU64(1000),
        expires_at: DecimalU64(301_000),
    };
    let bytes = manifest.signing_bytes().unwrap();
    let signatures = keys
        .iter()
        .enumerate()
        .map(|(i, key)| RecoveryAuthoritySignatureV1 {
            key_id: format!("offline-{i}"),
            signature: URL_SAFE_NO_PAD.encode(key.sign(&bytes).to_bytes()),
        })
        .collect();
    (
        policy,
        SignedRecoveryActivationV1 {
            manifest,
            signatures,
        },
    )
}
fn current(fence: &RecoveryFence) -> RecoveryActivationCurrent<'_> {
    RecoveryActivationCurrent {
        environment: "test",
        authority_id: "independent-recovery-one",
        object_id: "physical-fence-one",
        state_digest: "abababababababababababababababababababababababababababababababab",
        fence,
    }
}
#[test]
fn independent_quorum_advances_only_the_exact_service_epoch() {
    let (policy, signed) = fixture();
    let fence = RecoveryFence::new(0, true);
    let next = signed
        .authorize(&policy, &current(&fence), 1000, &mut Strict)
        .unwrap();
    assert_eq!(next.epoch(), 1);
    assert!(!next.disabled());
    assert!(fence.disabled()); // Pure verification cannot mutate persisted authority.
    assert!(
        signed
            .authorize(&policy, &current(&next), 1001, &mut Strict)
            .is_err()
    );
    assert!(
        signed
            .authorize(&policy, &current(&fence), 301_000, &mut Strict)
            .is_err()
    );
    assert!(
        signed
            .authorize(&policy, &current(&fence), 999, &mut Strict)
            .is_err()
    );
    let mut still_monotonic = fence;
    assert_eq!(
        still_monotonic.tighten(1, false),
        Err(RecoveryError::Reenable)
    );
}
#[test]
fn one_signer_duplicate_signers_or_tampered_scope_never_form_a_quorum() {
    let (policy, signed) = fixture();
    let fence = RecoveryFence::new(0, true);
    let mut one = signed.clone();
    one.signatures.pop();
    assert!(
        one.authorize(&policy, &current(&fence), 1001, &mut Strict)
            .is_err()
    );
    let mut duplicate = signed.clone();
    duplicate.signatures[1] = duplicate.signatures[0].clone();
    assert!(
        duplicate
            .authorize(&policy, &current(&fence), 1001, &mut Strict)
            .is_err()
    );
    let mut tampered = signed.clone();
    tampered.manifest.evidence_digest = "ef".repeat(32);
    assert!(
        tampered
            .authorize(&policy, &current(&fence), 1001, &mut Strict)
            .is_err()
    );
    let mut wrong = current(&fence);
    wrong.object_id = "cloned-namespace";
    assert!(
        signed
            .authorize(&policy, &wrong, 1001, &mut Strict)
            .is_err()
    );
    let mut wrong = current(&fence);
    wrong.state_digest = "unknown";
    assert!(
        signed
            .authorize(&policy, &wrong, 1001, &mut Strict)
            .is_err()
    );
    let mut reused = policy.clone();
    reused.keys[1].public_key = reused.keys[0].public_key.clone();
    assert!(reused.validate().is_err());
}
#[test]
fn wire_is_closed_and_overflow_or_overlong_activation_windows_are_rejected() {
    let (_, signed) = fixture();
    let mut wire = serde_json::to_value(&signed).unwrap();
    wire["manifest"]["enabled"] = true.into();
    assert!(serde_json::from_value::<SignedRecoveryActivationV1>(wire).is_err());
    let mut manifest = signed.manifest;
    manifest.expires_at = DecimalU64(301_001);
    assert!(manifest.signing_bytes().is_err());
    manifest.expires_at = DecimalU64(301_000);
    manifest.from_epoch = DecimalU64(u64::MAX);
    manifest.to_epoch = DecimalU64(0);
    assert!(manifest.signing_bytes().is_err());
}
