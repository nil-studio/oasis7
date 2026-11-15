use super::*;
use crate::controlled_authority::formal::test_fixtures::{public, sign};
use std::cell::Cell;

#[derive(Debug, PartialEq)]
enum Error {
    Protocol(FormalError),
    Stopped,
}
impl From<FormalError> for Error {
    fn from(value: FormalError) -> Self { Self::Protocol(value) }
}
#[test]
fn preimage_verification_matches_original_signature_and_domain_binding() {
    let value = ("actual signed fixture", 17u64);
    let preimage = bytes("test.domain", &value).unwrap();
    let signature = sign(preimage.clone(), 9);
    let key = public(9);
    verify("test.domain", &value, &signature, &key).unwrap();
    let mut charged = Vec::new();
    verify_with_preimage(&signature, &key, || Ok::<_, Error>(preimage.clone()), |n| {charged.push(n); Ok(())}).unwrap();
    assert_eq!(charged, vec![64, 128, preimage.len()]);
    assert_eq!(verify_with_preimage(&signature, &key, || Ok::<_, Error>(bytes("other.domain", &value).unwrap()), |_| Ok(())),
        Err(Error::Protocol(invalid("formal signature verification"))));
    assert_eq!(verify_with_preimage(&signature, &public(8), || Ok::<_, Error>(preimage), |_| Ok(())),
        Err(Error::Protocol(invalid("formal signature verification"))));
}
#[test]
fn preimage_validation_errors_precede_builder_and_preserve_canonical_encoding() {
    let key = public(9);
    for (signature, key, message) in [
        ("bad".to_owned(), "bad".to_owned(), "formal digest/key requires canonical lowercase hex32"),
        ("bad".to_owned(), key.clone(), "formal signature requires canonical lowercase hex64"),
        ("AA".repeat(64), key, "formal signature requires canonical lowercase hex64"),
    ] {
        let built = Cell::new(false);
        let result = verify_with_preimage(&signature, &key, || {
            built.set(true); Ok::<_, Error>(Vec::<u8>::new())
        }, |_| Ok(()));
        assert_eq!(result, Err(Error::Protocol(invalid(message))));
        assert!(!built.get());
    }
}
#[test]
fn preimage_work_failure_and_builder_failure_propagate_without_retry() {
    let preimage = bytes("test.domain", &42u64).unwrap();
    let signature = sign(preimage.clone(), 9);
    let key = public(9);
    for stop in 1..=3 {
        let built = Cell::new(0);
        let mut calls = 0;
        let result = verify_with_preimage(&signature, &key, || {
            built.set(built.get() + 1); Ok::<_, Error>(preimage.clone())
        }, |_| { calls += 1; if calls == stop { Err(Error::Stopped) } else { Ok(()) } });
        assert_eq!(result, Err(Error::Stopped));
        assert_eq!(calls, stop);
        assert_eq!(built.get(), usize::from(stop == 3));
    }
    let mut calls = 0;
    let result = verify_with_preimage(&signature, &key, || Err::<Vec<u8>, _>(Error::Stopped), |_| { calls += 1; Ok(()) });
    assert_eq!(result, Err(Error::Stopped));
    assert_eq!(calls, 2);
}

#[test]
fn preimage_single_borrow_is_both_charged_and_verified() {
    struct Changing<'a> { bytes: &'a [u8], calls: &'a Cell<usize> }
    impl AsRef<[u8]> for Changing<'_> {
        fn as_ref(&self) -> &[u8] {
            let n = self.calls.get();
            self.calls.set(n + 1);
            if n == 0 { &[] } else { self.bytes }
        }
    }
    let bytes = bytes("test.domain", &vec![1u8; 4096]).unwrap();
    let signature = sign(bytes.clone(), 9);
    let calls = Cell::new(0);
    let mut charges = Vec::new();
    let result = verify_with_preimage(&signature, &public(9),
        || Ok::<_, Error>(Changing { bytes: &bytes, calls: &calls }),
        |n| { charges.push(n); Ok(()) });
    assert_eq!(result, Err(Error::Protocol(invalid("formal signature verification"))));
    assert_eq!(calls.get(), 1);
    assert_eq!(charges, vec![64, 128, 0]);
}
