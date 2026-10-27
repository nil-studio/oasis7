//! Explicit bounded multiheight read contract. Existing V1/V2 signing domains remain unchanged.
use super::{ContractError, FormalProjectionInclusionV1, FormalReadAnchorRefV1, WorldIdentity};
use serde::{Deserialize, Serialize};

pub const FORMAL_BFT_READ_PATH_V3: &str = "/v3/world/read";
pub const FORMAL_BFT_READ_SCHEMA_VERSION_V3: u32 = 3;
pub const FORMAL_BFT_READ_CONTRACT_VERSION_V3: u32 = 3;
pub const MAX_FORMAL_BFT_PREFIX_BYTES_V3: usize = 16 * 1024 * 1024;
pub const MAX_FORMAL_BFT_PREFIX_HEIGHTS_V3: u64 = 64;
pub const MAX_FORMAL_BFT_READ_RESPONSE_BYTES_V3: usize = 40 * 1024 * 1024;

/// An independently pinned historical cut within the complete H-anchored
/// prefix capacity. This is never a freshness floor or provider-selected head.

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftReadTargetV3 {
    pub height: u64,
    pub block_id: String,
}
impl FormalBftReadTargetV3 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.height == 0 || !is_digest(&self.block_id) {
            return Err(ContractError("formal BFT exact cut pin"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftReadProfileV3 {
    pub name: String,
    pub version: u32,
    pub wire_version: u16,
}
impl FormalBftReadProfileV3 {
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

/// Signed on the independent `/v3/world/read` domain. The cut is an exact
/// local pin; it cannot be advanced by a response or by the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftReadRequestV3 {
    pub schema_version: u32,
    pub contract_version: u32,
    pub world: WorldIdentity,
    pub scope_id: String,
    pub observation_nonce: String,
    pub handoff_anchor: FormalReadAnchorRefV1,
    pub target_config_digest: String,
    pub activation_digest: String,
    pub exact_target: FormalBftReadTargetV3,
}
impl FormalBftReadRequestV3 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != FORMAL_BFT_READ_SCHEMA_VERSION_V3
            || self.contract_version != FORMAL_BFT_READ_CONTRACT_VERSION_V3
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
        if distance == 0 || distance > MAX_FORMAL_BFT_PREFIX_HEIGHTS_V3 {
            return Err(ContractError(
                "formal BFT prefix target exceeds H-anchored capacity",
            ));
        }
        Ok(())
    }
}

/// Cursor/head identity fields are repeated from the certified block so the
/// application projection cannot be detached from its QC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftReadHeadV3 {
    pub height: u64,
    pub block_id: String,
    pub execution_block_hash: String,
    pub state_root: String,
    pub post_manifest_hash: String,
    pub projection_root: String,
    pub logical_tick: u64,
}
impl FormalBftReadHeadV3 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.height == 0
            || !is_digest(&self.block_id)
            || !is_digest(&self.execution_block_hash)
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
pub struct FormalBftReadResponseV3<T> {
    pub schema_version: u32,
    pub contract_version: u32,
    pub profile: FormalBftReadProfileV3,
    pub world: WorldIdentity,
    pub scope_id: String,
    pub observation_nonce: String,
    pub handoff_anchor: FormalReadAnchorRefV1,
    pub target_config_digest: String,
    pub activation_digest: String,
    pub exact_target: FormalBftReadTargetV3,
    pub head: FormalBftReadHeadV3,
    pub certified_prefix_cbor_hex: String,
    pub projection: FormalProjectionInclusionV1,
    pub view: T,
}
impl<T: Serialize> FormalBftReadResponseV3<T> {
    pub fn validate_structure(
        &self,
        request: &FormalBftReadRequestV3,
    ) -> Result<(), ContractError> {
        request.validate()?;
        self.profile.validate()?;
        self.world.validate()?;
        self.handoff_anchor.validate()?;
        self.exact_target.validate()?;
        self.head.validate()?;
        if self.schema_version != FORMAL_BFT_READ_SCHEMA_VERSION_V3
            || self.contract_version != FORMAL_BFT_READ_CONTRACT_VERSION_V3
            || self.world != request.world
            || self.scope_id != request.scope_id
            || self.observation_nonce != request.observation_nonce
            || self.handoff_anchor != request.handoff_anchor
            || self.target_config_digest != request.target_config_digest
            || self.activation_digest != request.activation_digest
            || self.exact_target != request.exact_target
            || self.head.height != self.exact_target.height
            || self.head.block_id != self.exact_target.block_id
            || !valid_hex_bytes(
                &self.certified_prefix_cbor_hex,
                MAX_FORMAL_BFT_PREFIX_BYTES_V3,
            )
            || self.projection.schema_version != 1
        {
            return Err(ContractError("formal BFT prefix response binding/bounds"));
        }
        let mut counter = JsonByteBudget(MAX_FORMAL_BFT_READ_RESPONSE_BYTES_V3);
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
    fn request() -> FormalBftReadRequestV3 {
        FormalBftReadRequestV3 {
            schema_version: 3,
            contract_version: 3,
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
            exact_target: FormalBftReadTargetV3 {
                height: 5,
                block_id: "77".repeat(32),
            },
        }
    }
    #[test]
    fn prefix_read_v3_binds_its_own_signed_domain_and_bounded_exact_target() {
        let mut request = request();
        request.validate().unwrap();
        let signed = crate::world_service::sign_read_request(
            FORMAL_BFT_READ_PATH_V3,
            request.clone(),
            &"12".repeat(32),
        )
        .unwrap();
        crate::world_service::verify_read_request(FORMAL_BFT_READ_PATH_V3, &signed).unwrap();
        assert!(
            crate::world_service::verify_read_request(
                super::super::FORMAL_BFT_READ_PATH_V2,
                &signed
            )
            .is_err()
        );
        request.exact_target.height = 66;
        request.validate().unwrap();
        request.exact_target.height = 67;
        assert!(request.validate().is_err());
        request.exact_target.height = 2;
        assert!(request.validate().is_err());
        request.exact_target.height = 1;
        assert!(request.validate().is_err());
        request.exact_target.height = 5;
        request.schema_version = 2;
        assert!(request.validate().is_err());
    }
    #[test]
    fn prefix_read_v3_json_budget_stops_streaming_before_oversized_allocation() {
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
        let mut counter = JsonByteBudget(MAX_FORMAL_BFT_READ_RESPONSE_BYTES_V3);
        assert!(serde_json::to_writer(&mut counter, &StreamingView(&count)).is_err());
        assert!(count.load(Ordering::SeqCst) < 6000);
        assert!(counter.0 < 8192);
    }
}
