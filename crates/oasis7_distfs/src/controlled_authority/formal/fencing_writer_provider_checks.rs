//! Closed signing-purpose checks under the provider's original file lease.
use super::*;
use crate::controlled_authority::formal::endpoint::FormalCandidateMaterialV1;
use crate::controlled_authority::formal::transport::Command;

impl ControlledFileWriterProvider {
    pub(super) fn validate_frozen(&self, body: &FormalFrozenBodyV1) -> Result<(), FormalError> {
        body.validate()?;
        if body.world != self.authority.definition().world
            || body.authority_id != self.ledger.binding.authority_id
            || body.epoch != self.authority.definition().epoch
            || body.activation_ref != self.authority.activation_ref()
            || body.next_position < self.authority.definition().valid_from_position
            || body.schema_version == 4
            || self.recovery.as_ref().is_some_and(|recovery| {
                recovery
                    .body()
                    .first_normal_position
                    .is_none_or(|first| body.next_position < first)
            })
        {
            return Err(invalid(
                "writer frozen original authority/epoch/normal position binding",
            ));
        }
        Ok(())
    }

    pub(super) fn validate_candidate(
        &self,
        record: &CanonicalRecordV1,
        frozen: Option<&FormalFrozenCertificateV1>,
    ) -> Result<(), FormalError> {
        record.validate()?;
        if matches!(record.kind, FormalRecordKindV1::RecoveryActivation { .. }) {
            return self.validate_activation_record(record, frozen);
        }
        if record.world != self.authority.definition().world || record.schema_version >= 7 {
            return Err(invalid("writer candidate original world/profile"));
        }
        if record.original_authority_id == self.ledger.binding.authority_id {
            if record.original_epoch != self.authority.definition().epoch
                || record.activation_ref != self.authority.activation_ref()
                || record.position < self.authority.definition().valid_from_position
                || self.recovery.as_ref().is_some_and(|recovery| {
                    recovery
                        .body()
                        .first_normal_position
                        .is_none_or(|first| record.position < first)
                })
            {
                return Err(invalid(
                    "writer candidate original authority/normal position",
                ));
            }
            match (&record.kind, frozen) {
                (FormalRecordKindV1::InitialActivation, None) => {}
                (FormalRecordKindV1::Execution, Some(certificate)) => {
                    certificate
                        .verify(self.authority.definition(), self.authority.activation_ref())?;
                    certificate.matches_record(record)?;
                }
                _ => {
                    return Err(invalid(
                        "writer candidate requires its exact original Frozen certificate",
                    ));
                }
            }
            return Ok(());
        }
        let recovery = self
            .recovery
            .as_ref()
            .ok_or_else(|| invalid("writer has no independently verified recovery grant"))?;
        let RecoveryTailV1::Frozen {
            position,
            reservation_id,
            handoff,
        } = &recovery.body().tail
        else {
            return Err(invalid(
                "writer may not resign a retained Prepared or Decision candidate",
            ));
        };
        let certificate = frozen
            .ok_or_else(|| invalid("writer replay requires the original Frozen certificate"))?;
        self.validate_recovery_certificate(certificate)?;
        certificate.matches_record(record)?;
        if record.position != *position
            || record.frozen_reservation_id.as_deref() != Some(reservation_id.as_str())
            || record.handoff.as_ref() != handoff.as_ref()
            || record.parent_record_id != recovery.body().sealed_cut.record_id
        {
            return Err(invalid(
                "writer replay changed the authorized original tail",
            ));
        }
        let original_plan = recovery
            .recovery_plan()
            .ok_or_else(|| invalid("writer replay has no original pre-seal plan"))?;
        if original_plan.body.original_requests_commitment
            != recovery_requests_commitment(&RecoveryTailWitnessV1::Frozen(certificate.clone()))?
        {
            return Err(invalid(
                "writer replay changed the complete original signed request",
            ));
        }
        Ok(())
    }

    fn validate_recovery_certificate(
        &self,
        certificate: &FormalFrozenCertificateV1,
    ) -> Result<(), FormalError> {
        let recovery = self
            .recovery
            .as_ref()
            .ok_or_else(|| invalid("writer recovery certificate was not provisioned"))?;
        certificate.verify(
            recovery.original_authority().definition(),
            recovery.original_authority().activation_ref(),
        )?;
        let (position, reservation_id, handoff) = match &recovery.body().tail {
            RecoveryTailV1::Frozen {
                position,
                reservation_id,
                handoff,
            }
            | RecoveryTailV1::Prepared {
                position,
                reservation_id,
                handoff,
                ..
            }
            | RecoveryTailV1::Decision {
                position,
                reservation_id,
                handoff,
                ..
            } => (position.to_owned(), reservation_id, handoff),
            RecoveryTailV1::Empty => {
                return Err(invalid(
                    "empty recovery does not grant an original execution",
                ));
            }
        };
        let body = &certificate.reservation.body;
        if body.next_position != position
            || body.reservation_id()?.as_str() != reservation_id.as_str()
            || body.handoff.as_ref() != handoff.as_ref()
            || body.parent != recovery.body().sealed_cut
        {
            return Err(invalid(
                "writer recovery certificate changed original reservation/cut",
            ));
        }
        Ok(())
    }

    pub(super) fn validate_closing_plan(
        &self,
        plan: &SignedRecoveryPlanV1,
    ) -> Result<(), FormalError> {
        let Some(closing) = &self.ledger.closing else {
            return Ok(());
        };
        let record = &closing.record;
        let (position, reservation, handoff, candidate) = match &plan.body.intent.tail {
            RecoveryTailV1::Frozen {
                position,
                reservation_id,
                handoff,
            } => (*position, reservation_id, handoff, None),
            RecoveryTailV1::Prepared {
                position,
                reservation_id,
                handoff,
                record_id,
            }
            | RecoveryTailV1::Decision {
                position,
                reservation_id,
                handoff,
                record_id,
            } => (*position, reservation_id, handoff, Some(record_id)),
            RecoveryTailV1::Empty => {
                return Err(invalid(
                    "a signed handoff cannot reopen a fresh controlled recovery",
                ));
            }
        };
        if position != record.position
            || Some(reservation) != record.frozen_reservation_id.as_ref()
            || handoff != &record.handoff
            || plan.body.intent.sealed_cut.position.checked_add(1) != Some(record.position)
            || plan.body.intent.sealed_cut.record_id != record.parent_record_id
            || candidate.is_some_and(|id| record.record_id().as_ref() != Ok(id))
        {
            return Err(invalid(
                "writer recovery may only complete the exact original handoff",
            ));
        }
        Ok(())
    }

    pub(super) fn validate_endpoint(
        &mut self,
        request: &WriterEndpointRequestV1,
        recovering: bool,
    ) -> Result<(), FormalError> {
        let body = &request.body;
        crypto::hash_hex(&body.challenge)?;
        crypto::hash_hex(&body.minimum.record_id)?;
        if body.schema_version != 1
            || body.authority_id != self.ledger.binding.authority_id
            || body.world != self.authority.definition().world
            || ![
                &self.authority.definition().primary.instance,
                &self.authority.definition().replica.instance,
            ]
            .iter()
            .any(|instance| instance.digest().as_ref() == Ok(&body.instance_digest))
            || (body.minimum.position == 0 && body.minimum != body.world.genesis_head()?)
        {
            return Err(invalid(
                "writer endpoint request original authority/world/instance/minimum",
            ));
        }
        if recovering {
            return self.validate_recovery_endpoint(&body.command);
        }
        if read_command(&body.command) {
            return Ok(());
        }
        self.require_normal()?;
        if self.activation_endpoint(&body.command)? {
            return Ok(());
        }
        self.require_activated()?;
        if let Some(closing) = &self.ledger.closing {
            return self.validate_closing_endpoint(&body.command, closing);
        }
        match &body.command {
            Command::Freeze(input, material) => {
                self.validate_frozen(&input.body)?;
                crypto::verify(
                    FROZEN_RESERVATION_SIGN_DOMAIN_V1,
                    &input.body,
                    &input.signature_hex,
                    &self.authority.definition().writer_public_key,
                )?;
                verify_frozen_material(&input.body, material)?;
            }
            Command::CompleteFrozen(certificate) => {
                self.validate_endpoint_certificate(certificate)?
            }
            Command::InstallRecoveryFrozen(certificate, material) => {
                self.validate_recovery_certificate(certificate)?;
                verify_frozen_material(&certificate.reservation.body, material)?;
            }
            Command::Prepare(candidate) => self.validate_prepared_rpc(candidate)?,
            Command::InstallProof(proof) => self.validate_decision_rpc(proof)?,
            Command::PublishQualified(proof) => self.validate_decision_rpc(&proof.decision)?,
            Command::Seal { .. }
            | Command::PrepareRecoveryPlan { .. }
            | Command::SealPlanned { .. } => {
                return Err(invalid(
                    "ordinary writer credential cannot authorize recovery RPCs",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_endpoint_certificate(
        &self,
        certificate: &FormalFrozenCertificateV1,
    ) -> Result<(), FormalError> {
        if certificate.reservation.body.authority_id == self.ledger.binding.authority_id {
            self.validate_frozen(&certificate.reservation.body)?;
            certificate.verify(self.authority.definition(), self.authority.activation_ref())?;
            Ok(())
        } else {
            self.validate_recovery_certificate(certificate)
        }
    }

    fn validate_prepared_rpc(
        &self,
        candidate: &FormalCandidateMaterialV1,
    ) -> Result<(), FormalError> {
        match candidate.branch {
            FormalCompletionBranchV1::Normal | FormalCompletionBranchV1::ReplayFrozen => {
                self.validate_candidate(&candidate.candidate.record, candidate.frozen.as_ref())?;
                crypto::verify(
                    CANDIDATE_SIGN_DOMAIN_V1,
                    &candidate.candidate.record,
                    &candidate.candidate.writer_signature_hex,
                    &self.authority.definition().writer_public_key,
                )?;
            }
            FormalCompletionBranchV1::CompletePrepared => {
                self.validate_retained(&candidate.candidate.record, candidate.frozen.as_ref())?;
                candidate
                    .candidate
                    .verify_original(self.recovery.as_ref().unwrap().original_authority())?;
            }
        }
        Ok(())
    }

    fn validate_retained(
        &self,
        record: &CanonicalRecordV1,
        frozen: Option<&FormalFrozenCertificateV1>,
    ) -> Result<(), FormalError> {
        let recovery = self
            .recovery
            .as_ref()
            .ok_or_else(|| invalid("writer has no retained-candidate grant"))?;
        let expected = match &recovery.body().tail {
            RecoveryTailV1::Prepared { record_id, .. }
            | RecoveryTailV1::Decision { record_id, .. } => record_id,
            _ => {
                return Err(invalid(
                    "writer may not substitute a retained completion branch",
                ));
            }
        };
        let frozen = frozen.ok_or_else(|| invalid("retained candidate has no original Frozen"))?;
        self.validate_recovery_certificate(frozen)?;
        frozen.matches_record(record)?;
        if record.record_id()?.as_str() != expected.as_str() {
            return Err(invalid("writer changed the retained candidate"));
        }
        Ok(())
    }

    fn validate_decision_rpc(&self, proof: &FormalDecisionProofV1) -> Result<(), FormalError> {
        let key = if proof.branch == FormalCompletionBranchV1::CompletePrepared {
            self.validate_retained(&proof.record, proof.frozen.as_ref())?;
            &self
                .recovery
                .as_ref()
                .unwrap()
                .original_authority()
                .definition()
                .writer_public_key
        } else {
            self.validate_candidate(&proof.record, proof.frozen.as_ref())?;
            &self.authority.definition().writer_public_key
        };
        crypto::verify(
            CANDIDATE_SIGN_DOMAIN_V1,
            &proof.record,
            &proof.writer_signature_hex,
            key,
        )
    }

    fn validate_closing_endpoint(
        &self,
        command: &Command,
        closing: &ClosingCandidate,
    ) -> Result<(), FormalError> {
        let record = &closing.record;
        let allowed = match command {
            command if read_command(command) => true,
            Command::Freeze(input, material) => {
                let original = closing
                    .frozen
                    .as_ref()
                    .ok_or_else(|| invalid("closing candidate has no original Frozen"))?;
                if &original.reservation != input.as_ref() {
                    return Err(invalid("closing retry changed original frozen bytes"));
                }
                verify_frozen_material(&input.body, material)?;
                true
            }
            Command::CompleteFrozen(certificate) => {
                closing.frozen.as_ref() == Some(certificate.as_ref())
            }
            Command::Prepare(candidate) => {
                self.validate_prepared_rpc(candidate)?;
                &candidate.candidate.record == record && candidate.frozen == closing.frozen
            }
            Command::Decide(request, primary, replica) => {
                request == &record.request_digest
                    && primary.body.final_receipt.record_id == record.record_id()?
                    && replica.body.final_receipt.record_id == record.record_id()?
            }
            Command::InstallProof(proof) => {
                self.validate_decision_rpc(proof)?;
                &proof.record == record
            }
            Command::PublishQualified(proof) => {
                self.validate_decision_rpc(&proof.decision)?;
                &proof.decision.record == record
            }
            Command::VerifyPublishedClosure { request, .. } => request == &record.request_digest,
            Command::ReleasePublishedClosure { candidate_digest } => {
                crypto::hash_hex(candidate_digest)?;
                true
            }
            Command::ReleaseNativeEnvironmentVerification { descriptor_digest } => {
                crypto::hash_hex(descriptor_digest)?;
                true
            }
            _ => false,
        };
        if !allowed {
            return Err(invalid(
                "writer profile closure permits only the original handoff completion",
            ));
        }
        Ok(())
    }
}

pub(super) fn read_command(command: &Command) -> bool {
    matches!(
        command,
        Command::Head
            | Command::Query(_)
            | Command::ArchiveManifest
            | Command::ArchiveObject(_)
            | Command::ArchiveChunk { .. }
            | Command::BusinessIndexPath { .. }
            | Command::RecoveryPendingHandoff
            | Command::RecoveryDrainedProof
            | Command::QueryDurableSet(_)
    )
}
