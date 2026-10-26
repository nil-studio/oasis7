use ed25519_dalek::{Signature, VerifyingKey};
use oasis7_proto::distributed_bft::{
    BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1, BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1,
    BFT_MAX_SIGNED_MATERIAL_BYTES_V1,
};
use serde::{Serialize, de::DeserializeOwned};

use super::binding::{BftSignerErrorV2, decode_key, invalid};

pub const BFT_SIGNER_MAX_HISTORY_BYTES_V2: usize = BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1;
pub const BFT_SIGNER_MAX_JOURNAL_BYTES_V2: usize = BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1;
pub const BFT_SIGNER_MAX_CERTIFICATE_BYTES_V2: usize = BFT_MAX_SIGNED_MATERIAL_BYTES_V1;
pub const BFT_SIGNER_MAX_INSTALL_RECORD_BYTES_V2: usize = 8 * 1024;
pub const BFT_SIGNER_MAX_INSTALL_HISTORY_BYTES_V2: usize =
    BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 * BFT_SIGNER_MAX_INSTALL_RECORD_BYTES_V2;
pub const BFT_SIGNER_MAX_FRAME_BYTES_V2: usize = 2 * BFT_SIGNER_MAX_HISTORY_BYTES_V2
    + 2 * BFT_SIGNER_MAX_CERTIFICATE_BYTES_V2
    + BFT_SIGNER_MAX_INSTALL_HISTORY_BYTES_V2
    + 1024 * 1024;
pub const BFT_SIGNER_MAX_SIGNING_BYTES_V2: usize = BFT_SIGNER_MAX_FRAME_BYTES_V2;

pub(crate) fn framed_cbor<T: Serialize>(
    domain: &str,
    value: &T,
    max_bytes: usize,
) -> Result<Vec<u8>, BftSignerErrorV2> {
    let payload = oasis7_cas::to_canonical_cbor(value)
        .map_err(|error| invalid(format!("encode BFT V2 canonical CBOR: {error:?}")))?;
    let total = 8usize
        .checked_add(domain.len())
        .and_then(|length| length.checked_add(payload.len()))
        .ok_or_else(|| invalid("BFT V2 frame length overflow"))?;
    if total > max_bytes {
        return Err(invalid("BFT V2 frame exceeds configured bound"));
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
) -> Result<String, BftSignerErrorV2> {
    let frame = framed_cbor(domain, value, BFT_SIGNER_MAX_SIGNING_BYTES_V2)?;
    Ok(blake3::hash(&frame).to_hex().to_string())
}

pub(crate) fn sign_preimage<T: Serialize>(
    key: &dyn super::binding::BftSigningKeyHandleV2,
    domain: &str,
    value: &T,
) -> Result<String, BftSignerErrorV2> {
    let frame = framed_cbor(domain, value, BFT_SIGNER_MAX_SIGNING_BYTES_V2)?;
    let signature = key.sign_preimage(&frame)?;
    Ok(hex::encode(signature))
}

pub(crate) fn verify_signature<T: Serialize>(
    domain: &str,
    value: &T,
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<(), BftSignerErrorV2> {
    let key = decode_key(public_key_hex)?;
    if signature_hex.len() != 128
        || signature_hex
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT V2 signature must be canonical lowercase hex"));
    }
    let bytes: [u8; 64] = hex::decode(signature_hex)
        .map_err(|error| invalid(format!("BFT V2 signature: {error}")))?
        .try_into()
        .map_err(|_| invalid("BFT V2 signature length"))?;
    let preimage = framed_cbor(domain, value, BFT_SIGNER_MAX_SIGNING_BYTES_V2)?;
    verify_preimage(&key, &preimage, &Signature::from_bytes(&bytes))
}

fn verify_preimage(
    key: &VerifyingKey,
    preimage: &[u8],
    signature: &Signature,
) -> Result<(), BftSignerErrorV2> {
    key.verify_strict(preimage, signature)
        .map_err(|_| invalid("BFT V2 signature verification failed"))
}

pub(crate) fn encode_bounded<T: Serialize>(
    value: &T,
    limit: usize,
) -> Result<Vec<u8>, BftSignerErrorV2> {
    let bytes = oasis7_cas::to_canonical_cbor(value)
        .map_err(|error| invalid(format!("encode BFT V2 payload: {error:?}")))?;
    if bytes.len() > limit {
        return Err(invalid("BFT V2 encoded payload exceeds its bound"));
    }
    Ok(bytes)
}

pub(crate) fn decode_bounded<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    limit: usize,
) -> Result<T, BftSignerErrorV2> {
    if bytes.is_empty() || bytes.len() > limit {
        return Err(invalid("BFT V2 payload length is outside its bound"));
    }
    scan_cbor_bounds(bytes)?;
    let value = serde_cbor::from_slice(bytes)
        .map_err(|error| invalid(format!("decode BFT V2 payload: {error}")))?;
    let canonical = encode_bounded(&value, limit)?;
    if canonical != bytes {
        return Err(invalid("BFT V2 payload is not canonical CBOR"));
    }
    Ok(value)
}

/// Rejects indefinite CBOR and impossible container declarations before the
/// deserializer can reserve attacker-selected vector/map capacities.
fn scan_cbor_bounds(bytes: &[u8]) -> Result<(), BftSignerErrorV2> {
    let mut offset = 0usize;
    scan_item(bytes, &mut offset, 0)?;
    if offset != bytes.len() {
        return Err(invalid("trailing bytes in BFT V2 CBOR"));
    }
    Ok(())
}

fn scan_item(bytes: &[u8], offset: &mut usize, depth: usize) -> Result<(), BftSignerErrorV2> {
    if depth > 64 {
        return Err(invalid("BFT V2 CBOR nesting limit"));
    }
    let initial = *bytes
        .get(*offset)
        .ok_or_else(|| invalid("truncated BFT V2 CBOR"))?;
    *offset += 1;
    let major = initial >> 5;
    let additional = initial & 0x1f;
    if additional == 31 {
        return Err(invalid("indefinite BFT V2 CBOR is not accepted"));
    }
    let argument = read_argument(bytes, offset, additional)?;
    match major {
        0 | 1 => Ok(()),
        2 | 3 => {
            let size = usize::try_from(argument)
                .map_err(|_| invalid("BFT V2 CBOR string length overflow"))?;
            let end = offset
                .checked_add(size)
                .ok_or_else(|| invalid("BFT V2 CBOR string bound overflow"))?;
            if end > bytes.len() {
                return Err(invalid("truncated BFT V2 CBOR string"));
            }
            *offset = end;
            Ok(())
        }
        4 | 5 => {
            let count = usize::try_from(argument)
                .map_err(|_| invalid("BFT V2 CBOR container length overflow"))?;
            let children = if major == 5 {
                count
                    .checked_mul(2)
                    .ok_or_else(|| invalid("BFT V2 CBOR map length overflow"))?
            } else {
                count
            };
            if children > bytes.len().saturating_sub(*offset) {
                return Err(invalid("BFT V2 CBOR container exceeds encoded byte bound"));
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
        _ => Err(invalid("unsupported BFT V2 CBOR primitive")),
    }
}

fn read_argument(
    bytes: &[u8],
    offset: &mut usize,
    additional: u8,
) -> Result<u64, BftSignerErrorV2> {
    let width = match additional {
        0..=23 => return Ok(u64::from(additional)),
        24 => 1,
        25 => 2,
        26 => 4,
        27 => 8,
        _ => return Err(invalid("invalid BFT V2 CBOR argument")),
    };
    let end = offset
        .checked_add(width)
        .ok_or_else(|| invalid("BFT V2 CBOR argument overflow"))?;
    let bytes = bytes
        .get(*offset..end)
        .ok_or_else(|| invalid("truncated BFT V2 CBOR argument"))?;
    let mut value = 0u64;
    for byte in bytes {
        value = (value << 8) | u64::from(*byte);
    }
    *offset = end;
    Ok(value)
}
