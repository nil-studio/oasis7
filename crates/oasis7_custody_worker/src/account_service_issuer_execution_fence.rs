//! Two closed physical RF purposes through the one Account seed. There is no
//! arbitrary message signer and no authority from IDs or stored history.
use super::*;
use oasis7_custody_core::{
    execution_fence_dispatch_v2::*,
    provisioning::{
        AccountProvisionApprovalState, ExecutionPendingDirectoryDispatchV2, PendingFenceDispatchV2,
    },
    time_units::UnixMillis,
};
impl AccountServiceIssuer {
    pub(crate) fn pending_fence_receiver_name_v2(
        &self,
        state: &AccountProvisionApprovalState,
        id: &str,
    ) -> Result<String> {
        state
            .pending_first_fence_receiver_name_v2(id, &self.issuer)
            .map_err(|_| denied())
    }

    pub(crate) fn pending_fence_dispatch_v2(
        &self,
        state: &mut AccountProvisionApprovalState,
        directory: ExecutionPendingDirectoryDispatchV2,
        object: String,
        fence_object: String,
        now: UnixMillis,
    ) -> Result<Option<PendingFenceDispatchV2>> {
        let authority = state
            .pending_first_fence_dispatch_authority_v2(
                directory,
                self.issuer.clone(),
                object,
                fence_object,
                now,
            )
            .map_err(|_| denied())?;
        let b = authority.binding();
        self.validate_lifetime(
            AccountMachinePurposeV2::ExecutionFenceDispatchV2,
            b.issued_at.0,
            b.expires_at.0,
            now.get(),
        )?;
        let pin = self
            .pins
            .iter()
            .find(|p| p.allowed_purpose == AccountMachinePurposeV2::ExecutionFenceDispatchV2)
            .ok_or_else(denied)?
            .clone();
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        };
        let bytes = Zeroizing::new(authority.signing_bytes().map_err(|_| denied())?);
        let signature = hex::encode(self.sign_bytes(&bytes));
        state
            .record_pending_first_fence_dispatch_v2(authority, signature, pin, floor, now)
            .map_err(|_| denied())
    }
    pub(crate) fn pending_fence_read_v2(
        &self,
        state: &mut AccountProvisionApprovalState,
        proof: &SignedExecutionFenceDispatchV2,
        dispatch_pin: &AccountMachineIssuerPinV2,
        dispatch_floor: &AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<SignedExecutionFenceReadV2> {
        if proof.binding.issuer != self.issuer {
            return Err(denied());
        }
        let authority = state
            .pending_first_fence_read_authority_v2(proof, dispatch_pin, dispatch_floor, now)
            .map_err(|_| denied())?;
        let c = authority.claim();
        self.validate_lifetime(
            AccountMachinePurposeV2::ExecutionFenceReadV2,
            c.claimed_at.0,
            c.binding.expires_at.0,
            now.get(),
        )?;
        let pin = self
            .pins
            .iter()
            .find(|p| p.allowed_purpose == AccountMachinePurposeV2::ExecutionFenceReadV2)
            .ok_or_else(denied)?
            .clone();
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        };
        let bytes = Zeroizing::new(authority.signing_bytes().map_err(|_| denied())?);
        let signature = hex::encode(self.sign_bytes(&bytes));
        state
            .record_pending_first_fence_read_v2(authority, signature, pin, floor, now)
            .map_err(|_| denied())
    }
}
