//! Signature/closed wire contract only. No issuer, payment, floor attestation or
//! network verifier is implemented here; these values cannot enable a route.
use super::*;
use ed25519_dalek::{Signature, VerifyingKey};
pub const PAID_RESOURCE_READ_DOMAIN: &str = "oasis7.custody.paid-resource-read-proof.v1";
#[cfg(test)]
fn required_nullable_string_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type": ["string", "null"]})
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaidResourceReadStageV1 {
    OwnerSetupFinal,
    TestSessionOwnerRead,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidResourceReadPayloadV1 {
    pub api_version: u8,
    pub issuer_namespace: String,
    pub issuer_service_id: String,
    pub issuer_environment: String,
    pub issuer_key_id: String,
    pub issuer_epoch: DecimalU64,
    pub issuer_config_generation: DecimalU64,
    pub native_audience_origin: String,
    pub world_audience: ApprovedWorldV1,
    pub native_path: String,
    pub world_path: String,
    pub stage: PaidResourceReadStageV1,
    pub custody_account_id: String,
    pub manager_native_account_id: String,
    pub participant_native_account_id: String,
    pub provision_id: String,
    pub setup_id: String,
    #[serde(deserialize_with = "required_option")]
    #[cfg_attr(test, schemars(schema_with = "required_nullable_string_schema"))]
    pub test_session_id: Option<String>,
    pub approval_operation_id: String,
    pub approval_input_digest: String,
    pub plan_digest: String,
    #[serde(deserialize_with = "required_option")]
    #[cfg_attr(test, schemars(schema_with = "required_nullable_string_schema"))]
    pub prepared_resource_basis_digest: Option<String>,
    pub selection_digest: String,
    pub owner_public_key: String,
    pub owner_binding: String,
    pub task_fence_digest: String,
    pub signer_record_digest: String,
    pub native_request_digest: String,
    pub world_request_digest: String,
    #[serde(deserialize_with = "required_option")]
    #[cfg_attr(test, schemars(schema_with = "required_nullable_string_schema"))]
    pub preflight_world_request_digest: Option<String>,
    pub source_operation_id: String,
    pub source_record_digest: String,
    pub source_security_epoch: DecimalU64,
    pub service_epoch: DecimalU64,
    pub directory_generation: DecimalU64,
    pub resource_flag_generation: DecimalU64,
    pub paid_attempt_id: String,
    pub paid_record_digest: String,
    pub native_http_units: DecimalU64,
    pub world_http_units: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidResourceReadProofV1 {
    pub payload: PaidResourceReadPayloadV1,
    pub signature_hex: String,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidResourceReadIssuerPinV1 {
    pub namespace: String,
    pub service_id: String,
    pub environment: String,
    pub key_id: String,
    pub public_key: String,
    pub issuer_epoch: DecimalU64,
    pub issuer_config_generation: DecimalU64,
    pub not_before: DecimalU64,
    pub not_after: DecimalU64,
    pub native_audience_origin: String,
    pub world_audience: ApprovedWorldV1,
    pub allowed_stage: PaidResourceReadStageV1,
}
/// Serialized floor values do not attest protected storage or rollback safety.
/// A future verifier must acquire them from actual nonrollbackable authority.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidResourceReadIssuerFloorV1 {
    pub issuer_epoch: DecimalU64,
    pub config_generation: DecimalU64,
}
fn decode<const N: usize>(s: &str) -> Result<[u8; N], String> {
    if s.len() != N * 2
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    let mut a = [0; N];
    for (i, b) in a.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| invalid())?;
    }
    Ok(a)
}
impl PaidResourceReadPayloadV1 {
    pub fn validate_shape(&self) -> Result<(), String> {
        let strings = [
            &self.issuer_namespace,
            &self.issuer_service_id,
            &self.issuer_environment,
            &self.issuer_key_id,
            &self.custody_account_id,
            &self.manager_native_account_id,
            &self.participant_native_account_id,
            &self.provision_id,
            &self.setup_id,
            &self.approval_operation_id,
            &self.owner_binding,
            &self.source_operation_id,
            &self.paid_attempt_id,
        ];
        let digests = [
            &self.approval_input_digest,
            &self.plan_digest,
            &self.selection_digest,
            &self.owner_public_key,
            &self.task_fence_digest,
            &self.signer_record_digest,
            &self.native_request_digest,
            &self.world_request_digest,
            &self.source_record_digest,
            &self.paid_record_digest,
        ];
        let generation = [
            self.issuer_epoch.0,
            self.issuer_config_generation.0,
            self.source_security_epoch.0,
            self.service_epoch.0,
            self.directory_generation.0,
            self.resource_flag_generation.0,
        ];
        if self.api_version != 1
            || strings.iter().any(|s| !id(s))
            || digests.iter().any(|s| !hex(s))
            || generation.contains(&0)
            || !self.world_audience.valid()
            || crate::http_pop::CanonicalHttpTarget::new(
                &self.native_audience_origin,
                crate::http_pop::HttpPopRoute::ProofChallenges,
            )
            .is_err()
            || self.expires_at.0 <= self.issued_at.0
            || self.expires_at.0 > self.issued_at.0.checked_add(5000).ok_or_else(invalid)?
            || self.native_http_units.0 != 1
        {
            return Err(invalid());
        }
        let valid = match self.stage {
            PaidResourceReadStageV1::OwnerSetupFinal => {
                self.test_session_id.is_none()
                    && self.prepared_resource_basis_digest.is_none()
                    && self
                        .preflight_world_request_digest
                        .as_ref()
                        .is_some_and(|v| hex(v))
                    && self.world_http_units.0 == 2
                    && self.native_path == "/entry-owner/control"
                    && self.world_path == wire::RESOURCE_GOAL_PATH_V2
            }
            PaidResourceReadStageV1::TestSessionOwnerRead => {
                self.test_session_id.as_ref().is_some_and(|v| id(v))
                    && self
                        .prepared_resource_basis_digest
                        .as_ref()
                        .is_some_and(|v| hex(v))
                    && self.preflight_world_request_digest.is_none()
                    && self.world_http_units.0 == 1
                    && self.native_path == wire::NATIVE_OWNER_RESOURCE_VIEW_PATH_V1
                    && self.world_path == wire::OWNER_RESOURCE_VIEW_PATH_V1
            }
        };
        if valid { Ok(()) } else { Err(invalid()) }
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        oasis7_wasm_abi::encode_canonical_cbor(&(PAID_RESOURCE_READ_DOMAIN, self))
            .map_err(|e| e.to_string())
    }
}
impl PaidResourceReadProofV1 {
    /// Only the configured signature/lease contract, not payment consumption,
    /// current Source/account authority or an assertion of antirollback storage.
    pub fn verify_issuer(
        &self,
        pin: &PaidResourceReadIssuerPinV1,
        floor: &PaidResourceReadIssuerFloorV1,
        now: u64,
    ) -> Result<(), String> {
        let p = &self.payload;
        p.validate_shape()?;
        if pin.namespace != p.issuer_namespace
            || pin.service_id != p.issuer_service_id
            || pin.environment != p.issuer_environment
            || pin.key_id != p.issuer_key_id
            || pin.issuer_epoch.0 != p.issuer_epoch.0
            || pin.issuer_config_generation.0 != p.issuer_config_generation.0
            || pin.issuer_epoch.0 < floor.issuer_epoch.0
            || pin.issuer_config_generation.0 < floor.config_generation.0
            || pin.native_audience_origin != p.native_audience_origin
            || pin.world_audience != p.world_audience
            || pin.allowed_stage != p.stage
            || pin.not_before.0 > p.issued_at.0
            || pin.not_after.0 < p.expires_at.0
            || p.issued_at.0 > now
            || now >= p.expires_at.0
        {
            return Err(invalid());
        }
        let k = VerifyingKey::from_bytes(&decode::<32>(&pin.public_key)?).map_err(|_| invalid())?;
        k.verify_strict(
            &p.signing_bytes()?,
            &Signature::from_bytes(&decode::<64>(&self.signature_hex)?),
        )
        .map_err(|_| invalid())
    }
}

impl core::fmt::Debug for PaidResourceReadPayloadV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PaidResourceReadPayloadV1(<private>)")
    }
}
impl core::fmt::Debug for PaidResourceReadProofV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PaidResourceReadProofV1(<private>)")
    }
}
