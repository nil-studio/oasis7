//! Fixed mounted caller checks; metadata is never a fresh Current observation.
use super::*;
impl AccountProvisionApprovalState {
    /// Fixed Identity service reentry must present the exact last committed
    /// receipt. Stored metadata cannot reconstruct the first dispatch handle.
    pub fn claim_test_session_v2_native_identity_receipted_leaf(
        &mut self,
        id: &str,
        evidence: (
            &NativeExecutionPrepareRequestV2,
            &SignedExecutionDispatchClaimV2,
        ),
        leaf_body: (ExecutionPreparePhysicalEdgeV2, &str),
        configured: &ExecutionDispatchIssueContextV2<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ExecutionNativeLeafSigningAuthorityV2> {
        let (request, previous) = evidence;
        previous
            .verify_issuer(
                request,
                configured.dispatch_pin,
                configured.dispatch_floor,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let s = self.test_session_v2_record(id)?;
        let record = s
            .execution_book
            .as_ref()
            .and_then(|b| b.native_prepare_record())
            .ok_or_else(invalid)?;
        let last = record
            .dispatch_gate
            .as_ref()
            .and_then(|g| g.claims.last())
            .ok_or_else(invalid)?;
        if record.proof != request.proof
            || self.test_session_v2_prepare_original_history(id)?.as_ref()
                != Some(request.original.as_ref())
            || last.claim != previous.claim
            || last.signature_hex.as_deref() != Some(previous.signature_hex.as_str())
        {
            return Err(invalid());
        }
        self.claim_test_session_v2_native_identity_leaf(
            id,
            leaf_body,
            configured,
            crate::http::Role::Identity,
            now,
            verifier,
        )
    }
    pub fn claim_test_session_v2_native_receipted_world_leaf(
        &mut self,
        id: &str,
        callback: NativeExecutionDispatchCallbackV2,
        configured: &ExecutionDispatchIssueContextV2<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ExecutionNativeLeafSigningAuthorityV2> {
        callback.wire_bytes().map_err(|_| invalid())?;
        callback
            .previous
            .verify_issuer(
                &callback.request,
                configured.dispatch_pin,
                configured.dispatch_floor,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let s = self.test_session_v2_record(id)?;
        let record = s
            .execution_book
            .as_ref()
            .and_then(|b| b.native_prepare_record())
            .ok_or_else(invalid)?;
        let last = record
            .dispatch_gate
            .as_ref()
            .and_then(|g| g.claims.last())
            .ok_or_else(invalid)?;
        if record.proof != callback.request.proof
            || self.test_session_v2_prepare_original_history(id)?.as_ref()
                != Some(callback.request.original.as_ref())
            || last.claim != callback.previous.claim
            || last.signature_hex.as_deref() != Some(callback.previous.signature_hex.as_str())
        {
            return Err(invalid());
        }
        self.claim_test_session_v2_native_world_leaf(id, callback.query, configured, now, verifier)
    }
}

impl AccountProvisionApprovalState {
    /// Deployment metadata comparison only; never substitutes Directory/RF or
    /// Native Current. Real receivers construct these inputs from fixed env.
    pub fn test_session_v2_dispatch_static_configuration(
        &self,
        id: &str,
        context: &crate::approval::ApprovalContextV1,
        world: &crate::approval::ApprovedWorldV1,
        native_identity: (&str, crate::DecimalU64),
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        if &s.input.request.context != context
            || &s.header()?.1.world_identity != world
            || s.session.scope.identity_namespace != native_identity.0
            || s.session.scope.identity_instance_epoch != native_identity.1
            || context.security_version != self.provision.security_epoch()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
