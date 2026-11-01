//! Executor reads public fixed deployment pins only, never Account seed/KEK.
use super::*;
use oasis7_custody_core::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachinePurposeV2,
};
use oasis7_custody_core::provisioning::{
    ExecutionPrepareRecipientV2, ExecutionSigningBundleV2, SignedAccountExecutionPreparePermitV2,
    VerifiedExecutionPreparePermitV2,
};
use worker::Env;
pub(crate) struct ExecutionIssuerConfigurationV2 {
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    environment: String,
}
impl ExecutionIssuerConfigurationV2 {
    pub(crate) fn load(env: &Env) -> Result<Self> {
        let value = |key| env.var(key).map(|v| v.to_string()).map_err(|_| rejected());
        if COMPILED_ROLE != Role::Executor || value("CUSTODY_TEST_SESSION_V2_ENABLED")? != "true" {
            return Err(rejected());
        }
        let pins: Vec<AccountMachineIssuerPinV2> =
            serde_json::from_str(&value("CUSTODY_ACCOUNT_SERVICE_ISSUER_PINS")?)
                .map_err(|_| rejected())?;
        let floor: AccountMachineIssuerFloorV2 =
            serde_json::from_str(&value("CUSTODY_ACCOUNT_SERVICE_ISSUER_FLOOR")?)
                .map_err(|_| rejected())?;
        if pins.is_empty()
            || pins.len() > 16
            || floor.issuer_epoch.0 == 0
            || floor.config_generation.0 == 0
        {
            return Err(rejected());
        }
        let mut selected = pins
            .into_iter()
            .filter(|p| p.allowed_purpose == AccountMachinePurposeV2::ExecutionPrepareSignerV2);
        let pin = selected.next().ok_or_else(rejected)?;
        if selected.next().is_some() {
            return Err(rejected());
        }
        let environment = value("CUSTODY_ROLE_KEY_ENVIRONMENT")?;
        if environment.is_empty()
            || environment.len() > 128
            || environment.chars().any(char::is_control)
        {
            return Err(rejected());
        }
        Ok(Self {
            pin,
            floor,
            environment,
        })
    }
    pub(crate) fn verify(
        &self,
        proof: SignedAccountExecutionPreparePermitV2,
        bundle: ExecutionSigningBundleV2,
        recipient: ExecutionPrepareRecipientV2<'_>,
        at: u64,
    ) -> Result<VerifiedExecutionPreparePermitV2> {
        if recipient.scope.context.identity.environment != self.environment
            || recipient.scope.context.identity.role != Role::Executor
        {
            return Err(rejected());
        }
        proof
            .verify_owned(
                &self.pin,
                &self.floor,
                bundle,
                recipient,
                oasis7_custody_core::time_units::UnixMillis::new(at),
            )
            .map_err(|_| rejected())
    }
}
