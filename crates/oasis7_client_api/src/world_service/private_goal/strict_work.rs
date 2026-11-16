//! The original strict owner-envelope/key/curve predicates with work-only
//! canonical/heap operations. A prepared image is not authority or a receipt.
use super::*;
use crate::world_service::authority::work::{
    PaidServiceProtocolRefusalV1, ServiceAuthorityWorkErrorV1, refusal, signing_bytes_with_work_v1,
};
use oasis7_wasm_abi::{CanonicalCborStreamError, CapabilityCanonicalWorkV1};
use std::mem::size_of;
pub struct PaidPreparedOwnerSignatureV1<C> {
    prepared: PreparedOwnerSignature,
    _image: Option<C>,
    _owned: Option<C>,
}
impl<C> PaidPreparedOwnerSignatureV1<C> {
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.prepared.canonical_bytes
    }
    pub(super) fn into_legacy(self) -> PreparedOwnerSignature {
        debug_assert!(self._image.is_none() && self._owned.is_none());
        self.prepared
    }
}
fn fail<A: CapabilityCanonicalWorkV1, T>(
    source: &A,
    reason: &str,
) -> Result<T, ServiceAuthorityWorkErrorV1<A::Charge>> {
    Err(ServiceAuthorityWorkErrorV1::Protocol(refusal(
        source, reason,
    )?))
}
fn canonical<A: CapabilityCanonicalWorkV1>(
    source: &A,
    value: &str,
    n: usize,
) -> Result<bool, ServiceAuthorityWorkErrorV1<A::Charge>> {
    source
        .charge_work(1, 0)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    if value.len() != n {
        return Ok(false);
    }
    for byte in value.bytes() {
        source
            .charge_work(1, 1)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        if !(byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
            return Ok(false);
        }
    }
    Ok(true)
}
fn clone_text<A: CapabilityCanonicalWorkV1>(
    source: &A,
    value: &str,
) -> Result<String, ServiceAuthorityWorkErrorV1<A::Charge>> {
    let mut bytes = Vec::with_capacity(value.len());
    if bytes.capacity() != value.len() {
        return Err(ServiceAuthorityWorkErrorV1::Work(
            CanonicalCborStreamError::ResourceLimited("owner signature text capacity"),
        ));
    }
    for part in value.as_bytes().chunks(8192) {
        source
            .charge_work(1, part.len() as u64)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        bytes.extend_from_slice(part)
    }
    Ok(String::from_utf8(bytes).expect("original UTF-8"))
}
pub fn prepare_owner_signature_strict_with_work_v1<T: Serialize, A: CapabilityCanonicalWorkV1>(
    domain: &str,
    request: &T,
    subject_public_key: &str,
    signature_hex: &str,
    source: &A,
) -> Result<PaidPreparedOwnerSignatureV1<A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>> {
    if !canonical(source, subject_public_key, 64)? || !canonical(source, signature_hex, 128)? {
        return fail(source, "invalid owner signature encoding");
    }
    let public_key: [u8; 32] = {
        source
            .charge_work(1, 64)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        let _fee = source
            .reserve(32)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        let bytes = match hex::decode(subject_public_key) {
            Ok(v) => v,
            Err(_) => return fail(source, "invalid private goal owner key"),
        };
        match bytes.try_into() {
            Ok(v) => v,
            Err(_) => return fail(source, "invalid private goal owner key"),
        }
    };
    source
        .charge_work(1, 32)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let key = match VerifyingKey::from_bytes(&public_key) {
        Ok(key) => key,
        Err(_) => return fail(source, "invalid private goal owner key"),
    };
    source
        .charge_work(1, 32)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    if key.is_weak() {
        return fail(source, "weak private goal owner key");
    }
    let signature: [u8; 64] = {
        source
            .charge_work(1, 128)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        let _fee = source
            .reserve(64)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        let bytes = match hex::decode(signature_hex) {
            Ok(v) => v,
            Err(_) => return fail(source, "invalid private goal signature"),
        };
        match bytes.try_into() {
            Ok(v) => v,
            Err(_) => return fail(source, "invalid private goal signature"),
        }
    };
    let image = signing_bytes_with_work_v1(domain, request, source)?;
    let size = size_of::<PaidPreparedOwnerSignatureV1<A::Charge>>()
        .checked_add(domain.len())
        .and_then(|n| n.checked_add(subject_public_key.len()))
        .and_then(|n| n.checked_add(signature_hex.len()))
        .ok_or(ServiceAuthorityWorkErrorV1::Work(
            CanonicalCborStreamError::ResourceLimited("owner signature graph"),
        ))?;
    source
        .charge_work(1, size as u64)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let owned = if source.is_metered() {
        Some(
            source
                .reserve(size)
                .map_err(ServiceAuthorityWorkErrorV1::Work)?,
        )
    } else {
        None
    };
    Ok(PaidPreparedOwnerSignatureV1 {
        prepared: PreparedOwnerSignature {
            domain: clone_text(source, domain)?,
            original_public_key: clone_text(source, subject_public_key)?,
            original_signature: clone_text(source, signature_hex)?,
            signature,
            key,
            canonical_bytes: image.bytes,
        },
        _image: image.fee,
        _owned: owned,
    })
}
impl<C> PaidPreparedOwnerSignatureV1<C> {
    /// The fixed curve operation uses exactly the parsed key, signature and
    /// original canonical signing image held above. No verification callback.
    pub fn verify_strict_with_work_v1<A: CapabilityCanonicalWorkV1>(
        &self,
        source: &A,
    ) -> Result<(), ServiceAuthorityWorkErrorV1<A::Charge>> {
        let n = self.prepared.canonical_bytes.len();
        let bytes = u64::try_from(n)
            .ok()
            .and_then(|v| v.checked_add(96))
            .ok_or(ServiceAuthorityWorkErrorV1::Work(
                CanonicalCborStreamError::ResourceLimited("owner signature verify work"),
            ))?;
        source
            .charge_work(1 + (n as u64).div_ceil(128), bytes)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        let reason = "invalid private goal owner signature";
        let fee = if source.is_metered() {
            Some(
                source
                    .reserve(
                        size_of::<ServiceAuthorityWorkErrorV1<A::Charge>>()
                            .max(size_of::<PaidServiceProtocolRefusalV1<A::Charge>>())
                            + reason.len(),
                    )
                    .map_err(ServiceAuthorityWorkErrorV1::Work)?,
            )
        } else {
            None
        };
        match super::verify_owner_signature_bytes_strict_v1(
            &self.prepared.key,
            &self.prepared.signature,
            &self.prepared.canonical_bytes,
        ) {
            Ok(()) => Ok(()),
            Err(message) => {
                source
                    .charge_work(1, message.len() as u64)
                    .map_err(ServiceAuthorityWorkErrorV1::Work)?;
                Err(ServiceAuthorityWorkErrorV1::Protocol(
                    PaidServiceProtocolRefusalV1 {
                        message: message.to_owned(),
                        fee,
                    },
                ))
            }
        }
    }
}
pub fn verify_owner_signature_strict_with_work_v1<T: Serialize, A: CapabilityCanonicalWorkV1>(
    domain: &str,
    request: &T,
    key: &str,
    signature: &str,
    source: &A,
) -> Result<(), ServiceAuthorityWorkErrorV1<A::Charge>> {
    prepare_owner_signature_strict_with_work_v1(domain, request, key, signature, source)?
        .verify_strict_with_work_v1(source)
}
