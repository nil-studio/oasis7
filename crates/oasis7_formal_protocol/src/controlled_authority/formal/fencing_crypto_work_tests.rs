use super::*;
use std::cell::Cell;
#[derive(Debug, PartialEq)]
enum Error {
    Protocol(FencingError),
    Stop,
}
impl From<FencingError> for Error {
    fn from(value: FencingError) -> Self {
        Self::Protocol(value)
    }
}
fn key() -> SigningKey {
    SigningKey::from_bytes(&[9; 32])
}
#[test]
fn fencing_crypto_paid_sign_and_verify_match_legacy() {
    let key = key();
    let public = hex::encode(key.verifying_key().to_bytes());
    let value = ("real payload", 42u64);
    let preimage = bytes(SEAL_DOMAIN, &value).unwrap();
    let expected = sign(SEAL_DOMAIN, &value, &key).unwrap();
    let mut charged = Vec::new();
    let actual = sign_with_preimage(
        &key,
        || Ok::<_, Error>(preimage.clone()),
        |n| {
            charged.push(n);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(charged, vec![preimage.len()]);
    verify_with_preimage(
        &actual,
        &public,
        || Ok::<_, Error>(preimage.clone()),
        |_| Ok(()),
    )
    .unwrap();
    verify(SEAL_DOMAIN, &value, &actual, &public).unwrap();
    assert!(
        verify_with_preimage(
            &actual,
            &public,
            || Ok::<_, Error>(bytes(PROVIDER_DOMAIN, &value).unwrap()),
            |_| Ok(())
        )
        .is_err()
    );
    // Existing fencing signatures accept either hex case; do not silently tighten the legacy wire.
    verify_with_preimage(
        &actual.to_uppercase(),
        &public,
        || Ok::<_, Error>(preimage),
        |_| Ok(()),
    )
    .unwrap();
}
#[test]
fn fencing_crypto_errors_and_cancellation_precede_crypto_work() {
    let key = key();
    let public = hex::encode(key.verifying_key().to_bytes());
    let preimage = bytes(SEAL_DOMAIN, &42).unwrap();
    let signature = sign(SEAL_DOMAIN, &42, &key).unwrap();
    for stop in 1..=5 {
        let built = Cell::new(0);
        let mut calls = 0;
        let result = verify_with_preimage(
            &signature,
            &public,
            || {
                built.set(built.get() + 1);
                Ok::<_, Error>(preimage.clone())
            },
            |_| {
                calls += 1;
                if calls == stop {
                    Err(Error::Stop)
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result, Err(Error::Stop));
        assert_eq!(calls, stop);
        assert_eq!(built.get(), usize::from(stop == 5));
    }
    let built = Cell::new(false);
    let bad_key = verify_with_preimage(
        "bad",
        "bad",
        || {
            built.set(true);
            Ok::<_, Error>(preimage.clone())
        },
        |_| Ok(()),
    );
    assert!(matches!(bad_key, Err(Error::Protocol(_))));
    assert!(!built.get());
    assert_eq!(
        sign_with_preimage(&key, || Ok::<_, Error>(preimage), |_| Err(Error::Stop)),
        Err(Error::Stop)
    );
    assert_eq!(
        sign_with_preimage(
            &key,
            || Err::<Vec<u8>, _>(Error::Stop),
            |_| panic!("work after failed builder")
        ),
        Err(Error::Stop)
    );
}
struct Changing<'a> {
    bytes: &'a [u8],
    calls: &'a Cell<usize>,
}
impl AsRef<[u8]> for Changing<'_> {
    fn as_ref(&self) -> &[u8] {
        let n = self.calls.get();
        self.calls.set(n + 1);
        if n == 0 { &[] } else { self.bytes }
    }
}
#[test]
fn fencing_crypto_sign_charges_exactly_the_borrowed_message() {
    let key = key();
    let long = vec![1u8; 4096];
    let calls = Cell::new(0);
    let mut charges = Vec::new();
    let result = sign_with_preimage(
        &key,
        || {
            Ok::<_, Error>(Changing {
                bytes: &long,
                calls: &calls,
            })
        },
        |n| {
            charges.push(n);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(result, hex::encode(key.sign(&[]).to_bytes()));
    assert_eq!(calls.get(), 1);
    assert_eq!(charges, vec![0]);
}
#[test]
fn fencing_crypto_verify_charges_exactly_the_borrowed_message() {
    let key = key();
    let public = hex::encode(key.verifying_key().to_bytes());
    let long = vec![1u8; 4096];
    let signature = hex::encode(key.sign(&long).to_bytes());
    let calls = Cell::new(0);
    let mut charges = Vec::new();
    let result = verify_with_preimage(
        &signature,
        &public,
        || {
            Ok::<_, Error>(Changing {
                bytes: &long,
                calls: &calls,
            })
        },
        |n| {
            charges.push(n);
            Ok(())
        },
    );
    assert!(matches!(result, Err(Error::Protocol(_))));
    assert_eq!(calls.get(), 1);
    assert_eq!(charges, vec![64, 64, 128, 32, 0]);
}
