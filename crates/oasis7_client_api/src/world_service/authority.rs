use super::{SignedReadRequest, SignedServiceResponse};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::Serialize;

pub fn signing_bytes<T: Serialize>(domain: &str, value: &T) -> Result<Vec<u8>, String> {
    let result = signing_bytes_with_work_v1(
        domain,
        value,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .map_err(work::legacy_error)?;
    debug_assert!(result.fee.is_none());
    Ok(result.bytes)
}

pub fn request_digest<T: Serialize>(domain: &str, value: &T) -> Result<String, String> {
    let result = request_digest_with_work_v1(
        domain,
        value,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .map_err(work::legacy_error)?;
    debug_assert!(result.fee.is_none());
    Ok(result.text)
}

fn decode<const N: usize>(value: &str) -> Result<[u8; N], String> {
    hex::decode(value)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "invalid key or signature length".to_owned())
}

pub fn sign_read_request<T: Serialize>(
    domain: &str,
    request: T,
    private_key_hex: &str,
) -> Result<SignedReadRequest<T>, String> {
    let key = SigningKey::from_bytes(&decode(private_key_hex)?);
    let signature_hex = hex::encode(key.sign(&signing_bytes(domain, &request)?).to_bytes());
    Ok(SignedReadRequest {
        request,
        subject_public_key: hex::encode(key.verifying_key().to_bytes()),
        signature_hex,
    })
}

pub fn verify_read_request<T: Serialize>(
    domain: &str,
    request: &SignedReadRequest<T>,
) -> Result<(), String> {
    verify_read_request_with_work_v1(
        domain,
        request,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .map_err(work::legacy_error)
}

fn verify(key: &str, signature: &str, bytes: &[u8]) -> Result<(), String> {
    work::verify_bytes_with_work_v1(
        key,
        signature,
        bytes,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .map_err(work::legacy_error)
}

pub fn sign_service_response<T: Serialize>(
    domain: &str,
    request_digest: String,
    payload: T,
    private_key_hex: &str,
) -> Result<SignedServiceResponse<T>, String> {
    let key = SigningKey::from_bytes(&decode(private_key_hex)?);
    let bytes = signing_bytes(domain, &(&request_digest, &payload))?;
    Ok(SignedServiceResponse {
        request_digest,
        payload,
        signature_hex: hex::encode(key.sign(&bytes).to_bytes()),
    })
}

pub fn verify_service_response<T: Serialize>(
    domain: &str,
    expected_request_digest: &str,
    response: &SignedServiceResponse<T>,
    trusted_public_key: &str,
) -> Result<(), String> {
    if response.request_digest != expected_request_digest {
        return Err("response request mismatch".into());
    }
    verify(
        trusted_public_key,
        &response.signature_hex,
        &signing_bytes(domain, &(&response.request_digest, &response.payload))?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_codec_keeps_published_domain_tuple_bytes() {
        // Freeze the literal CBOR tuple ["oasis7.world-service.v1", "view", [1, 2]]
        // so extraction cannot silently change the published signature preimage.
        assert_eq!(
            hex::encode(signing_bytes("view", &vec![1u8, 2]).unwrap()),
            "83776f61736973372e776f726c642d736572766963652e76316476696577820102"
        );
    }

    #[test]
    fn request_and_response_bind_domain_content_and_configured_trust() {
        let secret = hex::encode([7u8; 32]);
        let request = sign_read_request("view", vec![1u8, 2], &secret).unwrap();
        verify_read_request("view", &request).unwrap();
        assert!(verify_read_request("lookup", &request).is_err());
        let mut altered = request.clone();
        altered.request.push(3);
        assert!(verify_read_request("view", &altered).is_err());
        let digest = request_digest("view", &request.request).unwrap();
        let response = sign_service_response("view", digest.clone(), 42u64, &secret).unwrap();
        verify_service_response("view", &digest, &response, &request.subject_public_key).unwrap();
        assert!(
            verify_service_response("view", "other", &response, &request.subject_public_key)
                .is_err()
        );
        let wrong = sign_read_request("view", (), &hex::encode([8u8; 32])).unwrap();
        assert!(
            verify_service_response("view", &digest, &response, &wrong.subject_public_key).is_err()
        );
        let mut altered = response;
        altered.payload = 43;
        assert!(
            verify_service_response("view", &digest, &altered, &request.subject_public_key)
                .is_err()
        );
    }
}

#[path = "authority_work.rs"]
pub(in crate::world_service) mod work;
pub use work::{
    PaidServiceProtocolRefusalV1, PaidServiceRequestDigestV1, PaidServiceSigningBytesV1,
    ServiceAuthorityWorkErrorV1, request_digest_with_work_v1, signing_bytes_with_work_v1,
};

pub use work::verify_read_request_with_work_v1;
