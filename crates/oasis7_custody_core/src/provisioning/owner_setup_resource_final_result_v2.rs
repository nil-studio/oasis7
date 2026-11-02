//! Full strong final evidence acceptance, separate from old FinalObservation.
use super::*;
use oasis7_client_api::world_service::resource_basis as resource;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteOwnerSetupResourceFinalV2 {
    pub response:
        resource::SignedResourceResponseV1<resource::OwnerSetupFinalResourceObservationV2>,
    pub session: ParticipantSessionEvidenceV1,
    pub paid_result: OwnerSetupPaidResourceResultV2,
}
impl AccountProvisionApprovalState {
    pub(super) fn validate_resource_final_prefix_v2(
        &self,
        id: &str,
        proof: &OwnerSetupResourceFinalProofV2,
    ) -> Result<bool> {
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        if v.plan.setup_id != id {
            return Err(invalid());
        }
        let paid_result: OwnerSetupPaidResourceResultV2 =
            decode(proof.paid_result_bytes.as_ref().ok_or_else(invalid)?)?;
        self.validate_resource_paid_result_v2(id, &paid_result)?;
        let (_, response) = proof.typed(&v.plan)?;
        let r = self.owner_setup_engine_record(id)?;
        for stage in [
            OwnerSetupStageV1::CreateTask,
            OwnerSetupStageV1::PrepareGoal,
            OwnerSetupStageV1::PublishGoal,
            OwnerSetupStageV1::Delegation,
            OwnerSetupStageV1::Bind,
        ] {
            if r.stages
                .get(&stage)
                .and_then(|o| o.result.as_ref())
                .is_none_or(|x| !x.succeeded())
            {
                return Err(invalid());
            }
        }
        let expected = self
            .owner_setup_expected_control(id, OwnerSetupStageV1::FinalObservation)?
            .ok_or_else(invalid)?;
        if response.payload.owner.control != expected {
            return Err(invalid());
        }
        self.owner_setup_active_goal(id, &expected)?;
        let OwnerSetupResultV1::PreparedGoal(prepared) = r
            .stages
            .get(&OwnerSetupStageV1::PrepareGoal)
            .and_then(|o| o.result.as_ref())
            .ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        if response.payload.current_goal.active_goal.as_ref() != Some(&prepared.payload) {
            return Err(invalid());
        }
        let get_commit = |stage| -> Result<&ws::CommitRef> {
            let OwnerSetupResultV1::Canonical { response, .. } = r
                .stages
                .get(&stage)
                .and_then(|o| o.result.as_ref())
                .ok_or_else(invalid)?
            else {
                return Err(invalid());
            };
            let ws::IntentOutcome::Committed { commit, .. } = &response.payload.outcome else {
                return Err(invalid());
            };
            Ok(commit)
        };
        if response.payload.current_goal.activation_commit.as_ref()
            != Some(&resource::ResourceCommitRefV1::from(get_commit(
                OwnerSetupStageV1::PublishGoal,
            )?))
        {
            return Err(invalid());
        }
        let read: ws::CommitRef = response.payload.owner.read_commit.clone().into();
        if !read
            .satisfies_minimum(get_commit(OwnerSetupStageV1::Bind)?)
            .map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        Ok(matches!(
            response.payload.resource,
            resource::ResourceBasisResolutionV1::Ready { .. }
        ))
    }
    /// Typed signature work for the existing Worker two-pass verified SQL transaction.
    pub fn owner_setup_resource_final_completion_checks_v2(
        &self,
        id: &str,
        completed: &CompleteOwnerSetupResourceFinalV2,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        self.validate_persistence()?;
        self.validate_resource_paid_result_v2(id, &completed.paid_result)?;
        let checks = self.owner_setup_stored_admission_checks(
            id,
            OwnerSetupStageV1::FinalObservation,
            current,
            now,
        )?;
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        let final_outbox = v.resource_final.as_ref().ok_or_else(invalid)?;
        let callback = final_outbox.result_callback.as_ref().ok_or_else(invalid)?;
        if now < callback.claimed_at() {
            return Err(invalid());
        }
        if final_outbox.result.is_some() || final_outbox.dispatch.is_none() {
            return Err(ProvisionApprovalError::Conflict);
        }
        if now
            >= final_outbox
                .dispatch
                .as_ref()
                .ok_or_else(invalid)?
                .expires_at()
        {
            return Err(invalid());
        }
        self.validate_owner_setup_session(
            id,
            &final_outbox.common.session_nonce,
            &completed.session,
            now,
            current.provision.approval.authority.service_epoch,
        )?;
        let checks = append_native_history_checks(
            checks,
            current.provision.approval,
            NativeFact::Status(&completed.session.status),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &digest(&(id, "resource-final-v2"))?,
        )?;
        Ok(checks)
    }
    pub fn complete_owner_setup_resource_final_v2(
        &mut self,
        id: &str,
        completed: CompleteOwnerSetupResourceFinalV2,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        let checks =
            self.owner_setup_resource_final_completion_checks_v2(id, &completed, current, now)?;
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        let final_outbox = v.resource_final.as_ref().ok_or_else(invalid)?;
        verifier.verify(&checks)?;
        let original = decode(final_outbox.original_bytes.as_ref().ok_or_else(invalid)?)?;
        let proof = OwnerSetupResourceFinalProofV2::from_typed(
            &v.plan,
            original,
            completed.response,
            completed.paid_result,
        )?;
        let ready = self.validate_resource_final_prefix_v2(id, &proof)?;
        if ready {
            proof.prepared(&v.plan)?;
        }
        let receipt_digest = digest(&proof)?;
        let mut next = self.clone();
        let saved = next
            .owner_setup_v2
            .as_mut()
            .ok_or_else(invalid)?
            .resource_final
            .as_mut()
            .ok_or_else(invalid)?;
        saved.result = Some(proof);
        saved.session = Some(completed.session);
        saved.completed_at = Some(DecimalU64(now));
        if ready {
            next.provision.confirm_owner_setup(PhaseOriginal {
                operation_id: id.into(),
                body_digest: next.owner_setup_phase_body_digest(id)?,
                receipt_digest: Some(receipt_digest),
            })?;
        }
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub(in crate::provisioning) fn owner_setup_resource_final_phase_v2(
        &self,
        id: &str,
    ) -> Result<Option<PhaseOriginal>> {
        let Some(v) = &self.owner_setup_v2 else {
            return Ok(None);
        };
        if v.plan.setup_id != id {
            return Ok(None);
        }
        let Some(f) = &v.resource_final else {
            return Ok(None);
        };
        let Some(proof) = &f.result else {
            return Ok(None);
        };
        if f.completed_at.is_none()
            || f.session.is_none()
            || !self.validate_resource_final_prefix_v2(id, proof)?
        {
            return Ok(None);
        }
        proof.prepared(&v.plan)?;
        Ok(Some(PhaseOriginal {
            operation_id: id.into(),
            body_digest: self.owner_setup_phase_body_digest(id)?,
            receipt_digest: Some(digest(proof)?),
        }))
    }
    /// Historical producer only. The caller supplies externally configured
    /// Native Root pins, not saved self-pins. No Source is refreshed here.
    pub fn owner_setup_prepared_resource_v2(
        &self,
        id: &str,
        native: &ApprovalAdmissionContext,
        native_scope: &NativeStageScope<'_>,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PreparedResourceBasisV1> {
        self.validate_persistence()?;
        if self.provision.stage() != ProvisionStage::Prepared {
            return Err(invalid());
        }
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        if v.plan.setup_id != id || v.plan.selection.resource().is_none() {
            return Err(invalid());
        }
        let r = v.book.original(id).ok_or_else(invalid)?;
        if r.recorded_context()
            .native_key_pins
            .iter()
            .any(|(id, key)| native.native_key_pins.get(id) != Some(key))
        {
            return Err(invalid());
        }
        let f = v.resource_final.as_ref().ok_or_else(invalid)?;
        let proof = f.result.as_ref().ok_or_else(invalid)?;
        if self.owner_setup_resource_final_phase_v2(id)?.is_none() {
            return Err(invalid());
        }
        let phase = self.owner_setup_engine_record(id)?;
        let mut facts = Vec::new();
        for o in phase.stages.values().chain(core::iter::once(&f.common)) {
            for evidence in o
                .admission_session
                .iter()
                .chain(o.session.iter())
                .chain(o.last_observation_session.iter())
            {
                facts.push(&evidence.status);
            }
            if let Some(access) = &o.access {
                facts.push(&access.policy_session.status);
                facts.push(&access.authorize_session.status);
            }
        }
        facts.push(&f.session.as_ref().ok_or_else(invalid)?.status);
        for fact in facts {
            let history_digest =
                digest(&("oasis7.custody.owner-resource-native-history.v2", fact))?;
            verifier.verify(&registered_native_fact_checks(
                id,
                &history_digest,
                r.recorded_context(),
                NativeFact::Status(fact),
                native_scope.native_root_key_id,
                native_scope.native_root_public_key,
                &history_digest,
            )?)?;
        }
        proof.prepared(&v.plan)
    }
}

impl AccountProvisionApprovalState {
    fn validate_resource_paid_result_v2(
        &self,
        id: &str,
        result: &OwnerSetupPaidResourceResultV2,
    ) -> Result<()> {
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        let record = v
            .resource_final
            .as_ref()
            .ok_or_else(invalid)?
            .dispatch
            .as_ref()
            .ok_or_else(invalid)?;
        let frame = record.recorded_frame(self, id, result.paid_proof.clone())?;
        result.verify_frame(&frame, &v.plan.world_identity.service_public_key)
    }
}
