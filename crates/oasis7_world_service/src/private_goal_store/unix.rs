use super::*;
use cap_std::fs::Dir;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
pub(super) use oasis7_client_storage::private_storage::{
    initialize_key, open_root, publish_bounded_record, publish_record, read_bounded_record,
    read_record,
};
fn aad(binding: &PrivateGoalBindingV1, reference: &PrivateGoalRef) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(&("oasis7/private-goal-store/v1", reference.as_str(), binding))
        .map_err(|_| "private goal AAD encoding failed".into())
}
fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| "private goal randomness unavailable")?;
    Ok(bytes)
}
pub(super) fn put(
    dir: &Dir,
    key: &[u8; 32],
    binding: &PrivateGoalBindingV1,
    plaintext: &[u8],
) -> Result<PrivateGoalRef, String> {
    put_with_reference(dir, key, binding, plaintext, || {
        Ok(PrivateGoalRef(hex::encode(random::<32>()?)))
    })
}

pub(super) fn put_with_reference(
    dir: &Dir,
    key: &[u8; 32],
    binding: &PrivateGoalBindingV1,
    plaintext: &[u8],
    mut next_reference: impl FnMut() -> Result<PrivateGoalRef, String>,
) -> Result<PrivateGoalRef, String> {
    oasis7_client_storage::private_storage::private_directory(dir)?;
    oasis7_client_storage::private_storage::verify_key(dir, key)?;
    for _ in 0..8 {
        let reference = next_reference()?;
        let nonce = random::<24>()?;
        let ciphertext = XChaCha20Poly1305::new(key.into())
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad(binding, &reference)?,
                },
            )
            .map_err(|_| "private goal encryption failed")?;
        let mut envelope = Vec::with_capacity(32 + ciphertext.len());
        envelope.extend_from_slice(MAGIC);
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&ciphertext);
        if envelope.len() > MAX_PRIVATE_GOAL_FILE_BYTES {
            return Err("private goal file exceeds byte limit".into());
        }
        if oasis7_client_storage::private_storage::publish_immutable(
            dir,
            reference.as_str(),
            &envelope,
        )? {
            return Ok(reference);
        }
    }
    Err("private goal reference collision retry budget exhausted".into())
}
pub(super) fn get(
    dir: &Dir,
    key: &[u8; 32],
    reference: &PrivateGoalRef,
    binding: &PrivateGoalBindingV1,
) -> Result<Zeroizing<Vec<u8>>, String> {
    oasis7_client_storage::private_storage::private_directory(dir)?;
    oasis7_client_storage::private_storage::verify_key(dir, key)?;
    let bytes = oasis7_client_storage::private_storage::read_private(
        dir,
        reference.as_str(),
        MAX_PRIVATE_GOAL_FILE_BYTES,
    )?;
    if bytes.len() > MAX_PRIVATE_GOAL_FILE_BYTES || bytes.len() < 48 || &bytes[..8] != MAGIC {
        return Err("invalid private goal encrypted envelope".into());
    }
    XChaCha20Poly1305::new(key.into())
        .decrypt(
            &XNonce::try_from(&bytes[8..32]).map_err(|_| "invalid private goal nonce")?,
            Payload {
                msg: &bytes[32..],
                aad: &aad(binding, reference)?,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| "private goal authentication failed".into())
}
