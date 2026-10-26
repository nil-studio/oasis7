//! Domain-separated authenticated-data format. Platform encryption lives in Worker.
use crate::DecimalU64;
use crate::http::Role;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyIdentity {
    pub environment: String,
    pub account_id: String,
    pub role: Role,
    pub key_id: String,
    pub key_version: DecimalU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeError {
    EmptyIdentity,
    UnsupportedRole,
    Encoding,
    InvalidIdentity,
    InvalidVersion,
    UnsupportedSchema,
    InvalidHex,
    InvalidLength,
    ContextMismatch,
    NonceReuse,
}
impl core::fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid custody envelope: {self:?}")
    }
}

impl KeyIdentity {
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        if [&self.environment, &self.account_id, &self.key_id]
            .iter()
            .any(|s| s.is_empty())
        {
            return Err(EnvelopeError::EmptyIdentity);
        }
        if [&self.environment, &self.account_id, &self.key_id]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(EnvelopeError::InvalidIdentity);
        }
        if self.key_version.0 == 0 {
            return Err(EnvelopeError::InvalidVersion);
        }
        if self.role == Role::RecoveryFence {
            return Err(EnvelopeError::UnsupportedRole);
        }
        Ok(())
    }
    pub fn value_aad(&self) -> Result<Vec<u8>, EnvelopeError> {
        self.validate()?;
        serde_cbor::to_vec(&("oasis7.custody.key-value.v1", self))
            .map_err(|_| EnvelopeError::Encoding)
    }
    pub fn wrap_aad(&self, wrapping_key_version: DecimalU64) -> Result<Vec<u8>, EnvelopeError> {
        self.validate()?;
        serde_cbor::to_vec(&("oasis7.custody.dek-wrap.v1", self, wrapping_key_version))
            .map_err(|_| EnvelopeError::Encoding)
    }
}

/// New envelopes have a separate AAD domain. Legacy KeyIdentity AAD remains unchanged.
pub const ENVELOPE_SCHEMA_VERSION: u8 = 1;
pub const AES_GCM_NONCE_BYTES: usize = 12;
pub const AES_GCM_TAG_BYTES: usize = 16;
pub const DEK_BYTES: usize = 32;
pub const MAX_ENVELOPE_VALUE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopeAlgorithmV1 {
    #[serde(rename = "aes_256_gcm")]
    Aes256Gcm,
}

/// Complete trusted context is available before any encryption is performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "EnvelopeContextWireV1")]
pub struct EnvelopeContextV1 {
    pub schema_version: u8,
    pub identity: KeyIdentity,
    pub algorithm: EnvelopeAlgorithmV1,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeContextWireV1 {
    schema_version: u8,
    identity: KeyIdentity,
    algorithm: EnvelopeAlgorithmV1,
}
impl TryFrom<EnvelopeContextWireV1> for EnvelopeContextV1 {
    type Error = EnvelopeError;
    fn try_from(raw: EnvelopeContextWireV1) -> Result<Self, Self::Error> {
        let value = Self {
            schema_version: raw.schema_version,
            identity: raw.identity,
            algorithm: raw.algorithm,
        };
        value.validate()?;
        Ok(value)
    }
}
impl EnvelopeContextV1 {
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        if self.schema_version != ENVELOPE_SCHEMA_VERSION {
            return Err(EnvelopeError::UnsupportedSchema);
        }
        self.identity.validate()
    }
    pub fn value_aad(&self) -> Result<Vec<u8>, EnvelopeError> {
        self.validate()?;
        oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.key-envelope.value.v1", self))
            .map_err(|_| EnvelopeError::Encoding)
    }
    pub fn wrap_aad(&self, wrapping_key_version: DecimalU64) -> Result<Vec<u8>, EnvelopeError> {
        self.validate()?;
        if wrapping_key_version.0 == 0 {
            return Err(EnvelopeError::InvalidVersion);
        }
        oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.key-envelope.wrap.v1",
            self,
            wrapping_key_version,
        ))
        .map_err(|_| EnvelopeError::Encoding)
    }
}

/// Opaque AES-256-GCM output including the 128-bit authentication tag. All byte
/// strings are canonical lowercase hex. Neither plaintext nor wrapping keys belong here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "KeyEnvelopeWireV1")]
pub struct KeyEnvelopeV1 {
    pub context: EnvelopeContextV1,
    pub ciphertext: String,
    pub value_nonce: String,
    pub wrapped_dek: String,
    pub wrap_nonce: String,
    pub wrapping_key_version: DecimalU64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyEnvelopeWireV1 {
    context: EnvelopeContextV1,
    ciphertext: String,
    value_nonce: String,
    wrapped_dek: String,
    wrap_nonce: String,
    wrapping_key_version: DecimalU64,
}
impl TryFrom<KeyEnvelopeWireV1> for KeyEnvelopeV1 {
    type Error = EnvelopeError;
    fn try_from(raw: KeyEnvelopeWireV1) -> Result<Self, Self::Error> {
        let value = Self {
            context: raw.context,
            ciphertext: raw.ciphertext,
            value_nonce: raw.value_nonce,
            wrapped_dek: raw.wrapped_dek,
            wrap_nonce: raw.wrap_nonce,
            wrapping_key_version: raw.wrapping_key_version,
        };
        value.validate()?;
        Ok(value)
    }
}
fn validate_hex(value: &str, min_bytes: usize, max_bytes: usize) -> Result<(), EnvelopeError> {
    if value.len() < min_bytes * 2 || value.len() > max_bytes * 2 || value.len() % 2 != 0 {
        return Err(EnvelopeError::InvalidLength);
    }
    if !value
        .bytes()
        .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err(EnvelopeError::InvalidHex);
    }
    Ok(())
}
impl KeyEnvelopeV1 {
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        self.context.validate()?;
        if self.wrapping_key_version.0 == 0 {
            return Err(EnvelopeError::InvalidVersion);
        }
        validate_hex(&self.value_nonce, AES_GCM_NONCE_BYTES, AES_GCM_NONCE_BYTES)?;
        validate_hex(&self.wrap_nonce, AES_GCM_NONCE_BYTES, AES_GCM_NONCE_BYTES)?;
        if self.value_nonce == self.wrap_nonce {
            return Err(EnvelopeError::NonceReuse);
        }

        validate_hex(
            &self.wrapped_dek,
            DEK_BYTES + AES_GCM_TAG_BYTES,
            DEK_BYTES + AES_GCM_TAG_BYTES,
        )?;
        validate_hex(
            &self.ciphertext,
            AES_GCM_TAG_BYTES + 1,
            MAX_ENVELOPE_VALUE_BYTES + AES_GCM_TAG_BYTES,
        )
    }
    /// The expected context comes from the role-local trusted storage address,
    /// never from the envelope being opened or a caller-selected actor role.
    pub fn validate_for(&self, expected: &EnvelopeContextV1) -> Result<(), EnvelopeError> {
        self.validate()?;
        expected.validate()?;
        if &self.context != expected {
            return Err(EnvelopeError::ContextMismatch);
        }
        Ok(())
    }
    pub fn value_aad(&self) -> Result<Vec<u8>, EnvelopeError> {
        self.validate()?;
        self.context.value_aad()
    }
    pub fn wrap_aad(&self) -> Result<Vec<u8>, EnvelopeError> {
        self.validate()?;
        self.context.wrap_aad(self.wrapping_key_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aad_is_bound_to_identity_role_version_and_domain() {
        let key = KeyIdentity {
            environment: "staging".into(),
            account_id: "one".into(),
            role: Role::Executor,
            key_id: "key".into(),
            key_version: DecimalU64(u64::MAX),
        };
        let original = key.value_aad().unwrap();
        assert_ne!(original, key.wrap_aad(DecimalU64(1)).unwrap());
        assert_ne!(
            key.wrap_aad(DecimalU64(1)).unwrap(),
            key.wrap_aad(DecimalU64(2)).unwrap()
        );
        for changed in [
            KeyIdentity {
                account_id: "two".into(),
                ..key.clone()
            },
            KeyIdentity {
                environment: "dev".into(),
                ..key.clone()
            },
            KeyIdentity {
                role: Role::Owner,
                ..key.clone()
            },
            KeyIdentity {
                key_id: "other".into(),
                ..key.clone()
            },
            KeyIdentity {
                key_version: DecimalU64(1),
                ..key.clone()
            },
        ] {
            assert_ne!(original, changed.value_aad().unwrap());
        }
        assert!(
            KeyIdentity {
                role: Role::RecoveryFence,
                ..key
            }
            .value_aad()
            .is_err()
        );
    }
    fn context() -> EnvelopeContextV1 {
        EnvelopeContextV1 {
            schema_version: 1,
            identity: KeyIdentity {
                environment: "staging".into(),
                account_id: "one".into(),
                role: Role::Identity,
                key_id: "login".into(),
                key_version: DecimalU64(u64::MAX),
            },
            algorithm: EnvelopeAlgorithmV1::Aes256Gcm,
        }
    }
    fn envelope() -> KeyEnvelopeV1 {
        KeyEnvelopeV1 {
            context: context(),
            ciphertext: "aa".repeat(48),
            value_nonce: "bb".repeat(12),
            wrapped_dek: "cc".repeat(48),
            wrap_nonce: "dd".repeat(12),
            wrapping_key_version: DecimalU64(u64::MAX),
        }
    }
    #[test]
    fn legacy_aad_bytes_are_frozen_and_new_envelopes_have_distinct_domains() {
        let key = KeyIdentity {
            environment: "staging".into(),
            account_id: "one".into(),
            role: Role::Executor,
            key_id: "key".into(),
            key_version: DecimalU64(u64::MAX),
        };
        let hex = |bytes: Vec<u8>| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(
            hex(key.value_aad().unwrap()),
            concat!(
                "82781b6f61736973372e637573746f64792e6b65792d76616c75652e7631a56b656e7669726f6e6d",
                "656e746773746167696e676a6163636f756e745f6964636f6e6564726f6c65686578656375746f72",
                "666b65795f6964636b65796b6b65795f76657273696f6e7431383434363734343037333730393535",
                "31363135",
            )
        );
        assert_eq!(
            hex(key.wrap_aad(DecimalU64(1)).unwrap()),
            concat!(
                "83781a6f61736973372e637573746f64792e64656b2d777261702e7631a56b656e7669726f6e6d65",
                "6e746773746167696e676a6163636f756e745f6964636f6e6564726f6c65686578656375746f7266",
                "6b65795f6964636b65796b6b65795f76657273696f6e743138343436373434303733373039353531",
                "3631356131",
            )
        );
        let context = EnvelopeContextV1 {
            identity: key.clone(),
            ..context()
        };
        assert_ne!(context.value_aad().unwrap(), key.value_aad().unwrap());
        assert_ne!(
            context.wrap_aad(DecimalU64(1)).unwrap(),
            key.wrap_aad(DecimalU64(1)).unwrap()
        );
        assert_ne!(
            context.value_aad().unwrap(),
            context.wrap_aad(DecimalU64(1)).unwrap()
        );
    }
    #[test]
    fn new_context_binds_every_identity_field_and_wrapping_version() {
        let context = context();
        let aad = context.value_aad().unwrap();
        let wrap = context.wrap_aad(DecimalU64(1)).unwrap();
        for field in 0..5 {
            let mut other = context.clone();
            match field {
                0 => other.identity.environment = "dev".into(),
                1 => other.identity.account_id = "two".into(),
                2 => other.identity.role = Role::Owner,
                3 => other.identity.key_id = "other".into(),
                _ => other.identity.key_version = DecimalU64(2),
            };
            assert_ne!(other.value_aad().unwrap(), aad);
            assert_ne!(other.wrap_aad(DecimalU64(1)).unwrap(), wrap);
        }
        assert_ne!(context.wrap_aad(DecimalU64(2)).unwrap(), wrap);
        assert!(context.wrap_aad(DecimalU64(0)).is_err());
        let mut invalid = context.clone();
        invalid.schema_version = 2;
        assert!(invalid.value_aad().is_err());
        invalid = context;
        invalid.identity.role = Role::RecoveryFence;
        assert!(invalid.value_aad().is_err());
    }
    #[test]
    fn envelope_restores_strictly_and_preserves_decimal_u64_max() {
        let original = envelope();
        let json = serde_json::to_value(&original).unwrap();
        assert_eq!(json["wrapping_key_version"], "18446744073709551615");
        assert_eq!(
            json["context"]["identity"]["key_version"],
            "18446744073709551615"
        );
        let restored: KeyEnvelopeV1 = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(restored, original);
        restored.validate_for(&context()).unwrap();
        for field in ["ciphertext", "value_nonce", "wrapped_dek", "wrap_nonce"] {
            let mut uppercase = json.clone();
            uppercase[field] =
                serde_json::json!(json[field].as_str().unwrap().to_ascii_uppercase());
            assert!(serde_json::from_value::<KeyEnvelopeV1>(uppercase).is_err());

            for bad in ["", "a", "AA", "zz", " aa", "00\n"] {
                let mut changed = json.clone();
                changed[field] = serde_json::json!(bad);
                assert!(
                    serde_json::from_value::<KeyEnvelopeV1>(changed).is_err(),
                    "{field}:{bad:?}"
                );
            }
        }
        for bad in [
            serde_json::json!(1),
            serde_json::json!("01"),
            serde_json::json!("0"),
            serde_json::json!("18446744073709551616"),
        ] {
            let mut changed = json.clone();
            changed["wrapping_key_version"] = bad;
            assert!(serde_json::from_value::<KeyEnvelopeV1>(changed).is_err());
        }
        let mut extra = json.clone();
        extra["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<KeyEnvelopeV1>(extra).is_err());
        for (field, bad) in [
            ("schema_version", serde_json::json!(2)),
            ("algorithm", serde_json::json!("aes_cbc")),
            ("unknown", serde_json::json!(true)),
        ] {
            let mut changed = json["context"].clone();
            changed[field] = bad;
            assert!(serde_json::from_value::<EnvelopeContextV1>(changed).is_err());
        }
        let mut invalid = json;
        invalid["context"]["identity"]["key_version"] = serde_json::json!("0");
        assert!(serde_json::from_value::<KeyEnvelopeV1>(invalid).is_err());
    }
    #[test]
    fn envelope_length_limits_and_trusted_context_prevent_swapping() {
        let mut value = envelope();
        value.ciphertext = "00".repeat(AES_GCM_TAG_BYTES + 1);
        value.validate().unwrap();
        value.ciphertext = "00".repeat(AES_GCM_TAG_BYTES);
        assert!(value.validate().is_err());
        value.ciphertext = "00".repeat(MAX_ENVELOPE_VALUE_BYTES + AES_GCM_TAG_BYTES);
        value.validate().unwrap();
        value.ciphertext.push_str("00");
        assert!(value.validate().is_err());
        value = envelope();
        let aad = value.value_aad().unwrap();
        let wrap = value.wrap_aad().unwrap();
        value.ciphertext = "00".repeat(48);
        value.value_nonce = "11".repeat(12);
        assert_eq!(value.value_aad().unwrap(), aad);
        assert_eq!(value.wrap_aad().unwrap(), wrap);
        value.wrapping_key_version = DecimalU64(1);
        assert_eq!(value.value_aad().unwrap(), aad);
        assert_ne!(value.wrap_aad().unwrap(), wrap);
        let mut other = context();
        other.identity.account_id = "other".into();
        assert_eq!(
            value.validate_for(&other),
            Err(EnvelopeError::ContextMismatch)
        );
        let mut repeated = envelope();
        repeated.wrap_nonce = repeated.value_nonce.clone();
        assert_eq!(repeated.validate(), Err(EnvelopeError::NonceReuse));
        for bytes in [11, 13] {
            let mut value = envelope();
            value.value_nonce = "00".repeat(bytes);
            assert!(value.validate().is_err());
        }
        for bytes in [47, 49] {
            let mut value = envelope();
            value.wrapped_dek = "00".repeat(bytes);
            assert!(value.validate().is_err());
        }
        for bad in [" ".into(), "界".repeat(86), "line\nbreak".into()] {
            let mut value = context();
            value.identity.key_id = bad;
            assert!(value.validate().is_err());
        }
    }
}
