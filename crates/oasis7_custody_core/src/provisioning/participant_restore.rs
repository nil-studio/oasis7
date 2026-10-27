use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_participant_persistence(&self) -> Result<u64> {
        if self.participant_provisions.len() > 1 {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        for (id, o) in &self.participant_provisions {
            o.request.validate()?;
            self.validate_participant_descriptor(o)?;
            let source = self.accepted_approval()?.verification().ok_or_else(bad)?;
            let expires = o
                .created_at
                .0
                .checked_add(30_000)
                .ok_or_else(bad)?
                .min(self.draft().plan.expires_at.0)
                .min(source.verification.source_session_expires_at.0)
                .min(source.verification.source_session_idle_expires_at.0);
            let values = [
                &o.source_inspection.verify_nonce,
                &o.descriptor.policy_nonce,
                &o.descriptor.owner_view_nonce,
                &o.session_nonce,
            ];
            let mut unique = std::collections::BTreeSet::new();
            if id != &o.request.operation_id
                || o.request.custody_account_id != self.draft().entry.custody_account_id
                || o.request.provision_id != self.draft().entry.provision_id
                || o.target_security_epoch.0 != 1
                || o.oracle_reads > 8
                || o.expires_at.0 != expires
                || o.created_at.0 >= expires
                || values.iter().any(|v| !nonce(v) || !unique.insert(*v))
                || o.source_inspection.created_at != o.created_at
                || o.source_inspection.expires_at != o.expires_at
                || o.source_inspection.operation_id
                    != digest(&("oasis7.custody.participant-source.v1", &o.request))?
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            let i = &o.source_inspection;
            if i.claimed_at.is_some() != i.request_bytes.is_some()
                || i.verification.is_some() != i.verified_at.is_some()
            {
                return Err(bad());
            }
            if let Some(at) = i.claimed_at {
                if at.0 < o.created_at.0 || at.0 >= expires {
                    return Err(bad());
                }
                let mut request = self.accepted_approval()?.native_verify_request()?;
                request.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
                request.verify_nonce = i.verify_nonce.clone();
                if i.request_bytes.as_deref()
                    != Some(serde_json::to_string(&request).map_err(|_| bad())?.as_str())
                {
                    return Err(bad());
                }
            }
            if let (Some(s), Some(at)) = (&i.verification, i.verified_at) {
                self.validate_origin_response(i, s, at.0)?;
            }
            if o.workflow_claimed_at.is_none() && (!o.claims.is_empty() || !o.callbacks.is_empty())
            {
                return Err(bad());
            }
            if let Some(at) = o.workflow_claimed_at {
                check_record_time(self, o, at)?;
            }
            for (k, at) in &o.claims {
                if !["policy", "sign_bundle", "authorize", "submit", "owner_view"]
                    .contains(&k.as_str())
                    || o.workflow_claimed_at.is_none_or(|t| at.0 < t.0)
                {
                    return Err(bad());
                }
                check_record_time(self, o, *at)?;
                if match k.as_str() {
                    "policy" => false,
                    "sign_bundle" => o.policy.is_none(),
                    "authorize" => o.bundle.is_none(),
                    "submit" => o.admission.is_none() || o.submit.is_none(),
                    _ => !o.claims.contains_key("submit") || o.owner_view.is_none(),
                } {
                    return Err(bad());
                }
            }
            for (k, at) in &o.callbacks {
                let need = match k.as_str() {
                    "policy" => "policy",
                    "admission" => "authorize",
                    "final" => "owner_view",
                    _ => return Err(bad()),
                };
                if at.0 < o.created_at.0
                    || at.0 >= expires
                    || o.claims.get(need).is_none_or(|v| v.0 > at.0)
                {
                    return Err(bad());
                }
            }
            match (&o.policy, &o.policy_session, o.policy_recorded_at) {
                (None, None, None) => {}
                (Some(p), Some(s), Some(at)) => {
                    validation::policy(o, p)?;
                    check_record_time(self, o, at)?;
                    if o.callbacks.get("policy").is_none_or(|v| v.0 > at.0) {
                        return Err(bad());
                    }
                    session_at(
                        o,
                        ParticipantStepV1::Policy,
                        s,
                        self.accepted_approval()?
                            .input()
                            .request
                            .context
                            .service_epoch,
                        at.0,
                    )?;
                }
                _ => return Err(bad()),
            }
            match (&o.bundle, o.bundle_recorded_at) {
                (None, None) => {}
                (Some(b), Some(at)) => {
                    validation::bundle(o, b)?;
                    check_record_time(self, o, at)?;
                    if o.claims.get("sign_bundle").is_none_or(|v| v.0 > at.0)
                        || o.policy_recorded_at.is_none_or(|v| v.0 > at.0)
                    {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
            match (&o.admission, &o.admission_session, o.admission_recorded_at) {
                (None, None, None) => {
                    if o.submit.is_some()
                        || o.submit_bytes.is_some()
                        || o.lookup.is_some()
                        || o.owner_view.is_some()
                    {
                        return Err(bad());
                    }
                }
                (Some(a), Some(s), Some(at)) => {
                    validation::admission(o, a)?;
                    check_record_time(self, o, at)?;
                    session_at(
                        o,
                        ParticipantStepV1::Authorize,
                        s,
                        self.accepted_approval()?
                            .input()
                            .request
                            .context
                            .service_epoch,
                        at.0,
                    )?;
                    if o.callbacks.get("admission").is_none_or(|v| v.0 > at.0) {
                        return Err(bad());
                    }
                    let (submit, lookup, owner) = validation::wire(o, a)?;
                    if o.submit.as_ref() != Some(&submit)
                        || o.lookup.as_ref() != Some(&lookup)
                        || o.owner_view.as_ref() != Some(&owner)
                        || o.submit_bytes.as_deref()
                            != Some(serde_json::to_string(&submit).map_err(|_| bad())?.as_str())
                    {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
            match (&o.result, &o.result_session, o.completed_at) {
                (None, None, None) => {}
                (Some(r), Some(s), Some(at)) => {
                    check_record_time(self, o, at)?;
                    validation::result(o, r)?;
                    session_at(
                        o,
                        ParticipantStepV1::Submit,
                        &r.submit_session,
                        self.accepted_approval()?
                            .input()
                            .request
                            .context
                            .service_epoch,
                        at.0,
                    )?;
                    match (&r.response_source, &r.lookup_session) {
                        (ParticipantResultSourceV1::Submit, None) => {}
                        (ParticipantResultSourceV1::Lookup, Some(evidence)) => session_at_nonce(
                            o,
                            &source::participant_lookup_nonce(o)?,
                            evidence,
                            self.accepted_approval()?
                                .input()
                                .request
                                .context
                                .service_epoch,
                            at.0,
                        )?,
                        _ => return Err(bad()),
                    }
                    session_at(
                        o,
                        ParticipantStepV1::OwnerView,
                        s,
                        self.accepted_approval()?
                            .input()
                            .request
                            .context
                            .service_epoch,
                        at.0,
                    )?;
                    if o.callbacks.get("final").is_none_or(|v| v.0 > at.0)
                        || !o.claims.contains_key("submit")
                        || !o.claims.contains_key("owner_view")
                    {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
        }
        Ok(self.participant_provisions.len() as u64 * 104)
    }
    pub(in crate::provisioning) fn participant_phase_replay(
        &self,
    ) -> Result<Vec<(ProvisionStage, PhaseOriginal, Option<String>)>> {
        let mut result = Vec::new();
        for (id, o) in &self.participant_provisions {
            if let Some(receipt) = &o.result {
                result.push((
                    ProvisionStage::ParticipantBound,
                    PhaseOriginal {
                        operation_id: id.clone(),
                        body_digest: digest(&o.submit.as_ref().ok_or_else(bad)?)?,
                        receipt_digest: Some(digest(receipt)?),
                    },
                    None,
                ));
            }
        }
        Ok(result)
    }
}
fn check_record_time(
    state: &AccountProvisionApprovalState,
    o: &ParticipantOriginalV1,
    at: DecimalU64,
) -> Result<()> {
    if at.0 < o.created_at.0 || at.0 >= o.expires_at.0 {
        return Err(bad());
    }
    state.validate_origin_response(
        &o.source_inspection,
        o.source_inspection.verification.as_ref().ok_or_else(bad)?,
        at.0,
    )
}
fn session_at(
    o: &ParticipantOriginalV1,
    step: ParticipantStepV1,
    e: &ParticipantSessionEvidenceV1,
    epoch: DecimalU64,
    now: u64,
) -> Result<()> {
    session_at_nonce(
        o,
        &source::participant_session_nonce(o, step)?,
        e,
        epoch,
        now,
    )
}
fn session_at_nonce(
    o: &ParticipantOriginalV1,
    nonce: &str,
    e: &ParticipantSessionEvidenceV1,
    epoch: DecimalU64,
    now: u64,
) -> Result<()> {
    e.status.validate().map_err(|_| bad())?;
    e.status
        .status
        .validate_for(
            &o.descriptor.native_binding,
            nonce,
            o.descriptor.session.native_binding_generation,
            epoch,
            now,
        )
        .map_err(|_| bad())?;
    if e.private_sid_generation != o.descriptor.session.private_sid_generation
        || e.status.status.session_ref != o.descriptor.session.session_ref
        || !e.status.status.active
        || e.status.status.frozen
        || e.status.status.pending
    {
        return Err(bad());
    }
    Ok(())
}
