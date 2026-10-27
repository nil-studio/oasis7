use super::*;
use ws::private_goal::verify_owner_signature_strict;
impl AccountProvisionApprovalState {
    pub fn owner_setup_archive_checks(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        fact: &OwnerSetupCanonicalFactV1,
        context: &OwnerSetupArchiveContext<'_>,
    ) -> Result<ExactSignatureChecks> {
        self.validate_owner_setup_archive(id, stage, fact, context)?;
        let setup = self.owner_setups.get(id).ok_or_else(invalid)?;
        let original = self
            .owner_setup_outbox(id, stage)
            .and_then(|o| o.original.as_ref())
            .ok_or_else(invalid)?;
        Ok(owner_setup_archive_signature_checks(
            id,
            stage,
            fact,
            context.world_public_key,
            original,
            &setup.plan.digest()?,
        )?)
    }
    /// Fact-only retention. This deliberately does not observe current source,
    /// thaw an account, settle budgets, confirm an operation, or advance phases.
    pub fn archive_owner_setup_canonical(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        fact: OwnerSetupCanonicalFactV1,
        context: &OwnerSetupArchiveContext<'_>,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupCanonicalFactV1> {
        verifier.verify(&self.owner_setup_archive_checks(id, stage, &fact, context)?)?;
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?;
        if let Some(old) = o.canonical_history.get(&fact.source) {
            return if old == &fact {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        o.canonical_history.insert(fact.source, fact.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(fact)
    }
    pub(super) fn validate_owner_setup_archive(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        fact: &OwnerSetupCanonicalFactV1,
        context: &OwnerSetupArchiveContext<'_>,
    ) -> Result<()> {
        let setup = self.owner_setups.get(id).ok_or_else(invalid)?;
        let o = setup.stages.get(&stage).ok_or_else(invalid)?;
        if context.world_public_key != setup.plan.world_identity.service_public_key
            || context.world_public_key != self.draft.world_identity.service_public_key
            || o.claimed_at.is_none()
            || fact.source == OwnerSetupCanonicalSourceV1::Lookup && o.lookup_claimed_at.is_none()
        {
            return Err(invalid());
        }
        if o.canonical_history
            .get(&fact.source)
            .is_some_and(|old| old != fact)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        self.validate_owner_setup_canonical_observation(id, stage, fact)
    }
    pub(super) fn validate_owner_setup_canonical_observation(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        fact: &OwnerSetupCanonicalFactV1,
    ) -> Result<()> {
        let setup = self.owner_setups.get(id).ok_or_else(invalid)?;
        let o = setup.stages.get(&stage).ok_or_else(invalid)?;
        let Some(OwnerSetupOriginalV1::Canonical { submit, lookup }) = &o.original else {
            return Err(invalid());
        };
        let domain = fact.source.response_domain();
        let expected = match fact.source {
            OwnerSetupCanonicalSourceV1::Submit => ws::request_digest(domain, submit),
            OwnerSetupCanonicalSourceV1::Lookup => ws::request_digest(domain, lookup),
        }
        .map_err(|_| invalid())?;
        if fact.response.request_digest != expected {
            return Err(invalid());
        }
        fact.response
            .payload
            .validate(&submit.correlation)
            .map_err(|_| invalid())?;
        if let ws::IntentOutcome::Committed { commit, .. } = &fact.response.payload.outcome {
            if commit.binding.branch_id != setup.plan.world_identity.branch_id
                || commit.binding.reorg_generation != setup.plan.world_identity.reorg_epoch.0
            {
                return Err(invalid());
            }
        }
        verify_owner_signature_strict(
            domain,
            &(&fact.response.request_digest, &fact.response.payload),
            &setup.plan.world_identity.service_public_key,
            &fact.response.signature_hex,
        )
        .map_err(|_| invalid())?;
        Ok(())
    }
}
