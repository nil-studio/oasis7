//! Shared Account-to-Native model reservation codec. No caller-ready boolean.
use crate::{
    DecimalU64,
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
        AccountMachinePurposeV2,
    },
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeModelCallKindV2 {
    ProbeFirst,
    ProbeSecond,
    Inference,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeModelProtocolResultV2 {
    FirstProbeEcho,
    SecondProbeReady,
    ProbeRejected,
    InferenceComplete,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeModelReservedEdgeV2 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub session_id: String,
    pub approved_input_digest: String,
    pub budget_digest: String,
    pub reservation_id: String,
    pub edge_id: String,
    pub call_kind: NativeModelCallKindV2,
    pub graph_digest: String,
    pub provider_request_digest: String,
    pub price_policy_digest: String,
    pub native_account_id: String,
    pub native_instance_public_key: String,
    pub connection_ref_digest: String,
    pub connection_generation: DecimalU64,
    pub preset_id: String,
    pub preset_revision: DecimalU64,
    pub model: String,
    pub ordinal: DecimalU64,
    pub floor: DecimalU64,
    pub max_input_tokens: DecimalU64,
    pub max_output_tokens: DecimalU64,
    pub max_tokens: DecimalU64,
    pub dispatch_until: DecimalU64,
    pub completion_until: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeModelAccountFloorV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub edge: NativeModelReservedEdgeV2,
    pub query_nonce: String,
    pub observed_at: DecimalU64,
    pub expires_at: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeModelAccountFloorV2 {
    pub fact: NativeModelAccountFloorV2,
    pub signature_hex: String,
}
fn invalid() -> String {
    "invalid Native model reservation".into()
}
fn hex32(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl NativeModelAccountFloorV2 {
    pub fn validate(&self) -> Result<(), String> {
        let e = &self.edge;
        if self.api_version != 2
            || e.api_version != 2
            || ![
                &e.approved_input_digest,
                &e.budget_digest,
                &e.graph_digest,
                &e.provider_request_digest,
                &e.price_policy_digest,
                &e.connection_ref_digest,
                &e.native_instance_public_key,
            ]
            .iter()
            .all(|v| hex32(v))
            || [
                &e.custody_account_id,
                &e.session_id,
                &e.reservation_id,
                &e.edge_id,
                &e.native_account_id,
                &e.preset_id,
                &e.model,
            ]
            .iter()
            .any(|v| v.is_empty() || v.len() > 256)
            || match e.call_kind {
                NativeModelCallKindV2::ProbeFirst => e.ordinal.0 != 1,
                NativeModelCallKindV2::ProbeSecond => e.ordinal.0 != 2,
                NativeModelCallKindV2::Inference => e.ordinal.0 < 3,
            }
            || e.ordinal.0 == 0
            || e.floor.0 == 0
            || e.preset_revision.0 == 0
            || e.max_tokens.0 == 0
            || e.max_input_tokens.0.checked_add(e.max_output_tokens.0) != Some(e.max_tokens.0)
            || !crate::http_pop::nonce_valid(&self.query_nonce)
            || self.observed_at.0 >= self.expires_at.0
            || self.expires_at.0 > self.observed_at.0.saturating_add(5000)
            || self.expires_at.0 > e.dispatch_until.0
            || e.dispatch_until.0 > e.completion_until.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        crate::native_cleanup_v2::bytes("oasis7.custody.native-model-account-floor.v2", self)
    }
}
impl SignedNativeModelAccountFloorV2 {
    pub fn verify(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        self.fact.validate()?;
        self.fact.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::NativeModelReservationV2,
            self.fact.observed_at.0,
            self.fact.expires_at.0,
            now,
        )?;
        crate::native_cleanup_v2::verify(
            "oasis7.custody.native-model-account-floor.v2",
            &self.fact,
            &self.signature_hex,
            &pin.public_key,
        )
    }
}
/// Only the actual Account reservation transaction may create this token.
/// It is deliberately neither Clone, Debug, Serialize nor Deserialize.
pub struct PreparedNativeModelFloorV2 {
    pub(crate) fact: NativeModelAccountFloorV2,
    pub(crate) pin: AccountMachineIssuerPinV2,
}
impl PreparedNativeModelFloorV2 {
    pub fn issuer(&self) -> &AccountMachineIssuerV2 {
        &self.fact.issuer
    }
    pub fn issuer_pin(&self) -> &AccountMachineIssuerPinV2 {
        &self.pin
    }
    pub fn validity(&self) -> (u64, u64) {
        (self.fact.observed_at.0, self.fact.expires_at.0)
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.fact.signing_bytes()
    }
    pub fn finish(self, signature_hex: String) -> SignedNativeModelAccountFloorV2 {
        SignedNativeModelAccountFloorV2 {
            fact: self.fact,
            signature_hex,
        }
    }
}

/// Actual Native TLS token observation. It is not a vendor money/invoice claim,
/// a Current admission, or permission to advance Cognition/World.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedProviderUsageV2 {
    pub api_version: u8,
    pub approved_input_digest: String,
    pub budget_digest: String,
    pub reservation_id: String,
    pub edge_id: String,
    pub call_kind: NativeModelCallKindV2,
    pub ordinal: DecimalU64,
    pub floor: DecimalU64,
    pub native_account_id: String,
    pub native_instance_public_key: String,
    pub preset_id: String,
    pub preset_revision: DecimalU64,
    pub model: String,
    pub price_policy_digest: String,
    pub input_tokens: DecimalU64,
    pub output_tokens: DecimalU64,
    pub total_tokens: DecimalU64,
    pub observed_at: DecimalU64,
    pub protocol_result: NativeModelProtocolResultV2,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAuthenticatedProviderUsageV2 {
    pub usage: AuthenticatedProviderUsageV2,
    pub signature_hex: String,
}
impl AuthenticatedProviderUsageV2 {
    pub fn validate_edge(&self, edge: &NativeModelReservedEdgeV2) -> Result<(), String> {
        if self.api_version != 2
            || self.approved_input_digest != edge.approved_input_digest
            || self.budget_digest != edge.budget_digest
            || self.reservation_id != edge.reservation_id
            || self.edge_id != edge.edge_id
            || self.call_kind != edge.call_kind
            || !matches!(
                (self.call_kind, self.protocol_result),
                (
                    NativeModelCallKindV2::ProbeFirst,
                    NativeModelProtocolResultV2::FirstProbeEcho
                        | NativeModelProtocolResultV2::ProbeRejected
                ) | (
                    NativeModelCallKindV2::ProbeSecond,
                    NativeModelProtocolResultV2::SecondProbeReady
                        | NativeModelProtocolResultV2::ProbeRejected
                ) | (
                    NativeModelCallKindV2::Inference,
                    NativeModelProtocolResultV2::InferenceComplete
                )
            )
            || self.ordinal != edge.ordinal
            || self.floor != edge.floor
            || self.native_account_id != edge.native_account_id
            || self.native_instance_public_key != edge.native_instance_public_key
            || self.preset_id != edge.preset_id
            || self.preset_revision != edge.preset_revision
            || self.model != edge.model
            || self.price_policy_digest != edge.price_policy_digest
            || self.input_tokens.0.checked_add(self.output_tokens.0) != Some(self.total_tokens.0)
            || self.input_tokens.0 > edge.max_input_tokens.0
            || self.output_tokens.0 > edge.max_output_tokens.0
            || self.total_tokens.0 > edge.max_tokens.0
            || self.observed_at.0 == 0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        crate::native_cleanup_v2::bytes("oasis7.custody.native-provider-token-usage.v2", self)
    }
}
impl SignedAuthenticatedProviderUsageV2 {
    pub fn verify_edge(&self, edge: &NativeModelReservedEdgeV2) -> Result<(), String> {
        self.usage.validate_edge(edge)?;
        crate::native_cleanup_v2::verify(
            "oasis7.custody.native-provider-token-usage.v2",
            &self.usage,
            &self.signature_hex,
            &edge.native_instance_public_key,
        )
    }
}

/// Same canonical price-policy commitment as the Native signed basis; no quote
/// or vendor-money qualification is created by hashing operator configuration.
pub fn native_model_price_policy_digest(
    policy: &crate::provisioning::ModelPricePolicyV1,
) -> Result<String, String> {
    let bytes = crate::native_cleanup_v2::bytes("oasis7.custody.model-price-policy.v1", policy)?;
    Ok(crate::approval::bytes_digest(&bytes))
}

#[cfg(test)]
#[path = "native_model_proxy_v2_tests.rs"]
mod tests;
