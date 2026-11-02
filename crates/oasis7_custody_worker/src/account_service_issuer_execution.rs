//! Fixed child of the single Account issuer; no arbitrary bytes/HTTP entry.
use super::*;
use crate::owner_provision::native::test_session::ExecutionPrepareObservationV2;
use oasis7_custody_core::{
    provisioning::{
        ExecutionPrepareDispatchV2, ExecutionPrepareSigningAuthorityV2,
        SignedAccountExecutionPreparePermitV2, SignedExecutionPreparePermitsV2,
    },
    time_units::UnixMillis,
};
impl AccountServiceIssuer {
    /// The real Account SQL closure owns the only opaque input. Persist the
    /// exact returned proof in that same transaction before any RKV RPC. A lost
    /// reply recovers that original; it must not reissue/retimestamp the grant.
    fn execution_prepare(
        &self,
        authority: ExecutionPrepareSigningAuthorityV2,
        observed: &ExecutionPrepareObservationV2,
        now: UnixMillis,
    ) -> Result<SignedExecutionPreparePermitsV2> {
        observed.check_authority(&authority, now)?;
        let permits = authority
            .into_service_permits(
                self.issuer.clone(),
                observed.directory_observed_at(),
                observed.fence_started(),
                observed.until(),
            )
            .map_err(|_| denied())?;
        let sign_one = |permit: oasis7_custody_core::provisioning::AccountExecutionPreparePermitV2| -> Result<SignedAccountExecutionPreparePermitV2> {
            self.validate_lifetime(AccountMachinePurposeV2::ExecutionPrepareSignerV2,
                permit.issued_at.get(), permit.expires_at.get(), now.get())?;
            let bytes = Zeroizing::new(permit.signing_bytes().map_err(|_| denied())?);
            Ok(SignedAccountExecutionPreparePermitV2 { permit, signature_hex: hex::encode(self.sign_bytes(&bytes)) })
        };
        Ok(SignedExecutionPreparePermitsV2 {
            initialize: sign_one(permits.initialize)?,
            sign: sign_one(permits.sign)?,
        })
    }
}

impl AccountServiceIssuer {
    pub(crate) fn execution_prepare_recorded(
        &self,
        state: &mut oasis7_custody_core::provisioning::AccountProvisionApprovalState,
        session_id: &str,
        authority: ExecutionPrepareSigningAuthorityV2,
        observed: &ExecutionPrepareObservationV2,
        now: UnixMillis,
    ) -> Result<Option<ExecutionPrepareDispatchV2>> {
        let permits = self.execution_prepare(authority, observed, now)?;
        let pin = self
            .pins
            .iter()
            .find(|p| p.allowed_purpose == AccountMachinePurposeV2::ExecutionPrepareSignerV2)
            .ok_or_else(denied)?;
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        };
        state
            .record_test_session_v2_prepare_service_permits(
                session_id,
                permits.clone(),
                pin,
                &floor,
                now,
            )
            .map_err(|_| denied())
    }
}
