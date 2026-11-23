//! The original epoch must finish its sole fixed continuation before sealing.
//! This evidence does not authorize a new request or establish a reader anchor.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryDrainedProofV1 {
    pub schema_version: u32,
    pub plan_digest: String,
    pub fixed: RecoveryTailWitnessV1,
    pub qualified: FormalQualifiedCommitProofV1,
}
impl RecoveryDrainedProofV1 {
    pub fn verify(
        &self,
        plan: &SignedRecoveryPlanV1,
        authority: &VerifiedAuthorityV1,
    ) -> Result<FormalHeadRefV1, FormalError> {
        plan.verify(authority)?;
        if self.schema_version != 1
            || self.plan_digest != plan.digest()?
            || recovery_requests_commitment(&self.fixed)? != plan.body.original_requests_commitment
            || recovery_tail_from_witness(&self.fixed)? != plan.body.intent.tail
        {
            return Err(invalid(
                "drained proof original plan and fixed continuation",
            ));
        }
        let head = super::super::proof::verify_recovery_drained_qualified(plan, authority, self)?;
        if head.position.checked_add(1) != Some(plan.body.intent.successor.valid_from_position)
            || head.position.checked_add(2) != plan.body.intent.first_normal_position
            || self.qualified.decision.record.handoff.is_some()
        {
            return Err(invalid(
                "drained proof fixed cut/control/business positions",
            ));
        }
        Ok(head)
    }
    pub fn digest(&self) -> Result<String, FormalError> {
        hash("oasis7.formal.recovery_drained.v1", self)
    }
}

/// Protected checkpoint metadata for the one allowlisted old-epoch request.
/// Original private closure bytes remain in the native CAS; these values bind
/// each signing purpose to that exact persisted candidate and decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryDrainProgressV1 {
    pub fixed: RecoveryTailWitnessV1,
    pub candidate: Option<SignedCanonicalCandidateV1>,
    pub candidate_material_digest: Option<String>,
    pub decision: Option<FormalDecisionProofV1>,
}
impl RecoveryDrainProgressV1 {
    pub fn verify(
        &self,
        plan: &SignedRecoveryPlanV1,
        authority: &VerifiedAuthorityV1,
    ) -> Result<(), FormalError> {
        if recovery_requests_commitment(&self.fixed)? != plan.body.original_requests_commitment
            || recovery_tail_from_witness(&self.fixed)? != plan.body.intent.tail
        {
            return Err(invalid("recovery progress changed original fixed witness"));
        }
        let original = match &self.fixed {
            RecoveryTailWitnessV1::Empty => {
                if self.candidate.is_some()
                    || self.candidate_material_digest.is_some()
                    || self.decision.is_some()
                {
                    return Err(invalid(
                        "empty recovery plan cannot introduce an old-epoch request",
                    ));
                }
                return Ok(());
            }
            RecoveryTailWitnessV1::Frozen(frozen) => frozen,
            RecoveryTailWitnessV1::Prepared { frozen, .. }
            | RecoveryTailWitnessV1::Decision { frozen, .. } => frozen,
        };
        original.verify(authority.definition(), authority.activation_ref())?;
        if original.reservation.body.parent != plan.body.intent.sealed_cut {
            return Err(invalid("recovery progress original parent cut"));
        }
        match (&self.candidate, &self.candidate_material_digest) {
            (Some(candidate), Some(material)) => {
                hash_hex(material)?;
                candidate.verify_original(authority)?;
                original.matches_record(&candidate.record)?;
                if let RecoveryTailWitnessV1::Prepared {
                    candidate: retained,
                    ..
                }
                | RecoveryTailWitnessV1::Decision {
                    candidate: retained,
                    ..
                } = &self.fixed
                    && candidate != retained
                {
                    return Err(invalid(
                        "recovery progress replaced a fixed original candidate",
                    ));
                }
            }
            (None, None) if matches!(self.fixed, RecoveryTailWitnessV1::Frozen(_)) => {}
            _ => {
                return Err(invalid(
                    "recovery progress complete candidate/material binding",
                ));
            }
        }
        if let Some(decision) = &self.decision {
            let candidate = self
                .candidate
                .as_ref()
                .ok_or_else(|| invalid("recovery decision without candidate"))?;
            if decision.record != candidate.record
                || decision.writer_signature_hex != candidate.writer_signature_hex
                || decision.frozen.as_ref() != Some(original)
            {
                return Err(invalid(
                    "recovery progress decision replaced original candidate",
                ));
            }
            super::super::proof::verify_recovery_drain_decision(plan, authority, decision)?;
        }
        Ok(())
    }
}
