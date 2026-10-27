use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_owner_setup_persistence(&self) -> Result<()> {
        if self.owner_setups.len() > 1 {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        for (id, v) in &self.owner_setups {
            v.plan.validate()?;
            if v.book.records().count() != 1 {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            let r = v.book.original(id).ok_or_else(invalid)?;
            self.check_owner_setup_proof_unique(r.input())?;
            if r.input().operation_id != v.plan.setup_id
                || r.input().request.context.security_version.0 == 0
                || r.input().request.context.custody_account_id
                    != self.draft.entry.custody_account_id
                || r.input().request.context.environment != self.draft.entry.environment
                || v.plan.bootstrap_config_version != self.draft.entry.bootstrap_config_version
                || v.plan.owner_key_ref
                    != self
                        .provision
                        .role_key(Role::Owner)
                        .ok_or_else(invalid)?
                        .key_id
                || v.plan.executor_key_ref
                    != self
                        .provision
                        .role_key(Role::Executor)
                        .ok_or_else(invalid)?
                        .key_id
                || v.plan.executor_public_key
                    != self
                        .provision
                        .role_key(Role::Executor)
                        .ok_or_else(invalid)?
                        .public_key
                || self
                    .participant_provisions
                    .values()
                    .find(|p| p.result.is_some())
                    .and_then(|p| p.result.as_ref())
                    .is_none_or(|result| {
                        digest(result).ok().as_ref() != Some(&v.plan.participant_receipt_digest)
                    })
                || r.input().request.intent != v.plan.intent()?
                || v.plan.provision_id != self.draft.entry.provision_id
                || v.plan.world_identity != self.draft.world_identity
            {
                return Err(invalid());
            }
            if r.stage() != ApprovalAdmissionStage::Consumed {
                if v.transport.is_some()
                    || v.source.is_some()
                    || v.session.is_some()
                    || !v.stages.is_empty()
                {
                    return Err(invalid());
                }
                continue;
            }
            let session = v.session.as_ref().ok_or_else(invalid)?;
            let origin = &self
                .native_stages
                .values()
                .next()
                .ok_or_else(invalid)?
                .descriptor
                .http_origin;
            let participant = self
                .participant_provisions
                .values()
                .find(|v| v.result.is_some())
                .ok_or_else(invalid)?;
            if !self.participant_session_candidates(origin)?.iter().any(
                |(_, _, reference, status)| {
                    reference == session
                        && status.status.receipt == participant.descriptor.native_binding
                },
            ) {
                return Err(invalid());
            }
            let mut budget = BudgetLedger::new(v.plan.transport_limit.0);
            budget.charge(8).map_err(|_| invalid())?;
            let i = v.source.as_ref().ok_or_else(invalid)?;
            let signed = r.verification().ok_or_else(invalid)?;
            if !hex64(&i.verify_nonce)
                || i.created_at.0 != r.consumed_at().ok_or_else(invalid)?
                || i.expires_at.0
                    != v.plan
                        .deadline
                        .0
                        .min(signed.verification.source_session_expires_at.0)
                        .min(signed.verification.source_session_idle_expires_at.0)
                        .min(i.created_at.0.checked_add(30_000).ok_or_else(invalid)?)
                || i.operation_id
                    != digest(&("oasis7.custody.owner-setup-source.v1", id, &v.plan.setup_id))?
                || i.claimed_at.is_some() != i.request_bytes.is_some()
                || i.verification.is_some() != i.verified_at.is_some()
            {
                return Err(invalid());
            }
            if let Some(at) = i.claimed_at {
                if at.0 < i.created_at.0 || at.0 >= i.expires_at.0 {
                    return Err(invalid());
                }
                let mut req = r.native_verify_request()?;
                req.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
                req.verify_nonce = i.verify_nonce.clone();
                if i.request_bytes.as_deref()
                    != Some(serde_json::to_string(&req).map_err(|_| invalid())?.as_str())
                {
                    return Err(invalid());
                }
            }
            if let (Some(fact), Some(at)) = (&i.verification, i.verified_at) {
                let mut context = r.recorded_context().clone();
                context.source.session_expires_at = fact.verification.source_session_expires_at;
                context.source.session_idle_expires_at =
                    fact.verification.source_session_idle_expires_at;
                r.check_recorded_context(&context)?;
                self.validate_owner_setup_source_fact(id, fact, &context, at.0)?;
            }
            if !v.stages.is_empty() && i.verification.is_none() {
                return Err(invalid());
            }
            let mut nonces = BTreeSet::new();
            nonces.insert(i.verify_nonce.clone());
            for (stage, o) in &v.stages {
                budget
                    .charge(OWNER_SETUP_STAGE_TRANSPORT_COST)
                    .map_err(|_| invalid())?;
                if !nonces.insert(o.session_nonce.clone()) {
                    return Err(invalid());
                }
                self.validate_owner_setup_outbox(id, *stage, o, r)?;
            }
            if v.transport.as_ref() != Some(&budget) {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub(crate) fn owner_setup_approval_records(
        &self,
    ) -> impl Iterator<Item = &ApprovalAdmissionRecord> {
        self.owner_setups
            .values()
            .flat_map(|v| v.book.records())
            .chain(
                self.owner_setup_reconciles
                    .values()
                    .flat_map(|v| v.book.records()),
            )
    }
}
