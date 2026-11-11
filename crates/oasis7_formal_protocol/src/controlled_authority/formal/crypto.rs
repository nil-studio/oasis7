use super::FormalError;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Serialize;
pub fn invalid(reason: impl Into<String>) -> FormalError {
    FormalError::Invalid(reason.into())
}
pub fn hash_hex(value: &str) -> Result<(), FormalError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "formal digest/key requires canonical lowercase hex32",
        ));
    }
    Ok(())
}
pub fn public_key(value: &str) -> Result<(), FormalError> {
    hash_hex(value)?;
    let decoded: [u8; 32] = hex::decode(value)
        .map_err(|_| invalid("formal public key encoding"))?
        .try_into()
        .map_err(|_| invalid("formal public key length"))?;
    let key = VerifyingKey::from_bytes(&decoded)
        .map_err(|_| invalid("formal public key curve encoding"))?;
    if key.is_weak() {
        return Err(invalid("formal weak Ed25519 public key"));
    }
    Ok(())
}
pub fn bytes<T: Serialize>(domain: &str, value: &T) -> Result<Vec<u8>, FormalError> {
    let encoded = serde_cbor::to_vec(value).map_err(|e| invalid(e.to_string()))?;
    // One frame, matching the formal RPC frame bound. Combined input/output
    // closures can exceed 32 MiB after Vec<u8> CBOR expansion plus metadata.
    if encoded.len() > 64 * 1024 * 1024 {
        return Err(invalid("formal encoding exceeds byte bound"));
    }
    let mut bytes = (domain.len() as u64).to_be_bytes().to_vec();
    bytes.extend(domain.as_bytes());
    bytes.extend(encoded);
    Ok(bytes)
}
pub fn hash<T: Serialize>(domain: &str, value: &T) -> Result<String, FormalError> {
    Ok(blake3::hash(&bytes(domain, value)?).to_hex().to_string())
}
pub fn verify<T: Serialize>(
    domain: &str,
    value: &T,
    signature: &str,
    public_key: &str,
) -> Result<(), FormalError> {
    verify_with_preimage(
        signature,
        public_key,
        || bytes(domain, value),
        |_| Ok::<(), FormalError>(()),
    )
}
/// Same original verification predicates/order; only preimage production and
/// continuing work are supplied by a caller. No verification verdict is minted.
pub fn verify_with_preimage<B: AsRef<[u8]>, E: From<FormalError>>(
    signature: &str,
    public_key: &str,
    build: impl FnOnce() -> Result<B, E>,
    mut work: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), E> {
    work(public_key.len())?;
    hash_hex(public_key)?;
    work(signature.len())?;
    if signature.len() != 128
        || !signature
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("formal signature requires canonical lowercase hex64").into());
    }
    let key: [u8; 32] = hex::decode(public_key)
        .map_err(|_| invalid("formal public key"))?
        .try_into()
        .map_err(|_| invalid("formal public key"))?;
    let signature: [u8; 64] = hex::decode(signature)
        .map_err(|_| invalid("formal signature"))?
        .try_into()
        .map_err(|_| invalid("formal signature"))?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| invalid("formal public key"))?;
    let preimage = build()?;
    work(preimage.as_ref().len())?;
    key.verify_strict(preimage.as_ref(), &Signature::from_bytes(&signature))
        .map_err(|_| invalid("formal signature verification").into())
}
