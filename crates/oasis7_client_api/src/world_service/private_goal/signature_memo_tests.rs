use super::super::{signing_bytes, verify_owner_signature_strict};
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::Value;
const DOMAIN: &str = "test-only-exact-owner-proof";
fn public(key: &SigningKey) -> String {
    hex::encode(key.verifying_key().to_bytes())
}
fn signature(key: &SigningKey, domain: &str, value: &Value) -> String {
    hex::encode(key.sign(&signing_bytes(domain, value).unwrap()).to_bytes())
}
fn cached(key: &SigningKey, domain: &str, value: &Value, signature: &str) -> bool {
    contains(
        &key.verifying_key().to_bytes(),
        &hex::decode(signature).unwrap().try_into().unwrap(),
        &signing_bytes(domain, value).unwrap(),
    )
}
fn count() -> usize {
    BATCH.with(|state| state.borrow().as_ref().map_or(0, |b| b.entries.len()))
}
#[test]
fn exact_success_reused_only_inside_outer_synchronous_scope() {
    let key = SigningKey::from_bytes(&[7; 32]);
    let value = serde_json::json!({"private":"original"});
    let sig = signature(&key, DOMAIN, &value);
    for _ in 0..2 {
        verify_owner_signature_strict(DOMAIN, &value, &public(&key), &sig).unwrap();
    }
    assert!(!cached(&key, DOMAIN, &value, &sig));
    with_owner_signature_verification_batch(|| {
        verify_owner_signature_strict(DOMAIN, &value, &public(&key), &sig).unwrap();
        assert!(cached(&key, DOMAIN, &value, &sig));
        assert_eq!(count(), 1);
        with_owner_signature_verification_batch(|| {
            verify_owner_signature_strict(DOMAIN, &value, &public(&key), &sig).unwrap();
            assert_eq!(count(), 1);
        });
        assert!(cached(&key, DOMAIN, &value, &sig));
    });
    assert!(!cached(&key, DOMAIN, &value, &sig));
    let result: Result<(), ()> = with_owner_signature_verification_batch(|| {
        verify_owner_signature_strict(DOMAIN, &value, &public(&key), &sig).unwrap();
        Err(())
    });
    assert!(result.is_err());
    assert_eq!(count(), 0);
    let panicked = std::panic::catch_unwind(|| {
        with_owner_signature_verification_batch(|| {
            verify_owner_signature_strict(DOMAIN, &value, &public(&key), &sig).unwrap();
            panic!("test unwind");
        })
    });
    assert!(panicked.is_err());
    assert_eq!(count(), 0);
    with_owner_signature_verification_batch(|| assert_eq!(count(), 0));
}
#[test]
fn full_message_domain_key_signature_and_strict_failures_are_not_conflated() {
    let key = SigningKey::from_bytes(&[8; 32]);
    let other = SigningKey::from_bytes(&[9; 32]);
    let value = serde_json::json!({"private":"a"});
    let sig = signature(&key, DOMAIN, &value);
    with_owner_signature_verification_batch(|| {
        verify_owner_signature_strict(DOMAIN, &value, &public(&key), &sig).unwrap();
        let changed = serde_json::json!({"private":"b"});
        for _ in 0..2 {
            assert!(verify_owner_signature_strict(DOMAIN, &changed, &public(&key), &sig).is_err());
            assert!(
                verify_owner_signature_strict("different-domain", &value, &public(&key), &sig)
                    .is_err()
            );
            assert!(verify_owner_signature_strict(DOMAIN, &value, &public(&other), &sig).is_err());
            let mut changed_signature = hex::decode(&sig).unwrap();
            changed_signature[0] ^= 1;
            assert!(
                verify_owner_signature_strict(
                    DOMAIN,
                    &value,
                    &public(&key),
                    &hex::encode(changed_signature)
                )
                .is_err()
            );
            assert!(
                verify_owner_signature_strict(DOMAIN, &value, &"00".repeat(32), &"00".repeat(64))
                    .is_err()
            );
            assert!(
                verify_owner_signature_strict(DOMAIN, &value, &public(&key), &"00".repeat(64))
                    .is_err()
            );
        }
        assert_eq!(count(), 1);
        assert!(!cached(&key, DOMAIN, &changed, &sig));
        let other_sig = signature(&other, DOMAIN, &value);
        verify_owner_signature_strict(DOMAIN, &value, &public(&other), &other_sig).unwrap();
        let domain_sig = signature(&key, "different-domain", &value);
        verify_owner_signature_strict("different-domain", &value, &public(&key), &domain_sig)
            .unwrap();
        assert_eq!(count(), 3);
    });
}
#[test]
fn tuple_and_byte_capacity_fall_back_to_actual_strict_verification() {
    let key = SigningKey::from_bytes(&[10; 32]);
    with_owner_signature_verification_batch(|| {
        for i in 0..MAX_TUPLES {
            let v = serde_json::json!({"i":i});
            let sig = signature(&key, DOMAIN, &v);
            verify_owner_signature_strict(DOMAIN, &v, &public(&key), &sig).unwrap();
        }
        assert_eq!(count(), MAX_TUPLES);
        let v = serde_json::json!({"i":MAX_TUPLES});
        let sig = signature(&key, DOMAIN, &v);
        for _ in 0..2 {
            verify_owner_signature_strict(DOMAIN, &v, &public(&key), &sig).unwrap();
        }
        assert!(!cached(&key, DOMAIN, &v, &sig));
        assert_eq!(count(), MAX_TUPLES);
        assert!(
            verify_owner_signature_strict(
                DOMAIN,
                &serde_json::json!({"i":MAX_TUPLES+1}),
                &public(&key),
                &sig
            )
            .is_err()
        );
    });
    with_owner_signature_verification_batch(|| {
        let large = serde_json::json!({"private":"x".repeat(MAX_BYTES)});
        let sig = signature(&key, DOMAIN, &large);
        for _ in 0..2 {
            verify_owner_signature_strict(DOMAIN, &large, &public(&key), &sig).unwrap();
        }
        assert_eq!(count(), 0);
        assert!(!cached(&key, DOMAIN, &large, &sig));
        BATCH.with(|state| assert!(state.borrow().as_ref().unwrap().retained_bytes <= MAX_BYTES));
    });
}
#[test]
#[cfg(feature = "signature-metrics")]
fn metrics_separate_memo_success_from_real_crypto_attempts() {
    use super::super::signature_metrics::{begin_scope, take_summary};
    let key = SigningKey::from_bytes(&[11; 32]);
    let v = serde_json::json!({"private":"not exported"});
    let sig = signature(&key, DOMAIN, &v);
    begin_scope();
    with_owner_signature_verification_batch(|| {
        for _ in 0..3 {
            verify_owner_signature_strict(DOMAIN, &v, &public(&key), &sig).unwrap();
        }
        let changed = serde_json::json!({"private":"changed"});
        for _ in 0..2 {
            assert!(verify_owner_signature_strict(DOMAIN, &changed, &public(&key), &sig).is_err());
        }
    });
    let s = take_summary();
    assert_eq!(
        (
            s.calls,
            s.strict_attempts,
            s.memo_hits,
            s.successes,
            s.failures
        ),
        (5, 3, 2, 3, 2)
    );
    begin_scope();
    verify_owner_signature_strict(DOMAIN, &v, &public(&key), &sig).unwrap();
    let s = take_summary();
    assert_eq!((s.strict_attempts, s.memo_hits), (1, 0));
}
