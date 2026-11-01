//! Deployment pins for the new closed machine protocol. These public pins do
//! not establish an approved Book, a paid request or managed-SID authority.
use super::*;
use oasis7_custody_core::native_cleanup_v2::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MachineConfig {
    api_version: u8,
    pub(super) pending_pin: AccountMachineIssuerPinV2,
    pub(super) active_pin: AccountMachineIssuerPinV2,
    pub(super) current_pin: AccountMachineIssuerPinV2,
    pub(super) term_current_pin: AccountMachineIssuerPinV2,
    pub(super) issuer_floor: AccountMachineIssuerFloorV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preflight_access_pin: Option<AccountMachineIssuerPinV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_prepare_pin: Option<AccountMachineIssuerPinV2>,
    native_origin: String,
}
fn unavailable() -> StoreError {
    StoreError::Rejected("cleanup_machine_configuration_unavailable".into())
}
impl MachineConfig {
    pub(super) fn from_env() -> Result<Self, StoreError> {
        if std::env::var("OASIS7_CUSTODY_CLEANUP_MACHINE_V2_ENABLED")
            .ok()
            .as_deref()
            != Some("true")
        {
            return Err(unavailable());
        }
        let raw = std::env::var("OASIS7_CUSTODY_CLEANUP_ACCOUNT_ISSUER_CONFIG")
            .map_err(|_| unavailable())?;
        if raw.len() > 16_384 {
            return Err(unavailable());
        }
        let value: Self = serde_json::from_str(&raw).map_err(|_| unavailable())?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), StoreError> {
        use oasis7_custody_core::http_pop::{CanonicalHttpTarget, HttpPopRoute};
        if self.api_version != 2
            || CanonicalHttpTarget::new(&self.native_origin, HttpPopRoute::ProofChallenges).is_err()
        {
            return Err(unavailable());
        }
        let pins = [
            (&self.pending_pin, AccountMachinePurposeV2::ConsumedPending),
            (&self.active_pin, AccountMachinePurposeV2::ActiveThreeRoots),
            (
                &self.current_pin,
                AccountMachinePurposeV2::NativeCurrentAttempt,
            ),
            (
                &self.term_current_pin,
                AccountMachinePurposeV2::NativeTermCurrentAttempt,
            ),
        ];
        for (pin, purpose) in pins
            .into_iter()
            .chain(
                self.preflight_access_pin
                    .as_ref()
                    .map(|pin| (pin, AccountMachinePurposeV2::NativePreflightAccessV2)),
            )
            .chain(
                self.execution_prepare_pin
                    .as_ref()
                    .map(|pin| (pin, AccountMachinePurposeV2::NativeExecutionPrepareV2)),
            )
        {
            let identity = AccountMachineIssuerV2 {
                namespace: pin.namespace.clone(),
                service_id: pin.service_id.clone(),
                environment: pin.environment.clone(),
                key_id: pin.key_id.clone(),
                issuer_epoch: pin.issuer_epoch,
                config_generation: pin.config_generation,
                native_audience_origin: pin.native_audience_origin.clone(),
            };
            if pin.native_audience_origin != self.native_origin
                || pin.not_after.0 <= pin.not_before.0
            {
                return Err(unavailable());
            }
            identity
                .verify_pin(
                    pin,
                    &self.issuer_floor,
                    purpose,
                    pin.not_before.0,
                    pin.not_after.0,
                    pin.not_before.0,
                )
                .map_err(|_| unavailable())?;
            let key: [u8; 32] = hex::decode(&pin.public_key)
                .map_err(|_| unavailable())?
                .try_into()
                .map_err(|_| unavailable())?;
            if ed25519_dalek::VerifyingKey::from_bytes(&key)
                .map_err(|_| unavailable())?
                .is_weak()
            {
                return Err(unavailable());
            }
        }
        let p = &self.pending_pin;
        if [&self.active_pin, &self.current_pin, &self.term_current_pin]
            .into_iter()
            .chain(self.preflight_access_pin.iter())
            .chain(self.execution_prepare_pin.iter())
            .any(|other| {
                other.namespace != p.namespace
                    || other.service_id != p.service_id
                    || other.environment != p.environment
                    || other.key_id != p.key_id
                    || other.public_key != p.public_key
                    || other.issuer_epoch != p.issuer_epoch
                    || other.config_generation != p.config_generation
            })
        {
            return Err(unavailable());
        }
        Ok(())
    }
    pub(super) fn preflight_access_pin(&self) -> Result<&AccountMachineIssuerPinV2, StoreError> {
        self.validate()?;
        self.preflight_access_pin.as_ref().ok_or_else(unavailable)
    }
    pub(super) fn execution_prepare_pin(&self) -> Result<&AccountMachineIssuerPinV2, StoreError> {
        self.validate()?;
        self.execution_prepare_pin.as_ref().ok_or_else(unavailable)
    }
    pub(super) fn fingerprint(
        &self,
        custody: &CustodyConfig,
        identity: &super::super::config::IdentityConfig,
        world_configuration_digest: &str,
    ) -> Result<String, StoreError> {
        self.validate()?;
        if world_configuration_digest.len() != 64
            || !world_configuration_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(unavailable());
        }
        // The HMAC binding detects a changed SID hashing secret without exposing
        // that secret or accepting the old saved raw/HMAC SID as new authority.
        let sid_configuration = digest(
            &identity.session_key,
            "cleanup-session-configuration-v2",
            "fixed",
        );
        let bytes = serde_json::to_vec(&(
            self,
            &custody.key_id,
            &custody.audience,
            custody.full_session_v2_summary_contract_enabled,
            custody.model_budget_summary_contract_enabled,
            hex::encode(custody.key.verifying_key().to_bytes()),
            identity.namespace.key(),
            &identity.rp.rp_id,
            &identity.rp.allowed_origins,
            identity.rp.version,
            identity.rp.identity_instance_epoch,
            sid_configuration,
            world_configuration_digest,
        ))
        .map_err(|_| unavailable())?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}
