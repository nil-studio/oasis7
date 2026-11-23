//! Exact old-epoch draining under the same writer custody lock as signing.
use super::*;
use crate::controlled_authority::formal::endpoint::{
    FormalCandidateMaterialV1, FormalPrepareReceiptV1, formal_candidate_material_digest,
};
use crate::controlled_authority::formal::transport::Command;
use crate::controlled_authority::replicated_protocol::ClosedRecord;
use crate::controlled_authority::replicated_protocol::EndpointRole;

impl ControlledFileWriterProvider {
    pub(super) fn validate_recovery_original(&self) -> Result<(), FormalError> {
        let Some(original) = &self.ledger.planned else {
            if self.ledger.drained.is_some() || self.ledger.seal.is_some() {
                return Err(invalid("writer drain/seal has no original plan"));
            }
            return Ok(());
        };
        self.validate_plan_fixed(&original.plan, &original.fixed, &original.pending)?;
        if original.fixed_material_digest.as_deref()
            != fixed_frozen(&original.fixed)
                .map(|frozen| frozen.reservation.body.frozen_material_digest.as_str())
        {
            return Err(invalid("writer original fixed material commitment changed"));
        }
        if let Some(environment) = &original.native_environment {
            environment.digest().map_err(invalid)?;
            if original.fixed_material_digest.is_none() {
                return Err(invalid("empty recovery has no native descriptor"));
            }
        }
        original
            .acknowledgement
            .verify(&original.plan, &self.authority)?;
        if original.acknowledgement.body.revision > self.ledger.revision {
            return Err(invalid(
                "writer original acknowledgement exceeds ledger revision",
            ));
        }
        if let Some(candidate) = &original.candidate {
            self.check_drain_candidate(original, candidate)?;
        }
        if let Some(material) = &original.candidate_material_digest {
            crypto::hash_hex(material)?;
            if original.candidate.is_none() {
                return Err(invalid("writer original material binding has no candidate"));
            }
        }
        if let Some(drained) = &self.ledger.drained {
            self.check_drained(original, drained)?;
        }
        if let Some(seal) = &self.ledger.seal {
            seal.verify_with_ack(&original.plan, &self.authority, &original.acknowledgement)?;
            if self.ledger.drained.as_ref() != Some(&seal.drained)
                || seal.body.revision != self.ledger.revision
            {
                return Err(invalid("writer original seal/drained/revision binding"));
            }
        }
        self.validate_closing_plan(&original.plan)
    }

    fn validate_plan_fixed(
        &self,
        plan: &SignedRecoveryPlanV1,
        fixed: &RecoveryTailWitnessV1,
        pending: &RecoveryPendingHandoffV1,
    ) -> Result<(), FormalError> {
        plan.verify(&self.authority)?;
        pending.validate()?;
        let cut = &plan.body.original_cut;
        if plan.body.authorization.recovery_credential_public_key
            != self.ledger.binding.recovery_credential
            || pending.root()? != plan.body.pending_handoff_root
            || recovery_requests_commitment(fixed)? != plan.body.original_requests_commitment
            || recovery_tail_from_witness(fixed)? != plan.body.intent.tail
            || cut.activation_ref != self.authority.activation_ref()
            || (plan.body.original_qualified_proof_digest.is_none()
                && (cut.original_authority_id != self.ledger.binding.authority_id
                    || cut.original_epoch != self.authority.definition().epoch))
        {
            return Err(invalid(
                "writer plan original credential/cut/fixed/pending binding",
            ));
        }
        if let Some(frozen) = fixed_frozen(fixed) {
            frozen.verify(self.authority.definition(), self.authority.activation_ref())?;
            let body = &frozen.reservation.body;
            if body.parent != plan.body.intent.sealed_cut
                || body.pre_state_root != cut.after_state_root
                || body.governing_manifest_digest != cut.governing_manifest_digest
                || body.execution_policy_digest != cut.execution_policy_digest
            {
                return Err(invalid(
                    "writer fixed reservation differs from original cut",
                ));
            }
            if let Some(candidate) = fixed_candidate(fixed) {
                candidate.verify_original(&self.authority)?;
                frozen.matches_record(&candidate.record)?;
            }
        }
        Ok(())
    }

    pub(super) fn install_plan(
        &mut self,
        plan: &SignedRecoveryPlanV1,
        fixed: &RecoveryTailWitnessV1,
        fixed_material: Option<&ClosedRecord>,
        pending: &RecoveryPendingHandoffV1,
        expected: Option<u64>,
    ) -> Result<WriterCustodyValueV1, FormalError> {
        self.validate_plan_fixed(plan, fixed, pending)?;
        let (fixed_material_digest, native_environment) =
            match (fixed_frozen(fixed), fixed_material) {
                (None, None) => (None, None),
                (Some(frozen), Some(material)) => {
                    verify_frozen_material(&frozen.reservation.body, material)?;
                    (
                        Some(frozen.reservation.body.frozen_material_digest.clone()),
                        material
                            .native_environment()
                            .map_err(|error| invalid(error.to_string()))?,
                    )
                }
                _ => {
                    return Err(invalid(
                        "writer plan requires the exact complete fixed material",
                    ));
                }
            };
        if let Some(original) = &self.ledger.planned {
            if &original.plan != plan
                || &original.fixed != fixed
                || &original.pending != pending
                || original.fixed_material_digest != fixed_material_digest
                || original.native_environment != native_environment
            {
                return Err(FormalError::Conflict(
                    "writer already fixed another recovery plan".into(),
                ));
            }
            return Ok(WriterCustodyValueV1::Plan(Box::new(
                original.acknowledgement.clone(),
            )));
        }
        if expected != Some(self.ledger.revision) || self.ledger.seal.is_some() {
            return Err(FormalError::Conflict(
                "writer plan requires the exact original CAS revision".into(),
            ));
        }
        self.validate_closing_plan(plan)?;
        let revision = self.next_revision()?;
        let acknowledgement = SignedWriterRecoveryPlanAckV1::sign(
            WriterRecoveryPlanAckBodyV1 {
                schema_version: 1,
                world: self.authority.definition().world.clone(),
                authority_id: self.ledger.binding.authority_id.clone(),
                authority_epoch: self.authority.definition().epoch,
                operation_id: plan.body.intent.operation_id.clone(),
                plan_digest: plan.digest()?,
                pending_handoff_root: pending.root()?,
                revision,
            },
            &self.key,
        )?;
        acknowledgement.verify(plan, &self.authority)?;
        let mut next = self.ledger.clone();
        next.revision = revision;
        next.planned = Some(PlannedRecovery {
            plan: plan.clone(),
            fixed: fixed.clone(),
            fixed_material_digest,
            native_environment,
            pending: pending.clone(),
            candidate: fixed_candidate(fixed).cloned(),
            candidate_material_digest: None,
            acknowledgement: acknowledgement.clone(),
        });
        self.commit(next)?;
        Ok(WriterCustodyValueV1::Plan(Box::new(acknowledgement)))
    }

    fn active_drain(&self) -> Result<&PlannedRecovery, FormalError> {
        if self.ledger.drained.is_some() || self.ledger.seal.is_some() {
            return Err(invalid("writer original drain already completed or sealed"));
        }
        self.ledger
            .planned
            .as_ref()
            .ok_or_else(|| invalid("writer recovery requires its original durable plan"))
    }

    fn check_drain_candidate(
        &self,
        original: &PlannedRecovery,
        candidate: &SignedCanonicalCandidateV1,
    ) -> Result<(), FormalError> {
        candidate.verify_original(&self.authority)?;
        let frozen = fixed_frozen(&original.fixed)
            .ok_or_else(|| invalid("empty recovery plan cannot create an old-epoch candidate"))?;
        frozen.matches_record(&candidate.record)?;
        if let Some(fixed) = fixed_candidate(&original.fixed)
            && fixed != candidate
        {
            return Err(invalid(
                "writer recovery changed an already fixed candidate/signature",
            ));
        }
        if let Some(kept) = &original.candidate
            && kept != candidate
        {
            return Err(invalid(
                "writer recovery may only retry its first durable candidate",
            ));
        }
        if let Some(closing) = &self.ledger.closing
            && (closing.record != candidate.record || closing.frozen.as_ref() != Some(frozen))
        {
            return Err(invalid(
                "writer recovery replaced the signed original handoff",
            ));
        }
        Ok(())
    }

    fn bind_drain_candidate(
        &mut self,
        candidate: &SignedCanonicalCandidateV1,
    ) -> Result<(), FormalError> {
        let original = self.active_drain()?;
        self.check_drain_candidate(original, candidate)?;
        if original.candidate.is_none() {
            let mut next = self.ledger.clone();
            next.revision = self.next_revision()?;
            next.planned.as_mut().unwrap().candidate = Some(candidate.clone());
            self.commit(next)?;
        }
        Ok(())
    }

    pub(super) fn sign_original_frozen(
        &self,
        body: &FormalFrozenBodyV1,
    ) -> Result<WriterCustodyValueV1, FormalError> {
        let original = self.active_drain()?;
        let frozen = fixed_frozen(&original.fixed)
            .ok_or_else(|| invalid("empty recovery has no Frozen signing permission"))?;
        if body != &frozen.reservation.body {
            return Err(invalid(
                "recovery may only return the original Frozen signature",
            ));
        }
        Ok(WriterCustodyValueV1::Signature(
            frozen.reservation.signature_hex.clone(),
        ))
    }

    pub(super) fn sign_original_candidate(
        &mut self,
        record: &CanonicalRecordV1,
        frozen: Option<&FormalFrozenCertificateV1>,
    ) -> Result<WriterCustodyValueV1, FormalError> {
        let original = self.active_drain()?;
        let expected = fixed_frozen(&original.fixed)
            .ok_or_else(|| invalid("empty recovery has no candidate signing permission"))?;
        if frozen != Some(expected) {
            return Err(invalid(
                "writer recovery candidate changed the original Frozen certificate",
            ));
        }
        expected.matches_record(record)?;
        if let Some(candidate) = &original.candidate {
            if &candidate.record != record {
                return Err(invalid(
                    "writer recovery changed the first durable candidate",
                ));
            }
            // Prepared/Decision retains the original signature. Do not re-sign it.
            return Ok(WriterCustodyValueV1::Signature(
                candidate.writer_signature_hex.clone(),
            ));
        }
        let candidate = SignedCanonicalCandidateV1 {
            record: record.clone(),
            writer_signature_hex: hex::encode(
                self.key
                    .sign(&canonical_candidate_signing_bytes(record)?)
                    .to_bytes(),
            ),
        };
        self.bind_drain_candidate(&candidate)?;
        Ok(WriterCustodyValueV1::Signature(
            candidate.writer_signature_hex,
        ))
    }

    fn check_drained(
        &self,
        original: &PlannedRecovery,
        drained: &RecoveryDrainedProofV1,
    ) -> Result<(), FormalError> {
        drained.verify(&original.plan, &self.authority)?;
        if drained.fixed != original.fixed {
            return Err(invalid(
                "writer drained proof changed its original fixed witness",
            ));
        }
        if !matches!(original.fixed, RecoveryTailWitnessV1::Empty) {
            self.check_drain_candidate(
                original,
                &SignedCanonicalCandidateV1 {
                    record: drained.qualified.decision.record.clone(),
                    writer_signature_hex: drained.qualified.decision.writer_signature_hex.clone(),
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn complete_drain(
        &mut self,
        drained: &RecoveryDrainedProofV1,
        expected: Option<u64>,
    ) -> Result<WriterCustodyValueV1, FormalError> {
        let original = self
            .ledger
            .planned
            .as_ref()
            .ok_or_else(|| invalid("writer drain requires original plan"))?;
        self.check_drained(original, drained)?;
        if let Some(kept) = &self.ledger.drained {
            if kept != drained {
                return Err(FormalError::Conflict(
                    "writer retained another drained proof".into(),
                ));
            }
            return Ok(WriterCustodyValueV1::Drained(Box::new(kept.clone())));
        }
        if expected != Some(self.ledger.revision) || self.ledger.seal.is_some() {
            return Err(FormalError::Conflict(
                "writer drain requires the exact original CAS revision".into(),
            ));
        }
        let mut next = self.ledger.clone();
        next.revision = self.next_revision()?;
        if !matches!(original.fixed, RecoveryTailWitnessV1::Empty) {
            next.planned.as_mut().unwrap().candidate = Some(SignedCanonicalCandidateV1 {
                record: drained.qualified.decision.record.clone(),
                writer_signature_hex: drained.qualified.decision.writer_signature_hex.clone(),
            });
        }
        next.drained = Some(drained.clone());
        self.commit(next)?;
        Ok(WriterCustodyValueV1::Drained(Box::new(drained.clone())))
    }

    pub(super) fn seal(
        &mut self,
        plan_digest: &str,
        cut: &FormalHeadRefV1,
        expected: Option<u64>,
    ) -> Result<WriterCustodyValueV1, FormalError> {
        let original = self
            .ledger
            .planned
            .as_ref()
            .ok_or_else(|| invalid("writer seal requires original plan"))?;
        let drained = self
            .ledger
            .drained
            .as_ref()
            .ok_or_else(|| invalid("writer seal requires its durable qualified drain"))?;
        if original.plan.digest()? != plan_digest
            || drained.verify(&original.plan, &self.authority)? != *cut
        {
            return Err(invalid(
                "writer seal changed the original plan or drained cut",
            ));
        }
        if let Some(seal) = &self.ledger.seal {
            return Ok(WriterCustodyValueV1::Seal(Box::new(seal.clone())));
        }
        if expected != Some(self.ledger.revision) {
            return Err(FormalError::Conflict(
                "writer seal requires the exact original CAS revision".into(),
            ));
        }
        let revision = self.next_revision()?;
        let seal = SignedWriterEpochSealV1::sign(
            WriterEpochSealBodyV1 {
                schema_version: 1,
                world: self.authority.definition().world.clone(),
                authority_id: self.ledger.binding.authority_id.clone(),
                authority_epoch: self.authority.definition().epoch,
                operation_id: original.plan.body.intent.operation_id.clone(),
                plan_digest: plan_digest.into(),
                pending_handoff_root: original.pending.root()?,
                cut: cut.clone(),
                next_epoch: original.plan.body.intent.successor.epoch,
                plan_ack_digest: original.acknowledgement.digest()?,
                drained_proof_digest: drained.digest()?,
                revision,
            },
            original.acknowledgement.clone(),
            drained.clone(),
            &self.key,
        )?;
        seal.verify(&original.plan, &self.authority)?;
        let mut next = self.ledger.clone();
        next.revision = revision;
        next.seal = Some(seal.clone());
        self.commit(next)?;
        Ok(WriterCustodyValueV1::Seal(Box::new(seal)))
    }

    pub(super) fn validate_recovery_endpoint(
        &mut self,
        command: &Command,
    ) -> Result<(), FormalError> {
        if checks::read_command(command) {
            return Ok(());
        }
        match command {
            Command::PrepareRecoveryPlan {
                plan,
                pending,
                writer_ack,
                checkpoint,
            } => {
                checkpoint
                    .validate()
                    .map_err(|error| invalid(error.to_string()))?;
                let original = self
                    .ledger
                    .planned
                    .as_ref()
                    .ok_or_else(|| invalid("writer plan must precede endpoint preparation"))?;
                if plan.as_ref() != &original.plan
                    || pending.as_ref() != &original.pending
                    || writer_ack.as_ref() != &original.acknowledgement
                {
                    return Err(invalid(
                        "endpoint preparation changed original plan/pending/ack",
                    ));
                }
                return Ok(());
            }
            Command::SealPlanned {
                writer_seal,
                checkpoint,
            } => {
                checkpoint
                    .validate()
                    .map_err(|error| invalid(error.to_string()))?;
                if self.ledger.seal.as_ref() != Some(writer_seal.as_ref()) {
                    return Err(invalid("writer seal must precede endpoint sealing"));
                }
                return Ok(());
            }
            _ => {}
        }
        let original = self.active_drain()?;
        let frozen = fixed_frozen(&original.fixed)
            .ok_or_else(|| invalid("empty recovery grants no old-epoch RPC mutation"))?;
        match command {
            Command::VerifyNativeEnvironment(environment) => {
                if original.native_environment.as_ref() != Some(environment.as_ref()) {
                    return Err(invalid(
                        "recovery verification changed the fixed native descriptor",
                    ));
                }
            }
            Command::ReleaseNativeEnvironmentVerification { descriptor_digest } => {
                if original
                    .native_environment
                    .as_ref()
                    .map(|environment| environment.digest())
                    .transpose()
                    .map_err(invalid)?
                    .as_ref()
                    != Some(descriptor_digest)
                {
                    return Err(invalid(
                        "recovery release changed the fixed native descriptor",
                    ));
                }
            }
            Command::Freeze(input, material) => {
                if input.as_ref() != &frozen.reservation {
                    return Err(invalid("recovery RPC replaced original Frozen bytes"));
                }
                verify_frozen_material(&input.body, material)?;
            }
            Command::CompleteFrozen(certificate) => {
                if certificate.as_ref() != frozen {
                    return Err(invalid("recovery RPC replaced original Frozen certificate"));
                }
            }
            Command::Prepare(candidate) => return self.prepare_original(candidate),
            Command::Decide(request, primary, replica) => {
                return self.decide_original(request, primary, replica);
            }
            Command::InstallProof(proof) => {
                original
                    .plan
                    .verify_drain_decision(&self.authority, proof)?;
                if proof.frozen.as_ref() != Some(frozen) {
                    return Err(invalid("recovery decision changed original Frozen"));
                }
                return self.bind_drain_candidate(&SignedCanonicalCandidateV1 {
                    record: proof.record.clone(),
                    writer_signature_hex: proof.writer_signature_hex.clone(),
                });
            }
            Command::PublishQualified(proof) => {
                let drained = RecoveryDrainedProofV1 {
                    schema_version: 1,
                    plan_digest: original.plan.digest()?,
                    fixed: original.fixed.clone(),
                    qualified: *proof.clone(),
                };
                self.check_drained(original, &drained)?;
                return self.bind_drain_candidate(&SignedCanonicalCandidateV1 {
                    record: proof.decision.record.clone(),
                    writer_signature_hex: proof.decision.writer_signature_hex.clone(),
                });
            }
            Command::VerifyPublishedClosure {
                request,
                candidate_digest,
            } => {
                if request != &frozen.reservation.body.request_digest
                    || original.candidate_material_digest.as_ref() != Some(candidate_digest)
                {
                    return Err(invalid(
                        "recovery verification changed original request/material",
                    ));
                }
            }
            Command::ReleasePublishedClosure { candidate_digest } => {
                if original.candidate_material_digest.as_ref() != Some(candidate_digest) {
                    return Err(invalid("recovery release changed original material"));
                }
            }
            _ => {
                return Err(invalid(
                    "recovery credential permits only its fixed old-epoch continuation",
                ));
            }
        }
        Ok(())
    }

    fn prepare_original(
        &mut self,
        material: &FormalCandidateMaterialV1,
    ) -> Result<(), FormalError> {
        let original = self.active_drain()?;
        if material.branch != FormalCompletionBranchV1::Normal
            || material.frozen.as_ref() != fixed_frozen(&original.fixed)
        {
            return Err(invalid(
                "writer drain must prepare the original Normal branch",
            ));
        }
        if let Some(closed) = &material.frozen_material {
            verify_frozen_material(&material.frozen.as_ref().unwrap().reservation.body, closed)?;
        }
        self.check_drain_candidate(original, &material.candidate)?;
        let digest = formal_candidate_material_digest(material)?;
        self.bind_drain_material(&material.candidate, &digest)
    }

    fn bind_drain_material(
        &mut self,
        candidate: &SignedCanonicalCandidateV1,
        digest: &str,
    ) -> Result<(), FormalError> {
        let original = self.active_drain()?;
        self.check_drain_candidate(original, candidate)?;
        crypto::hash_hex(digest)?;
        if let Some(kept) = &original.candidate_material_digest {
            if kept != digest {
                return Err(invalid(
                    "writer recovery changed the original candidate material",
                ));
            }
            return Ok(());
        }
        let mut next = self.ledger.clone();
        next.revision = self.next_revision()?;
        let planned = next.planned.as_mut().unwrap();
        planned.candidate = Some(candidate.clone());
        planned.candidate_material_digest = Some(digest.into());
        self.commit(next)
    }

    fn decide_original(
        &mut self,
        request: &str,
        primary: &FormalPrepareReceiptV1,
        replica: &FormalPrepareReceiptV1,
    ) -> Result<(), FormalError> {
        let original = self.active_drain()?;
        let candidate = original
            .candidate
            .as_ref()
            .ok_or_else(|| invalid("writer decision has no fixed candidate"))?;
        if candidate.record.request_digest != request
            || primary.body.candidate_material_digest != replica.body.candidate_material_digest
        {
            return Err(invalid("writer decision changed original request/material"));
        }
        for (receipt, role, binding) in [
            (
                primary,
                EndpointRole::Primary,
                &self.authority.definition().primary,
            ),
            (
                replica,
                EndpointRole::Replica,
                &self.authority.definition().replica,
            ),
        ] {
            let expected = FormalFinalReceiptBodyV1 {
                schema_version: 1,
                record_id: candidate.record.record_id()?,
                position: candidate.record.position,
                finalizing_authority_id: self.ledger.binding.authority_id.clone(),
                role,
                instance_digest: binding
                    .instance
                    .digest()
                    .map_err(|error| invalid(error.to_string()))?,
                branch: FormalCompletionBranchV1::Normal,
                recovery_certificate_digest: None,
            };
            if receipt.body.final_receipt != expected {
                return Err(invalid("writer decision original prepare receipt context"));
            }
            crypto::verify(
                "oasis7.formal.prepare_receipt.sign.v1",
                &receipt.body,
                &receipt.signature_hex,
                &binding.instance.endpoint_public_key,
            )?;
        }
        let candidate = candidate.clone();
        self.bind_drain_material(&candidate, &primary.body.candidate_material_digest)
    }
}

fn fixed_frozen(fixed: &RecoveryTailWitnessV1) -> Option<&FormalFrozenCertificateV1> {
    match fixed {
        RecoveryTailWitnessV1::Empty => None,
        RecoveryTailWitnessV1::Frozen(frozen)
        | RecoveryTailWitnessV1::Prepared { frozen, .. }
        | RecoveryTailWitnessV1::Decision { frozen, .. } => Some(frozen),
    }
}
fn fixed_candidate(fixed: &RecoveryTailWitnessV1) -> Option<&SignedCanonicalCandidateV1> {
    match fixed {
        RecoveryTailWitnessV1::Prepared { candidate, .. }
        | RecoveryTailWitnessV1::Decision { candidate, .. } => Some(candidate),
        _ => None,
    }
}
