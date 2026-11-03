//! Fixed public Source pin pair only. No Native adapter secret or Account seed.
use oasis7_custody_core::native_cleanup_v2::*;
use worker::{Env, Result};
fn denied() -> worker::Error {
    worker::Error::RustError("source pins unavailable".into())
}
pub(crate) fn load(env: &Env) -> Result<[AccountMachineIssuerPinV2; 2]> {
    if env
        .var("CUSTODY_EXECUTION_SOURCE_INSPECT_V2_ENABLED")
        .ok()
        .is_none_or(|v| v.to_string() != "true")
    {
        return Err(denied());
    }
    let raw = env
        .var("CUSTODY_EXECUTION_SOURCE_INSPECT_ISSUER_PINS")
        .map_err(|_| denied())?
        .to_string();
    if raw.len() > 16384 {
        return Err(denied());
    }
    let pair: [AccountMachineIssuerPinV2; 2] = serde_json::from_str(&raw).map_err(|_| denied())?;
    let p = &pair[0];
    let issuer = AccountMachineIssuerV2 {
        namespace: p.namespace.clone(),
        service_id: p.service_id.clone(),
        environment: p.environment.clone(),
        key_id: p.key_id.clone(),
        issuer_epoch: p.issuer_epoch,
        config_generation: p.config_generation,
        native_audience_origin: p.native_audience_origin.clone(),
    };
    let floor = AccountMachineIssuerFloorV2 {
        issuer_epoch: p.issuer_epoch,
        config_generation: p.config_generation,
    };
    for (pin, purpose) in pair.iter().zip([
        AccountMachinePurposeV2::ExecutionPendingSourceInspectDispatchV2,
        AccountMachinePurposeV2::ExecutionPendingSourceNativeLeafV2,
    ]) {
        if pin.public_key != p.public_key
            || pin.not_before != p.not_before
            || pin.not_after != p.not_after
        {
            return Err(denied());
        }
        issuer
            .verify_pin(
                pin,
                &floor,
                purpose,
                pin.not_before.0,
                pin.not_after.0,
                pin.not_before.0,
            )
            .map_err(|_| denied())?;
        let bytes: [u8; 32] = hex::decode(&pin.public_key)
            .map_err(|_| denied())?
            .try_into()
            .map_err(|_| denied())?;
        if hex::encode(bytes) != pin.public_key
            || ed25519_dalek::VerifyingKey::from_bytes(&bytes)
                .map_err(|_| denied())?
                .is_weak()
        {
            return Err(denied());
        }
    }
    Ok(pair)
}
