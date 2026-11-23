#[path = "recovery_coverage.rs"]
mod coverage;
#[path = "recovery_retained.rs"]
mod retained;
use super::{
    crypto::*,
    fencing::{FencingVerdict, SealedTailKindV1},
    *,
};
pub use coverage::VerifiedAppendFencingCoverageV1;
pub use retained::{RetainedRecoveryContextV2, VerifiedActiveRecoveryContextV2};
use serde::{Deserialize, Serialize};

pub fn recovery_signing_bytes(body: &RecoveryCertificateBodyV1) -> Result<Vec<u8>, FormalError> {
    bytes(recovery_signing_domain(body)?, body)
}
pub fn recovery_body_digest(body: &RecoveryCertificateBodyV1) -> Result<String, FormalError> {
    hash(
        match body.schema_version {
            1 => "oasis7.formal.recovery_certificate.v1",
            2 => "oasis7.formal.recovery_certificate.v2",
            3 => "oasis7.formal.recovery_certificate.v3",
            _ => return Err(invalid("recovery certificate schema")),
        },
        body,
    )
}
fn recovery_signing_domain(body: &RecoveryCertificateBodyV1) -> Result<&'static str, FormalError> {
    match body.schema_version {
        1 => Ok("oasis7.formal.recovery_certificate.sign.v1"),
        2 => Ok("oasis7.formal.recovery_certificate.sign.v2"),
        3 => Ok("oasis7.formal.recovery_certificate.sign.v3"),
        _ => Err(invalid("recovery certificate schema")),
    }
}
/// Certificate identity is its canonical body, independent of its signature.
pub fn recovery_certificate_digest(
    certificate: &SignedRecoveryCertificateV1,
) -> Result<String, FormalError> {
    recovery_body_digest(&certificate.body)
}

/// Ordered primary and replica seals are jointly committed by the issuer.
pub fn recovery_fencing_digest(seals: [&FencingVerdict; 2]) -> Result<String, FormalError> {
    hash(
        "oasis7.formal.recovery_fencing_pair.v1",
        &[seals[0].seal_digest(), seals[1].seal_digest()],
    )
}

pub fn validate_recovery_body(
    body: &RecoveryCertificateBodyV1,
    predecessor: &AuthorityDefinitionV1,
    policy: &RecoveryPolicyV1,
) -> Result<(), FormalError> {
    if !matches!(body.schema_version, 1 | 3) || body.retained_context.is_some() {
        return Err(invalid("v1 recovery refuses retained-context extensions"));
    }
    validate_recovery_body_fields(body, predecessor, policy, false)
}
pub(super) fn validate_recovery_body_fields(
    body: &RecoveryCertificateBodyV1,
    predecessor: &AuthorityDefinitionV1,
    policy: &RecoveryPolicyV1,
    retained: bool,
) -> Result<(), FormalError> {
    predecessor.verify_policy(policy)?;
    body.successor.verify_policy(policy)?;
    for digest in [
        &body.operation_id,
        &body.predecessor_authority_id,
        &body.sealed_cut.record_id,
        &body.fencing_evidence_digest,
    ] {
        hash_hex(digest)?;
    }
    let next = body
        .sealed_cut
        .position
        .checked_add(1)
        .ok_or_else(|| invalid("recovery position overflow"))?;
    if (if retained {
        body.schema_version != 2
    } else {
        !matches!(body.schema_version, 1 | 3)
    }) || body.world != predecessor.world
        || body.successor.world != body.world
        || body.successor.profile_version != predecessor.profile_version
        || body.predecessor_authority_id != predecessor.authority_id()?
        || body.predecessor_epoch != predecessor.epoch
        || predecessor.epoch.checked_add(1) != Some(body.successor.epoch)
        || body.successor.predecessor_authority_id.as_ref() != Some(&body.predecessor_authority_id)
        || body.successor.valid_from_position != next
        || (!retained && body.sealed_cut.position < predecessor.valid_from_position)
    {
        return Err(invalid(
            "recovery predecessor/world/epoch/cut/successor boundary",
        ));
    }
    if body.schema_version == 3 && body.tail != RecoveryTailV1::Empty {
        return Err(invalid(
            "planned recovery must drain the old epoch before activation",
        ));
    }
    // An old signing identity or storage instance cannot be revived in a new epoch.
    for successor in [
        &body.successor.primary.instance,
        &body.successor.replica.instance,
    ] {
        for old in [&predecessor.primary.instance, &predecessor.replica.instance] {
            if successor.endpoint_public_key == old.endpoint_public_key
                || successor.instance_id == old.instance_id
                || successor.storage_id == old.storage_id
            {
                return Err(invalid(
                    "recovery requires new endpoint key/instance/storage",
                ));
            }
        }
    }
    if body.successor.writer_public_key == predecessor.writer_public_key {
        return Err(invalid("recovery requires new writer key"));
    }
    let (tail_position, reservation, record, handoff) = match &body.tail {
        RecoveryTailV1::Empty => (None, None, None, None),
        RecoveryTailV1::Frozen {
            position,
            reservation_id,
            handoff,
        } => {
            if !policy.allow_replay_frozen {
                return Err(invalid("replay frozen disabled"));
            }
            (
                Some(*position),
                Some(reservation_id),
                None,
                handoff.as_ref(),
            )
        }
        RecoveryTailV1::Prepared {
            position,
            reservation_id,
            record_id,
            handoff,
        }
        | RecoveryTailV1::Decision {
            position,
            reservation_id,
            record_id,
            handoff,
        } => {
            if !policy.allow_complete_prepared {
                return Err(invalid("complete prepared disabled"));
            }
            (
                Some(*position),
                Some(reservation_id),
                Some(record_id),
                handoff.as_ref(),
            )
        }
    };
    if tail_position.is_some_and(|position| position != next) {
        return Err(invalid("recovery tail must immediately follow cut"));
    }
    if let Some(digest) = reservation {
        hash_hex(digest)?;
    }
    if let Some(digest) = record {
        hash_hex(digest)?;
    }
    if let Some(handoff) = handoff {
        handoff.validate(next)?;
    }
    let expected = if body.schema_version == 3 {
        next.checked_add(1)
    } else if handoff.is_some() {
        None
    } else if tail_position.is_some() {
        next.checked_add(1)
    } else {
        Some(next)
    };
    if body.first_normal_position != expected {
        return Err(invalid("recovery fresh-write boundary/handoff termination"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryTailWitnessV1 {
    Empty,
    Frozen(FormalFrozenCertificateV1),
    Prepared {
        frozen: FormalFrozenCertificateV1,
        candidate: SignedCanonicalCandidateV1,
    },
    Decision {
        frozen: FormalFrozenCertificateV1,
        candidate: SignedCanonicalCandidateV1,
    },
}
/// This private token is the only production issuance input. It binds a proven
/// immutable cut, a trusted seal and original signed tail before issuer signing.
#[derive(Debug, Clone)]
pub struct VerifiedRecoveryIntent {
    body: RecoveryCertificateBodyV1,
    policy: RecoveryPolicyV1,
    retained: Option<VerifiedActiveRecoveryContextV2>,
    plan: Option<SignedRecoveryPlanV1>,
    sealed_commit: VerifiedFormalCommitV1,
}
impl VerifiedRecoveryIntent {
    pub fn body(&self) -> &RecoveryCertificateBodyV1 {
        &self.body
    }
    pub fn policy(&self) -> &RecoveryPolicyV1 {
        &self.policy
    }
    pub fn recovery_plan(&self) -> Option<&SignedRecoveryPlanV1> {
        self.plan.as_ref()
    }
    pub fn verify(
        body: RecoveryCertificateBodyV1,
        predecessor: &VerifiedAuthorityV1,
        cut: &VerifiedFormalCommitV1,
        seals: [&FencingVerdict; 2],
        witness: &RecoveryTailWitnessV1,
    ) -> Result<Self, FormalError> {
        use crate::controlled_authority::replicated_protocol::EndpointRole;
        if seals[0].seal_body().instance.endpoint_role != EndpointRole::Primary
            || seals[1].seal_body().instance.endpoint_role != EndpointRole::Replica
        {
            return Err(invalid("recovery ordered distinct endpoint seals required"));
        }
        let digest = recovery_fencing_digest(seals)?;
        Self::verify_seals(body, predecessor, cut, &seals, &digest, witness)
    }
    pub fn verify_single_survivor(
        body: RecoveryCertificateBodyV1,
        predecessor: &VerifiedAuthorityV1,
        cut: &VerifiedFormalCommitV1,
        coverage: &VerifiedAppendFencingCoverageV1,
        witness: &RecoveryTailWitnessV1,
    ) -> Result<Self, FormalError> {
        if coverage.predecessor_authority_id != predecessor.authority_id()?
            || coverage.cut != cut.head()
        {
            return Err(invalid("single-survivor fencing coverage context"));
        }
        let digest = coverage.evidence_digest()?;
        Self::verify_seals(body, predecessor, cut, &[&coverage.seal], &digest, witness)
    }
    fn verify_seals(
        body: RecoveryCertificateBodyV1,
        predecessor: &VerifiedAuthorityV1,
        cut: &VerifiedFormalCommitV1,
        seals: &[&FencingVerdict],
        evidence_digest: &str,
        witness: &RecoveryTailWitnessV1,
    ) -> Result<Self, FormalError> {
        validate_recovery_body(&body, predecessor.definition(), predecessor.policy())?;
        if cut.head() != body.sealed_cut
            || cut.finalizing_authority_id() != body.predecessor_authority_id
            || cut.record().handoff.is_some()
        {
            return Err(invalid(
                "recovery cut is not active predecessor or profile already closed",
            ));
        }
        for seal in seals {
            let expected_instance = match seal.seal_body().instance.endpoint_role {
                crate::controlled_authority::replicated_protocol::EndpointRole::Primary => {
                    &predecessor.definition().primary.instance
                }
                crate::controlled_authority::replicated_protocol::EndpointRole::Replica => {
                    &predecessor.definition().replica.instance
                }
            };
            if seal.provider_public_key() != predecessor.policy().fencing_provider_public_key {
                return Err(FormalError::FencingUnproven);
            }
            if !predecessor
                .policy()
                .authorized_fencing_resource_bindings
                .iter()
                .any(|resource| resource == seal.resource_binding())
            {
                return Err(FormalError::FencingUnproven);
            }
            let sealed = seal.seal_body();
            if &sealed.instance != expected_instance
                || sealed.recovery_operation != body.operation_id
                || sealed.highest_complete.position != body.sealed_cut.position
                || sealed.highest_complete.decision_hash != body.sealed_cut.record_id
                || sealed.next_epoch_floor != body.successor.epoch
            {
                return Err(invalid(
                    "recovery trusted seal instance/cut/operation/floor mismatch",
                ));
            }
        }
        if body.fencing_evidence_digest != evidence_digest {
            return Err(invalid("recovery both-seal evidence binding"));
        }
        let (kind, position, reservation, record) = match &body.tail {
            RecoveryTailV1::Empty => (SealedTailKindV1::Empty, None, None, None),
            RecoveryTailV1::Frozen {
                position,
                reservation_id,
                ..
            } => (
                SealedTailKindV1::Frozen,
                Some(*position),
                Some(reservation_id),
                None,
            ),
            RecoveryTailV1::Prepared {
                position,
                reservation_id,
                record_id,
                ..
            } => (
                SealedTailKindV1::Prepared,
                Some(*position),
                Some(reservation_id),
                Some(record_id),
            ),
            RecoveryTailV1::Decision {
                position,
                reservation_id,
                record_id,
                ..
            } => (
                SealedTailKindV1::Decision,
                Some(*position),
                Some(reservation_id),
                Some(record_id),
            ),
        };
        let rank = |kind: &SealedTailKindV1| match kind {
            SealedTailKindV1::Empty => 0,
            SealedTailKindV1::Frozen => 1,
            SealedTailKindV1::Prepared => 2,
            SealedTailKindV1::Decision => 3,
        };
        if seals
            .iter()
            .map(|seal| rank(&seal.seal_body().tail.kind))
            .max()
            != Some(rank(&kind))
        {
            return Err(invalid("recovery sealed tail cannot be downgraded"));
        }
        for seal in seals {
            let tail = &seal.seal_body().tail;
            if tail.position != position
                || tail.frozen_input_digest.as_ref() != reservation
                || (rank(&tail.kind) >= 2 && tail.proposal_digest.as_ref() != record)
            {
                return Err(invalid("recovery endpoint sealed tails disagree"));
            }
        }
        verify_tail_witness(&body, predecessor, witness)?;
        match witness {
            RecoveryTailWitnessV1::Empty => {}
            RecoveryTailWitnessV1::Frozen(frozen)
            | RecoveryTailWitnessV1::Prepared { frozen, .. }
            | RecoveryTailWitnessV1::Decision { frozen, .. } => {
                verify_formal_reservation(&frozen.reservation, predecessor, cut)?;
            }
        }
        let plan =
            super::recovery_plan::verify_recovery_plan_seals(&body, predecessor, seals, witness)?;
        Ok(Self {
            body,
            policy: predecessor.policy().clone(),
            retained: None,
            plan,
            sealed_commit: cut.clone(),
        })
    }
}
pub(super) fn verify_tail_witness(
    body: &RecoveryCertificateBodyV1,
    authority: &VerifiedAuthorityV1,
    witness: &RecoveryTailWitnessV1,
) -> Result<(), FormalError> {
    let (frozen, candidate) = match (&body.tail, witness) {
        (RecoveryTailV1::Empty, RecoveryTailWitnessV1::Empty) => return Ok(()),
        (RecoveryTailV1::Frozen { .. }, RecoveryTailWitnessV1::Frozen(frozen)) => (frozen, None),
        (
            RecoveryTailV1::Prepared { .. },
            RecoveryTailWitnessV1::Prepared { frozen, candidate },
        )
        | (
            RecoveryTailV1::Decision { .. },
            RecoveryTailWitnessV1::Decision { frozen, candidate },
        ) => (frozen, Some(candidate)),
        _ => return Err(invalid("recovery original tail witness branch mismatch")),
    };
    frozen.verify(authority.definition(), authority.activation_ref())?;
    let reservation = &frozen.reservation.body;
    let (position, id, handoff) = match &body.tail {
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
        } => (*position, reservation_id, handoff),
        RecoveryTailV1::Empty => unreachable!(),
    };
    if reservation.parent != body.sealed_cut
        || reservation.next_position != position
        || &reservation.reservation_id()? != id
        || &reservation.handoff != handoff
    {
        return Err(invalid("recovery signed frozen cut/tail/handoff mismatch"));
    }
    if let Some(candidate) = candidate {
        candidate.verify_original(authority)?;
        frozen.matches_record(&candidate.record)?;
        let expected = match &body.tail {
            RecoveryTailV1::Prepared { record_id, .. }
            | RecoveryTailV1::Decision { record_id, .. } => record_id,
            _ => unreachable!(),
        };
        if &candidate.record.record_id()? != expected {
            return Err(invalid("recovery retained candidate identity mismatch"));
        }
    }
    Ok(())
}
#[derive(Debug, Clone)]
pub struct VerifiedRecovery {
    pub(crate) body: RecoveryCertificateBodyV1,
    pub(crate) predecessor: VerifiedAuthorityV1,
    pub(crate) successor: VerifiedAuthorityV1,
    pub(crate) certificate_digest: String,
    pub(crate) original: VerifiedAuthorityV1,
    pub(crate) retained_candidate: Option<SignedCanonicalCandidateV1>,
    pub(crate) plan: Option<SignedRecoveryPlanV1>,
    sealed_commit: VerifiedFormalCommitV1,
}
impl VerifiedRecovery {
    /// The exact opaque Qualified cut supplied to, and checked by, the native
    /// recovery intent verifier. This is not reconstructed from plan fields.
    pub fn sealed_commit(&self) -> &VerifiedFormalCommitV1 {
        &self.sealed_commit
    }
    pub fn recovery_plan(&self) -> Option<&SignedRecoveryPlanV1> {
        self.plan.as_ref()
    }
    pub fn original_authority(&self) -> &VerifiedAuthorityV1 {
        &self.original
    }
    pub fn predecessor(&self) -> &VerifiedAuthorityV1 {
        &self.predecessor
    }
    pub fn body(&self) -> &RecoveryCertificateBodyV1 {
        &self.body
    }
    pub fn successor(&self) -> &VerifiedAuthorityV1 {
        &self.successor
    }
    pub fn certificate_digest(&self) -> &str {
        &self.certificate_digest
    }
    pub fn verify(
        certificate: &SignedRecoveryCertificateV1,
        intent: &VerifiedRecoveryIntent,
        predecessor: &VerifiedAuthorityV1,
        issuance: &issuer::VerifiedIssuance,
    ) -> Result<Self, FormalError> {
        if &certificate.body != intent.body() || intent.policy() != predecessor.policy() {
            return Err(invalid("recovery signed intent mismatch"));
        }
        let (original, retained_candidate) = if let Some(context) = &intent.retained {
            if certificate.body.schema_version != 2
                || certificate.body.retained_context.as_ref() != Some(context.commitment())
                || predecessor.authority_id()? != context.active().authority_id()?
            {
                return Err(invalid("retained signed permission context"));
            }
            validate_recovery_body_fields(
                &certificate.body,
                predecessor.definition(),
                predecessor.policy(),
                true,
            )?;
            (
                context.original().clone(),
                context.retained_candidate().cloned(),
            )
        } else {
            validate_recovery_body(
                &certificate.body,
                predecessor.definition(),
                predecessor.policy(),
            )?;
            (predecessor.clone(), None)
        };
        issuance.verify_policy(predecessor.policy())?;
        let digest = recovery_certificate_digest(certificate)?;
        if issuance.certificate_digest() != digest
            || issuance.predecessor_authority_id() != certificate.body.predecessor_authority_id
        {
            return Err(FormalError::IssuerUnproven);
        }
        verify(
            recovery_signing_domain(&certificate.body)?,
            &certificate.body,
            &certificate.signature_hex,
            &intent.policy().recovery_issuer_public_key,
        )?;
        let successor = VerifiedAuthorityV1 {
            definition: certificate.body.successor.clone(),
            policy: intent.policy().clone(),
            activation_ref: predecessor.activation_ref().to_owned(),
            durable_set_activation_digest: predecessor.durable_set_activation_digest.clone(),
            initial: None,
            first_normal_position: certificate.body.first_normal_position,
            recovery_cut: Some(certificate.body.sealed_cut.clone()),
            recovery_retained_record: match &certificate.body.tail {
                RecoveryTailV1::Prepared { record_id, .. }
                | RecoveryTailV1::Decision { record_id, .. } => Some(record_id.clone()),
                _ => None,
            },
            initial_input_digest: None,
            recovery_activation: if certificate.body.schema_version == 3 {
                let plan = intent.plan.as_ref().ok_or_else(|| {
                    invalid("planned recovery activation requires original issuer plan")
                })?;
                Some(RecoveryActivationAuthorizationV1 {
                    certificate_digest: digest.clone(),
                    plan_digest: plan.digest()?,
                    pending_handoff_root: plan.body.pending_handoff_root.clone(),
                })
            } else {
                None
            },
        };
        Ok(Self {
            body: certificate.body.clone(),
            predecessor: predecessor.clone(),
            successor,
            certificate_digest: digest,
            original,
            retained_candidate,
            plan: intent.plan.clone(),
            sealed_commit: intent.sealed_commit.clone(),
        })
    }
}
