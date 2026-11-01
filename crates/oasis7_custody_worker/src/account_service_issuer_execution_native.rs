//! A fixed typed child of the existing unique Account issuer, not a raw signer.
use super::*;
use crate::owner_provision::native::test_session::ExecutionPrepareObservationV2;
use oasis7_custody_core::{
    native_execution_prepare_v2::SignedAccountNativeExecutionPrepareV2,
    provisioning::{
        AccountProvisionApprovalState, ExecutionNativePrepareAuthorityV2,
        ExecutionNativePrepareDispatchV2,
    },
    time_units::UnixMillis,
};
impl AccountServiceIssuer {
    pub(crate) fn execution_native_prepare_recorded(
        &self,
        state: &mut AccountProvisionApprovalState,
        id: &str,
        authority: ExecutionNativePrepareAuthorityV2,
        observed: &ExecutionPrepareObservationV2,
        now: UnixMillis,
    ) -> Result<ExecutionNativePrepareDispatchV2> {
        observed.check_native_prepare_authority(&authority, now)?;
        let c = authority.certificate();
        if c.issuer != self.issuer || c.session_id != id {
            return Err(denied());
        }
        self.validate_lifetime(
            AccountMachinePurposeV2::NativeExecutionPrepareV2,
            c.issued_at.0,
            c.expires_at.0,
            now.get(),
        )?;
        let bytes = Zeroizing::new(c.signing_bytes().map_err(|_| denied())?);
        let proof = SignedAccountNativeExecutionPrepareV2 {
            certificate: c.clone(),
            signature_hex: hex::encode(self.sign_bytes(&bytes)),
        };
        let pin = self
            .pins
            .iter()
            .find(|p| p.allowed_purpose == AccountMachinePurposeV2::NativeExecutionPrepareV2)
            .ok_or_else(denied)?;
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        };
        // The store caller must return this token only after the containing
        // real SQL transaction commits. An error cannot leak a dispatch handle.
        state
            .record_test_session_v2_native_prepare(id, authority, proof, pin, &floor, now)
            .map_err(|_| denied())
    }
    pub(crate) fn execution_private_current_recorded(
        &self,
        state: &mut AccountProvisionApprovalState,
        id: &str,
        token: oasis7_custody_core::provisioning::ExecutionPrivateCurrentPortsV2,
        observed: &ExecutionPrepareObservationV2,
        now: UnixMillis,
    ) -> Result<oasis7_custody_core::provisioning::ExecutionPrivateCurrentPortsV2> {
        use oasis7_custody_core::execution_private_current_v2::SignedAccountExecutionPrivateCurrentV2;
        let certificate = token
            .certificate(self.issuer.clone())
            .map_err(|_| denied())?;
        observed.check_private_current_certificate(&certificate, now)?;
        if certificate.session_id != id {
            return Err(denied());
        }
        self.validate_lifetime(
            AccountMachinePurposeV2::ExecutionPrivateCurrentV2,
            certificate.issued_at.0,
            certificate.expires_at.0,
            now.get(),
        )?;
        let bytes = Zeroizing::new(certificate.signing_bytes().map_err(|_| denied())?);
        let proof = SignedAccountExecutionPrivateCurrentV2 {
            certificate,
            signature_hex: hex::encode(self.sign_bytes(&bytes)),
        };
        let pin = self
            .pins
            .iter()
            .find(|p| p.allowed_purpose == AccountMachinePurposeV2::ExecutionPrivateCurrentV2)
            .ok_or_else(denied)?;
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        };
        // The real store transaction must commit before this first-only token
        // is returned to the service sender. A failed commit grants no handle.
        state
            .record_test_session_v2_prepare_private_proof(id, token, proof, pin, &floor, now)
            .map_err(|_| denied())
    }
    pub(crate) fn execution_native_issuer(&self) -> AccountMachineIssuerV2 {
        self.issuer.clone()
    }
}
