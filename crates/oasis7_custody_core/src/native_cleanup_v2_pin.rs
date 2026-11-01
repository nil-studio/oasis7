use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountMachinePurposeV2 {
    ConsumedPending,
    ActiveThreeRoots,
    NativeFinalCurrent,
    NativeCurrentAttempt,
    NativeTermCurrentAttempt,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountMachineIssuerPinV2 {
    pub namespace: String,
    pub service_id: String,
    pub environment: String,
    pub key_id: String,
    pub public_key: String,
    pub issuer_epoch: DecimalU64,
    pub config_generation: DecimalU64,
    pub not_before: DecimalU64,
    pub not_after: DecimalU64,
    pub native_audience_origin: String,
    pub allowed_purpose: AccountMachinePurposeV2,
}
/// These numbers are not themselves an attestation of protected storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountMachineIssuerFloorV2 {
    pub issuer_epoch: DecimalU64,
    pub config_generation: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountMachineIssuerV2 {
    pub namespace: String,
    pub service_id: String,
    pub environment: String,
    pub key_id: String,
    pub issuer_epoch: DecimalU64,
    pub config_generation: DecimalU64,
    pub native_audience_origin: String,
}
impl AccountMachineIssuerV2 {
    pub fn verify_pin(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        purpose: AccountMachinePurposeV2,
        issued: u64,
        expires: u64,
        now: u64,
    ) -> Result<(), String> {
        if ![
            &self.namespace,
            &self.service_id,
            &self.environment,
            &self.key_id,
        ]
        .iter()
        .all(|s| id(s))
            || self.issuer_epoch.0 == 0
            || self.config_generation.0 == 0
            || self.namespace != pin.namespace
            || self.service_id != pin.service_id
            || self.environment != pin.environment
            || self.key_id != pin.key_id
            || self.issuer_epoch != pin.issuer_epoch
            || self.config_generation != pin.config_generation
            || self.issuer_epoch.0 < floor.issuer_epoch.0
            || self.config_generation.0 < floor.config_generation.0
            || self.native_audience_origin != pin.native_audience_origin
            || pin.allowed_purpose != purpose
            || pin.not_before.0 > issued
            || pin.not_after.0 < expires
            || issued > now
            || now >= expires
            || expires <= issued
            || crate::http_pop::CanonicalHttpTarget::new(
                &self.native_audience_origin,
                crate::http_pop::HttpPopRoute::ProofChallenges,
            )
            .is_err()
        {
            return Err(invalid());
        }
        decode::<32>(&pin.public_key)?;
        Ok(())
    }
}
