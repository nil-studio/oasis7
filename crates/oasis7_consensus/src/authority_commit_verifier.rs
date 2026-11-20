//! Unified pure authority-commit verification port across Controlled and BFT profiles.
//!
//! Pure proof verification (signatures, writer epoch, P/R proof-complete receipts,
//! BFT quorum and `H / H+1` activation boundaries) lives behind [`AuthorityCommitVerifier`].
//! Durable persistence, protected QC journaling, and `StatusUnknown` fencing belong
//! exclusively to the outer node/runtime persistence gate.

use std::fmt;

use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3, FormalBftCertifiedHeaderV3, VerifiedBftHeaderCertificateV3,
    verify_formal_bft_header_certificate_v3,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalDecisionProofV1, FormalQualifiedCommitProofV1, VerifiedAuthorityV1,
    VerifiedFormalCommitV1, VerifiedFormalDecisionV1, VerifiedRecovery, verify_formal_decision,
    verify_formal_qualified,
};

/// Trusted historical anchor supplied by the caller before verifying a candidate commit.
#[derive(Debug, Clone)]
pub enum TrustedCommitAnchor {
    /// Single-authority Controlled profile anchor (authority set + optional parent & recovery).
    Controlled {
        authority: VerifiedAuthorityV1,
        parent: Option<VerifiedFormalCommitV1>,
        recovery: Option<VerifiedRecovery>,
    },
    /// Qualified-H BFT V3 profile anchor (active validator set, policy, parent link & `H / H+1` schedule).
    BftV3 { schedule: BftHeaderHeightScheduleV3 },
}

impl TrustedCommitAnchor {
    pub fn controlled(
        authority: VerifiedAuthorityV1,
        parent: Option<VerifiedFormalCommitV1>,
        recovery: Option<VerifiedRecovery>,
    ) -> Self {
        Self::Controlled {
            authority,
            parent,
            recovery,
        }
    }

    pub fn bft_v3(schedule: BftHeaderHeightScheduleV3) -> Self {
        Self::BftV3 { schedule }
    }

    fn profile_name(&self) -> &'static str {
        match self {
            Self::Controlled { .. } => "controlled_single_authority",
            Self::BftV3 { .. } => "bft_v3",
        }
    }
}

/// Optional caller-side candidate reference checked against the verified proof output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitCandidateRef<'a> {
    Controlled {
        expected_position: Option<u64>,
        expected_record_id: Option<&'a str>,
    },
    BftV3 {
        expected_height: Option<u64>,
        expected_block_id: Option<&'a str>,
    },
}

impl<'a> CommitCandidateRef<'a> {
    pub fn controlled(expected_position: Option<u64>, expected_record_id: Option<&'a str>) -> Self {
        Self::Controlled {
            expected_position,
            expected_record_id,
        }
    }

    pub fn bft_v3(expected_height: Option<u64>, expected_block_id: Option<&'a str>) -> Self {
        Self::BftV3 {
            expected_height,
            expected_block_id,
        }
    }

    fn profile_name(&self) -> &'static str {
        match self {
            Self::Controlled { .. } => "controlled_single_authority",
            Self::BftV3 { .. } => "bft_v3",
        }
    }
}

/// Authority proof payload presented for pure cryptographic & schedule verification.
///
/// Controlled decision-only proofs and qualified commit proofs (carrying both Primary and
/// Replica proof-complete receipts) are represented as distinct variants so callers cannot
/// silently conflate a decision with a qualified commit.
#[derive(Debug, Clone)]
pub enum AuthorityCommitProof {
    ControlledDecision(Box<FormalDecisionProofV1>),
    ControlledQualified(Box<FormalQualifiedCommitProofV1>),
    BftV3(Box<FormalBftCertifiedHeaderV3>),
}

impl AuthorityCommitProof {
    pub fn controlled_decision(proof: FormalDecisionProofV1) -> Self {
        Self::ControlledDecision(Box::new(proof))
    }

    pub fn controlled_qualified(proof: FormalQualifiedCommitProofV1) -> Self {
        Self::ControlledQualified(Box::new(proof))
    }

    pub fn bft_v3(certificate: FormalBftCertifiedHeaderV3) -> Self {
        Self::BftV3(Box::new(certificate))
    }

    fn profile_name(&self) -> &'static str {
        match self {
            Self::ControlledDecision(_) | Self::ControlledQualified(_) => {
                "controlled_single_authority"
            }
            Self::BftV3(_) => "bft_v3",
        }
    }
}

/// Typed output of pure authority verification, wrapping the underlying formal verifier tokens.
#[derive(Debug, Clone)]
pub enum VerifiedAuthorityCommit {
    /// Verified Controlled decision only; NOT a qualified commit (lacks P/R complete receipts).
    ControlledDecision(Box<VerifiedFormalDecisionV1>),
    /// Verified Controlled qualified commit (decision + Primary & Replica complete receipts).
    ControlledQualified(Box<VerifiedFormalCommitV1>),
    /// Verified BFT V3 header certificate (quorum + deterministic schedule & next-height handoff).
    BftV3(Box<VerifiedBftHeaderCertificateV3>),
}

impl VerifiedAuthorityCommit {
    /// Returns `true` only when the verified output is a qualified commit (not decision-only).
    pub fn is_qualified_commit(&self) -> bool {
        matches!(self, Self::ControlledQualified(_) | Self::BftV3(_))
    }

    /// Returns the verified position (Controlled) or block height (BFT V3).
    pub fn height_or_position(&self) -> u64 {
        match self {
            Self::ControlledDecision(decision) => decision.record().position,
            Self::ControlledQualified(commit) => commit.record().position,
            Self::BftV3(cert) => cert.bound_header().commitment().height,
        }
    }

    pub fn into_controlled_qualified(self) -> Result<VerifiedFormalCommitV1, AuthorityCommitReject> {
        match self {
            Self::ControlledQualified(commit) => Ok(*commit),
            Self::ControlledDecision(decision) => Err(AuthorityCommitReject::DecisionOnlyNotQualified {
                position: decision.record().position,
            }),
            Self::BftV3(_) => Err(AuthorityCommitReject::ProfileMismatch {
                expected: "controlled_single_authority",
                actual: "bft_v3",
            }),
        }
    }

    pub fn into_bft_v3(self) -> Result<VerifiedBftHeaderCertificateV3, AuthorityCommitReject> {
        match self {
            Self::BftV3(cert) => Ok(*cert),
            Self::ControlledDecision(_) | Self::ControlledQualified(_) => {
                Err(AuthorityCommitReject::ProfileMismatch {
                    expected: "bft_v3",
                    actual: "controlled_single_authority",
                })
            }
        }
    }
}

/// Deterministic rejection returned by pure authority-commit verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityCommitReject {
    ProfileMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    CandidateMismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    DecisionOnlyNotQualified {
        position: u64,
    },
    ControlledVerificationFailed(String),
    BftVerificationFailed(String),
}

impl fmt::Display for AuthorityCommitReject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProfileMismatch { expected, actual } => {
                write!(
                    f,
                    "authority commit profile mismatch: expected {expected}, got {actual}"
                )
            }
            Self::CandidateMismatch {
                field,
                expected,
                actual,
            } => write!(
                f,
                "authority commit candidate {field} mismatch: expected {expected}, got {actual}"
            ),
            Self::DecisionOnlyNotQualified { position } => write!(
                f,
                "controlled decision proof at position {position} lacks Primary/Replica proof-complete receipts"
            ),
            Self::ControlledVerificationFailed(reason) => {
                write!(f, "controlled authority verification failed: {reason}")
            }
            Self::BftVerificationFailed(reason) => {
                write!(f, "BFT V3 authority verification failed: {reason}")
            }
        }
    }
}

impl std::error::Error for AuthorityCommitReject {}

/// Pure authority-commit verification port shared between Controlled and BFT profiles.
pub trait AuthorityCommitVerifier {
    fn verify_authority_commit(
        &self,
        trusted_head: &TrustedCommitAnchor,
        candidate: &CommitCandidateRef<'_>,
        proof: &AuthorityCommitProof,
    ) -> Result<VerifiedAuthorityCommit, AuthorityCommitReject>;
}

/// Controlled single-authority verifier wrapping `verify_formal_qualified` and `verify_formal_decision`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ControlledAuthorityCommitVerifier {
    allow_decision_only: bool,
}

impl ControlledAuthorityCommitVerifier {
    /// Verifier that requires a full qualified commit (decision + Primary & Replica complete receipts).
    pub const fn qualified_only() -> Self {
        Self {
            allow_decision_only: false,
        }
    }

    /// Verifier that also accepts decision-only proofs, returning [`VerifiedAuthorityCommit::ControlledDecision`].
    pub const fn allowing_decision_only() -> Self {
        Self {
            allow_decision_only: true,
        }
    }
}

impl AuthorityCommitVerifier for ControlledAuthorityCommitVerifier {
    fn verify_authority_commit(
        &self,
        trusted_head: &TrustedCommitAnchor,
        candidate: &CommitCandidateRef<'_>,
        proof: &AuthorityCommitProof,
    ) -> Result<VerifiedAuthorityCommit, AuthorityCommitReject> {
        let TrustedCommitAnchor::Controlled {
            authority,
            parent,
            recovery,
        } = trusted_head
        else {
            return Err(AuthorityCommitReject::ProfileMismatch {
                expected: "controlled_single_authority",
                actual: trusted_head.profile_name(),
            });
        };
        let CommitCandidateRef::Controlled {
            expected_position,
            expected_record_id,
        } = *candidate
        else {
            return Err(AuthorityCommitReject::ProfileMismatch {
                expected: "controlled_single_authority",
                actual: candidate.profile_name(),
            });
        };

        match proof {
            AuthorityCommitProof::ControlledQualified(qualified) => {
                let verified =
                    verify_formal_qualified(qualified, authority, parent.as_ref(), recovery.as_ref())
                        .map_err(|err| {
                            AuthorityCommitReject::ControlledVerificationFailed(err.to_string())
                        })?;
                validate_controlled_candidate(
                    verified.record().position,
                    &hex::encode(verified.verified_record_id()),
                    expected_position,
                    expected_record_id,
                )?;
                Ok(VerifiedAuthorityCommit::ControlledQualified(Box::new(
                    verified,
                )))
            }
            AuthorityCommitProof::ControlledDecision(decision) => {
                if !self.allow_decision_only {
                    return Err(AuthorityCommitReject::DecisionOnlyNotQualified {
                        position: decision.record.position,
                    });
                }
                let verified =
                    verify_formal_decision(decision, authority, parent.as_ref(), recovery.as_ref())
                        .map_err(|err| {
                            AuthorityCommitReject::ControlledVerificationFailed(err.to_string())
                        })?;
                let record_id = verified.record().record_id().map_err(|err| {
                    AuthorityCommitReject::ControlledVerificationFailed(err.to_string())
                })?;
                validate_controlled_candidate(
                    verified.record().position,
                    &record_id,
                    expected_position,
                    expected_record_id,
                )?;
                Ok(VerifiedAuthorityCommit::ControlledDecision(Box::new(
                    verified,
                )))
            }
            AuthorityCommitProof::BftV3(_) => Err(AuthorityCommitReject::ProfileMismatch {
                expected: "controlled_single_authority",
                actual: proof.profile_name(),
            }),
        }
    }
}

fn validate_controlled_candidate(
    actual_position: u64,
    actual_record_id: &str,
    expected_position: Option<u64>,
    expected_record_id: Option<&str>,
) -> Result<(), AuthorityCommitReject> {
    if let Some(expected) = expected_position
        && actual_position != expected
    {
        return Err(AuthorityCommitReject::CandidateMismatch {
            field: "position",
            expected: expected.to_string(),
            actual: actual_position.to_string(),
        });
    }
    if let Some(expected) = expected_record_id
        && actual_record_id != expected
    {
        return Err(AuthorityCommitReject::CandidateMismatch {
            field: "record_id",
            expected: expected.to_string(),
            actual: actual_record_id.to_string(),
        });
    }
    Ok(())
}

/// Qualified-H BFT V3 verifier wrapping `verify_formal_bft_header_certificate_v3`.
#[derive(Debug, Clone, Copy, Default)]
pub struct BftV3AuthorityCommitVerifier;

impl BftV3AuthorityCommitVerifier {
    pub const fn new() -> Self {
        Self
    }
}

impl AuthorityCommitVerifier for BftV3AuthorityCommitVerifier {
    fn verify_authority_commit(
        &self,
        trusted_head: &TrustedCommitAnchor,
        candidate: &CommitCandidateRef<'_>,
        proof: &AuthorityCommitProof,
    ) -> Result<VerifiedAuthorityCommit, AuthorityCommitReject> {
        let TrustedCommitAnchor::BftV3 { schedule } = trusted_head else {
            return Err(AuthorityCommitReject::ProfileMismatch {
                expected: "bft_v3",
                actual: trusted_head.profile_name(),
            });
        };
        let CommitCandidateRef::BftV3 {
            expected_height,
            expected_block_id,
        } = *candidate
        else {
            return Err(AuthorityCommitReject::ProfileMismatch {
                expected: "bft_v3",
                actual: candidate.profile_name(),
            });
        };
        let AuthorityCommitProof::BftV3(certificate) = proof else {
            return Err(AuthorityCommitReject::ProfileMismatch {
                expected: "bft_v3",
                actual: proof.profile_name(),
            });
        };

        let verified = verify_formal_bft_header_certificate_v3(schedule, certificate)
            .map_err(AuthorityCommitReject::BftVerificationFailed)?;
        let actual_height = verified.bound_header().commitment().height;
        if let Some(expected) = expected_height
            && actual_height != expected
        {
            return Err(AuthorityCommitReject::CandidateMismatch {
                field: "height",
                expected: expected.to_string(),
                actual: actual_height.to_string(),
            });
        }
        if let Some(expected) = expected_block_id
            && verified.block_id() != expected
        {
            return Err(AuthorityCommitReject::CandidateMismatch {
                field: "block_id",
                expected: expected.to_string(),
                actual: verified.block_id().to_string(),
            });
        }
        Ok(VerifiedAuthorityCommit::BftV3(Box::new(verified)))
    }
}

/// Profile-dispatching verifier covering both Controlled and BFT V3 anchors/proofs.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompositeAuthorityCommitVerifier {
    controlled: ControlledAuthorityCommitVerifier,
    bft_v3: BftV3AuthorityCommitVerifier,
}

impl CompositeAuthorityCommitVerifier {
    pub const fn qualified_only() -> Self {
        Self {
            controlled: ControlledAuthorityCommitVerifier::qualified_only(),
            bft_v3: BftV3AuthorityCommitVerifier::new(),
        }
    }

    pub const fn allowing_controlled_decision_only() -> Self {
        Self {
            controlled: ControlledAuthorityCommitVerifier::allowing_decision_only(),
            bft_v3: BftV3AuthorityCommitVerifier::new(),
        }
    }
}

impl AuthorityCommitVerifier for CompositeAuthorityCommitVerifier {
    fn verify_authority_commit(
        &self,
        trusted_head: &TrustedCommitAnchor,
        candidate: &CommitCandidateRef<'_>,
        proof: &AuthorityCommitProof,
    ) -> Result<VerifiedAuthorityCommit, AuthorityCommitReject> {
        match trusted_head {
            TrustedCommitAnchor::Controlled { .. } => {
                self.controlled
                    .verify_authority_commit(trusted_head, candidate, proof)
            }
            TrustedCommitAnchor::BftV3 { .. } => {
                self.bft_v3
                    .verify_authority_commit(trusted_head, candidate, proof)
            }
        }
    }
}
