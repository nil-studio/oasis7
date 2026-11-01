//! Two actual Native receipts enter the existing GetPermit callback, not a new RPC.
use super::*;
use crate::native_cleanup_v2::{
    NativePreflightWorldOperationV2, SignedNativePreflightWorldReceiptV2,
};
impl AccountProvisionApprovalState {
    pub fn test_session_v2_preflight_world_access_checks(
        &self,
        id: &str,
        access: &OwnerSetupAccessEvidenceV1,
        receipts: &[SignedNativePreflightWorldReceiptV2; 2],
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_session_v2_record(id)?;
        let w = s.preflight_world_access.as_ref().ok_or_else(invalid)?;
        if w.admitted_at.is_some() {
            if current.oracle_ticket != 4
                || current.source_ticket != 5
                || s.preflight_access.as_ref() != Some(access)
                || w.receipts[0].as_ref() != Some(&receipts[0])
                || w.receipts[1].as_ref() != Some(&receipts[1])
            {
                return Err(invalid());
            }
            // The existing withKey/beforeSend callback reuses exact historical
            // facts but still performs its separately paid current ticket4.
            w.verify_native_history(s, current.native_scope)?;
            return self.test_session_v2_preflight_access_checks(id, access, current, now);
        }
        if current.oracle_ticket != 3 || current.source_ticket != 4 {
            return Err(invalid());
        }
        let proof = w.signed()?;
        for (index, operation, result) in [
            (
                0,
                NativePreflightWorldOperationV2::PolicyOriginal,
                digest(&access.policy)?,
            ),
            (
                1,
                NativePreflightWorldOperationV2::AuthorizePolicy,
                digest(&access.admission)?,
            ),
        ] {
            if receipts[index].receipt.operation != operation
                || receipts[index].receipt.completed_at.0 > now.get()
            {
                return Err(invalid());
            }
            receipts[index]
                .verify_native(
                    &proof,
                    &result,
                    current.native_scope.native_root_key_id,
                    current.native_scope.native_root_public_key,
                )
                .map_err(|_| invalid())?;
        }
        // These fields are staged only in this owned clone to compose the old
        // exact access checks. They are not published or deserializable authority.
        let mut staged = self.clone();
        let s = staged.test_session_v2_record_mut(id)?;
        s.preflight_access = Some(access.clone());
        let w = s.preflight_world_access.as_mut().ok_or_else(invalid)?;
        w.receipts = [Some(receipts[0].clone()), Some(receipts[1].clone())];
        w.admitted_at = Some(now);
        staged.test_session_v2_preflight_access_checks(id, access, current, now)
    }
    pub fn record_test_session_v2_preflight_world_access(
        &mut self,
        id: &str,
        access: OwnerSetupAccessEvidenceV1,
        receipts: [SignedNativePreflightWorldReceiptV2; 2],
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.test_session_v2_preflight_world_access_checks(
            id, &access, &receipts, current, now,
        )?)?;
        if self
            .test_session_v2_record(id)?
            .preflight_world_access
            .as_ref()
            .is_some_and(|w| w.admitted())
        {
            return Ok(());
        }
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let w = s.preflight_world_access.as_mut().ok_or_else(invalid)?;
        w.receipts = [Some(receipts[0].clone()), Some(receipts[1].clone())];
        w.admitted_at = Some(now);
        s.preflight_access = Some(access);
        append_session_event_v2(s, SessionEventKindV2::PreflightWorldAccessAdmitted, now)?;
        append_session_event_v2(s, SessionEventKindV2::PreflightAccessRecorded, now)?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
