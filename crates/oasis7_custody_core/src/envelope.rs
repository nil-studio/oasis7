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
}

impl KeyIdentity {
    fn validate(&self) -> Result<(), EnvelopeError> {
        if [&self.environment, &self.account_id, &self.key_id]
            .iter()
            .any(|s| s.is_empty())
        {
            return Err(EnvelopeError::EmptyIdentity);
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
}
