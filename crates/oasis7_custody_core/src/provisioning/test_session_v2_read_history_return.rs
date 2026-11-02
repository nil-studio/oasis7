//! Exact completed paid callback readback. No new Current, source ticket,
//! signing/dispatch token, outcome upgrade or query is created from history.
use super::*;
impl AccountProvisionApprovalState {
    pub fn test_session_v2_prepare_history_paid_reply(
        &self,
        id: &str,
        proof: SessionProofRequest<'_>,
        native: &NativeStageScope<'_>,
        now: UnixMillis,
    ) -> Result<ReadPrepareHistoryReplyV2> {
        let q = action_original_v2(&proof)?;
        let s = self.test_session_v2_local(id, SessionPurposeV1::ReconcileRead, now)?;
        authenticated_receipt_v2(s, SessionPurposeV1::ReconcileRead, proof.possession, now)?;
        execution_target_v2(self, s, SessionPurposeV1::ReconcileRead, proof.possession)?;
        let a = s
            .purpose_http_admissions
            .get(proof.ingress_attempt_id)
            .ok_or_else(invalid)?;
        let r = fee(s, proof.ingress_attempt_id)?;
        if a.purpose != SessionPurposeV1::ReconcileRead
            || a.hop != SessionControlHopV1::Host
            || a.signed_attempt_digest != proof.possession.transport_attempt_digest()
            || r.request != q
        {
            return Err(invalid());
        }
        if r.original_digest.is_none() {
            return Ok(ReadPrepareHistoryReplyV2::UnknownOriginal);
        }
        let Some(outcome) = &r.outcome else {
            return Ok(ReadPrepareHistoryReplyV2::Pending);
        };
        let Some(expected) = &outcome.fact_digest else {
            return Ok(ReadPrepareHistoryReplyV2::Pending);
        };
        outcome
            .lifetime
            .verify_root(native.native_root_key_id, native.native_root_public_key)
            .map_err(|_| invalid())?;
        if r.original_digest.as_deref() != Some(original(self, id, &q)?.as_str())
            || fact(s)?.as_ref().map(|(d, _)| d) != Some(expected)
        {
            return Err(invalid());
        }
        let observation = self
            .read_prepared_public_history_v2(id, native)?
            .ok_or_else(invalid)?;
        Ok(ReadPrepareHistoryReplyV2::HistoricalPublicObservation {
            observation: Box::new(observation),
        })
    }
}
