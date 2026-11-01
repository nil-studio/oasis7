use super::*;
use crate::operations::OperationState;
#[path = "owner_setup_preflight_storage.rs"]
mod preflight_storage;
impl AccountProvisionApprovalState {
    pub(super) fn validate_owner_setup_outbox(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        o: &OwnerSetupOutboxV1,
        r: &ApprovalAdmissionRecord,
        signature_scope: &mut RestoreSignatureScope,
    ) -> Result<()> {
        self.validate_owner_setup_preflight_storage(id, stage, o, r)?;
        let v = self.owner_setup_engine_record(id)?;
        let source = &o.source;
        let published = o.original.is_some();
        if published != o.original_bytes.is_some()
            || published != o.observation.is_some()
            || published != o.observation_bytes.is_some()
            || published != o.candidate_bytes.is_some()
            || published != o.published_at.is_some()
        {
            return Err(invalid());
        }
        if let Some(original) = &o.original {
            let observation = o.observation.as_ref().ok_or_else(invalid)?;
            let candidate = OwnerSetupCandidateV1 {
                original: original.clone(),
                observation: observation.clone(),
            };
            self.validate_owner_setup_candidate_scoped(
                id,
                &candidate,
                &OwnerSetupSigningBundleV1 {
                    action: self.owner_setup_expected(id, stage)?,
                    observation: self.owner_setup_read_request(id, stage)?,
                    participant_access: self.owner_setup_stage_access(id, stage)?,
                },
                Some(&mut *signature_scope),
            )?;
            validate_owner_setup_signature_fact_scoped(
                OwnerSetupSignatureFact::Original(
                    original,
                    &self
                        .provision
                        .role_key(Role::Owner)
                        .ok_or_else(invalid)?
                        .public_key,
                ),
                &mut *signature_scope,
            )?;
            let at = o.published_at.ok_or_else(invalid)?.0;
            source_fresh(source, at)?;
            self.validate_owner_setup_access(
                id,
                stage,
                o.access.as_ref().ok_or_else(invalid)?,
                at,
                r.input().request.context.service_epoch,
            )?;
            if at < o.reserved_at.0
                || at >= v.plan.deadline.0
                || o.candidate_bytes.as_deref()
                    != Some(
                        serde_json::to_string(&candidate)
                            .map_err(|_| invalid())?
                            .as_str(),
                    )
                || o.original_bytes.as_deref()
                    != Some(
                        serde_json::to_string(original)
                            .map_err(|_| invalid())?
                            .as_str(),
                    )
                || o.observation_bytes.as_deref()
                    != Some(
                        serde_json::to_string(observation)
                            .map_err(|_| invalid())?
                            .as_str(),
                    )
            {
                return Err(invalid());
            }
        }
        let admitted = o.admitted_at.is_some();
        if admitted != o.world.is_some() || admitted != o.admission_session.is_some() {
            return Err(invalid());
        }
        if let Some(at) = o.admission_callback_claimed_at {
            if !published
                || at.0 < o.published_at.ok_or_else(invalid)?.0
                || at.0 >= v.plan.deadline.0
            {
                return Err(invalid());
            }
        }
        if let Some(at) = o.admitted_at {
            source_fresh(source, at.0)?;
            if o.admission_callback_claimed_at.is_none_or(|t| t.0 > at.0)
                || at.0 >= v.plan.deadline.0
            {
                return Err(invalid());
            }
            let evidence = OwnerSetupAdmissionEvidenceV1 {
                world: o.world.clone().ok_or_else(invalid)?,
                session: o.admission_session.clone().ok_or_else(invalid)?,
            };
            self.validate_owner_setup_world_admission(
                id,
                stage,
                &evidence,
                at.0,
                r.input().request.context.service_epoch,
            )?;
            validate_owner_setup_signature_fact_scoped(
                OwnerSetupSignatureFact::World(
                    &evidence.world,
                    &v.plan.world_identity.service_public_key,
                ),
                &mut *signature_scope,
            )?;
        }
        let mut expected = Operation::admit(
            ledger::stage_operation_id(&v.plan, stage)?,
            digest(&(
                "oasis7.custody.owner-setup-stage.v1",
                self.owner_setup_signed_plan_digest(id)?,
                stage,
            ))?,
        )
        .map_err(|_| invalid())?;
        if let Some(bytes) = &o.candidate_bytes {
            expected
                .prepare(bytes.as_bytes().to_vec())
                .map_err(|_| invalid())?;
        }
        let dispatched = matches!(
            o.operation.state(),
            OperationState::Dispatching | OperationState::Unknown | OperationState::Confirmed
        );
        if dispatched != o.claimed_at.is_some() {
            return Err(invalid());
        }
        if let Some(at) = o.claimed_at {
            source_fresh(source, at.0)?;
            self.owner_setup_admission_at(id, stage, o, at.0, r)?;
            if at.0 >= v.plan.deadline.0 {
                return Err(invalid());
            }
            expected
                .claim(r.input().request.context.security_version.0)
                .map_err(|_| invalid())?;
        }
        match o.operation.state() {
            OperationState::Unknown => expected.mark_unknown().map_err(|_| invalid())?,
            OperationState::Confirmed => expected
                .confirm(digest(o.result.as_ref().ok_or_else(invalid)?)?)
                .map_err(|_| invalid())?,
            OperationState::CancelledBeforeDispatch => return Err(invalid()),
            _ => {}
        }
        if expected != o.operation
            || o.result.is_some() != (o.operation.state() == OperationState::Confirmed)
            || o.result.is_some() != o.completed_at.is_some()
        {
            return Err(invalid());
        }
        if let Some(at) = o.lookup_reserved_at {
            if o.claimed_at.is_none_or(|t| t.0 > at.0)
                || !matches!(o.original, Some(OwnerSetupOriginalV1::Canonical { .. }))
                || at.0 >= v.plan.deadline.0
            {
                return Err(invalid());
            }
        }
        if let Some(at) = o.lookup_claimed_at {
            source_fresh(source, at.0)?;
            self.owner_setup_admission_at(id, stage, o, at.0, r)?;
            if o.lookup_reserved_at.is_none_or(|t| t.0 > at.0)
                || !matches!(o.original, Some(OwnerSetupOriginalV1::Canonical { .. }))
                || at.0 >= v.plan.deadline.0
            {
                return Err(invalid());
            }
        }
        if let Some(at) = o.callback_claimed_at {
            if o.claimed_at.is_none_or(|t| t.0 > at.0) || at.0 >= v.plan.deadline.0 {
                return Err(invalid());
            }
        }
        if let Some(at) = o.completed_at {
            if matches!(
                o.result_provenance,
                Some(OwnerSetupResultProvenanceV1::Reconcile { .. })
            ) {
                self.validate_reconciled_stage_result(id, stage, o)?;
            } else {
                source_fresh(source, at.0)?;
                if o.callback_claimed_at.is_none_or(|t| t.0 > at.0) || at.0 >= v.plan.deadline.0 {
                    return Err(invalid());
                }
                let result = o.result.as_ref().ok_or_else(invalid)?;
                if !result.terminal() {
                    return Err(invalid());
                }
                if let Some(fact) = result.canonical_fact() {
                    if o.canonical_history.get(&fact.source) != Some(&fact) {
                        return Err(invalid());
                    }
                }
                self.validate_owner_setup_result_scoped(
                    id,
                    stage,
                    result,
                    Some(&mut *signature_scope),
                )?;
                let nonce = match result {
                    OwnerSetupResultV1::Canonical {
                        from_lookup: true, ..
                    } => {
                        if o.lookup_claimed_at.is_none() {
                            return Err(invalid());
                        }
                        ledger::lookup_nonce(id, stage, &o.session_nonce)?
                    }
                    _ => o.session_nonce.clone(),
                };
                self.validate_owner_setup_session(
                    id,
                    &nonce,
                    o.session.as_ref().ok_or_else(invalid)?,
                    at.0,
                    r.input().request.context.service_epoch,
                )?;
            }
        } else if o.session.is_some() || o.result_provenance.is_some() {
            return Err(invalid());
        }
        if o.canonical_history.len() > 2 {
            return Err(invalid());
        }
        for (source, fact) in &o.canonical_history {
            if source != &fact.source {
                return Err(invalid());
            }
            self.validate_owner_setup_archive_scoped(
                id,
                stage,
                fact,
                &OwnerSetupArchiveContext {
                    world_public_key: &v.plan.world_identity.service_public_key,
                },
                Some(&mut *signature_scope),
            )?;
        }
        if o.last_observation.is_some() != o.last_observation_session.is_some()
            || o.last_observation.is_some() != o.last_observed_at.is_some()
        {
            return Err(invalid());
        }

        if let Some(observation) = o.last_observation.as_ref() {
            if observation.terminal()
                || o.result.is_some() && o.result_provenance.is_none()
                || o.operation.state() != OperationState::Unknown && o.result_provenance.is_none()
            {
                return Err(invalid());
            }
            self.validate_owner_setup_result_scoped(
                id,
                stage,
                observation,
                Some(&mut *signature_scope),
            )?;
            let fact = observation.canonical_fact().ok_or_else(invalid)?;
            if o.canonical_history.get(&fact.source) != Some(&fact) {
                return Err(invalid());
            }
            let at = o.last_observed_at.ok_or_else(invalid)?.0;
            source_fresh(source, at)?;
            if o.callback_claimed_at.is_none_or(|t| t.0 > at) || at >= v.plan.deadline.0 {
                return Err(invalid());
            }
            let nonce = if fact.source == OwnerSetupCanonicalSourceV1::Lookup {
                ledger::lookup_nonce(id, stage, &o.session_nonce)?
            } else {
                o.session_nonce.clone()
            };
            self.validate_owner_setup_session(
                id,
                &nonce,
                o.last_observation_session.as_ref().ok_or_else(invalid)?,
                at,
                r.input().request.context.service_epoch,
            )?;
        }
        Ok(())
    }
    fn owner_setup_admission_at(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        o: &OwnerSetupOutboxV1,
        now: u64,
        r: &ApprovalAdmissionRecord,
    ) -> Result<()> {
        if o.admitted_at.is_none_or(|t| t.0 > now || now - t.0 > 5000) {
            return Err(invalid());
        }
        self.validate_owner_setup_world_admission(
            id,
            stage,
            &OwnerSetupAdmissionEvidenceV1 {
                world: o.world.clone().ok_or_else(invalid)?,
                session: o.admission_session.clone().ok_or_else(invalid)?,
            },
            now,
            r.input().request.context.service_epoch,
        )
    }
}
fn source_fresh(source: &ProvisionOriginInspectionV1, now: u64) -> Result<()> {
    let at = source
        .verification
        .as_ref()
        .ok_or_else(invalid)?
        .verification
        .observed_at
        .0;
    if at > now || now - at > 5000 {
        return Err(invalid());
    }
    Ok(())
}
