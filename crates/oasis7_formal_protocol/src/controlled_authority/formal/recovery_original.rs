//! Original recovery input commitments and legacy Qualified cut validation.
//! This dark module does not grant admission, sign a plan, activate an endpoint,
//! or change the live wire path. Consumers must authenticate the original request.
use super::{crypto::*, *};
use crate::controlled_authority::LocalRequestIdentity;
use serde::{Deserialize, Serialize};

const PENDING_LIMIT: usize = 16 * 1024 * 1024;
const PENDING_COUNT_LIMIT: usize = 4096;
/// The canonical Vec<u8> encoding stays unchanged: each byte may require two
/// CBOR bytes. Identity CBOR is already included in the 16 MiB admission bound;
/// each request adds at most 32 bytes of map/field/array framing, plus fewer
/// than 64 bytes for the collection. Thus every admitted value fits within
/// 2 * 16 MiB + 4096 * 32 + 64 bytes, strictly below this shared storage bound.
pub const RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1: usize = 34 * 1024 * 1024;
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
        if requests.len() > PENDING_COUNT_LIMIT {
            return Err(invalid("recovery pending schema/count"));
        }
        // Bound inputs before allocating identity encodings or sorting.
        for request in &requests {
            validate_request_fields(request)?;
        }
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
            validate_request_fields(request)?;
            let encoded_identity = bytes("oasis7.formal.pending_identity.v1", &request.identity)?;
            if previous
                .as_ref()
                .is_some_and(|old| old >= &encoded_identity)
            {
                return Err(invalid("recovery pending original identity/order/payload"));
            }
            size = size
                .checked_add(request.signed_request.len())
                .and_then(|total| total.checked_add(encoded_identity.len()))
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

fn validate_request_fields(request: &RecoveryPendingRequestV1) -> Result<(), FormalError> {
    let identity = &request.identity;
    if [
        &identity.verified_subject,
        &identity.operation_domain,
        &identity.nonce_scope,
        &identity.request_id,
    ]
    .iter()
    .any(|field| field.is_empty() || field.len() > 4096)
        || request.signed_request.is_empty()
        || request.signed_request.len() > PENDING_LIMIT
    {
        return Err(invalid("recovery pending original identity/order/payload"));
    }
    Ok(())
}

/// Commits the original signed bytes, not the lifecycle stage or an authorization.
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

#[derive(Debug, Clone)]
pub struct VerifiedRecoveryPlanLegacyCutV1 {
    cut: VerifiedFormalCommitV1,
    proof_digest: String,
}

impl VerifiedRecoveryPlanLegacyCutV1 {
    pub fn verify(
        proof: &FormalQualifiedCommitProofV1,
        original_recovery: &VerifiedRecovery,
    ) -> Result<Self, FormalError> {
        if !matches!(original_recovery.body().schema_version, 1 | 2)
            || original_recovery.body().tail == RecoveryTailV1::Empty
            || !matches!(
                proof.decision.branch,
                FormalCompletionBranchV1::CompletePrepared | FormalCompletionBranchV1::ReplayFrozen
            )
        {
            return Err(invalid(
                "legacy plan cut requires its completed original recovery",
            ));
        }
        // This is the original full verifier: writer/Frozen signatures, original
        // parent, exact recovery branch/certificate, both final receipts and
        // both ProofComplete receipts all precede the digest commitment.
        let cut = verify_formal_qualified(
            proof,
            original_recovery.original_authority(),
            Some(original_recovery.sealed_commit()),
            Some(original_recovery),
        )?;
        if cut.finalizing_authority_id() != original_recovery.successor().authority_id()?
            || cut.record().original_epoch >= original_recovery.successor().definition().epoch
        {
            return Err(invalid("legacy plan cut original/finalizing epoch"));
        }
        Ok(Self {
            cut,
            proof_digest: formal_qualified_proof_digest(proof)?,
        })
    }
    pub fn proof_digest(&self) -> &str {
        &self.proof_digest
    }
    pub fn cut(&self) -> &VerifiedFormalCommitV1 {
        &self.cut
    }
}

#[cfg(test)]
mod pending_tests {
    use super::*;
    use crate::controlled_authority::formal::test_fixtures::*;

    fn pending_request(id: &str, payload: &[u8]) -> RecoveryPendingRequestV1 {
        RecoveryPendingRequestV1 {
            identity: LocalRequestIdentity {
                verified_subject: "authenticated-original".into(),
                operation_domain: "world-submit".into(),
                nonce_scope: "original-nonce".into(),
                request_id: id.into(),
            },
            signed_request: payload.to_vec(),
        }
    }

    #[test]
    fn original_pending_order_bytes_and_identity_are_bound() {
        let a = pending_request("a", &[0, 255, 42]);
        let b = pending_request("b", &[3, 4]);
        let pending = RecoveryPendingHandoffV1::from_requests(vec![b.clone(), a.clone()]).unwrap();
        let root = pending.root().unwrap();
        assert_eq!(pending.requests, vec![a.clone(), b.clone()]);
        let encoded = serde_cbor::to_vec(&pending).unwrap();
        let restored: RecoveryPendingHandoffV1 = serde_cbor::from_slice(&encoded).unwrap();
        assert_eq!(restored.root().unwrap(), root);
        assert_eq!(restored.requests[0].signed_request, a.signed_request);
        let mut changed = restored.clone();
        changed.requests[0].signed_request[0] ^= 1;
        assert_ne!(changed.root().unwrap(), root);
        changed = restored.clone();
        changed.requests[0].identity.verified_subject = "other-subject".into();
        let changed = RecoveryPendingHandoffV1::from_requests(changed.requests).unwrap();
        assert_ne!(changed.root().unwrap(), root);
        assert_ne!(RecoveryPendingHandoffV1::empty().root().unwrap(), root);
        assert!(RecoveryPendingHandoffV1::from_requests(vec![a.clone(), a]).is_err());
        let mut reversed = restored.clone();
        reversed.requests.reverse();
        assert!(reversed.root().is_err());
        let mut malformed = restored;
        malformed.schema_version = 2;
        assert!(malformed.root().is_err());
    }

    #[test]
    fn rejects_invalid_pending_fields_and_count() {
        let mut invalid = pending_request("a", &[]);
        assert!(RecoveryPendingHandoffV1::from_requests(vec![invalid.clone()]).is_err());
        invalid.signed_request.push(1);
        invalid.identity.nonce_scope.clear();
        assert!(RecoveryPendingHandoffV1::from_requests(vec![invalid.clone()]).is_err());
        invalid.identity.nonce_scope = "x".repeat(4097);
        assert!(RecoveryPendingHandoffV1::from_requests(vec![invalid]).is_err());
        assert!(
            RecoveryPendingHandoffV1::from_requests(vec![
                pending_request("a", &[1]);
                PENDING_COUNT_LIMIT + 1
            ])
            .is_err()
        );
    }

    #[test]
    fn original_signed_witness_commitment_preserves_candidate_but_not_stage() {
        let (_, _, _, frozen, record) =
            recovery_fixture_with_profile(FormalCompletionBranchV1::CompletePrepared, None, 10, 2);
        let candidate = SignedCanonicalCandidateV1 {
            writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), 4),
            record,
        };
        let prepared = RecoveryTailWitnessV1::Prepared {
            frozen: frozen.clone(),
            candidate: candidate.clone(),
        };
        let decided = RecoveryTailWitnessV1::Decision {
            frozen: frozen.clone(),
            candidate: candidate.clone(),
        };
        let original = recovery_requests_commitment(&prepared).unwrap();
        assert_eq!(recovery_requests_commitment(&decided).unwrap(), original);
        assert_ne!(
            recovery_tail_from_witness(&prepared).unwrap(),
            recovery_tail_from_witness(&decided).unwrap()
        );
        let mut changed = candidate;
        changed.writer_signature_hex = "00".repeat(64);
        assert_ne!(
            recovery_requests_commitment(&RecoveryTailWitnessV1::Prepared {
                frozen,
                candidate: changed,
            })
            .unwrap(),
            original
        );
    }

    #[test]
    fn legacy_cut_requires_the_complete_original_qualified_proof() {
        for branch in [
            FormalCompletionBranchV1::CompletePrepared,
            FormalCompletionBranchV1::ReplayFrozen,
        ] {
            let (intent, original, _, frozen, record) =
                recovery_fixture_with_profile(branch, None, 10, 2);
            let recovery = verified_recovery(&intent, &original);
            let completed = proof(
                record,
                recovery.successor(),
                branch,
                Some(recovery.certificate_digest().into()),
                Some(frozen),
                if branch == FormalCompletionBranchV1::CompletePrepared {
                    4
                } else {
                    10
                },
                11,
                12,
            );
            let checked = VerifiedRecoveryPlanLegacyCutV1::verify(&completed, &recovery).unwrap();
            assert_eq!(
                checked.proof_digest(),
                formal_qualified_proof_digest(&completed).unwrap()
            );
            assert_eq!(checked.cut().record(), &completed.decision.record);
            for variant in 0..4 {
                let mut changed = completed.clone();
                match variant {
                    0 => changed.decision.writer_signature_hex = "00".repeat(64),
                    1 => changed.decision.primary.body.recovery_certificate_digest = Some(h(153)),
                    2 => changed.replica_complete.signature_hex = "00".repeat(64),
                    _ => changed.decision.branch = FormalCompletionBranchV1::Normal,
                }
                assert!(VerifiedRecoveryPlanLegacyCutV1::verify(&changed, &recovery).is_err());
            }
        }
    }


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
