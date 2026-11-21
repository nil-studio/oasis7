//! Original JSON/SHA-256 identity shared by Runtime and Simulator chain resources.
//!
//! Do not substitute canonical CBOR or change map ordering here: existing resource
//! manifests and chunk identity hashes are committed over serde_json::to_vec bytes.
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Hash exactly the serialized JSON bytes used by the existing chain-resource contracts.
pub fn hash_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(value)?;
    Ok(sha256_hex(&bytes))
}

/// The unprefixed, lower-case SHA-256 hex representation of the original bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{hash_json, sha256_hex};
    use std::collections::BTreeMap;

    #[test]
    fn sha256_hex_keeps_known_bytes_identity() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn chain_resource_json_hash_keeps_existing_map_order_and_bytes() {
        let values = BTreeMap::from([("alpha", 3_i64), ("beta", -7_i64)]);
        let encoded = serde_json::to_vec(&values).unwrap();
        assert_eq!(encoded, br#"{"alpha":3,"beta":-7}"#);
        assert_eq!(hash_json(&values).unwrap(), sha256_hex(&encoded));
    }
}
