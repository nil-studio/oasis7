use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Serialize, de::DeserializeOwned};

use super::binding::{BftSignerErrorV3, decode_key, invalid};

pub const BFT_SIGNER_MAX_HISTORY_BYTES_V3: usize =
    oasis7_formal_protocol::bft::BFT_FORMAL_HEADER_PREFIX_MAX_BYTES_V3;
pub const BFT_SIGNER_MAX_JOURNAL_BYTES_V3: usize = 16 * 1024 * 1024;
pub const BFT_SIGNER_MAX_CERTIFICATE_BYTES_V3: usize =
    oasis7_proto::distributed_bft::v3::BFT_MAX_SIGNED_MATERIAL_BYTES_V3;
pub const BFT_SIGNER_MAX_APPLIED_RECORD_BYTES_V3: usize = 512;
pub const BFT_SIGNER_MAX_APPLIED_HISTORY_BYTES_V3: usize =
    64 * BFT_SIGNER_MAX_APPLIED_RECORD_BYTES_V3;
pub const BFT_SIGNER_MAX_FRAME_BYTES_V3: usize = 2 * BFT_SIGNER_MAX_HISTORY_BYTES_V3
    + 2 * BFT_SIGNER_MAX_CERTIFICATE_BYTES_V3
    + BFT_SIGNER_MAX_APPLIED_HISTORY_BYTES_V3
    + 1024 * 1024;
pub const BFT_SIGNER_MAX_SIGNING_BYTES_V3: usize = BFT_SIGNER_MAX_FRAME_BYTES_V3;

pub(crate) fn framed_cbor<T: Serialize>(
    domain: &str,
    value: &T,
    max_bytes: usize,
) -> Result<Vec<u8>, BftSignerErrorV3> {
    let payload = oasis7_cas::to_canonical_cbor(value)
        .map_err(|error| invalid(format!("encode BFT V3 canonical CBOR: {error:?}")))?;
    let total = 8usize
        .checked_add(domain.len())
        .and_then(|length| length.checked_add(payload.len()))
        .ok_or_else(|| invalid("BFT V3 frame length overflow"))?;
    if total > max_bytes {
        return Err(invalid("BFT V3 frame exceeds configured bound"));
    }
    let mut frame = Vec::with_capacity(total);
    frame.extend_from_slice(&(domain.len() as u64).to_be_bytes());
    frame.extend_from_slice(domain.as_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub(crate) fn digest_hex<T: Serialize>(
    domain: &str,
    value: &T,
) -> Result<String, BftSignerErrorV3> {
    let frame = framed_cbor(domain, value, BFT_SIGNER_MAX_SIGNING_BYTES_V3)?;
    Ok(blake3::hash(&frame).to_hex().to_string())
}

pub(crate) fn sign_preimage<T: Serialize>(
    key: &dyn super::binding::BftSigningKeyHandleV3,
    domain: &str,
    value: &T,
) -> Result<String, BftSignerErrorV3> {
    let frame = framed_cbor(domain, value, BFT_SIGNER_MAX_SIGNING_BYTES_V3)?;
    let signature = key.sign_preimage(&frame)?;
    Ok(hex::encode(signature))
}

pub(crate) fn verify_signature<T: Serialize>(
    domain: &str,
    value: &T,
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<(), BftSignerErrorV3> {
    let key = decode_key(public_key_hex)?;
    if signature_hex.len() != 128
        || signature_hex
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT V3 signature must be canonical lowercase hex"));
    }
    let bytes: [u8; 64] = hex::decode(signature_hex)
        .map_err(|error| invalid(format!("BFT V3 signature: {error}")))?
        .try_into()
        .map_err(|_| invalid("BFT V3 signature length"))?;
    let preimage = framed_cbor(domain, value, BFT_SIGNER_MAX_SIGNING_BYTES_V3)?;
    verify_preimage(&key, &preimage, &Signature::from_bytes(&bytes))
}

fn verify_preimage(
    key: &VerifyingKey,
    preimage: &[u8],
    signature: &Signature,
) -> Result<(), BftSignerErrorV3> {
    key.verify_strict(preimage, signature)
        .map_err(|_| invalid("BFT V3 signature verification failed"))
}

pub(crate) fn encode_bounded<T: Serialize>(
    value: &T,
    limit: usize,
) -> Result<Vec<u8>, BftSignerErrorV3> {
    let bytes = oasis7_cas::to_canonical_cbor(value)
        .map_err(|error| invalid(format!("encode BFT V3 payload: {error:?}")))?;
    if bytes.len() > limit {
        return Err(invalid("BFT V3 encoded payload exceeds its bound"));
    }
    Ok(bytes)
}

pub(crate) fn decode_bounded<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    limit: usize,
) -> Result<T, BftSignerErrorV3> {
    if bytes.is_empty() || bytes.len() > limit {
        return Err(invalid("BFT V3 payload length is outside its bound"));
    }
    scan_cbor_bounds(bytes)?;
    let value = serde_cbor::from_slice(bytes)
        .map_err(|error| invalid(format!("decode BFT V3 payload: {error}")))?;
    let canonical = encode_bounded(&value, limit)?;
    if canonical != bytes {
        return Err(invalid("BFT V3 payload is not canonical CBOR"));
    }
    Ok(value)
}

/// Rejects indefinite CBOR and impossible container declarations before the
/// deserializer can reserve attacker-selected vector/map capacities.
fn scan_cbor_bounds(bytes: &[u8]) -> Result<(), BftSignerErrorV3> {
    let mut offset = 0usize;
    scan_item(bytes, &mut offset, 0)?;
    if offset != bytes.len() {
        return Err(invalid("trailing bytes in BFT V3 CBOR"));
    }
    Ok(())
}

fn scan_item(bytes: &[u8], offset: &mut usize, depth: usize) -> Result<(), BftSignerErrorV3> {
    if depth > 64 {
        return Err(invalid("BFT V3 CBOR nesting limit"));
    }
    let initial = *bytes
        .get(*offset)
        .ok_or_else(|| invalid("truncated BFT V3 CBOR"))?;
    *offset += 1;
    let major = initial >> 5;
    let additional = initial & 0x1f;
    if additional == 31 {
        return Err(invalid("indefinite BFT V3 CBOR is not accepted"));
    }
    let argument = read_argument(bytes, offset, additional)?;
    match major {
        0 | 1 => Ok(()),
        2 | 3 => {
            let size = usize::try_from(argument)
                .map_err(|_| invalid("BFT V3 CBOR string length overflow"))?;
            let end = offset
                .checked_add(size)
                .ok_or_else(|| invalid("BFT V3 CBOR string bound overflow"))?;
            if end > bytes.len() {
                return Err(invalid("truncated BFT V3 CBOR string"));
            }
            *offset = end;
            Ok(())
        }
        4 | 5 => {
            let count = usize::try_from(argument)
                .map_err(|_| invalid("BFT V3 CBOR container length overflow"))?;
            let children = if major == 5 {
                count
                    .checked_mul(2)
                    .ok_or_else(|| invalid("BFT V3 CBOR map length overflow"))?
            } else {
                count
            };
            if children > bytes.len().saturating_sub(*offset) {
                return Err(invalid("BFT V3 CBOR container exceeds encoded byte bound"));
            }
            for _ in 0..children {
                scan_item(bytes, offset, depth + 1)?;
            }
            Ok(())
        }
        6 if (argument > 0 && argument <= 64) || (argument == 55_799 && depth == 0) => {
            scan_item(bytes, offset, depth + 1)
        }
        7 if additional <= 23 => Ok(()),
        _ => Err(invalid("unsupported BFT V3 CBOR primitive")),
    }
}

fn read_argument(
    bytes: &[u8],
    offset: &mut usize,
    additional: u8,
) -> Result<u64, BftSignerErrorV3> {
    let width = match additional {
        0..=23 => return Ok(u64::from(additional)),
        24 => 1,
        25 => 2,
        26 => 4,
        27 => 8,
        _ => return Err(invalid("invalid BFT V3 CBOR argument")),
    };
    let end = offset
        .checked_add(width)
        .ok_or_else(|| invalid("BFT V3 CBOR argument overflow"))?;
    let bytes = bytes
        .get(*offset..end)
        .ok_or_else(|| invalid("truncated BFT V3 CBOR argument"))?;
    let mut value = 0u64;
    for byte in bytes {
        value = (value << 8) | u64::from(*byte);
    }
    *offset = end;
    Ok(value)
}
