//! Shared Source/access storage invariants; V1 code is moved verbatim.
use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_owner_setup_preflight_storage(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        o: &OwnerSetupOutboxV1,
        r: &ApprovalAdmissionRecord,
    ) -> Result<()> {
        let v = self.owner_setup_engine_record(id)?;
        let consumed = r.consumed_at().ok_or_else(invalid)?;
        let signed = r.verification().ok_or_else(invalid)?;
        let source = &o.source;
        let expiry = o
            .reserved_at
            .0
            .checked_add(30_000)
            .ok_or_else(invalid)?
            .min(v.plan.deadline.0)
            .min(signed.verification.source_session_expires_at.0)
            .min(signed.verification.source_session_idle_expires_at.0);
        if stage != o.stage
            || o.oracle_reads > OWNER_SETUP_MAX_ORACLE_READS
            || !hex64(&o.session_nonce)
            || o.reserved_at.0 < consumed
            || o.reserved_at.0 >= v.plan.deadline.0
            || stage.previous().is_some_and(|p| {
                v.stages.get(&p).is_none_or(|s| {
                    s.result.as_ref().is_none_or(|r| !r.succeeded())
                        || s.completed_at.is_none_or(|t| t.0 > o.reserved_at.0)
                })
            })
            || source.created_at != o.reserved_at
            || source.expires_at.0 != expiry
            || source.operation_id
                != digest(&("oasis7.custody.owner-setup-stage-source.v1", id, stage))?
            || source.verify_nonce
                != digest(&(
                    "oasis7.custody.owner-setup-stage-source-nonce.v1",
                    id,
                    stage,
                    &o.session_nonce,
                ))?
            || source.claimed_at.is_some() != source.request_bytes.is_some()
            || source.verification.is_some() != source.verified_at.is_some()
        {
            return Err(invalid());
        }
        if let Some(at) = source.claimed_at {
            let mut req = r.native_verify_request()?;
            req.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
            req.verify_nonce = source.verify_nonce.clone();
            if at.0 < source.created_at.0
                || at.0 >= source.expires_at.0
                || source.request_bytes.as_deref()
                    != Some(serde_json::to_string(&req).map_err(|_| invalid())?.as_str())
            {
                return Err(invalid());
            }
        }
        if let (Some(fact), Some(at)) = (&source.verification, source.verified_at) {
            let mut c = r.recorded_context().clone();
            c.source.session_expires_at = fact.verification.source_session_expires_at;
            c.source.session_idle_expires_at = fact.verification.source_session_idle_expires_at;
            r.check_recorded_context(&c)?;
            self.validate_owner_setup_inspection(id, source, fact, &c, at.0)?;
        }
        if o.access.is_some() != o.access_recorded_at.is_some() {
            return Err(invalid());
        }
        if let Some(at) = o.access_claimed_at {
            if o.oracle_reads < 2 || at.0 < o.reserved_at.0 || at.0 >= v.plan.deadline.0 {
                return Err(invalid());
            }
        }
        if o.oracle_reads > 2 && o.access_claimed_at.is_none()
            || o.oracle_reads > 3 && o.access.is_none()
        {
            return Err(invalid());
        }
        if let Some(access) = &o.access {
            let at = o.access_recorded_at.ok_or_else(invalid)?.0;
            source_fresh(source, at)?;
            if o.access_claimed_at.is_none_or(|t| t.0 > at) || at >= v.plan.deadline.0 {
                return Err(invalid());
            }
            self.validate_owner_setup_access(
                id,
                stage,
                access,
                at,
                r.input().request.context.service_epoch,
            )?;
        }
        Ok(())
    }
}
