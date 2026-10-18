use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::SignerError;

const OPERATION_DOMAIN: &[u8] = b"oasis7.local-operation.v2\0";
const REQUEST_DOMAIN: &[u8] = b"oasis7.local-request.v2\0";
const FINGERPRINT_DOMAIN: &[u8] = b"oasis7.local-request-fingerprint.v1\0";

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(sha256(bytes))
}

pub fn parse_sha256_hex(value: &str) -> Result<[u8; 32], SignerError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SignerError::InvalidInput(
            "SHA-256 digest must be 64 lowercase hexadecimal characters".to_owned(),
        ));
    }
    let decoded = hex::decode(value)
        .map_err(|_| SignerError::InvalidInput("malformed SHA-256 digest".to_owned()))?;
    decoded
        .try_into()
        .map_err(|_| SignerError::InvalidInput("malformed SHA-256 digest".to_owned()))
}

pub fn operation_key(
    installation_id: &str,
    deployment_id: &str,
    network_id: &str,
    purpose: &str,
    signer_id: &str,
    payload_sha256: &[u8; 32],
) -> Result<String, SignerError> {
    let mut hasher = Sha256::new();
    hasher.update(OPERATION_DOMAIN);
    for value in [
        installation_id,
        deployment_id,
        network_id,
        purpose,
        signer_id,
    ] {
        update_lp(&mut hasher, value.as_bytes())?;
    }
    hasher.update(payload_sha256);
    Ok(hex::encode(hasher.finalize()))
}

pub fn request_key(
    installation_id: &str,
    purpose: &str,
    caller_uid: u32,
    provider_id: Option<&str>,
    request_id: &str,
) -> Result<String, SignerError> {
    let mut hasher = Sha256::new();
    hasher.update(REQUEST_DOMAIN);
    let caller_uid = caller_uid.to_string();
    for value in [
        installation_id,
        purpose,
        caller_uid.as_str(),
        provider_id.unwrap_or(""),
        request_id,
    ] {
        update_lp(&mut hasher, value.as_bytes())?;
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn request_fingerprint<T: Serialize>(
    request: &T,
    grant_id: &str,
    operation_key: &str,
    policy_revision: &str,
) -> Result<String, SignerError> {
    let request_value = serde_json::to_value(request)
        .map_err(|_| SignerError::InvalidInput("request cannot be serialized".to_owned()))?;
    let canonical_request = serde_json::to_vec(&request_value)
        .map_err(|_| SignerError::InvalidInput("request cannot be serialized".to_owned()))?;
    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_DOMAIN);
    update_lp(&mut hasher, &canonical_request)?;
    for value in [grant_id, operation_key, policy_revision] {
        update_lp(&mut hasher, value.as_bytes())?;
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, SignerError> {
    let value = serde_json::to_value(value)
        .map_err(|_| SignerError::InvalidInput("control document is invalid".to_owned()))?;
    serde_json::to_vec(&value)
        .map_err(|_| SignerError::InvalidInput("control document is invalid".to_owned()))
}

fn update_lp(hasher: &mut Sha256, bytes: &[u8]) -> Result<(), SignerError> {
    let length = u32::try_from(bytes.len())
        .map_err(|_| SignerError::InvalidInput("length-prefixed field is too large".to_owned()))?;
    hasher.update(length.to_be_bytes());
    hasher.update(bytes);
    Ok(())
}
