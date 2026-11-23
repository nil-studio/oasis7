//! The issuer fixes the complete successor before any native epoch seal.
//! Private pending requests are an explicit, authenticated handoff collection;
//! absence of that collection is never the empty collection.
use super::{crypto::*, *};
use crate::controlled_authority::LocalRequestIdentity;
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

const PLAN_DOMAIN: &str = "oasis7.formal.recovery_plan.sign.v1";
const PENDING_LIMIT: usize = 16 * 1024 * 1024;
const PENDING_COUNT_LIMIT: usize = 4096;
/// The canonical Vec<u8> encoding stays unchanged: each byte may require two
/// CBOR bytes. Identity CBOR is already included in the 16 MiB admission bound;
/// each request adds at most 32 bytes of map/field/array framing, plus fewer
/// than 64 bytes for the collection. Thus every admitted value fits within
/// 2 * 16 MiB + 4096 * 32 + 64 bytes, strictly below this shared storage bound.
pub const RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1: usize = 34 * 1024 * 1024;
#[path = "recovery_plan_guard.rs"]
mod guard;
pub use guard::{
    validate_recovery_checkpoint_transition, validate_recovery_protected_signing_request,
    validate_recovery_provider_credential,
};
#[path = "recovery_plan_drain.rs"]
mod drain;
pub use drain::{RecoveryDrainProgressV1, RecoveryDrainedProofV1};
#[path = "recovery_plan_legacy_cut.rs"]
mod legacy_cut;
pub use legacy_cut::VerifiedRecoveryPlanLegacyCutV1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPendingRequestV1 {
    pub identity: LocalRequestIdentity,
    /// Original authenticated wire bytes, including their original nonce and
    /// signature. A plan does not authorize the payload's eventual execution.
    pub signed_request: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPendingHandoffV1 {
    pub schema_version: u32,
    pub requests: Vec<RecoveryPendingRequestV1>,
}
impl RecoveryPendingHandoffV1 {
    pub fn empty() -> Self {
        Self {
            schema_version: 1,
            requests: Vec::new(),
        }
    }
    pub fn from_requests(requests: Vec<RecoveryPendingRequestV1>) -> Result<Self, FormalError> {
        let mut ordered = requests
            .into_iter()
            .map(|request| {
                Ok((
                    bytes("oasis7.formal.pending_identity.v1", &request.identity)?,
                    request,
                ))
            })
            .collect::<Result<Vec<_>, FormalError>>()?;
        ordered.sort_by(|left, right| left.0.cmp(&right.0));
        let pending = Self {
            schema_version: 1,
            requests: ordered.into_iter().map(|(_, request)| request).collect(),
        };
        pending.validate()?;
        Ok(pending)
    }
    pub fn validate(&self) -> Result<(), FormalError> {
        if self.schema_version != 1 || self.requests.len() > PENDING_COUNT_LIMIT {
            return Err(invalid("recovery pending schema/count"));
        }
        let mut previous: Option<Vec<u8>> = None;
        let mut size = 0usize;
        for request in &self.requests {
            let identity = &request.identity;
            let encoded_identity = bytes("oasis7.formal.pending_identity.v1", identity)?;
            if previous
                .as_ref()
                .is_some_and(|old| old >= &encoded_identity)
                || [
                    &identity.verified_subject,
                    &identity.operation_domain,
                    &identity.nonce_scope,
                    &identity.request_id,
                ]
                .iter()
                .any(|field| field.is_empty() || field.len() > 4096)
                || request.signed_request.is_empty()
            {
                return Err(invalid("recovery pending original identity/order/payload"));
            }
            size = size
                .checked_add(
                    request
                        .signed_request
                        .len()
                        .saturating_add(encoded_identity.len()),
                )
                .ok_or_else(|| invalid("recovery pending size overflow"))?;
            if size > PENDING_LIMIT {
                return Err(invalid("recovery pending byte bound"));
            }
            previous = Some(encoded_identity);
        }
        Ok(())
    }
    pub fn root(&self) -> Result<String, FormalError> {
        self.validate()?;
        hash("oasis7.formal.pending_handoff.v1", self)
    }
}

pub fn recovery_requests_commitment(
    witness: &RecoveryTailWitnessV1,
) -> Result<String, FormalError> {
    let (frozen, candidate) = match witness {
        RecoveryTailWitnessV1::Empty => (None, None),
        RecoveryTailWitnessV1::Frozen(frozen) => (Some(frozen), None),
        RecoveryTailWitnessV1::Prepared { frozen, candidate }
        | RecoveryTailWitnessV1::Decision { frozen, candidate } => (Some(frozen), Some(candidate)),
    };
    hash("oasis7.formal.recovery_requests.v1", &(frozen, candidate))
}

fn unsealed_fencing_marker() -> Result<String, FormalError> {
    hash("oasis7.formal.recovery_plan.unsealed.v1", &())
}

/// The future certificate body is retained verbatim except for the fencing
/// digest, which cannot exist before the seals. Its explicit marker is not a
/// fencing proof and can never authorize a recovery certificate by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPlanBodyV1 {
    pub schema_version: u32,
    pub predecessor_activation_ref: String,
    pub original_cut: CanonicalRecordV1,
    /// A full legacy Qualified proof verified before this issuer plan. Normal
    /// cuts retain their existing canonical plan bytes through omitted None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_qualified_proof_digest: Option<String>,
    pub authorization: RecoveryPlanAuthorityV1,
    pub intent: RecoveryCertificateBodyV1,
    pub original_requests_commitment: String,
    pub pending_handoff_root: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPlanAuthorityV1 {
    pub recovery_credential_public_key: String,
    /// Reference to deployment isolation evidence. A controlled-file software
    /// test supplies software revocation evidence, never hardware attestation.
    pub isolation_evidence_digest: String,
}
impl RecoveryPlanBodyV1 {
    pub fn new(
        mut intent: RecoveryCertificateBodyV1,
        predecessor: &VerifiedAuthorityV1,
        cut: &VerifiedFormalCommitV1,
        witness: &RecoveryTailWitnessV1,
        pending: &RecoveryPendingHandoffV1,
        authorization: RecoveryPlanAuthorityV1,
    ) -> Result<Self, FormalError> {
        intent.fencing_evidence_digest = unsealed_fencing_marker()?;
        intent.schema_version = if intent.retained_context.is_some() {
            2
        } else {
            1
        };
        let final_position = cut
            .head()
            .position
            .checked_add(u64::from(!matches!(witness, RecoveryTailWitnessV1::Empty)))
            .ok_or_else(|| invalid("recovery drained position overflow"))?;
        intent.successor.valid_from_position = final_position
            .checked_add(1)
            .ok_or_else(|| invalid("recovery activation position overflow"))?;
        intent.first_normal_position = intent.successor.valid_from_position.checked_add(1);
        if intent.first_normal_position.is_none() {
            return Err(invalid("recovery first business position overflow"));
        }
        Ok(Self {
            schema_version: 1,
            predecessor_activation_ref: predecessor.activation_ref().into(),
            original_cut: cut.record().clone(),
            original_qualified_proof_digest: None,
            authorization,
            intent,
            original_requests_commitment: recovery_requests_commitment(witness)?,
            pending_handoff_root: pending.root()?,
        })
    }
    pub fn with_verified_legacy_cut(
        mut self,
        legacy: &VerifiedRecoveryPlanLegacyCutV1,
    ) -> Result<Self, FormalError> {
        if self.original_cut != *legacy.cut.record() || self.intent.sealed_cut != legacy.cut.head()
        {
            return Err(invalid(
                "legacy plan cut differs from original verified Qualified",
            ));
        }
        self.original_qualified_proof_digest = Some(legacy.proof_digest.clone());
        Ok(self)
    }
    pub fn validate(&self, predecessor: &VerifiedAuthorityV1) -> Result<(), FormalError> {
        if self.schema_version != 1
            || predecessor.definition().profile_version != 2
            || self.predecessor_activation_ref != predecessor.activation_ref()
            || self.intent.fencing_evidence_digest != unsealed_fencing_marker()?
        {
            return Err(invalid("recovery plan schema/profile/original activation"));
        }
        hash_hex(&self.original_requests_commitment)?;
        hash_hex(&self.pending_handoff_root)?;
        public_key(&self.authorization.recovery_credential_public_key)?;
        hash_hex(&self.authorization.isolation_evidence_digest)?;
        self.original_cut.validate()?;
        let legacy_original = self.original_cut.original_authority_id
            != predecessor.authority_id()?
            || self.original_cut.original_epoch != predecessor.definition().epoch;
        match (&self.original_qualified_proof_digest, legacy_original) {
            (Some(digest), true)
                if self.original_cut.original_epoch < predecessor.definition().epoch =>
            {
                hash_hex(digest)?
            }
            (None, false) => {}
            _ => {
                return Err(invalid(
                    "recovery plan exact legacy Qualified commitment required",
                ));
            }
        }
        if self.original_cut.position != self.intent.sealed_cut.position
            || self.original_cut.record_id()? != self.intent.sealed_cut.record_id
            || self.original_cut.world != self.intent.world
            || self.original_cut.activation_ref != self.predecessor_activation_ref
            || self.original_cut.handoff.is_some()
            || self.authorization.recovery_credential_public_key
                == predecessor.definition().writer_public_key
        {
            return Err(invalid(
                "recovery original cut or restricted recovery principal",
            ));
        }
        let final_position = self
            .intent
            .sealed_cut
            .position
            .checked_add(u64::from(self.intent.tail != RecoveryTailV1::Empty))
            .ok_or_else(|| invalid("recovery drained position overflow"))?;
        if Some(self.intent.successor.valid_from_position) != final_position.checked_add(1)
            || self.intent.first_normal_position != final_position.checked_add(2)
        {
            return Err(invalid(
                "recovery preplan fixes control and first business positions",
            ));
        }
        // Preserve the old certificate verifier's exact legacy rules. The
        // preplan has a future control slot and is not itself a certificate.
        let mut legacy = self.intent.clone();
        legacy.successor.valid_from_position = self
            .intent
            .sealed_cut
            .position
            .checked_add(1)
            .ok_or_else(|| invalid("recovery original next position overflow"))?;
        legacy.first_normal_position = if legacy.tail == RecoveryTailV1::Empty {
            Some(legacy.successor.valid_from_position)
        } else {
            legacy.successor.valid_from_position.checked_add(1)
        };
        super::recovery::validate_recovery_body_fields(
            &legacy,
            predecessor.definition(),
            predecessor.policy(),
            self.intent.retained_context.is_some(),
        )
    }
    pub fn matches_intent(&self, intent: &RecoveryCertificateBodyV1) -> Result<(), FormalError> {
        let mut planned = intent.clone();
        planned.fencing_evidence_digest = unsealed_fencing_marker()?;
        planned.sealed_cut = self.intent.sealed_cut.clone();
        planned.tail = self.intent.tail.clone();
        planned.schema_version = self.intent.schema_version;
        planned.retained_context = self.intent.retained_context.clone();
        if intent.schema_version != 3
            || intent.tail != RecoveryTailV1::Empty
            || intent.retained_context.is_some()
            || intent.sealed_cut.position.checked_add(1)
                != Some(self.intent.successor.valid_from_position)
            || self.intent != planned
        {
            return Err(invalid(
                "recovery certificate differs from the pre-seal original plan",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedRecoveryPlanV1 {
    body: RecoveryPlanBodyV1,
    policy: RecoveryPolicyV1,
}
impl VerifiedRecoveryPlanV1 {
    pub fn verify(
        body: RecoveryPlanBodyV1,
        predecessor: &VerifiedAuthorityV1,
        cut: &VerifiedFormalCommitV1,
        witness: &RecoveryTailWitnessV1,
        pending: &RecoveryPendingHandoffV1,
    ) -> Result<Self, FormalError> {
        Self::verify_inner(body, predecessor, cut, witness, pending, None)
    }
    pub fn verify_after_legacy_completion(
        body: RecoveryPlanBodyV1,
        predecessor: &VerifiedAuthorityV1,
        cut: &VerifiedFormalCommitV1,
        witness: &RecoveryTailWitnessV1,
        pending: &RecoveryPendingHandoffV1,
        legacy: &VerifiedRecoveryPlanLegacyCutV1,
    ) -> Result<Self, FormalError> {
        Self::verify_inner(body, predecessor, cut, witness, pending, Some(legacy))
    }
    fn verify_inner(
        body: RecoveryPlanBodyV1,
        predecessor: &VerifiedAuthorityV1,
        cut: &VerifiedFormalCommitV1,
        witness: &RecoveryTailWitnessV1,
        pending: &RecoveryPendingHandoffV1,
        legacy: Option<&VerifiedRecoveryPlanLegacyCutV1>,
    ) -> Result<Self, FormalError> {
        body.validate(predecessor)?;
        match (&body.original_qualified_proof_digest, legacy) {
            (None, None) => {}
            (Some(digest), Some(legacy))
                if digest == &legacy.proof_digest
                    && legacy.cut.record() == cut.record()
                    && legacy.cut.finalizing_authority_id() == cut.finalizing_authority_id() => {}
            _ => {
                return Err(invalid(
                    "issuer plan requires the fully verified legacy proof token",
                ));
            }
        }
        if body.intent.retained_context.is_some()
            || cut.head() != body.intent.sealed_cut
            || cut.record() != &body.original_cut
            || cut.finalizing_authority_id() != predecessor.authority_id()?
            || cut.record().handoff.is_some()
        {
            return Err(invalid(
                "recovery plan requires the exact active cut before profile handoff",
            ));
        }
        super::recovery::verify_tail_witness(&body.intent, predecessor, witness)?;
        if let RecoveryTailWitnessV1::Frozen(frozen)
        | RecoveryTailWitnessV1::Prepared { frozen, .. }
        | RecoveryTailWitnessV1::Decision { frozen, .. } = witness
        {
            verify_formal_reservation(&frozen.reservation, predecessor, cut)?;
        }
        Self::finish(body, predecessor, witness, pending)
    }
    pub fn verify_retained(
        body: RecoveryPlanBodyV1,
        context: &VerifiedActiveRecoveryContextV2,
        pending: &RecoveryPendingHandoffV1,
    ) -> Result<Self, FormalError> {
        body.validate(context.active())?;
        if body.intent.retained_context.as_ref() != Some(context.commitment())
            || body.intent.sealed_cut != context.cut().head()
            || context.cut().record().handoff.is_some()
        {
            return Err(invalid("recovery plan original retained context/cut"));
        }
        // The context already verified the original signed tail, including its
        // earlier authority. The new plan may not change its stage or identity.
        let tail = recovery_tail_from_witness(context.witness())?;
        if body.intent.tail != tail {
            return Err(invalid("recovery plan original retained tail"));
        }
        Self::finish(body, context.active(), context.witness(), pending)
    }
    fn finish(
        body: RecoveryPlanBodyV1,
        predecessor: &VerifiedAuthorityV1,
        witness: &RecoveryTailWitnessV1,
        pending: &RecoveryPendingHandoffV1,
    ) -> Result<Self, FormalError> {
        if body.original_requests_commitment != recovery_requests_commitment(witness)?
            || body.pending_handoff_root != pending.root()?
        {
            return Err(invalid(
                "recovery plan private original request/pending commitment",
            ));
        }
        Ok(Self {
            body,
            policy: predecessor.policy().clone(),
        })
    }
    pub fn body(&self) -> &RecoveryPlanBodyV1 {
        &self.body
    }
    pub fn policy(&self) -> &RecoveryPolicyV1 {
        &self.policy
    }
}

pub fn recovery_tail_from_witness(
    witness: &RecoveryTailWitnessV1,
) -> Result<RecoveryTailV1, FormalError> {
    let (frozen, candidate, stage) = match witness {
        RecoveryTailWitnessV1::Empty => return Ok(RecoveryTailV1::Empty),
        RecoveryTailWitnessV1::Frozen(frozen) => (frozen, None, 0),
        RecoveryTailWitnessV1::Prepared { frozen, candidate } => (frozen, Some(candidate), 1),
        RecoveryTailWitnessV1::Decision { frozen, candidate } => (frozen, Some(candidate), 2),
    };
    let body = &frozen.reservation.body;
    let position = body.next_position;
    let reservation_id = body.reservation_id()?;
    let handoff = body.handoff.clone();
    match candidate {
        None => Ok(RecoveryTailV1::Frozen {
            position,
            reservation_id,
            handoff,
        }),
        Some(candidate) if stage == 1 => Ok(RecoveryTailV1::Prepared {
            position,
            reservation_id,
            record_id: candidate.record.record_id()?,
            handoff,
        }),
        Some(candidate) => Ok(RecoveryTailV1::Decision {
            position,
            reservation_id,
            record_id: candidate.record.record_id()?,
            handoff,
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRecoveryPlanV1 {
    pub body: RecoveryPlanBodyV1,
    pub signature_hex: String,
}
impl SignedRecoveryPlanV1 {
    /// Validate only the allowlisted old-epoch decision. No reader anchor or
    /// generic authority token can be constructed through this service API.
    pub fn verify_drain_decision(
        &self,
        authority: &VerifiedAuthorityV1,
        decision: &FormalDecisionProofV1,
    ) -> Result<(), FormalError> {
        self.verify(authority)?;
        let frozen = decision
            .frozen
            .clone()
            .ok_or_else(|| invalid("draining decision original Frozen certificate missing"))?;
        let candidate = SignedCanonicalCandidateV1 {
            record: decision.record.clone(),
            writer_signature_hex: decision.writer_signature_hex.clone(),
        };
        let witness = match self.body.intent.tail {
            RecoveryTailV1::Empty => {
                return Err(invalid("empty plan does not authorize a draining decision"));
            }
            RecoveryTailV1::Frozen { .. } => RecoveryTailWitnessV1::Frozen(frozen),
            RecoveryTailV1::Prepared { .. } => {
                RecoveryTailWitnessV1::Prepared { frozen, candidate }
            }
            RecoveryTailV1::Decision { .. } => {
                RecoveryTailWitnessV1::Decision { frozen, candidate }
            }
        };
        if recovery_requests_commitment(&witness)? != self.body.original_requests_commitment {
            return Err(invalid(
                "draining decision differs from the planned original request",
            ));
        }
        super::proof::verify_recovery_drain_decision(self, authority, decision)
    }
    pub fn sign(plan: &VerifiedRecoveryPlanV1, key: &SigningKey) -> Result<Self, FormalError> {
        if hex::encode(key.verifying_key().to_bytes()) != plan.policy.recovery_issuer_public_key {
            return Err(invalid("recovery plan issuer signer"));
        }
        Ok(Self {
            body: plan.body.clone(),
            signature_hex: hex::encode(key.sign(&bytes(PLAN_DOMAIN, &plan.body)?).to_bytes()),
        })
    }
    pub fn verify(&self, predecessor: &VerifiedAuthorityV1) -> Result<(), FormalError> {
        self.body.validate(predecessor)?;
        verify(
            PLAN_DOMAIN,
            &self.body,
            &self.signature_hex,
            &predecessor.policy().recovery_issuer_public_key,
        )
    }
    pub fn digest(&self) -> Result<String, FormalError> {
        hash("oasis7.formal.recovery_plan.v1", &self.body)
    }
}

/// Legacy seals remain readable, but callers creating a new native profile2
/// recovery must independently require a plan. Any new plan-bearing seal is
/// checked even by historical proof consumers.
pub(crate) fn verify_recovery_plan_seals(
    body: &RecoveryCertificateBodyV1,
    predecessor: &VerifiedAuthorityV1,
    seals: &[&fencing::FencingVerdict],
    witness: &RecoveryTailWitnessV1,
) -> Result<Option<SignedRecoveryPlanV1>, FormalError> {
    let mut original: Option<&SignedRecoveryPlanV1> = None;
    let mut original_writer_seal: Option<&SignedWriterEpochSealV1> = None;
    let mut absent = false;
    for seal in seals {
        if let Some(plan) = &seal.seal_body().recovery_plan {
            plan.verify(predecessor)?;
            let writer =
                seal.seal_body().writer_seal.as_ref().ok_or_else(|| {
                    invalid("planned recovery requires the independent writer seal")
                })?;
            writer.verify(plan, predecessor)?;
            plan.body.matches_intent(body)?;
            if !matches!(witness, RecoveryTailWitnessV1::Empty)
                || writer.drained.verify(plan, predecessor)? != body.sealed_cut
                || original.is_some_and(|first| first != plan)
                || original_writer_seal.is_some_and(|first| first != writer)
            {
                return Err(invalid(
                    "recovery seals disagree on the original plan/private requests",
                ));
            }
            original = Some(plan);
            original_writer_seal = Some(writer);
        } else {
            if seal.seal_body().writer_seal.is_some() {
                return Err(invalid("writer seal without the original recovery plan"));
            }
            absent = true;
        }
    }
    if original.is_some() && absent {
        return Err(invalid("recovery planned and legacy seals cannot be mixed"));
    }
    if body.schema_version == 3 && original.is_none() {
        return Err(invalid(
            "planned recovery certificate requires all original plan/drained seals",
        ));
    }
    Ok(original.cloned())
}

#[cfg(test)]
mod pending_tests {
    use super::*;

    #[test]
    fn admitted_high_byte_pending_roundtrips_beyond_the_old_encoded_limit() {
        let identity = LocalRequestIdentity {
            verified_subject: "authenticated-original".into(),
            operation_domain: "world-submit".into(),
            nonce_scope: "original-nonce".into(),
            request_id: "original-request".into(),
        };
        let identity_charge = bytes("oasis7.formal.pending_identity.v1", &identity)
            .unwrap()
            .len();
        let mut pending = RecoveryPendingHandoffV1::from_requests(vec![RecoveryPendingRequestV1 {
            identity,
            signed_request: vec![0xff; PENDING_LIMIT - identity_charge],
        }])
        .unwrap();
        let root = pending.root().unwrap();
        let encoded = serde_cbor::to_vec(&pending).unwrap();
        assert!(encoded.len() > 20 * 1024 * 1024);
        assert!(encoded.len() <= RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1);
        let restored: RecoveryPendingHandoffV1 = serde_cbor::from_slice(&encoded).unwrap();
        restored.validate().unwrap();
        assert!(restored == pending);
        assert_eq!(restored.root().unwrap(), root);
        assert!(serde_cbor::to_vec(&restored).unwrap() == encoded);
        // The larger encoded-storage bound does not admit one more raw byte.
        pending.requests[0].signed_request.push(0xff);
        assert!(pending.validate().is_err());
    }
}
