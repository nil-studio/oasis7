//! Public fixed-purpose Resource configuration. No Owner or service seed.
use super::*;
use oasis7_custody_core::{
    DecimalU64,
    approval::ApprovedWorldV1,
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
        AccountMachinePurposeV2,
    },
    provisioning::{PaidResourceReadIssuerPinV1, PaidResourceReadStageV1},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceFinalConfigV2 {
    pub(super) issuer_pin: AccountMachineIssuerPinV2,
    pub(super) issuer_floor: AccountMachineIssuerFloorV2,
    pub(super) resource_feature_generation: DecimalU64,
}
fn unavailable() -> StoreError {
    reject("resource_final_unavailable")
}
impl ResourceFinalConfigV2 {
    pub(super) fn from_env() -> Result<Self, StoreError> {
        if std::env::var("OASIS7_CUSTODY_OWNER_RESOURCE_FINAL_V2_ENABLED")
            .ok()
            .as_deref()
            != Some("true")
        {
            return Err(unavailable());
        }
        let raw = std::env::var("OASIS7_CUSTODY_OWNER_RESOURCE_FINAL_V2_CONFIG_JSON")
            .map_err(|_| unavailable())?;
        if raw.len() > 65536 {
            return Err(unavailable());
        }
        let value: Self = serde_json::from_str(&raw).map_err(|_| unavailable())?;
        let p = &value.issuer_pin;
        value
            .issuer()
            .verify_pin(
                p,
                &value.issuer_floor,
                AccountMachinePurposeV2::OwnerSetupResourceFinalV2,
                p.not_before.0,
                p.not_after.0,
                p.not_before.0,
            )
            .map_err(|_| unavailable())?;
        let raw: [u8; 32] = hex::decode(&p.public_key)
            .map_err(|_| unavailable())?
            .try_into()
            .map_err(|_| unavailable())?;
        if value.resource_feature_generation.0 == 0
            || ed25519_dalek::VerifyingKey::from_bytes(&raw)
                .map_err(|_| unavailable())?
                .is_weak()
        {
            return Err(unavailable());
        }
        Ok(value)
    }
    pub(super) fn issuer(&self) -> AccountMachineIssuerV2 {
        let p = &self.issuer_pin;
        AccountMachineIssuerV2 {
            namespace: p.namespace.clone(),
            service_id: p.service_id.clone(),
            environment: p.environment.clone(),
            key_id: p.key_id.clone(),
            issuer_epoch: p.issuer_epoch,
            config_generation: p.config_generation,
            native_audience_origin: p.native_audience_origin.clone(),
        }
    }
    pub(super) fn paid_pin(&self, world: &ApprovedWorldV1) -> PaidResourceReadIssuerPinV1 {
        let p = &self.issuer_pin;
        PaidResourceReadIssuerPinV1 {
            namespace: p.namespace.clone(),
            service_id: p.service_id.clone(),
            environment: p.environment.clone(),
            key_id: p.key_id.clone(),
            public_key: p.public_key.clone(),
            issuer_epoch: p.issuer_epoch,
            issuer_config_generation: p.config_generation,
            not_before: p.not_before,
            not_after: p.not_after,
            native_audience_origin: p.native_audience_origin.clone(),
            world_audience: world.clone(),
            allowed_stage: PaidResourceReadStageV1::OwnerSetupFinal,
        }
    }
    pub(super) fn require_context(
        &self,
        config: &CustodyConfig,
        identity: &super::super::config::IdentityConfig,
    ) -> Result<(), StoreError> {
        if self.issuer_pin.environment != identity.namespace.environment_id
            || !identity
                .rp
                .allowed_origins
                .contains(&self.issuer_pin.native_audience_origin)
            || self.issuer_pin.public_key == hex::encode(config.key.verifying_key().to_bytes())
            || self.issuer_pin.key_id == config.key_id
            || self.issuer_pin.service_id == identity.namespace.identity_service_id
        {
            return Err(unavailable());
        }
        Ok(())
    }
    pub(super) fn fingerprint(&self, gateway: &str) -> Result<String, StoreError> {
        let bytes = serde_json::to_vec(&(
            "oasis7.custody.owner-resource-final-config.v2",
            self,
            gateway,
        ))
        .map_err(|_| unavailable())?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}
