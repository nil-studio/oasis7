//! Header-only consensus wire-3 reads on the independent read-contract-4 domain.
use super::{ContractError, FormalProjectionInclusionV1, FormalReadAnchorRefV1, WorldIdentity};
use serde::{Deserialize, Serialize};

pub const FORMAL_BFT_HEADER_READ_PATH_V4: &str = "/v4/world/read";
pub const FORMAL_BFT_HEADER_READ_SCHEMA_VERSION_V4: u32 = 4;
pub const FORMAL_BFT_HEADER_READ_CONTRACT_VERSION_V4: u32 = 4;
pub const MAX_FORMAL_BFT_HEADER_PREFIX_BYTES_V4: usize = 16 * 1024 * 1024;
pub const MAX_FORMAL_BFT_HEADER_PREFIX_HEIGHTS_V4: u64 = 64;
pub const MAX_FORMAL_BFT_HEADER_READ_RESPONSE_BYTES_V4: usize = 40 * 1024 * 1024;

/// An independently pinned historical cut within the complete H-anchored
/// prefix capacity. This is never a freshness floor or provider-selected head.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftHeaderReadTargetV4 {
    pub height: u64,
    pub header_id: String,
}
impl FormalBftHeaderReadTargetV4 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.height == 0 || !is_digest(&self.header_id) {
            return Err(ContractError("formal BFT exact cut pin"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftHeaderReadProfileV4 {
    pub name: String,
    pub version: u32,
    pub wire_version: u16,
}
impl FormalBftHeaderReadProfileV4 {
    pub fn bft_v1() -> Self {
        Self {
            name: "bft".into(),
            version: 1,
            wire_version: 3,
        }
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        if self != &Self::bft_v1() {
            return Err(ContractError("unsupported formal BFT read profile"));
        }
        Ok(())
    }
}

/// Signed on the independent `/v4/world/read` domain. The cut is an exact
/// local pin; it cannot be advanced by a response or by the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftHeaderReadRequestV4 {
    pub schema_version: u32,
    pub contract_version: u32,
    pub world: WorldIdentity,
    pub scope_id: String,
    pub observation_nonce: String,
    pub handoff_anchor: FormalReadAnchorRefV1,
    pub target_config_digest: String,
    pub activation_digest: String,
    pub exact_target: FormalBftHeaderReadTargetV4,
}
impl FormalBftHeaderReadRequestV4 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != FORMAL_BFT_HEADER_READ_SCHEMA_VERSION_V4
            || self.contract_version != FORMAL_BFT_HEADER_READ_CONTRACT_VERSION_V4
            || self.observation_nonce.len() != 64
            || !is_digest(&self.observation_nonce)
            || !is_digest(&self.target_config_digest)
            || !is_digest(&self.activation_digest)
            || self.scope_id != "public"
                && !self
                    .scope_id
                    .strip_prefix("agent:")
                    .is_some_and(|id| !id.trim().is_empty() && id.len() <= 256)
        {
            return Err(ContractError("formal BFT read request schema/binding"));
        }
        self.world.validate()?;
        self.handoff_anchor.validate()?;
        self.exact_target.validate()?;
        let distance = self
            .exact_target
            .height
            .checked_sub(self.handoff_anchor.position)
            .ok_or(ContractError("formal BFT prefix target precedes H"))?;
        if distance == 0 || distance > MAX_FORMAL_BFT_HEADER_PREFIX_HEIGHTS_V4 {
            return Err(ContractError(
                "formal BFT prefix target exceeds H-anchored capacity",
            ));
        }
        Ok(())
    }
}

/// Head identity fields are repeated from the certified header and commitment so the
/// application projection cannot be detached from its QC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftHeaderReadHeadV4 {
    pub height: u64,
    pub header_id: String,
    pub execution_commitment_hash: String,
    pub state_root: String,
    pub post_manifest_hash: String,
    pub projection_root: String,
    pub logical_tick: u64,
}
impl FormalBftHeaderReadHeadV4 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.height == 0
            || !is_digest(&self.header_id)
            || !is_digest(&self.execution_commitment_hash)
            || !is_digest(&self.state_root)
            || !is_digest(&self.post_manifest_hash)
            || !is_digest(&self.projection_root)
        {
            return Err(ContractError("formal BFT read head"));
        }
        Ok(())
    }
}

/// Canonical CBOR is carried as lowercase hex so the JSON transport preserves
/// exact bytes and the verifier can reject alternate encodings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftHeaderReadResponseV4<T> {
    pub schema_version: u32,
    pub contract_version: u32,
    pub profile: FormalBftHeaderReadProfileV4,
    pub world: WorldIdentity,
    pub scope_id: String,
    pub observation_nonce: String,
    pub handoff_anchor: FormalReadAnchorRefV1,
    pub target_config_digest: String,
    pub activation_digest: String,
    pub exact_target: FormalBftHeaderReadTargetV4,
    pub head: FormalBftHeaderReadHeadV4,
    pub certified_prefix_cbor_hex: String,
    pub projection: FormalProjectionInclusionV1,
    pub view: T,
}
impl<T: Serialize> FormalBftHeaderReadResponseV4<T> {
    pub fn validate_structure(
        &self,
        request: &FormalBftHeaderReadRequestV4,
    ) -> Result<(), ContractError> {
        request.validate()?;
        self.profile.validate()?;
        self.world.validate()?;
        self.handoff_anchor.validate()?;
        self.exact_target.validate()?;
        self.head.validate()?;
        if self.schema_version != FORMAL_BFT_HEADER_READ_SCHEMA_VERSION_V4
            || self.contract_version != FORMAL_BFT_HEADER_READ_CONTRACT_VERSION_V4
            || self.world != request.world
            || self.scope_id != request.scope_id
            || self.observation_nonce != request.observation_nonce
            || self.handoff_anchor != request.handoff_anchor
            || self.target_config_digest != request.target_config_digest
            || self.activation_digest != request.activation_digest
            || self.exact_target != request.exact_target
            || self.head.height != self.exact_target.height
            || self.head.header_id != self.exact_target.header_id
            || !valid_hex_bytes(
                &self.certified_prefix_cbor_hex,
                MAX_FORMAL_BFT_HEADER_PREFIX_BYTES_V4,
            )
            || self.projection.schema_version != 1
        {
            return Err(ContractError("formal BFT prefix response binding/bounds"));
        }
        let mut counter = JsonByteBudget(MAX_FORMAL_BFT_HEADER_READ_RESPONSE_BYTES_V4);
        serde_json::to_writer(&mut counter, self)
            .map_err(|_| ContractError("formal BFT prefix response encoding/byte bound"))?;
        Ok(())
    }
}

/// Count encoded bytes without constructing another potentially oversized
/// response allocation. serde_json stops as soon as this budget is exhausted.
struct JsonByteBudget(usize);
impl std::io::Write for JsonByteBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| std::io::Error::other("formal BFT prefix JSON byte capacity"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn valid_hex_bytes(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len().is_multiple_of(2)
        && value.len() <= maximum.saturating_mul(2)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> FormalBftHeaderReadRequestV4 {
        FormalBftHeaderReadRequestV4 {
            schema_version: 4,
            contract_version: 4,
            world: WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "11".repeat(32),
            },
            scope_id: "public".into(),
            observation_nonce: "22".repeat(32),
            handoff_anchor: FormalReadAnchorRefV1 {
                position: 2,
                record_id: "33".repeat(32),
                active_authority_id: "44".repeat(32),
            },
            target_config_digest: "55".repeat(32),
            activation_digest: "66".repeat(32),
            exact_target: FormalBftHeaderReadTargetV4 {
                height: 5,
                header_id: "77".repeat(32),
            },
        }
    }
    #[test]
    fn prefix_read_v4_binds_its_own_signed_domain_and_bounded_exact_target() {
        let mut request = request();
        request.validate().unwrap();
        let signed = crate::world_service::sign_read_request(
            FORMAL_BFT_HEADER_READ_PATH_V4,
            request.clone(),
            &"12".repeat(32),
        )
        .unwrap();
        crate::world_service::verify_read_request(FORMAL_BFT_HEADER_READ_PATH_V4, &signed).unwrap();
        assert!(
            crate::world_service::verify_read_request(
                super::super::FORMAL_BFT_READ_PATH_V3,
                &signed
            )
            .is_err()
        );
        let digest =
            crate::world_service::request_digest(FORMAL_BFT_HEADER_READ_PATH_V4, &signed).unwrap();
        let reply = crate::world_service::sign_service_response(
            FORMAL_BFT_HEADER_READ_PATH_V4,
            digest.clone(),
            42u64,
            &"12".repeat(32),
        )
        .unwrap();
        crate::world_service::verify_service_response(
            FORMAL_BFT_HEADER_READ_PATH_V4,
            &digest,
            &reply,
            &signed.subject_public_key,
        )
        .unwrap();
        for old_domain in [
            super::super::FORMAL_BFT_READ_PATH_V2,
            super::super::FORMAL_BFT_READ_PATH_V3,
        ] {
            assert!(crate::world_service::verify_read_request(old_domain, &signed).is_err());
            assert!(
                crate::world_service::verify_service_response(
                    old_domain,
                    &digest,
                    &reply,
                    &signed.subject_public_key
                )
                .is_err()
            );
        }
        request.exact_target.height = 66;
        request.validate().unwrap();
        request.exact_target.height = 67;
        assert!(request.validate().is_err());
        request.exact_target.height = 2;
        assert!(request.validate().is_err());
        request.exact_target.height = 1;
        assert!(request.validate().is_err());
        request.exact_target.height = 5;
        request.schema_version = 3;
        assert!(request.validate().is_err());
    }
    #[test]
    fn prefix_read_v4_json_budget_stops_streaming_before_oversized_allocation() {
        use serde::ser::SerializeSeq;
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct StreamingView<'a>(&'a AtomicUsize);
        impl Serialize for StreamingView<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let chunk = "x".repeat(8192);
                let mut seq = serializer.serialize_seq(Some(6000))?;
                for _ in 0..6000 {
                    self.0.fetch_add(1, Ordering::SeqCst);
                    seq.serialize_element(&chunk)?;
                }
                seq.end()
            }
        }
        let count = AtomicUsize::new(0);
        let mut counter = JsonByteBudget(MAX_FORMAL_BFT_HEADER_READ_RESPONSE_BYTES_V4);
        assert!(serde_json::to_writer(&mut counter, &StreamingView(&count)).is_err());
        assert!(count.load(Ordering::SeqCst) < 6000);
        assert!(counter.0 < 8192);
    }
    fn response() -> FormalBftHeaderReadResponseV4<serde_json::Value> {
        let request = request();
        FormalBftHeaderReadResponseV4 {
            schema_version: 4,
            contract_version: 4,
            profile: FormalBftHeaderReadProfileV4::bft_v1(),
            world: request.world,
            scope_id: request.scope_id,
            observation_nonce: request.observation_nonce,
            handoff_anchor: request.handoff_anchor,
            target_config_digest: request.target_config_digest,
            activation_digest: request.activation_digest,
            exact_target: request.exact_target.clone(),
            head: FormalBftHeaderReadHeadV4 {
                height: request.exact_target.height,
                header_id: request.exact_target.header_id,
                execution_commitment_hash: "88".repeat(32),
                state_root: "99".repeat(32),
                post_manifest_hash: "aa".repeat(32),
                projection_root: "bb".repeat(32),
                logical_tick: 5,
            },
            // Structural validation is distinct from protocol QC verification.
            certified_prefix_cbor_hex: "80".into(),
            projection: FormalProjectionInclusionV1 {
                schema_version: 1,
                audience: super::super::FormalProjectionAudienceV1 {
                    scope_id: "public".into(),
                    reader_public_key: None,
                },
                leaf_index: 0,
                leaf_count: 1,
                siblings: Vec::new(),
            },
            view: serde_json::json!({"public": true}),
        }
    }
    #[test]
    fn header_read_v4_structure_rejects_changed_echoes_old_fields_and_hex() {
        let request = request();
        response().validate_structure(&request).unwrap();
        let original = serde_json::to_value(response()).unwrap();
        for (field, value) in [
            ("schema_version", serde_json::json!(3)),
            ("contract_version", serde_json::json!(3)),
            ("scope_id", serde_json::json!("agent:other")),
            ("observation_nonce", serde_json::json!("cc".repeat(32))),
            ("target_config_digest", serde_json::json!("cc".repeat(32))),
            ("activation_digest", serde_json::json!("cc".repeat(32))),
            ("certified_prefix_cbor_hex", serde_json::json!("AA")),
        ] {
            let mut altered = original.clone();
            altered[field] = value;
            let response: FormalBftHeaderReadResponseV4<serde_json::Value> =
                serde_json::from_value(altered).unwrap();
            assert!(
                response.validate_structure(&request).is_err(),
                "accepted changed {field}"
            );
        }
        for field in ["exact_target", "head"] {
            let mut altered = original.clone();
            altered[field]["header_id"] = serde_json::json!("cc".repeat(32));
            let response: FormalBftHeaderReadResponseV4<serde_json::Value> =
                serde_json::from_value(altered).unwrap();
            assert!(response.validate_structure(&request).is_err());
        }
        let mut old_target = serde_json::to_value(&request.exact_target).unwrap();
        let id = old_target
            .as_object_mut()
            .unwrap()
            .remove("header_id")
            .unwrap();
        old_target["block_id"] = id;
        assert!(serde_json::from_value::<FormalBftHeaderReadTargetV4>(old_target).is_err());
        assert!(valid_hex_bytes("aabb", 2));
        for value in ["", "a", "AA", "aabbcc"] {
            assert!(!valid_hex_bytes(value, 2));
        }
        let mut profile = FormalBftHeaderReadProfileV4::bft_v1();
        profile.wire_version = 2;
        assert!(profile.validate().is_err());
    }
}
