//! A separate purpose/domain over the actual Core fixed World2 allocation only.
use super::*;
use oasis7_custody_core::provisioning::PreparedNativePreflightAccessV2;
impl AccountServiceIssuer {
    pub(crate) fn native_preflight_access_pin(&self) -> Result<&AccountMachineIssuerPinV2> {
        self.pins
            .iter()
            .find(|p| p.allowed_purpose == AccountMachinePurposeV2::NativePreflightAccessV2)
            .ok_or_else(denied)
    }
    pub(crate) fn native_preflight_access_floor(&self) -> AccountMachineIssuerFloorV2 {
        AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        }
    }
    pub(crate) fn sign_native_preflight_access(
        &self,
        prepared: &PreparedNativePreflightAccessV2,
        now: u64,
    ) -> Result<String> {
        if prepared.issuer() != self.issuer()
            || prepared.issuer_pin() != self.native_preflight_access_pin()?
        {
            return Err(denied());
        }
        let (issued, expires) = prepared.validity();
        self.validate_lifetime(
            AccountMachinePurposeV2::NativePreflightAccessV2,
            issued,
            expires,
            now,
        )?;
        let bytes = prepared.signing_bytes().map_err(|_| denied())?;
        Ok(hex::encode(self.sign_bytes(&bytes)))
    }
}
