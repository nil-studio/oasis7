use super::*;
fn same_registered_native_facts(
    a: &NativeBindingChallengeV1,
    b: &NativeBindingChallengeV1,
) -> bool {
    a.api_version == b.api_version
        && a.identity_namespace == b.identity_namespace
        && a.identity_instance_epoch == b.identity_instance_epoch
        && a.registration_operation_id == b.registration_operation_id
        && a.registration_request_digest == b.registration_request_digest
        && a.registration_receipt_digest == b.registration_receipt_digest
        && a.native_account_id == b.native_account_id
        && a.native_credential_ref == b.native_credential_ref
        && a.login_public_key == b.login_public_key
        && a.owner_context_ref == b.owner_context_ref
}
impl AccountProvisionApprovalState {
    pub fn native_binding_challenge_checks(
        &self,
        id: &str,
        signed: &SignedNativeBindingChallengeV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let original = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let known = self
            .native_stages
            .values()
            .find_map(|v| match &v.result {
                Some(NativeStageResultV1::RegistrationChallenge(v)) => Some(&v.challenge),
                _ => None,
            })
            .ok_or(ApprovalAdmissionError::Missing)?;
        signed
            .challenge
            .validate_at(now)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        if original.request.stage != NativeStageKindV1::Binding
            || original.claimed_at.is_none()
            || signed.challenge.challenge_id != original.request.binding_operation_id
            || !same_registered_native_facts(known, &signed.challenge)
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(append_native_history_checks(
            self.native_stage_checks(id, scope, current, now)?,
            current.approval,
            NativeFact::Challenge(signed),
            scope.native_root_key_id,
            scope.native_root_public_key,
            &original.request.digest()?,
        )?)
    }
    pub fn record_native_binding_challenge(
        &mut self,
        id: &str,
        signed: SignedNativeBindingChallengeV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier
            .verify(&self.native_binding_challenge_checks(id, &signed, scope, current, now)?)?;
        let mut next = self.clone();
        let original = next
            .native_stages
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if original
            .binding_challenge
            .as_ref()
            .is_some_and(|v| v != &signed)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        original.binding_challenge = Some(signed);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    /// A paid new attempt may recover only a receipt for an Account-known exact old statement.
    pub fn native_binding_recovery_checks(
        &self,
        id: &str,
        receipt: &SignedNativeBindingReceiptV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let stage = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if stage.request.stage != NativeStageKindV1::Binding
            || stage.claimed_at.is_none()
            || stage.result.is_some()
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let old = self
            .native_stages
            .values()
            .filter(|v| v.request.stage == NativeStageKindV1::Binding)
            .find_map(|v| {
                v.binding_statement.as_ref().filter(|s| {
                    s.statement.digest().ok().as_ref() == Some(&receipt.receipt.statement_digest)
                })
            })
            .ok_or(ApprovalAdmissionError::Missing)?;
        receipt
            .receipt
            .validate_for(&old.statement)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        Ok(append_native_history_checks(
            self.native_stage_checks(id, scope, current, now)?,
            current.approval,
            NativeFact::Receipt(receipt),
            scope.native_root_key_id,
            scope.native_root_public_key,
            &stage.request.digest()?,
        )?)
    }
    pub fn recover_native_binding_receipt(
        &mut self,
        id: &str,
        receipt: SignedNativeBindingReceiptV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<NativeStageResultV1> {
        verifier
            .verify(&self.native_binding_recovery_checks(id, &receipt, scope, current, now)?)?;
        let old = self
            .native_stages
            .values()
            .find_map(|v| {
                v.binding_statement.as_ref().filter(|s| {
                    s.statement.digest().ok().as_ref() == Some(&receipt.receipt.statement_digest)
                })
            })
            .ok_or(ApprovalAdmissionError::Missing)?
            .clone();
        let mut next = self.clone();
        let stage = next
            .native_stages
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        stage.binding_statement = Some(old.clone());
        let signed_challenge = next
            .native_stages
            .values()
            .find_map(|v| {
                v.binding_challenge
                    .as_ref()
                    .filter(|v| v.challenge == old.statement.challenge)
                    .cloned()
                    .or_else(|| match &v.result {
                        Some(NativeStageResultV1::RegistrationChallenge(v))
                            if v.challenge == old.statement.challenge =>
                        {
                            Some(v.clone())
                        }
                        _ => None,
                    })
            })
            .ok_or(ApprovalAdmissionError::Missing)?;
        next.native_stages
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?
            .binding_challenge = Some(signed_challenge);
        let result = next.complete_native_stage(
            id,
            NativeStageResultV1::BindingReceipt(receipt),
            scope,
            current,
            now,
            None,
            verifier,
        )?;
        *self = next;
        Ok(result)
    }
}
