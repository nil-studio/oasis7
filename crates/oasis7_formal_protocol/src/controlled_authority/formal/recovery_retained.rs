//! Consecutive recovery before the active epoch qualifies its first record.
//! This is a new typed permission path; schema 1's predecessor/cut rules remain
//! strict and its canonical signed bytes do not gain absent extension fields.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedRecoveryContextV2 {
    pub schema_version: u32,
    pub previous_recovery_certificate_digest: String,
    pub original_authority_id: String,
    pub original_epoch: u64,
    pub previous_tail: RecoveryTailV1,
    pub tail_witness_digest: String,
    pub retained_candidate_permission_digest: Option<String>,
}
#[derive(Debug, Clone)]
pub struct VerifiedActiveRecoveryContextV2 {
    active: VerifiedAuthorityV1,
    original: VerifiedAuthorityV1,
    cut: VerifiedFormalCommitV1,
    witness: RecoveryTailWitnessV1,
    retained_candidate: Option<SignedCanonicalCandidateV1>,
    commitment: RetainedRecoveryContextV2,
}
impl VerifiedActiveRecoveryContextV2 {
    pub(super) fn retained_candidate(&self) -> Option<&SignedCanonicalCandidateV1> {
        self.retained_candidate.as_ref()
    }
    pub fn active(&self) -> &VerifiedAuthorityV1 {
        &self.active
    }
    pub fn original(&self) -> &VerifiedAuthorityV1 {
        &self.original
    }
    pub fn cut(&self) -> &VerifiedFormalCommitV1 {
        &self.cut
    }
    pub fn commitment(&self) -> &RetainedRecoveryContextV2 {
        &self.commitment
    }
    pub fn witness(&self) -> &RecoveryTailWitnessV1 {
        &self.witness
    }
    /// Every argument is already authorized: a raw config definition, failed
    /// process observation or unsigned cut cannot enter this constructor.
    pub fn from_previous(
        previous: &VerifiedRecovery,
        original: &VerifiedAuthorityV1,
        cut: &VerifiedFormalCommitV1,
        witness: &RecoveryTailWitnessV1,
    ) -> Result<Self, FormalError> {
        let active = previous.successor();
        if previous.body().sealed_cut != cut.head()
            || active.recovery_cut.as_ref() != Some(&cut.head())
            || cut.record().handoff.is_some()
            || active.definition().world != original.definition().world
            || active.activation_ref() != original.activation_ref()
            || active.policy() != original.policy()
        {
            return Err(invalid(
                "retained recovery active permission/cut/world/profile",
            ));
        }
        let (frozen, candidate) = match witness {
            RecoveryTailWitnessV1::Empty => (None, None),
            RecoveryTailWitnessV1::Frozen(frozen) => (Some(frozen), None),
            RecoveryTailWitnessV1::Prepared { frozen, candidate }
            | RecoveryTailWitnessV1::Decision { frozen, candidate } => {
                (Some(frozen), Some(candidate))
            }
        };
        let previous_empty = previous.body().tail == RecoveryTailV1::Empty;
        let expected_original = if previous_empty && frozen.is_some() {
            active
        } else {
            previous.original_authority()
        };
        if original.authority_id()? != expected_original.authority_id()? {
            return Err(invalid(
                "retained recovery original authority must follow verified prior permission",
            ));
        }
        if let Some(frozen) = frozen {
            frozen.verify(original.definition(), original.activation_ref())?;
            verify_formal_reservation(&frozen.reservation, original, cut)?;
            let body = &frozen.reservation.body;
            if body.parent != cut.head()
                || body.next_position
                    != cut
                        .head()
                        .position
                        .checked_add(1)
                        .ok_or_else(|| invalid("retained slot overflow"))?
            {
                return Err(invalid("retained recovery original frozen exact cut/slot"));
            }
            if !previous_empty {
                let (reservation, handoff) = match &previous.body().tail {
                    RecoveryTailV1::Frozen {
                        reservation_id,
                        handoff,
                        ..
                    }
                    | RecoveryTailV1::Prepared {
                        reservation_id,
                        handoff,
                        ..
                    }
                    | RecoveryTailV1::Decision {
                        reservation_id,
                        handoff,
                        ..
                    } => (reservation_id, handoff),
                    RecoveryTailV1::Empty => unreachable!(),
                };
                if body.reservation_id()? != *reservation || body.handoff != *handoff {
                    return Err(invalid("retained recovery cannot replace prior tail"));
                }
            }
        } else if !previous_empty {
            return Err(invalid("retained recovery cannot erase previous tail"));
        }
        if let Some(candidate) = candidate {
            let branch = match &previous.body().tail {
                RecoveryTailV1::Empty => FormalCompletionBranchV1::Normal,
                RecoveryTailV1::Frozen { .. } => FormalCompletionBranchV1::ReplayFrozen,
                RecoveryTailV1::Prepared { .. } | RecoveryTailV1::Decision { .. } => {
                    FormalCompletionBranchV1::CompletePrepared
                }
            };
            verify_formal_candidate(
                candidate,
                frozen,
                original,
                Some(cut),
                (branch != FormalCompletionBranchV1::Normal).then_some(previous),
                branch,
            )?;
        } else if matches!(
            previous.body().tail,
            RecoveryTailV1::Prepared { .. } | RecoveryTailV1::Decision { .. }
        ) {
            return Err(invalid(
                "retained recovery cannot discard prepared candidate",
            ));
        }
        let permission_digest = candidate
            .map(|candidate| {
                hash(
                    "oasis7.formal.retained_candidate_permission.v2",
                    &(previous.certificate_digest(), candidate),
                )
            })
            .transpose()?;
        let commitment = RetainedRecoveryContextV2 {
            schema_version: 2,
            previous_recovery_certificate_digest: previous.certificate_digest().into(),
            original_authority_id: original.authority_id()?,
            original_epoch: original.definition().epoch,
            previous_tail: previous.body().tail.clone(),
            tail_witness_digest: hash("oasis7.formal.retained_tail_witness.v2", witness)?,
            retained_candidate_permission_digest: permission_digest,
        };
        Ok(Self {
            active: active.clone(),
            original: original.clone(),
            cut: cut.clone(),
            witness: witness.clone(),
            retained_candidate: candidate.cloned(),
            commitment,
        })
    }
}
impl VerifiedAppendFencingCoverageV1 {
    pub fn derive_from_active_recovery_context(
        context: &VerifiedActiveRecoveryContextV2,
        seal: &FencingVerdict,
    ) -> Result<Self, FormalError> {
        let authority = context.active();
        let body = seal.seal_body();
        let expected = match body.instance.endpoint_role {
            crate::controlled_authority::replicated_protocol::EndpointRole::Primary => {
                &authority.definition().primary.instance
            }
            crate::controlled_authority::replicated_protocol::EndpointRole::Replica => {
                &authority.definition().replica.instance
            }
        };
        if seal.provider_public_key() != authority.policy().fencing_provider_public_key
            || !authority
                .policy()
                .authorized_fencing_resource_bindings
                .iter()
                .any(|resource| resource == seal.resource_binding())
        {
            return Err(FormalError::FencingUnproven);
        }
        if &body.instance != expected
            || body.highest_complete.position != context.cut().head().position
            || body.highest_complete.decision_hash != context.cut().head().record_id
        {
            return Err(invalid(
                "retained necessary-endpoint active instance/cut mismatch",
            ));
        }
        Ok(Self {
            seal: seal.clone(),
            predecessor_authority_id: authority.authority_id()?,
            cut: context.cut().head(),
        })
    }
}
impl VerifiedRecoveryIntent {
    pub fn verify_retained(
        body: RecoveryCertificateBodyV1,
        context: &VerifiedActiveRecoveryContextV2,
        seals: [&FencingVerdict; 2],
    ) -> Result<Self, FormalError> {
        use crate::controlled_authority::replicated_protocol::EndpointRole;
        if seals[0].seal_body().instance.endpoint_role != EndpointRole::Primary
            || seals[1].seal_body().instance.endpoint_role != EndpointRole::Replica
        {
            return Err(invalid("retained recovery ordered distinct seals"));
        }
        let digest = recovery_fencing_digest(seals)?;
        verify_retained_intent(body, context, &seals, &digest)
    }
    pub fn verify_retained_single_survivor(
        body: RecoveryCertificateBodyV1,
        context: &VerifiedActiveRecoveryContextV2,
        coverage: &VerifiedAppendFencingCoverageV1,
    ) -> Result<Self, FormalError> {
        if coverage.predecessor_authority_id != context.active().authority_id()?
            || coverage.cut != context.cut().head()
        {
            return Err(invalid("retained single survivor context"));
        }
        verify_retained_intent(
            body,
            context,
            &[&coverage.seal],
            &coverage.evidence_digest()?,
        )
    }
}
fn verify_retained_intent(
    body: RecoveryCertificateBodyV1,
    context: &VerifiedActiveRecoveryContextV2,
    seals: &[&FencingVerdict],
    evidence_digest: &str,
) -> Result<VerifiedRecoveryIntent, FormalError> {
    if body.schema_version != 2
        || body.retained_context.as_ref() != Some(context.commitment())
        || body.sealed_cut != context.cut().head()
        || body.fencing_evidence_digest != evidence_digest
    {
        return Err(invalid(
            "retained recovery exact prior permission commitment/cut",
        ));
    }
    validate_recovery_body_fields(
        &body,
        context.active().definition(),
        context.active().policy(),
        true,
    )?;
    let (position, reservation, record, handoff, rank) = match &body.tail {
        RecoveryTailV1::Empty => (None, None, None, None, 0),
        RecoveryTailV1::Frozen {
            position,
            reservation_id,
            handoff,
        } => (
            Some(*position),
            Some(reservation_id),
            None,
            handoff.as_ref(),
            1,
        ),
        RecoveryTailV1::Prepared {
            position,
            reservation_id,
            record_id,
            handoff,
        } => (
            Some(*position),
            Some(reservation_id),
            Some(record_id),
            handoff.as_ref(),
            2,
        ),
        RecoveryTailV1::Decision {
            position,
            reservation_id,
            record_id,
            handoff,
        } => (
            Some(*position),
            Some(reservation_id),
            Some(record_id),
            handoff.as_ref(),
            3,
        ),
    };
    let sealed_rank = |kind: &SealedTailKindV1| match kind {
        SealedTailKindV1::Empty => 0,
        SealedTailKindV1::Frozen => 1,
        SealedTailKindV1::Prepared => 2,
        SealedTailKindV1::Decision => 3,
    };
    if seals
        .iter()
        .map(|seal| sealed_rank(&seal.seal_body().tail.kind))
        .max()
        != Some(rank)
    {
        return Err(invalid(
            "retained recovery sealed tail cannot be downgraded",
        ));
    }
    for seal in seals {
        let active = context.active();
        let sealed = seal.seal_body();
        let instance = match sealed.instance.endpoint_role {
            crate::controlled_authority::replicated_protocol::EndpointRole::Primary => {
                &active.definition().primary.instance
            }
            crate::controlled_authority::replicated_protocol::EndpointRole::Replica => {
                &active.definition().replica.instance
            }
        };
        if seal.provider_public_key() != active.policy().fencing_provider_public_key
            || !active
                .policy()
                .authorized_fencing_resource_bindings
                .iter()
                .any(|resource| resource == seal.resource_binding())
        {
            return Err(FormalError::FencingUnproven);
        }
        if &sealed.instance != instance
            || sealed.recovery_operation != body.operation_id
            || sealed.highest_complete.position != body.sealed_cut.position
            || sealed.highest_complete.decision_hash != body.sealed_cut.record_id
            || sealed.next_epoch_floor != body.successor.epoch
            || sealed.tail.position != position
            || sealed.tail.frozen_input_digest.as_ref() != reservation
            || (sealed_rank(&sealed.tail.kind) >= 2
                && sealed.tail.proposal_digest.as_ref() != record)
        {
            return Err(invalid("retained recovery trusted active seal/cut/slot"));
        }
    }
    match &context.witness {
        RecoveryTailWitnessV1::Empty if rank == 0 => {}
        RecoveryTailWitnessV1::Frozen(frozen) if rank == 1 => {
            if Some(&frozen.reservation.body.reservation_id()?) != reservation
                || frozen.reservation.body.handoff.as_ref() != handoff
            {
                return Err(invalid("retained recovery original frozen tail"));
            }
        }
        RecoveryTailWitnessV1::Prepared { frozen, candidate } if rank == 2 => {
            exact_candidate_tail(frozen, candidate, reservation, record, handoff)?;
        }
        RecoveryTailWitnessV1::Decision { frozen, candidate } if rank == 3 => {
            exact_candidate_tail(frozen, candidate, reservation, record, handoff)?;
        }
        _ => {
            return Err(invalid(
                "retained recovery witness stage must match selected tail",
            ));
        }
    }
    let plan = super::super::recovery_plan::verify_recovery_plan_seals(
        &body,
        context.active(),
        seals,
        context.witness(),
    )?;
    Ok(VerifiedRecoveryIntent {
        body,
        policy: context.active().policy().clone(),
        retained: Some(context.clone()),
        plan,
        sealed_commit: context.cut().clone(),
    })
}
fn exact_candidate_tail(
    frozen: &FormalFrozenCertificateV1,
    candidate: &SignedCanonicalCandidateV1,
    reservation: Option<&String>,
    record: Option<&String>,
    handoff: Option<&ProfileHandoffV1>,
) -> Result<(), FormalError> {
    if Some(&frozen.reservation.body.reservation_id()?) != reservation
        || Some(&candidate.record.record_id()?) != record
        || frozen.reservation.body.handoff.as_ref() != handoff
    {
        return Err(invalid(
            "retained recovery exact candidate identity/handoff",
        ));
    }
    Ok(())
}
