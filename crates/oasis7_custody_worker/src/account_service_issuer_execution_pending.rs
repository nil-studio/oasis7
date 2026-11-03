//! Only the first Core-owned claimed Directory handle reaches the one seed.
use super::*;
use oasis7_custody_core::{
    provisioning::{
        AccountProvisionApprovalState, ExecutionPendingDirectoryDispatchV2,
        PendingDirectoryAttemptV2,
    },
    time_units::UnixMillis,
};
impl AccountServiceIssuer {
    pub(crate) fn pending_directory_recorded_v2(
        &self,
        state: &mut AccountProvisionApprovalState,
        attempt: PendingDirectoryAttemptV2,
        now: UnixMillis,
    ) -> Result<Option<ExecutionPendingDirectoryDispatchV2>> {
        let authority = state
            .pending_directory_signing_authority_v2(attempt, self.issuer.clone())
            .map_err(|_| denied())?;
        let c = authority.claim();
        self.validate_lifetime(
            AccountMachinePurposeV2::ExecutionPendingCurrentLeafV2,
            c.issued_at.0,
            c.expires_at.0,
            now.get(),
        )?;
        let pin = self
            .pins
            .iter()
            .find(|p| p.allowed_purpose == AccountMachinePurposeV2::ExecutionPendingCurrentLeafV2)
            .ok_or_else(denied)?
            .clone();
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        };
        let raw = Zeroizing::new(authority.signing_bytes().map_err(|_| denied())?);
        let signature = hex::encode(self.sign_bytes(&raw));
        state
            .record_pending_directory_signature_v2(authority, signature, pin.clone(), floor, now)
            .map_err(|_| denied())
    }
}
