//! Transport-independent authorization for formal validator governance.
use super::Action;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The signer set and threshold are always resolved from committed policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernanceAuthorizationV1 {
    pub schema_version: u16,
    pub world_id: String,
    pub chain_id: String,
    pub genesis_digest: String,
    pub parent_position: u64,
    pub parent_record_id: String,
    pub parent_state_root: String,
    pub policy_digest: String,
    pub controller_account_id: String,
    pub nonce: u64,
    pub valid_from_position: u64,
    pub valid_through_position: u64,
    #[serde(deserialize_with = "unique_signatures")]
    pub signatures: BTreeMap<String, String>,
}

fn unique_signatures<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    struct Signatures;
    impl<'de> serde::de::Visitor<'de> for Signatures {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 32 unique governance signer signatures")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut signatures = BTreeMap::new();
            while let Some((public, signature)) = map.next_entry::<String, String>()? {
                if signatures.len() >= 32 || signatures.insert(public, signature).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate or excessive governance signer",
                    ));
                }
            }
            Ok(signatures)
        }
    }
    deserializer.deserialize_map(Signatures)
}

/// Replay retains the exact signed input alongside its deterministic effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernanceAuthorizedActionV1 {
    pub action: Box<Action>,
    pub authorization: GovernanceAuthorizationV1,
}

pub fn governance_validator_admission_controller(action: &Action) -> Option<&str> {
    match action {
        Action::SubmitGovernanceValidatorAdmission {
            controller_account_id,
            ..
        }
        | Action::ApproveGovernanceValidatorAdmission {
            controller_account_id,
            ..
        }
        | Action::ActivateGovernanceValidatorAdmission {
            controller_account_id,
            ..
        }
        | Action::RevokeGovernanceValidatorAdmission {
            controller_account_id,
            ..
        } => Some(controller_account_id),
        _ => None,
    }
}

pub fn governance_validator_admission_auth_required(action: &Action) -> bool {
    governance_validator_admission_controller(action).is_some()
}

fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (97..=102).contains(&byte))
}

impl GovernanceAuthorizationV1 {
    /// Validate bounded wire structure; cryptographic authority belongs to World.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported governance authorization version".into());
        }
        for value in [&self.world_id, &self.chain_id, &self.controller_account_id] {
            if value.is_empty() || value.len() > 256 || value.trim() != value {
                return Err("invalid governance authorization identity bounds".into());
            }
        }
        for value in [
            &self.genesis_digest,
            &self.parent_record_id,
            &self.parent_state_root,
            &self.policy_digest,
        ] {
            if !lower_hex(value, 64) {
                return Err("invalid governance authorization digest".into());
            }
        }
        if self.nonce == 0
            || self.parent_position == u64::MAX
            || self.valid_from_position == 0
            || self.valid_from_position > self.valid_through_position
        {
            return Err("invalid governance authorization nonce or position range".into());
        }
        if self.signatures.len() > 32 {
            return Err("governance authorization signer bound exceeded".into());
        }
        for (key, signature) in &self.signatures {
            if !lower_hex(key, 64) || !lower_hex(signature, 128) {
                return Err("invalid governance authorization signature encoding".into());
            }
        }
        Ok(())
    }

    /// Fixed tuple encoding includes every action field and excludes signatures.
    pub fn signing_bytes(&self, action: &Action) -> Result<Vec<u8>, String> {
        self.validate()?;
        let controller = governance_validator_admission_controller(action)
            .ok_or("unsupported governance authorization action")?;
        if controller != self.controller_account_id {
            return Err("governance authorization controller/action mismatch".into());
        }
        serde_cbor::to_vec(&(
            "oasis7.governance.authorization.v1",
            self.schema_version,
            &self.world_id,
            &self.chain_id,
            &self.genesis_digest,
            self.parent_position,
            &self.parent_record_id,
            &self.parent_state_root,
            &self.policy_digest,
            &self.controller_account_id,
            self.nonce,
            self.valid_from_position,
            self.valid_through_position,
            action,
        ))
        .map_err(|error| format!("governance authorization encoding failed: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (GovernanceAuthorizationV1, Action) {
        (
            GovernanceAuthorizationV1 {
                schema_version: 1,
                world_id: "w".into(),
                chain_id: "c".into(),
                genesis_digest: "0".repeat(64),
                parent_position: 1,
                parent_record_id: "1".repeat(64),
                parent_state_root: "2".repeat(64),
                policy_digest: "3".repeat(64),
                controller_account_id: "a".into(),
                nonce: 1,
                valid_from_position: 2,
                valid_through_position: 3,
                signatures: BTreeMap::new(),
            },
            Action::ApproveGovernanceValidatorAdmission {
                controller_account_id: "a".into(),
                candidate_id: "b".into(),
            },
        )
    }

    #[test]
    fn canonical_vector_and_signature_independence() {
        let (mut proof, action) = fixture();
        // Independent CBOR framing: array(14), text(domain), fixed scalar/text
        // fields, then adjacent-tagged Action map and its two field map.
        let mut expected = vec![0x8e, 0x78, 34];
        expected.extend_from_slice(b"oasis7.governance.authorization.v1");
        expected.extend_from_slice(&[1, 0x61, 119, 0x61, 99]);
        for (index, byte) in [48, 49, 50, 51].into_iter().enumerate() {
            if index == 1 {
                expected.push(1);
            }
            expected.extend_from_slice(&[0x78, 64]);
            expected.extend(std::iter::repeat_n(byte, 64));
        }
        expected.extend_from_slice(&[0x61, 97, 1, 2, 3, 0xa2, 0x64, 116, 121, 112, 101, 0x78, 35]);
        expected.extend_from_slice(b"ApproveGovernanceValidatorAdmission");
        expected.extend_from_slice(&[0x64, b'd', b'a', b't', b'a', 0xa2, 0x75]);
        expected.extend_from_slice(b"controller_account_id");
        expected.extend_from_slice(&[0x61, 97, 0x6c]);
        expected.extend_from_slice(b"candidate_id");
        expected.extend_from_slice(&[0x61, 98]);
        assert_eq!(proof.signing_bytes(&action).unwrap(), expected);
        proof.signatures.insert("a".repeat(64), "b".repeat(128));
        assert_eq!(proof.signing_bytes(&action).unwrap(), expected);
        let changed = Action::ApproveGovernanceValidatorAdmission {
            controller_account_id: "a".into(),
            candidate_id: "changed".into(),
        };
        assert_ne!(proof.signing_bytes(&changed).unwrap(), expected);
    }

    #[test]
    fn rejects_noncanonical_and_unbounded_fields() {
        let (mut proof, action) = fixture();
        proof.signatures.insert("A".repeat(64), "b".repeat(128));
        assert!(proof.signing_bytes(&action).is_err());
        proof.signatures.clear();
        proof.nonce = 0;
        assert!(proof.validate().is_err());
        proof.nonce = 1;
        proof.valid_through_position = 1;
        assert!(proof.validate().is_err());
    }

    #[test]
    fn duplicate_wire_signers_are_rejected_before_map_deduplication() {
        let (proof, _) = fixture();
        let encoded = serde_json::to_string(&proof).unwrap();
        let duplicate = format!(
            "\"signatures\":{{\"{}\":\"{}\",\"{}\":\"{}\"}}",
            "a".repeat(64),
            "b".repeat(128),
            "a".repeat(64),
            "c".repeat(128)
        );
        let encoded = encoded.replace("\"signatures\":{}", &duplicate);
        assert!(
            serde_json::from_str::<GovernanceAuthorizationV1>(&encoded)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
    }
}
