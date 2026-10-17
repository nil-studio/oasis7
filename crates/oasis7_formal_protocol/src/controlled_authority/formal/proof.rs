#[path = "proof_completion.rs"]
mod completion;
use super::{crypto::*, *};
use crate::controlled_authority::replicated_protocol::EndpointRole;
pub use completion::*;
use serde::{Deserialize, Serialize};
#[path = "checkpoint_restore.rs"]
mod checkpoint_restore;
pub use checkpoint_restore::restore_formal_commit_from_checkpoint;
#[path = "native_checkpoint.rs"]
mod native_checkpoint;
pub use native_checkpoint::*;
#[path = "read_anchor.rs"]
mod read_anchor;
pub use read_anchor::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedCanonicalCandidateV1 {
    pub record: CanonicalRecordV1,
    pub writer_signature_hex: String,
}
pub fn canonical_candidate_signing_bytes(
    record: &CanonicalRecordV1,
) -> Result<Vec<u8>, FormalError> {
    record.validate()?;
    bytes("oasis7.formal.candidate.sign.v1", record)
}
impl SignedCanonicalCandidateV1 {
    pub fn verify_original(&self, authority: &VerifiedAuthorityV1) -> Result<(), FormalError> {
        self.record.validate()?;
        let definition = authority.definition();
        if self.record.world != definition.world
            || self.record.original_authority_id != definition.authority_id()?
            || self.record.original_epoch != definition.epoch
            || self.record.activation_ref != authority.activation_ref()
            || self.record.position < definition.valid_from_position
        {
            return Err(invalid("formal candidate original authority binding"));
        }
        verify(
            "oasis7.formal.candidate.sign.v1",
            &self.record,
            &self.writer_signature_hex,
            &definition.writer_public_key,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormalCompletionBranchV1 {
    Normal,
    CompletePrepared,
    ReplayFrozen,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFinalReceiptBodyV1 {
    pub schema_version: u32,
    pub record_id: String,
    pub position: u64,
    pub finalizing_authority_id: String,
    pub role: EndpointRole,
    pub instance_digest: String,
    pub branch: FormalCompletionBranchV1,
    pub recovery_certificate_digest: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFinalReceiptV1 {
    pub body: FormalFinalReceiptBodyV1,
    pub signature_hex: String,
}
pub fn formal_final_receipt_signing_bytes(
    body: &FormalFinalReceiptBodyV1,
) -> Result<Vec<u8>, FormalError> {
    bytes("oasis7.formal.final_receipt.sign.v1", body)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalDecisionProofV1 {
    pub schema_version: u32,
    pub record: CanonicalRecordV1,
    pub branch: FormalCompletionBranchV1,
    /// Original candidate for Normal/CompletePrepared; new finalizer signature
    /// for ReplayFrozen. None of these proof bytes participate in record_id.
    pub writer_signature_hex: String,
    pub frozen: Option<FormalFrozenCertificateV1>,
    pub primary: FormalFinalReceiptV1,
    pub replica: FormalFinalReceiptV1,
}
/// Archive envelope identity; the canonical world head remains record_id.
pub fn formal_decision_proof_digest(proof: &FormalDecisionProofV1) -> Result<String, FormalError> {
    if proof.schema_version != 1 {
        return Err(invalid("formal proof version"));
    }
    proof.record.validate()?;
    hash("oasis7.formal.decision_proof_envelope.v1", proof)
}
#[derive(Debug, Clone)]
pub struct VerifiedFormalCommitV1 {
    record: CanonicalRecordV1,
    finalizing_authority_id: String,
}
impl VerifiedFormalCommitV1 {
    pub fn record(&self) -> &CanonicalRecordV1 {
        &self.record
    }
    pub fn finalizing_authority_id(&self) -> &str {
        &self.finalizing_authority_id
    }
    pub fn head(&self) -> FormalHeadRefV1 {
        FormalHeadRefV1 {
            position: self.record.position,
            record_id: self.record.record_id().expect("verified canonical record"),
        }
    }
}
/// Authenticated candidate authorization before endpoint final receipts exist.
/// It is not a committed receipt, execution correctness proof or durability ack.
#[derive(Debug, Clone)]
pub struct VerifiedCandidateContextV1 {
    record: CanonicalRecordV1,
    finalizer: VerifiedAuthorityV1,
    branch: FormalCompletionBranchV1,
    recovery_certificate_digest: Option<String>,
}
impl VerifiedCandidateContextV1 {
    pub fn record(&self) -> &CanonicalRecordV1 {
        &self.record
    }
    pub fn finalizing_authority(&self) -> &VerifiedAuthorityV1 {
        &self.finalizer
    }
    pub fn branch(&self) -> FormalCompletionBranchV1 {
        self.branch
    }
    pub fn recovery_certificate_digest(&self) -> Option<&str> {
        self.recovery_certificate_digest.as_deref()
    }
    pub fn receipt_body(
        &self,
        role: EndpointRole,
    ) -> Result<FormalFinalReceiptBodyV1, FormalError> {
        let binding = match role {
            EndpointRole::Primary => &self.finalizer.definition().primary,
            EndpointRole::Replica => &self.finalizer.definition().replica,
        };
        Ok(FormalFinalReceiptBodyV1 {
            schema_version: 1,
            record_id: self.record.record_id()?,
            position: self.record.position,
            finalizing_authority_id: self.finalizer.authority_id()?,
            role,
            instance_digest: binding
                .instance
                .digest()
                .map_err(|error| invalid(error.to_string()))?,
            branch: self.branch,
            recovery_certificate_digest: self.recovery_certificate_digest.clone(),
        })
    }
}
pub fn verify_formal_candidate(
    candidate: &SignedCanonicalCandidateV1,
    frozen: Option<&FormalFrozenCertificateV1>,
    authority: &VerifiedAuthorityV1,
    parent: Option<&VerifiedFormalCommitV1>,
    recovery: Option<&VerifiedRecovery>,
    branch: FormalCompletionBranchV1,
) -> Result<VerifiedCandidateContextV1, FormalError> {
    let record = &candidate.record;
    record.validate()?;
    let (finalizer, certificate) = match branch {
        FormalCompletionBranchV1::Normal => {
            if recovery.is_some()
                || authority
                    .first_normal_position
                    .is_none_or(|start| record.position < start)
            {
                return Err(invalid("formal normal write not authorized"));
            }
            SignedCanonicalCandidateV1 {
                record: record.clone(),
                writer_signature_hex: candidate.writer_signature_hex.clone(),
            }
            .verify_original(authority)?;
            (authority, None)
        }
        FormalCompletionBranchV1::CompletePrepared | FormalCompletionBranchV1::ReplayFrozen => {
            let recovery = recovery
                .ok_or_else(|| invalid("formal recovery proof missing verified authorization"))?;
            if authority.authority_id()? != recovery.original_authority().authority_id()?
                || record.position != recovery.body.sealed_cut.position + 1
            {
                return Err(invalid("formal recovery original authority/position"));
            }
            match (&recovery.body.tail, branch) {
                (
                    RecoveryTailV1::Prepared { record_id, .. }
                    | RecoveryTailV1::Decision { record_id, .. },
                    FormalCompletionBranchV1::CompletePrepared,
                ) if &record.record_id()? == record_id => {
                    if let Some(retained) = &recovery.retained_candidate {
                        if retained != candidate {
                            return Err(invalid(
                                "formal completion must preserve exact retained candidate/signature",
                            ));
                        }
                    } else {
                        SignedCanonicalCandidateV1 {
                            record: record.clone(),
                            writer_signature_hex: candidate.writer_signature_hex.clone(),
                        }
                        .verify_original(authority)?;
                    }
                }
                (RecoveryTailV1::Frozen { .. }, FormalCompletionBranchV1::ReplayFrozen) => {
                    verify(
                        "oasis7.formal.candidate.sign.v1",
                        record,
                        &candidate.writer_signature_hex,
                        &recovery.successor.definition().writer_public_key,
                    )?;
                }
                _ => {
                    return Err(invalid(
                        "formal recovery branch cannot downgrade retained prepared/decision",
                    ));
                }
            }
            (recovery.successor(), Some(recovery.certificate_digest()))
        }
    };
    match (&record.kind, parent) {
        (FormalRecordKindV1::InitialActivation, None)
            if branch == FormalCompletionBranchV1::Normal =>
        {
            let initial = authority
                .initial
                .as_ref()
                .ok_or_else(|| invalid("initial formal proof missing initial authorization"))?;
            if (initial.schema_version == 2) != (record.schema_version == 5)
                || record.parent_record_id != record.world.genesis_head()?.record_id
                || record.before_state_root != initial.initial_state_root
                || record.after_state_root != initial.initial_state_root
                || record.runtime_manifest_digest != initial.runtime_manifest_digest
                || record.governing_manifest_digest != initial.governing_manifest_digest
                || record.execution_policy_digest != initial.execution_policy_digest
                || frozen.is_some()
                || authority.initial_input_digest.as_ref() != Some(&record.payload_digest)
                || record
                    .artifact_roots
                    .get(&crate::controlled_authority::replicated_protocol::ArtifactRole::Input)
                    != Some(&record.payload_digest)
                || initial
                    .artifact_roots
                    .iter()
                    .any(|(role, root)| record.artifact_roots.get(role) != Some(root))
            {
                return Err(invalid(
                    "initial formal committed record differs from authorization",
                ));
            }
        }
        (FormalRecordKindV1::Execution, Some(parent)) => {
            if (parent.record.schema_version >= 5 && record.schema_version < 5)
                || parent.record.world != record.world
                || parent.record.position.checked_add(1) != Some(record.position)
                || parent.head().record_id != record.parent_record_id
                || parent.record.after_state_root != record.before_state_root
                || parent.record.handoff.is_some()
            {
                return Err(invalid("formal parent state/position/profile boundary"));
            }
            if branch == FormalCompletionBranchV1::Normal {
                verify_normal_parent_continuity(authority, parent, record.position)?;
            }
            if parent.record.artifact_roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment)
                != record.artifact_roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment) {
                return Err(invalid("native execution environment cannot change without upgrade protocol"));
            }
            let frozen =
                frozen.ok_or_else(|| invalid("formal execution missing original frozen proof"))?;
            frozen.verify(authority.definition(), authority.activation_ref())?;
            frozen.matches_record(record)?;
            // Recovery retains this ORIGINAL authority/cut; never substitute
            // successor context when checking signed input policy continuity.
            verify_formal_reservation(&frozen.reservation, authority, parent)?;
            if let Some(recovery) = recovery {
                if parent.head() != recovery.body.sealed_cut {
                    return Err(invalid("formal recovery proof cut mismatch"));
                }
                let (reservation, handoff) = match &recovery.body.tail {
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
                    RecoveryTailV1::Empty => {
                        return Err(invalid("empty recovery has no retained execution"));
                    }
                };
                if record.frozen_reservation_id.as_ref() != Some(reservation)
                    || &record.handoff != handoff
                {
                    return Err(invalid(
                        "formal recovery reservation/handoff target mismatch",
                    ));
                }
            }
        }
        _ => return Err(invalid("formal committed parent proof required")),
    }
    Ok(VerifiedCandidateContextV1 {
        record: record.clone(),
        finalizer: finalizer.clone(),
        branch,
        recovery_certificate_digest: certificate.map(str::to_owned),
    })
}
pub(crate) fn verify_normal_parent_continuity(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    next_position: u64,
) -> Result<(), FormalError> {
    if parent.record().durable_set_transition_digest.is_some()
        && (authority.durable_set_activation_digest.is_none()
            || authority.recovery_cut.as_ref() != Some(&parent.head())
            || authority.definition().predecessor_authority_id.as_deref()
                != Some(parent.finalizing_authority_id())
            || authority.first_normal_position != Some(next_position))
    {
        return Err(invalid(
            "durable set boundary requires verified successor activation",
        ));
    }
    if parent.record().world != authority.definition().world
        || parent.record().position.checked_add(1) != Some(next_position)
        || parent.record().handoff.is_some()
        || authority
            .first_normal_position
            .is_none_or(|start| next_position < start)
    {
        return Err(invalid("formal normal parent/profile/fresh-write boundary"));
    }
    if parent.finalizing_authority_id() != authority.authority_id()?
        && (authority.definition().predecessor_authority_id.as_deref()
            != Some(parent.finalizing_authority_id())
            || authority.first_normal_position != Some(next_position)
            || (authority.recovery_cut.as_ref() != Some(&parent.head())
                && authority.recovery_retained_record.as_ref() != Some(&parent.head().record_id)))
    {
        return Err(invalid("formal normal authority continuity"));
    }
    Ok(())
}

pub fn verify_formal_decision(
    proof: &FormalDecisionProofV1,
    authority: &VerifiedAuthorityV1,
    parent: Option<&VerifiedFormalCommitV1>,
    recovery: Option<&VerifiedRecovery>,
) -> Result<VerifiedFormalDecisionV1, FormalError> {
    if proof.schema_version != 1 {
        return Err(invalid("formal proof version"));
    }
    let candidate = SignedCanonicalCandidateV1 {
        record: proof.record.clone(),
        writer_signature_hex: proof.writer_signature_hex.clone(),
    };
    let context = verify_formal_candidate(
        &candidate,
        proof.frozen.as_ref(),
        authority,
        parent,
        recovery,
        proof.branch,
    )?;
    for (receipt, role) in [
        (&proof.primary, EndpointRole::Primary),
        (&proof.replica, EndpointRole::Replica),
    ] {
        if receipt.body != context.receipt_body(role)? {
            return Err(invalid(
                "formal final receipt record/instance/authorization context",
            ));
        }
        let binding = match role {
            EndpointRole::Primary => &context.finalizer.definition().primary,
            EndpointRole::Replica => &context.finalizer.definition().replica,
        };
        verify(
            "oasis7.formal.final_receipt.sign.v1",
            &receipt.body,
            &receipt.signature_hex,
            &binding.instance.endpoint_public_key,
        )?;
    }
    Ok(VerifiedFormalDecisionV1 {
        record: context.record,
        finalizer: context.finalizer,
        branch: context.branch,
        recovery_certificate_digest: context.recovery_certificate_digest,
        decision_proof_digest: formal_decision_proof_digest(proof)?,
    })
}

#[path = "proof_historical_inclusion.rs"]
mod historical_inclusion;
pub(crate) use historical_inclusion::verify_historical_qualified_from_inclusion;
