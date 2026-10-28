use super::*;
use ed25519_dalek::{Signer, SigningKey};
use std::cell::Cell;
struct Counted<'a>(&'a Cell<usize>);
impl Serialize for Counted<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.set(self.0.get() + 1);
        serializer.serialize_u64(7)
    }
}
#[test]
fn prepared_strict_verifies_the_single_owned_sdk_image() {
    let key = SigningKey::from_bytes(&[81; 32]);
    let public = hex::encode(key.verifying_key().to_bytes());
    let signature = hex::encode(
        key.sign(&signing_bytes("prepared-test", &7u64).unwrap())
            .to_bytes(),
    );
    let count = Cell::new(0);
    let prepared =
        prepare_owner_signature_strict("prepared-test", &Counted(&count), &public, &signature)
            .unwrap();
    assert_eq!(count.get(), 1);
    assert_eq!(
        prepared.canonical_bytes(),
        signing_bytes("prepared-test", &7u64).unwrap()
    );
    prepared.verify_strict().unwrap();
    prepared.verify_strict().unwrap();
    assert_eq!(count.get(), 1);
    assert!(
        prepare_owner_signature_strict("other", &7u64, &public, &signature)
            .unwrap()
            .verify_strict()
            .is_err()
    );
}
#[test]
fn prepared_strict_rejects_envelope_and_weak_key_before_serializing() {
    let count = Cell::new(0);
    let sig = "00".repeat(64);
    assert!(prepare_owner_signature_strict("d", &Counted(&count), "INVALID", &sig).is_err());
    let mut weak = [0u8; 32];
    weak[0] = 1;
    assert!(
        prepare_owner_signature_strict("d", &Counted(&count), &hex::encode(weak), &sig).is_err()
    );
    assert_eq!(count.get(), 0);
    #[cfg(feature = "signature-metrics")]
    {
        signature_metrics::begin_scope();
        assert!(verify_owner_signature_strict("d", &Counted(&count), "INVALID", &sig).is_err());
        assert!(
            verify_owner_signature_strict("d", &Counted(&count), &hex::encode(weak), &sig).is_err()
        );
        let m = signature_metrics::take_summary();
        assert_eq!((m.calls, m.strict_attempts, m.failures), (2, 0, 2));
    }
}
