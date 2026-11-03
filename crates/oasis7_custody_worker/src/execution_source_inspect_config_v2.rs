//! Identity-child private configuration; only public fields enter durable state.
use super::*;
use oasis7_custody_core::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Server {
    pub schema: u8,
    pub capacity: u32,
    pub service_epoch: DecimalU64,
    pub native_origin: String,
    pub native_namespace: String,
    pub native_audience: String,
    pub native_epoch: DecimalU64,
    pub native_key_id: String,
    pub native_public_key: String,
    pub dispatch_pin: AccountMachineIssuerPinV2,
    pub leaf_pin: AccountMachineIssuerPinV2,
    pub floor: AccountMachineIssuerFloorV2,
}
impl Server {
    pub(super) fn load(env: &Env, native: &NativeIdentityConfig) -> Result<Self> {
        checked(
            env.var("CUSTODY_EXECUTION_PENDING_START_V2_ENABLED")?
                .to_string()
                == "true"
                && env
                    .var("CUSTODY_EXECUTION_SOURCE_INSPECT_V2_ENABLED")?
                    .to_string()
                    == "true",
        )?;
        let raw = env
            .var("CUSTODY_EXECUTION_SOURCE_INSPECT_ADMISSION_CAPACITY")?
            .to_string();
        checked(
            !raw.is_empty()
                && raw.len() <= 10
                && !raw.starts_with('0')
                && raw.bytes().all(|b| b.is_ascii_digit()),
        )?;
        let capacity = raw.parse::<u32>().map_err(|_| rejected())?;
        checked(capacity > 0 && capacity.to_string() == raw)?;
        let [dispatch_pin, leaf_pin] = crate::execution_source_inspect_pins_v2::load(env)?;
        let mut equivalent = dispatch_pin.clone();
        equivalent.allowed_purpose = leaf_pin.allowed_purpose;
        checked(
            equivalent == leaf_pin
                && dispatch_pin.environment == native.environment
                && dispatch_pin.native_audience_origin == native.origin,
        )?;
        let raw_epoch = env.var("CUSTODY_SERVICE_EPOCH")?.to_string();
        let service_epoch: DecimalU64 =
            serde_json::from_value(raw_epoch.into()).map_err(|_| rejected())?;
        checked(service_epoch.0 > 0)?;
        Ok(Self {
            schema: 2,
            capacity,
            service_epoch,
            native_origin: native.origin.clone(),
            native_namespace: native.namespace.clone(),
            native_audience: native.audience.clone(),
            native_epoch: native.epoch,
            native_key_id: native.key_id.clone(),
            native_public_key: native.public_key.clone(),
            floor: AccountMachineIssuerFloorV2 {
                issuer_epoch: dispatch_pin.issuer_epoch,
                config_generation: dispatch_pin.config_generation,
            },
            dispatch_pin,
            leaf_pin,
        })
    }
    pub(super) fn issuer(&self) -> AccountMachineIssuerV2 {
        let p = &self.dispatch_pin;
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
}
