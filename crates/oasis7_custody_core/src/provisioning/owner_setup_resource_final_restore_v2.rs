//! Common preflight history is shared; the final original/result is exclusively V2.
use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_owner_setup_resource_final_common_restore_v2(
        &self,
        id: &str,
        r: &ApprovalAdmissionRecord,
        signatures: &mut RestoreSignatureScope,
    ) -> Result<()> {
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        let f = v.resource_final.as_ref().ok_or_else(invalid)?;
        let o = &f.common;
        let stage = OwnerSetupStageV1::FinalObservation;
        self.validate_owner_setup_preflight_storage(id, stage, o, r)?;
        let projected = self.owner_setup_engine_record(id)?;
        let operation = Operation::admit(
            digest(&(
                "oasis7.custody.owner-setup-final.v1",
                &projected.plan.setup_id,
                &projected.plan.provision_id,
            ))?,
            digest(&(
                "oasis7.custody.owner-setup-stage.v1",
                self.owner_setup_signed_plan_digest(id)?,
                stage,
            ))?,
        )
        .map_err(|_| invalid())?;
        if o.operation != operation
            || o.original.is_some()
            || o.original_bytes.is_some()
            || o.candidate_bytes.is_some()
            || o.published_at != f.published_at
            || o.claimed_at.is_some()
            || o.result.is_some()
            || o.result_provenance.is_some()
            || o.session.is_some()
            || o.completed_at.is_some()
            || o.last_observation.is_some()
            || o.last_observation_session.is_some()
            || o.last_observed_at.is_some()
            || !o.canonical_history.is_empty()
            || o.callback_claimed_at.is_some()
            || o.lookup_claimed_at.is_some()
            || o.lookup_reserved_at.is_some()
            || f.original_bytes.is_some() != o.observation.is_some()
            || o.observation.is_some() != o.observation_bytes.is_some()
        {
            return Err(invalid());
        }
        let fresh = |at: u64| -> Result<()> {
            let observed = o
                .source
                .verification
                .as_ref()
                .ok_or_else(invalid)?
                .verification
                .observed_at
                .0;
            if observed > at || at - observed > 5000 || at >= v.plan.deadline.0 {
                return Err(invalid());
            }
            Ok(())
        };
        if let Some(observation) = &o.observation {
            let access = self.owner_setup_stage_access(id, stage)?;
            let owner = &self
                .provision
                .role_key(Role::Owner)
                .ok_or_else(invalid)?
                .public_key;
            if observation.owner != access.owner
                || observation.admission != access.admission
                || observation.read.subject_public_key != *owner
                || observation.read.request != self.owner_setup_read_request(id, stage)?
                || observation.response_profile
                    != Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1)
                || o.observation_bytes.as_deref()
                    != Some(
                        serde_json::to_string(observation)
                            .map_err(|_| invalid())?
                            .as_str(),
                    )
                || o.oracle_reads != OWNER_SETUP_MAX_ORACLE_READS
            {
                return Err(invalid());
            }
            validate_owner_setup_signature_fact_scoped(
                OwnerSetupSignatureFact::Read(observation, owner),
                signatures,
            )?;
            let at = f.published_at.ok_or_else(invalid)?.0;
            fresh(at)?;
            self.validate_owner_setup_access(
                id,
                stage,
                o.access.as_ref().ok_or_else(invalid)?,
                at,
                r.input().request.context.service_epoch,
            )?;
        }
        if let Some(at) = o.admission_callback_claimed_at {
            fresh(at.0)?;
            if f.published_at.is_none_or(|p| p.0 > at.0) {
                return Err(invalid());
            }
        }
        if o.world.is_some() != o.admission_session.is_some()
            || o.world.is_some() != o.admitted_at.is_some()
        {
            return Err(invalid());
        }
        if let Some(world) = &o.world {
            let at = o.admitted_at.ok_or_else(invalid)?.0;
            fresh(at)?;
            if o.admission_callback_claimed_at.is_none_or(|p| p.0 > at) {
                return Err(invalid());
            }
            self.validate_owner_setup_world_admission(
                id,
                stage,
                &OwnerSetupAdmissionEvidenceV1 {
                    world: world.clone(),
                    session: o.admission_session.clone().ok_or_else(invalid)?,
                },
                at,
                r.input().request.context.service_epoch,
            )?;
        }
        Ok(())
    }
    pub fn reserve_owner_setup_resource_admission_callback_v2(
        &mut self,
        id: &str,
        now: u64,
    ) -> Result<()> {
        self.owner_setup_local(id, now)?;
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        if v.plan.setup_id != id {
            return Err(invalid());
        }
        let f = v.resource_final.as_ref().ok_or_else(invalid)?;
        let observed = f
            .common
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification
            .observed_at
            .0;
        if f.original_bytes.is_none()
            || f.common.admission_callback_claimed_at.is_some()
            || f.published_at.is_none_or(|p| p.0 > now)
            || observed > now
            || now - observed > 5000
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        next.owner_setup_v2
            .as_mut()
            .ok_or_else(invalid)?
            .resource_final
            .as_mut()
            .ok_or_else(invalid)?
            .common
            .admission_callback_claimed_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
