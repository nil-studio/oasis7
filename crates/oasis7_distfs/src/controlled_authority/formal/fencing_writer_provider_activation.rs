//! The successor signer opens business signing only after its one qualified
//! recovery control. The original verified cut is never manufactured here.
use super::*;
use crate::controlled_authority::formal::endpoint::{
    FormalCandidateMaterialV1, FormalPrepareReceiptV1, formal_candidate_material_digest,
};
use crate::controlled_authority::formal::transport::Command;
use crate::controlled_authority::replicated_protocol::{ArtifactRole, EndpointRole};

impl ControlledFileWriterProvider {
    fn activation_recovery(&self) -> Option<&VerifiedRecovery> {
        self.recovery
            .as_ref()
            .filter(|recovery| recovery.body().schema_version == 3)
    }

    pub(super) fn require_activated(&self) -> Result<(), FormalError> {
        if self.activation_recovery().is_some()
            && self
                .ledger
                .activation
                .as_ref()
                .is_none_or(|activation| activation.qualified.is_none())
        {
            return Err(invalid(
                "successor writer requires its original qualified recovery control",
            ));
        }
        Ok(())
    }

    fn control_context(
        &self,
        candidate: &SignedCanonicalCandidateV1,
    ) -> Result<VerifiedCandidateContextV1, FormalError> {
        let recovery = self
            .activation_recovery()
            .ok_or_else(|| invalid("writer has no schema3 recovery control permission"))?;
        if !matches!(
            candidate.record.kind,
            FormalRecordKindV1::RecoveryActivation { .. }
        ) {
            return Err(invalid(
                "writer activation must be the original control record",
            ));
        }
        verify_formal_candidate(
            candidate,
            None,
            &self.authority,
            Some(recovery.sealed_commit()),
            None,
            FormalCompletionBranchV1::Normal,
        )
    }

    pub(super) fn validate_activation_record(
        &self,
        record: &CanonicalRecordV1,
        frozen: Option<&FormalFrozenCertificateV1>,
    ) -> Result<(), FormalError> {
        if frozen.is_some() {
            return Err(invalid(
                "recovery control must not consume a Frozen request",
            ));
        }
        // The signature stays in this provider. Validation still uses the
        // actual private-token lineage supplied by verified bootstrap.
        let candidate = SignedCanonicalCandidateV1 {
            record: record.clone(),
            writer_signature_hex: hex::encode(
                self.key
                    .sign(&canonical_candidate_signing_bytes(record)?)
                    .to_bytes(),
            ),
        };
        self.control_context(&candidate)?;
        if self
            .ledger
            .activation
            .as_ref()
            .is_some_and(|original| original.candidate != candidate)
        {
            return Err(invalid(
                "writer recovery control differs from its first durable candidate",
            ));
        }
        Ok(())
    }

    pub(super) fn validate_activation_original(&self) -> Result<(), FormalError> {
        if let Some(original) = &self.ledger.activation {
            self.control_context(&original.candidate)?;
            if let Some(digest) = &original.material_digest {
                crypto::hash_hex(digest)?;
            }
            if let Some(proof) = &original.qualified {
                self.check_control_proof(proof)?;
            }
        }
        Ok(())
    }

    pub(super) fn sign_activation(
        &mut self,
        record: &CanonicalRecordV1,
        frozen: Option<&FormalFrozenCertificateV1>,
    ) -> Result<WriterCustodyValueV1, FormalError> {
        self.validate_activation_record(record, frozen)?;
        if let Some(original) = &self.ledger.activation {
            return Ok(WriterCustodyValueV1::Signature(
                original.candidate.writer_signature_hex.clone(),
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
        let mut next = self.ledger.clone();
        next.revision = self.next_revision()?;
        next.activation = Some(WriterActivation {
            candidate: candidate.clone(),
            material_digest: None,
            qualified: None,
        });
        self.commit(next)?;
        Ok(WriterCustodyValueV1::Signature(
            candidate.writer_signature_hex,
        ))
    }

    fn check_control_proof(&self, proof: &FormalQualifiedCommitProofV1) -> Result<(), FormalError> {
        let recovery = self
            .activation_recovery()
            .ok_or_else(|| invalid("writer control has no verified recovery"))?;
        let original = self
            .ledger
            .activation
            .as_ref()
            .ok_or_else(|| invalid("writer control has no durable candidate"))?;
        if proof.decision.record != original.candidate.record
            || proof.decision.writer_signature_hex != original.candidate.writer_signature_hex
            || proof.decision.frozen.is_some()
            || proof.decision.branch != FormalCompletionBranchV1::Normal
        {
            return Err(invalid(
                "writer control proof differs from original candidate",
            ));
        }
        verify_formal_qualified(proof, &self.authority, Some(recovery.sealed_commit()), None)?;
        Ok(())
    }

    fn bind_control_material(&mut self, digest: &str) -> Result<(), FormalError> {
        crypto::hash_hex(digest)?;
        let original = self
            .ledger
            .activation
            .as_ref()
            .ok_or_else(|| invalid("writer control has no durable candidate"))?;
        if let Some(kept) = &original.material_digest {
            if kept != digest {
                return Err(invalid(
                    "writer control replaced the exact original material",
                ));
            }
            return Ok(());
        }
        let mut next = self.ledger.clone();
        next.revision = self.next_revision()?;
        next.activation.as_mut().unwrap().material_digest = Some(digest.into());
        self.commit(next)
    }

    fn prepare_control(&mut self, material: &FormalCandidateMaterialV1) -> Result<(), FormalError> {
        self.control_context(&material.candidate)?;
        let original = self.ledger.activation.as_ref().ok_or_else(|| {
            invalid("writer must durably sign the original control before prepare")
        })?;
        if original.candidate != material.candidate
            || material.branch != FormalCompletionBranchV1::Normal
            || material.frozen.is_some()
            || material.frozen_material.is_some()
        {
            return Err(invalid("writer control prepare differs from its original"));
        }
        verify_record_material(&material.candidate.record, &material.material)?;
        let input = material
            .material
            .objects
            .iter()
            .find(|object| object.content_hash == material.candidate.record.payload_digest)
            .ok_or_else(|| invalid("writer control original certificate input missing"))?;
        verify_recovery_activation_input(
            &input.bytes,
            &material.candidate.record,
            &self.authority,
        )?;
        self.bind_control_material(&formal_candidate_material_digest(material)?)
    }

    fn decide_control(
        &mut self,
        primary: &FormalPrepareReceiptV1,
        replica: &FormalPrepareReceiptV1,
    ) -> Result<(), FormalError> {
        let original = self
            .ledger
            .activation
            .as_ref()
            .ok_or_else(|| invalid("writer control has no durable candidate"))?;
        let context = self.control_context(&original.candidate)?;
        if primary.body.candidate_material_digest != replica.body.candidate_material_digest {
            return Err(invalid("writer control dual prepare material differs"));
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
            if receipt.body.final_receipt != context.receipt_body(role)? {
                return Err(invalid(
                    "writer control prepare receipt differs from original context",
                ));
            }
            crypto::verify(
                "oasis7.formal.prepare_receipt.sign.v1",
                &receipt.body,
                &receipt.signature_hex,
                &binding.instance.endpoint_public_key,
            )?;
        }
        self.bind_control_material(&primary.body.candidate_material_digest)
    }

    /// True means the command was checked as part of the original control.
    /// Other business commands are handled by the ordinary checks only after
    /// `require_activated` has proved the persisted qualified control.
    pub(super) fn activation_endpoint(&mut self, command: &Command) -> Result<bool, FormalError> {
        let Some(recovery) = self.activation_recovery() else {
            return Ok(false);
        };
        let request = recovery_activation_request_digest(&self.authority)?;
        match command {
            Command::Prepare(material)
                if matches!(
                    material.candidate.record.kind,
                    FormalRecordKindV1::RecoveryActivation { .. }
                ) =>
            {
                self.prepare_control(material)?;
            }
            Command::Decide(candidate_request, primary, replica)
                if candidate_request == &request =>
            {
                self.decide_control(primary, replica)?;
            }
            Command::InstallProof(proof) if proof.record.request_digest == request => {
                let original = self
                    .ledger
                    .activation
                    .as_ref()
                    .ok_or_else(|| invalid("writer control has no durable candidate"))?;
                if proof.record != original.candidate.record
                    || proof.writer_signature_hex != original.candidate.writer_signature_hex
                {
                    return Err(invalid(
                        "writer control decision changed original candidate",
                    ));
                }
                verify_formal_decision(
                    proof,
                    &self.authority,
                    Some(recovery.sealed_commit()),
                    None,
                )?;
            }
            Command::PublishQualified(proof) if proof.decision.record.request_digest == request => {
                self.check_control_proof(proof)?;
                let original = self.ledger.activation.as_ref().unwrap();
                if let Some(kept) = &original.qualified {
                    if kept != proof.as_ref() {
                        return Err(invalid(
                            "writer control changed its original qualified proof",
                        ));
                    }
                } else {
                    let mut next = self.ledger.clone();
                    next.revision = self.next_revision()?;
                    next.activation.as_mut().unwrap().qualified = Some(*proof.clone());
                    self.commit(next)?;
                }
            }
            Command::VerifyNativeEnvironment(environment) if self.require_activated().is_err() => {
                if recovery
                    .sealed_commit()
                    .record()
                    .artifact_roots
                    .get(&ArtifactRole::NativeExecutionEnvironment)
                    != Some(&environment.digest().map_err(invalid)?)
                {
                    return Err(invalid(
                        "writer control verification changed original native descriptor",
                    ));
                }
            }
            Command::ReleaseNativeEnvironmentVerification { descriptor_digest }
                if self.require_activated().is_err() =>
            {
                if recovery
                    .sealed_commit()
                    .record()
                    .artifact_roots
                    .get(&ArtifactRole::NativeExecutionEnvironment)
                    != Some(descriptor_digest)
                {
                    return Err(invalid(
                        "writer control release changed original native descriptor",
                    ));
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
