//! Checksummed canonical private maintenance records; never business authority.
use super::{FormalError, crypto::invalid};
use serde::{Serialize, de::DeserializeOwned};
pub(super) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, FormalError> {
    let body = serde_cbor::to_vec(value).map_err(|e| invalid(e.to_string()))?;
    let checksum = blake3::hash(&body).to_hex().to_string();
    serde_cbor::to_vec(&(serde_cbor::Value::Bytes(body), checksum))
        .map_err(|e| invalid(e.to_string()))
}
pub(super) fn decode<T: Serialize + DeserializeOwned>(encoded: &[u8]) -> Result<T, FormalError> {
    let (body, checksum): (serde_cbor::Value, String) =
        serde_cbor::from_slice(encoded).map_err(|e| invalid(e.to_string()))?;
    let serde_cbor::Value::Bytes(body) = body else {
        return Err(invalid("maintenance body requires bytes"));
    };
    if blake3::hash(&body).to_hex().as_str() != checksum
        || serde_cbor::to_vec(&(serde_cbor::Value::Bytes(body.clone()), checksum))
            .map_err(|e| invalid(e.to_string()))?
            != encoded
    {
        return Err(invalid("maintenance checksum/canonical envelope"));
    }
    let value: T = serde_cbor::from_slice(&body).map_err(|e| invalid(e.to_string()))?;
    if serde_cbor::to_vec(&value).map_err(|e| invalid(e.to_string()))? != body {
        return Err(invalid("maintenance canonical state body"));
    }
    Ok(value)
}
