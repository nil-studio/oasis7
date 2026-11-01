//! Only SQL-produced typed authorities reach the unique existing Account seed.
use super::*;
use crate::owner_provision::native::test_session::ExecutionPrepareObservationV2;
use oasis7_custody_core::{
    execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2,
    provisioning::{
        AccountProvisionApprovalState, ExecutionNativeGatedDispatchV2,
        ExecutionNativeLeafSigningAuthorityV2, ExecutionNativePrepareAuthorityV2,
    },
    time_units::UnixMillis,
};
impl AccountServiceIssuer {
    pub(crate) fn execution_native_gated_recorded(
        &self,
        state: &mut AccountProvisionApprovalState,
        id: &str,
        authority: ExecutionNativePrepareAuthorityV2,
        observed: &ExecutionPrepareObservationV2,
        now: UnixMillis,
    ) -> Result<ExecutionNativeGatedDispatchV2> {
        let (proof, pin, floor) =
            self.execution_native_prepare_proof(id, &authority, observed, now)?;
        state
            .record_test_session_v2_native_prepare_gated(id, authority, proof, pin, &floor, now)
            .map_err(|_| denied())
    }
    /// The claim already linearized in a preceding committed Account SQL.
    /// This completes only that claim, including a late/F-A historical result;
    /// it cannot claim another leaf or mint a new Current observation.
    pub(crate) fn execution_leaf_receipt_recorded(
        &self,
        state: &mut AccountProvisionApprovalState,
        id: &str,
        authority: ExecutionNativeLeafSigningAuthorityV2,
        now: UnixMillis,
    ) -> Result<SignedExecutionDispatchClaimV2> {
        let c = authority.claim();
        if c.issuer != self.issuer || c.session_id != id {
            return Err(denied());
        }
        self.validate_lifetime(
            AccountMachinePurposeV2::ExecutionDispatchClaimV2,
            c.issued_at.0,
            c.expires_at.0,
            now.get(),
        )?;
        let bytes = Zeroizing::new(authority.signing_bytes().map_err(|_| denied())?);
        let signature = hex::encode(self.sign_bytes(&bytes));
        state
            .record_test_session_v2_native_leaf_signature(id, authority, signature, now)
            .map_err(|_| denied())
    }
}
