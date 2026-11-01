//! The shared Account identity signs only an actual Core-produced Pending token.
//! No HTTP body, caller-selected domain or caller-selected purpose is accepted.
use super::*;
use oasis7_custody_core::provisioning::PreparedNativeMachinePendingV2;
impl AccountServiceIssuer {
    pub(crate) fn native_pending_pin(&self) -> Result<&AccountMachineIssuerPinV2> {
        self.pins
            .iter()
            .find(|pin| pin.allowed_purpose == AccountMachinePurposeV2::ConsumedPending)
            .ok_or_else(denied)
    }
    pub(crate) fn native_pending_floor(&self) -> AccountMachineIssuerFloorV2 {
        AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        }
    }
    pub(crate) fn sign_native_pending(
        &self,
        prepared: &PreparedNativeMachinePendingV2,
        now: u64,
    ) -> Result<String> {
        if prepared.issuer() != self.issuer()
            || prepared.issuer_pin() != self.native_pending_pin()?
        {
            return Err(denied());
        }
        let (issued, expires) = prepared.validity();
        self.validate_lifetime(
            AccountMachinePurposeV2::ConsumedPending,
            issued,
            expires,
            now,
        )?;
        let bytes = prepared.signing_bytes().map_err(|_| denied())?;
        Ok(hex::encode(self.sign_bytes(&bytes)))
    }
}

impl AccountServiceIssuer {
    pub(crate) fn native_active_pin(&self) -> Result<&AccountMachineIssuerPinV2> {
        self.pins
            .iter()
            .find(|p| p.allowed_purpose == AccountMachinePurposeV2::ActiveThreeRoots)
            .ok_or_else(denied)
    }
    pub(crate) fn sign_native_active(
        &self,
        prepared: &oasis7_custody_core::provisioning::PreparedNativeMachineActiveV2,
        now: u64,
    ) -> Result<String> {
        if prepared.issuer() != self.issuer()
            || prepared.issuer_pin() != self.native_active_pin()?
        {
            return Err(denied());
        }
        let (issued, expires) = prepared.validity();
        self.validate_lifetime(
            AccountMachinePurposeV2::ActiveThreeRoots,
            issued,
            expires,
            now,
        )?;
        let bytes = prepared.signing_bytes().map_err(|_| denied())?;
        Ok(hex::encode(self.sign_bytes(&bytes)))
    }
}
