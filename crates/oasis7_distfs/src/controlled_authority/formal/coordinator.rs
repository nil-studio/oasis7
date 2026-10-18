//! Writer-side orchestration; receipt keys remain in the two endpoint services.
use super::{crypto::invalid, endpoint::*, *};
use crate::controlled_authority::replicated_protocol::{ClosedRecord, EndpointRole};

pub trait FormalDurabilityEndpoint: Send {
    fn stage_native_environment(
        &mut self,
        _descriptor: &native_environment::NativeExecutionEnvironmentV1,
        _chunks: &[NativeEnvironmentChunkBytesV1],
    ) -> Result<(), FormalError> {
        Err(invalid("native environment staging unavailable"))
    }
    fn advance_native_environment_verification(
        &mut self,
        _descriptor: &native_environment::NativeExecutionEnvironmentV1,
    ) -> Result<NativeEnvironmentVerificationProgressV1, FormalError> {
        Err(invalid("native environment verification unavailable"))
    }

    /// Release only maintenance read pins; this never changes authority,
    /// qualification, or the logical retention set.
    fn release_published_closure(&mut self, _candidate_digest: &str) -> Result<(), FormalError> {
        Ok(())
    }
    fn install_recovery_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
        material: ClosedRecord,
    ) -> Result<(), FormalError>;
    /// Independent registered-local typed closure readback. Unsupported adapters
    /// must fail closed; a qualification receipt alone is not this evidence.
    fn verify_published_closure(
        &mut self,
        _request: &str,
        _candidate_digest: &str,
    ) -> Result<FormalPublishedClosureReceiptV1, FormalError> {
        Err(invalid(
            "independent published closure readback unavailable",
        ))
    }
    fn head(&mut self) -> Result<FormalHeadRefV1, FormalError>;
    fn query(&mut self, request: &str) -> Result<FormalQueryOutcomeV1, FormalError>;
    fn freeze(
        &mut self,
        input: SignedFormalFrozenV1,
        material: ClosedRecord,
    ) -> Result<FormalFrozenReceiptV1, FormalError>;
    fn complete_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
    ) -> Result<(), FormalError>;
    fn prepare(
        &mut self,
        candidate: FormalCandidateMaterialV1,
    ) -> Result<FormalPrepareReceiptV1, FormalError>;
    fn decide(
        &mut self,
        request: &str,
        primary: FormalPrepareReceiptV1,
        replica: FormalPrepareReceiptV1,
    ) -> Result<FormalFinalReceiptV1, FormalError>;
    fn install_proof(
        &mut self,
        decision: FormalDecisionProofV1,
    ) -> Result<FormalProofCompleteReceiptV1, FormalError>;
    fn publish_qualified(
        &mut self,
        proof: FormalQualifiedCommitProofV1,
    ) -> Result<FormalHeadRefV1, FormalError>;
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalPublishedClosureReceiptV1 {
    pub candidate_digest: String,
    pub qualified_proof_digest: String,
    pub inventory_digest: String,
    pub logical_objects: u64,
    pub logical_bytes: u64,
}

impl FormalDurabilityEndpoint for FileFormalEndpoint {
    fn stage_native_environment(
        &mut self,
        descriptor: &native_environment::NativeExecutionEnvironmentV1,
        chunks: &[NativeEnvironmentChunkBytesV1],
    ) -> Result<(), FormalError> {
        FileFormalEndpoint::stage_native_environment(self, descriptor, chunks)
    }
    fn advance_native_environment_verification(
        &mut self,
        descriptor: &native_environment::NativeExecutionEnvironmentV1,
    ) -> Result<NativeEnvironmentVerificationProgressV1, FormalError> {
        FileFormalEndpoint::advance_native_environment_verification(self, descriptor)
    }

    fn release_published_closure(&mut self, candidate_digest: &str) -> Result<(), FormalError> {
        Self::release_published_closure(self, candidate_digest)
    }
    fn verify_published_closure(
        &mut self,
        request: &str,
        candidate_digest: &str,
    ) -> Result<FormalPublishedClosureReceiptV1, FormalError> {
        Self::verify_published_closure(self, request, candidate_digest)
    }
    fn install_recovery_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
        material: ClosedRecord,
    ) -> Result<(), FormalError> {
        Self::install_recovery_frozen(self, certificate, material)
    }
    fn head(&mut self) -> Result<FormalHeadRefV1, FormalError> {
        Self::head(self)
    }
    fn query(&mut self, request: &str) -> Result<FormalQueryOutcomeV1, FormalError> {
        Self::query(self, request)
    }
    fn freeze(
        &mut self,
        input: SignedFormalFrozenV1,
        material: ClosedRecord,
    ) -> Result<FormalFrozenReceiptV1, FormalError> {
        Self::freeze(self, input, material)
    }
    fn complete_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
    ) -> Result<(), FormalError> {
        Self::complete_frozen(self, certificate)
    }
    fn prepare(
        &mut self,
        candidate: FormalCandidateMaterialV1,
    ) -> Result<FormalPrepareReceiptV1, FormalError> {
        Self::prepare(self, candidate)
    }
    fn decide(
        &mut self,
        request: &str,
        primary: FormalPrepareReceiptV1,
        replica: FormalPrepareReceiptV1,
    ) -> Result<FormalFinalReceiptV1, FormalError> {
        Self::decide(self, request, primary, replica)
    }
    fn install_proof(
        &mut self,
        decision: FormalDecisionProofV1,
    ) -> Result<FormalProofCompleteReceiptV1, FormalError> {
        Self::install_proof(self, decision)
    }
    fn publish_qualified(
        &mut self,
        proof: FormalQualifiedCommitProofV1,
    ) -> Result<FormalHeadRefV1, FormalError> {
        Self::publish_qualified(self, proof)
    }
}
#[derive(Debug, Clone)]
pub struct ExecutableFormalFrozenV1 {
    input: SignedFormalFrozenV1,
    material: ClosedRecord,
    certificate: FormalFrozenCertificateV1,
}
impl ExecutableFormalFrozenV1 {
    pub fn input(&self) -> &SignedFormalFrozenV1 {
        &self.input
    }
    pub fn material(&self) -> &ClosedRecord {
        &self.material
    }
    pub fn certificate(&self) -> &FormalFrozenCertificateV1 {
        &self.certificate
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormalCoordinatorErrorV1 {
    Rejected(FormalError),
    OutcomeUnknown {
        request_digest: String,
        cause: FormalError,
    },
}
impl std::fmt::Display for FormalCoordinatorErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FormalCoordinatorErrorV1 {}
pub struct FormalCoordinator<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint> {
    primary: P,
    replica: R,
    authority: VerifiedAuthorityV1,
    original: VerifiedAuthorityV1,
    recovery: Option<VerifiedRecovery>,
    parent: Option<VerifiedFormalCommitV1>,
    poisoned: bool,
}
impl<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint> FormalCoordinator<P, R> {
    pub fn new(
        primary: P,
        replica: R,
        authority: VerifiedAuthorityV1,
        original: VerifiedAuthorityV1,
        recovery: Option<VerifiedRecovery>,
        parent: Option<VerifiedFormalCommitV1>,
    ) -> Result<Self, FormalError> {
        authority.definition().verify_policy(authority.policy())?;
        match &recovery {
            None if authority.authority_id()? != original.authority_id()? => {
                return Err(super::crypto::invalid(
                    "formal coordinator missing verified recovery",
                ));
            }
            Some(recovery)
                if recovery.successor().authority_id()? != authority.authority_id()?
                    || recovery.original_authority().authority_id()?
                        != original.authority_id()? =>
            {
                return Err(super::crypto::invalid(
                    "formal coordinator recovery context",
                ));
            }
            _ => {}
        }
        Ok(Self {
            primary,
            replica,
            authority,
            original,
            recovery,
            parent,
            poisoned: false,
        })
    }
    pub fn endpoints_mut(&mut self) -> (&mut P, &mut R) {
        (&mut self.primary, &mut self.replica)
    }
    pub fn parent(&self) -> Option<&VerifiedFormalCommitV1> {
        self.parent.as_ref()
    }
    fn unknown(&mut self, request: &str, cause: FormalError) -> FormalCoordinatorErrorV1 {
        self.poisoned = true;
        FormalCoordinatorErrorV1::OutcomeUnknown {
            request_digest: request.into(),
            cause,
        }
    }
    fn ready(&self, request: &str) -> Result<(), FormalCoordinatorErrorV1> {
        if self.poisoned {
            Err(FormalCoordinatorErrorV1::OutcomeUnknown {
                request_digest: request.into(),
                cause: FormalError::Io(
                    "formal coordinator poisoned; query from new session".into(),
                ),
            })
        } else {
            Ok(())
        }
    }
    pub fn freeze(
        &mut self,
        input: SignedFormalFrozenV1,
        material: ClosedRecord,
    ) -> Result<ExecutableFormalFrozenV1, FormalCoordinatorErrorV1> {
        let request = input.body.request_digest.clone();
        self.ready(&request)?;
        verify_frozen_material(&input.body, &material)
            .map_err(FormalCoordinatorErrorV1::Rejected)?;
        let parent = self.parent.as_ref().ok_or_else(|| {
            FormalCoordinatorErrorV1::Rejected(super::crypto::invalid(
                "formal freeze committed parent",
            ))
        })?;
        verify_formal_reservation(&input, &self.authority, parent)
            .map_err(FormalCoordinatorErrorV1::Rejected)?;
        // Reconcile both original request identities before any mutation.
        for outcome in [self.primary.query(&request), self.replica.query(&request)] {
            let outcome = outcome.map_err(|cause| self.unknown(&request, cause))?;
            if let FormalQueryOutcomeV1::Recorded(entry) = outcome {
                let (old, bytes) = match entry.as_ref() {
                    FormalRequestStateV1::Frozen {
                        input, material, ..
                    } => (input, material),
                    state => {
                        let candidate = state.candidate().expect("candidate state");
                        let certificate = candidate.frozen.as_ref().ok_or_else(|| {
                            FormalCoordinatorErrorV1::Rejected(super::crypto::invalid(
                                "formal old request lacks frozen",
                            ))
                        })?;
                        (
                            &certificate.reservation,
                            candidate.frozen_material.as_ref().ok_or_else(|| {
                                FormalCoordinatorErrorV1::Rejected(super::crypto::invalid(
                                    "formal old request lacks original bytes",
                                ))
                            })?,
                        )
                    }
                };
                if old != &input || bytes != &material {
                    return Err(FormalCoordinatorErrorV1::Rejected(FormalError::Conflict(
                        "formal original request identity".into(),
                    )));
                }
            }
        }
        let primary = self
            .primary
            .freeze(input.clone(), material.clone())
            .map_err(|cause| self.unknown(&request, cause))?;
        let replica = self
            .replica
            .freeze(input.clone(), material.clone())
            .map_err(|cause| self.unknown(&request, cause))?;
        let certificate = FormalFrozenCertificateV1 {
            reservation: input.clone(),
            primary,
            replica,
        };
        certificate
            .verify(self.authority.definition(), self.authority.activation_ref())
            .map_err(|cause| self.unknown(&request, cause))?;
        self.primary
            .complete_frozen(certificate.clone())
            .map_err(|cause| self.unknown(&request, cause))?;
        self.replica
            .complete_frozen(certificate.clone())
            .map_err(|cause| self.unknown(&request, cause))?;
        Ok(ExecutableFormalFrozenV1 {
            input,
            material,
            certificate,
        })
    }
    /// Query original durable candidates before invoking this method after an
    /// interruption; a changed candidate is rejected and cannot replace them.
    pub fn replay_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
        material: ClosedRecord,
    ) -> Result<ExecutableFormalFrozenV1, FormalCoordinatorErrorV1> {
        let body = &certificate.reservation.body;
        let request = body.request_digest.clone();
        self.ready(&request)?;
        let recovery = self.recovery.as_ref().ok_or_else(|| {
            FormalCoordinatorErrorV1::Rejected(super::crypto::invalid(
                "formal replay requires verified recovery",
            ))
        })?;
        certificate
            .verify(self.original.definition(), self.original.activation_ref())
            .map_err(FormalCoordinatorErrorV1::Rejected)?;
        verify_frozen_material(body, &material).map_err(FormalCoordinatorErrorV1::Rejected)?;
        let RecoveryTailV1::Frozen {
            position,
            reservation_id,
            handoff,
        } = &recovery.body().tail
        else {
            return Err(FormalCoordinatorErrorV1::Rejected(super::crypto::invalid(
                "formal candidate cannot be downgraded to replay",
            )));
        };
        if body.parent != recovery.body().sealed_cut
            || body.next_position != *position
            || body
                .reservation_id()
                .map_err(FormalCoordinatorErrorV1::Rejected)?
                != *reservation_id
            || &body.handoff != handoff
            || self.parent.as_ref().map(VerifiedFormalCommitV1::head)
                != Some(recovery.body().sealed_cut.clone())
        {
            return Err(FormalCoordinatorErrorV1::Rejected(super::crypto::invalid(
                "formal replay exact original frozen/cut",
            )));
        }
        // A success token is minted only after the entire certified old closure
        // is checkpointed by BOTH fresh finalizers under their provider guards.
        self.primary
            .install_recovery_frozen(certificate.clone(), material.clone())
            .map_err(|cause| self.unknown(&request, cause))?;
        self.replica
            .install_recovery_frozen(certificate.clone(), material.clone())
            .map_err(|cause| self.unknown(&request, cause))?;
        Ok(ExecutableFormalFrozenV1 {
            input: certificate.reservation.clone(),
            material,
            certificate,
        })
    }
    pub fn submit(
        &mut self,
        candidate: FormalCandidateMaterialV1,
    ) -> Result<FormalQualifiedCommitProofV1, FormalCoordinatorErrorV1> {
        let request = candidate.candidate.record.request_digest.clone();
        self.ready(&request)?;
        let mut qualified = None;
        let mut qualified_count = 0;
        for outcome in [self.primary.query(&request), self.replica.query(&request)] {
            if let FormalQueryOutcomeV1::Recorded(entry) =
                outcome.map_err(|cause| self.unknown(&request, cause))?
            {
                if let Some(old) = entry.candidate()
                    && old != &candidate
                    && !(candidate.branch == FormalCompletionBranchV1::CompletePrepared && {
                        let mut retained = old.clone();
                        retained.branch = candidate.branch;
                        retained == candidate
                    })
                {
                    return Err(FormalCoordinatorErrorV1::Rejected(FormalError::Conflict(
                        "formal original candidate retained".into(),
                    )));
                }
                if let FormalRequestStateV1::Qualified { proof, .. } = *entry {
                    if qualified.as_ref().is_some_and(|old| old != &proof) {
                        return Err(FormalCoordinatorErrorV1::Rejected(FormalError::Conflict(
                            "formal endpoint proofs disagree".into(),
                        )));
                    }
                    qualified_count += 1;
                    qualified = Some(proof);
                }
            }
        }
        if qualified_count == 2 {
            let proof = qualified.expect("two matching qualified results");
            // Retrying a known result may restore the current session parent,
            // but an older permanent-index result must never move it backwards.
            let next = self.parent.as_ref().map_or(
                proof.decision.record.kind == FormalRecordKindV1::InitialActivation,
                |parent| {
                    parent.head().position.checked_add(1) == Some(proof.decision.record.position)
                        && parent.head().record_id == proof.decision.record.parent_record_id
                },
            );
            if next {
                let original = if proof.decision.branch == FormalCompletionBranchV1::Normal {
                    &self.authority
                } else {
                    &self.original
                };
                if let Ok(verified) = verify_formal_qualified(
                    &proof,
                    original,
                    self.parent.as_ref(),
                    self.recovery.as_ref(),
                ) && self
                    .primary
                    .head()
                    .is_ok_and(|head| head == verified.head())
                    && self
                        .replica
                        .head()
                        .is_ok_and(|head| head == verified.head())
                {
                    self.parent = Some(verified);
                }
            }
            return Ok(proof);
        }
        let original = if candidate.branch == FormalCompletionBranchV1::Normal {
            &self.authority
        } else {
            &self.original
        };
        let recovery = (candidate.branch != FormalCompletionBranchV1::Normal)
            .then_some(self.recovery.as_ref())
            .flatten();
        let context = verify_formal_candidate(
            &candidate.candidate,
            candidate.frozen.as_ref(),
            original,
            self.parent.as_ref(),
            recovery,
            candidate.branch,
        )
        .map_err(FormalCoordinatorErrorV1::Rejected)?;
        verify_record_material(&candidate.candidate.record, &candidate.material)
            .map_err(FormalCoordinatorErrorV1::Rejected)?;
        let decision = if let Some(proof) = qualified {
            proof.decision
        } else {
            let primary = self
                .primary
                .prepare(candidate.clone())
                .map_err(|cause| self.unknown(&request, cause))?;
            let replica = self
                .replica
                .prepare(candidate.clone())
                .map_err(|cause| self.unknown(&request, cause))?;
            verify_formal_prepare_receipt(&primary, &candidate, &context, EndpointRole::Primary)
                .map_err(|cause| self.unknown(&request, cause))?;
            verify_formal_prepare_receipt(&replica, &candidate, &context, EndpointRole::Replica)
                .map_err(|cause| self.unknown(&request, cause))?;
            let p = self
                .primary
                .decide(&request, primary.clone(), replica.clone())
                .map_err(|cause| self.unknown(&request, cause))?;
            let r = self
                .replica
                .decide(&request, primary, replica)
                .map_err(|cause| self.unknown(&request, cause))?;
            FormalDecisionProofV1 {
                schema_version: 1,
                record: candidate.candidate.record.clone(),
                branch: candidate.branch,
                writer_signature_hex: candidate.candidate.writer_signature_hex.clone(),
                frozen: candidate.frozen.clone(),
                primary: p,
                replica: r,
            }
        };
        let original = if decision.branch == FormalCompletionBranchV1::Normal {
            &self.authority
        } else {
            &self.original
        };
        let recovery = (decision.branch != FormalCompletionBranchV1::Normal)
            .then_some(self.recovery.as_ref())
            .flatten();
        let verified_decision =
            verify_formal_decision(&decision, original, self.parent.as_ref(), recovery)
                .map_err(|cause| self.unknown(&request, cause))?;
        let primary_complete = self
            .primary
            .install_proof(decision.clone())
            .map_err(|cause| self.unknown(&request, cause))?;
        let replica_complete = self
            .replica
            .install_proof(decision.clone())
            .map_err(|cause| self.unknown(&request, cause))?;
        let proof = FormalQualifiedCommitProofV1 {
            schema_version: 1,
            decision,
            primary_complete,
            replica_complete,
        };
        let verified = verify_formal_qualified_from_decision(&proof, &verified_decision)
            .map_err(|cause| self.unknown(&request, cause))?;
        let primary = self
            .primary
            .publish_qualified(proof.clone())
            .map_err(|cause| self.unknown(&request, cause))?;
        let replica = self
            .replica
            .publish_qualified(proof.clone())
            .map_err(|cause| self.unknown(&request, cause))?;
        if primary != verified.head() || replica != verified.head() {
            return Err(self.unknown(
                &request,
                super::crypto::invalid("formal qualification canonical head mismatch"),
            ));
        }
        self.parent = Some(verified);
        Ok(proof)
    }
    pub fn query(
        &mut self,
        request: &str,
    ) -> Result<(FormalQueryOutcomeV1, FormalQueryOutcomeV1), FormalCoordinatorErrorV1> {
        let primary = self
            .primary
            .query(request)
            .map_err(|cause| self.unknown(request, cause))?;
        let replica = self
            .replica
            .query(request)
            .map_err(|cause| self.unknown(request, cause))?;
        Ok((primary, replica))
    }
}
