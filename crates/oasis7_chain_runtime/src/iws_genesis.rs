//! Serial, real dual-endpoint GEN coordination. This is not Host activation,
//! an ordinary/Frozen executor or a WorldReady/serving permission constructor.
use oasis7_distfs::controlled_authority::{
    fencing::{BoundedAcquireErrorV1, RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1},
    formal::endpoint::{
        FileFormalEndpoint, FormalArchiveReadWorkV1, FormalCandidateMaterialV1,
        PaidGenesisAcceptanceV1,
    },
};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError, FormalQualifiedCommitProofV1, PaidVerifiedGenesisCommitV1,
    VerifiedCandidateContextV1, build_genesis_decision_bounded, build_genesis_qualified_bounded,
    context_wire,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{fmt, mem::size_of};
/// Each failure may follow durable progress on either endpoint. The caller
/// retains the SAME candidate/context and retries through persisted originals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenesisSubmissionStageV1 {
    Admission,
    ReadPrimary,
    ReadReplica,
    CompareRetained,
    PreparePrimary,
    PrepareReplica,
    FinalizePrimary,
    FinalizeReplica,
    BuildDecision,
    CompletePrimary,
    CompleteReplica,
    BuildQualified,
    PublishPrimary,
    PublishReplica,
    ObserveCaches,
}
#[derive(Debug)]
pub enum GenesisSubmissionCauseV1 {
    Formal(FormalError),
    Endpoint(BoundedAcquireErrorV1),
}
#[derive(Debug)]
pub struct GenesisSubmissionErrorV1 {
    stage: GenesisSubmissionStageV1,
    cause: GenesisSubmissionCauseV1,
}
impl GenesisSubmissionErrorV1 {
    pub fn stage(&self) -> GenesisSubmissionStageV1 {
        self.stage
    }
    pub fn cause(&self) -> &GenesisSubmissionCauseV1 {
        &self.cause
    }
}
impl fmt::Display for GenesisSubmissionErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GEN submission {:?}: ", self.stage)?;
        match &self.cause {
            GenesisSubmissionCauseV1::Formal(e) => write!(f, "{e}"),
            GenesisSubmissionCauseV1::Endpoint(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for GenesisSubmissionErrorV1 {}
fn formal(stage: GenesisSubmissionStageV1, e: FormalError) -> GenesisSubmissionErrorV1 {
    GenesisSubmissionErrorV1 {
        stage,
        cause: GenesisSubmissionCauseV1::Formal(e),
    }
}
fn endpoint(stage: GenesisSubmissionStageV1, e: BoundedAcquireErrorV1) -> GenesisSubmissionErrorV1 {
    GenesisSubmissionErrorV1 {
        stage,
        cause: GenesisSubmissionCauseV1::Endpoint(e),
    }
}
/// Borrowed real endpoints/providers; no roots, caller success flags or keys.
pub struct GenesisEndpointsV1<'a> {
    pub primary: &'a mut FileFormalEndpoint,
    pub replica: &'a mut FileFormalEndpoint,
    pub primary_provider: &'a RemoteMonotonicEndpointProvider,
    pub replica_provider: &'a RemoteMonotonicEndpointProvider,
    pub primary_policy: &'a TrustedFencingPolicyV1,
    pub replica_policy: &'a TrustedFencingPolicyV1,
}
/// Only a completely successful coordinator call constructs this owner. It
/// retains the complete paid proof and BOTH actual paid acceptance results.
/// Selects control flow from the initial paid retained-state reads only. It is
/// not evidence of freshness, a new native write or execution qualification:
/// a prior unknown result can leave cached Ready while disk is already Qualified.
enum SubmissionPath {
    FreshDual,
    RetainedProof,
}
pub struct PaidGenesisSubmissionV1 {
    proof: PaidVerifiedGenesisCommitV1,
    primary: PaidGenesisAcceptanceV1,
    replica: PaidGenesisAcceptanceV1,
    _outer: ReservedValue<()>,
}
impl PaidGenesisSubmissionV1 {
    pub fn proof(&self) -> &FormalQualifiedCommitProofV1 {
        self.proof.proof()
    }
    pub fn primary(&self) -> &PaidGenesisAcceptanceV1 {
        &self.primary
    }
    pub fn replica(&self) -> &PaidGenesisAcceptanceV1 {
        &self.replica
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.proof.uses_budget(memory)
            && self.primary.uses_budget(memory)
            && self.replica.uses_budget(memory)
            && self._outer.uses_budget(memory)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.proof.reserved_bytes()
            + self.primary.reserved_bytes()
            + self.replica.reserved_bytes()
            + self._outer.reserved_bytes()
    }
}
/// Source staging is explicit: call each endpoint's stage_genesis_source first
/// under its existing SourceWork/Current/page budget domain. On exact retry the
/// durable original proof takes priority; no stage reset or receipt re-signing.
pub fn submit_staged_genesis_bounded(
    candidate: &FormalCandidateMaterialV1,
    context: &VerifiedCandidateContextV1,
    endpoints: GenesisEndpointsV1<'_>,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidGenesisSubmissionV1, GenesisSubmissionErrorV1> {
    use GenesisSubmissionStageV1 as S;
    work.consume_steps(1, stop)
        .map_err(|e| formal(S::Admission, e))?;
    let covered = size_of::<PaidVerifiedGenesisCommitV1>()
        .checked_add(2 * size_of::<PaidGenesisAcceptanceV1>())
        .ok_or_else(|| {
            formal(
                S::Admission,
                FormalError::Invalid("GEN coordinator owner layout".into()),
            )
        })?;
    let extra = size_of::<PaidGenesisSubmissionV1>()
        .checked_sub(covered)
        .ok_or_else(|| {
            formal(
                S::Admission,
                FormalError::Invalid("GEN coordinator owner layout".into()),
            )
        })?;
    let outer = memory
        .reserve(extra, MemoryClass::Ordinary)
        .map_err(|_| {
            formal(
                S::Admission,
                FormalError::Invalid("GEN coordinator owner admission".into()),
            )
        })?
        .retain(());
    let p = endpoints
        .primary
        .read_genesis_qualified_bounded(candidate, context, memory, work, stop)
        .map_err(|e| formal(S::ReadPrimary, e))?;
    let r = endpoints
        .replica
        .read_genesis_qualified_bounded(candidate, context, memory, work, stop)
        .map_err(|e| formal(S::ReadReplica, e))?;
    let (proof, path) = match (p, r) {
        (Some(p), Some(r)) => {
            let mut ctx = work.bounded_context(memory, stop);
            p.validate_context(&mut ctx)
                .map_err(|e| formal(S::CompareRetained, e))?;
            r.validate_context(&mut ctx)
                .map_err(|e| formal(S::CompareRetained, e))?;
            let left = context_wire::encode_paid(
                p.proof(),
                64 * 1024 * 1024,
                &mut ctx,
                "formal encoding exceeds byte bound",
                "GEN retained proof comparison",
            )
            .map_err(|e| formal(S::CompareRetained, e))?;
            let right = context_wire::encode_paid(
                r.proof(),
                64 * 1024 * 1024,
                &mut ctx,
                "formal encoding exceeds byte bound",
                "GEN retained proof comparison",
            )
            .map_err(|e| formal(S::CompareRetained, e))?;
            let n = left.len().checked_add(right.len()).ok_or_else(|| {
                formal(
                    S::CompareRetained,
                    FormalError::Invalid("GEN retained proof comparison layout".into()),
                )
            })?;
            ctx.charge(1, n as u64)
                .map_err(|e| formal(S::CompareRetained, e))?;
            if left.as_slice() != right.as_slice() {
                return Err(formal(
                    S::CompareRetained,
                    FormalError::Conflict("GEN distinct retained full quorum envelopes".into()),
                ));
            }
            drop(ctx);
            (p, SubmissionPath::RetainedProof)
        }
        (Some(p), None) => (p, SubmissionPath::RetainedProof),
        (None, Some(r)) => (r, SubmissionPath::RetainedProof),
        (None, None) => {
            let p = endpoints
                .primary
                .prepare_genesis_bounded(
                    candidate,
                    context,
                    endpoints.primary_provider,
                    endpoints.primary_policy,
                    memory,
                    work,
                    stop,
                )
                .map_err(|e| endpoint(S::PreparePrimary, e))?;
            let r = endpoints
                .replica
                .prepare_genesis_bounded(
                    candidate,
                    context,
                    endpoints.replica_provider,
                    endpoints.replica_policy,
                    memory,
                    work,
                    stop,
                )
                .map_err(|e| endpoint(S::PrepareReplica, e))?;
            let pf = endpoints
                .primary
                .finalize_genesis_bounded(
                    candidate,
                    context,
                    p.receipt(),
                    r.receipt(),
                    endpoints.primary_provider,
                    endpoints.primary_policy,
                    memory,
                    work,
                    stop,
                )
                .map_err(|e| endpoint(S::FinalizePrimary, e))?;
            let rf = endpoints
                .replica
                .finalize_genesis_bounded(
                    candidate,
                    context,
                    p.receipt(),
                    r.receipt(),
                    endpoints.replica_provider,
                    endpoints.replica_policy,
                    memory,
                    work,
                    stop,
                )
                .map_err(|e| endpoint(S::FinalizeReplica, e))?;
            let mut ctx = work.bounded_context(memory, stop);
            let decision = build_genesis_decision_bounded(
                &candidate.candidate,
                context,
                pf.receipt(),
                rf.receipt(),
                &mut ctx,
            )
            .map_err(|e| formal(S::BuildDecision, e))?;
            drop(ctx);
            let pc = endpoints
                .primary
                .complete_genesis_bounded(
                    candidate,
                    context,
                    decision.decision(),
                    endpoints.primary_provider,
                    endpoints.primary_policy,
                    memory,
                    work,
                    stop,
                )
                .map_err(|e| endpoint(S::CompletePrimary, e))?;
            let rc = endpoints
                .replica
                .complete_genesis_bounded(
                    candidate,
                    context,
                    decision.decision(),
                    endpoints.replica_provider,
                    endpoints.replica_policy,
                    memory,
                    work,
                    stop,
                )
                .map_err(|e| endpoint(S::CompleteReplica, e))?;
            let mut ctx = work.bounded_context(memory, stop);
            let proof = build_genesis_qualified_bounded(
                &decision,
                &candidate.candidate,
                context,
                pc.receipt(),
                rc.receipt(),
                &mut ctx,
            )
            .map_err(|e| formal(S::BuildQualified, e))?;
            drop(ctx);
            (proof, SubmissionPath::FreshDual)
        }
    };
    let primary = endpoints
        .primary
        .publish_genesis_qualified_bounded(
            candidate,
            proof.proof(),
            context,
            endpoints.primary_provider,
            endpoints.primary_policy,
            memory,
            work,
            stop,
        )
        .map_err(|e| endpoint(S::PublishPrimary, e))?;
    let replica = endpoints
        .replica
        .publish_genesis_qualified_bounded(
            candidate,
            proof.proof(),
            context,
            endpoints.replica_provider,
            endpoints.replica_policy,
            memory,
            work,
            stop,
        )
        .map_err(|e| endpoint(S::PublishReplica, e))?;
    // Historical Qualified proofs may outlive GEN as current parent or be read
    // after metadata-only cold reopen. They do not replace Current or establish
    // a live serving fence. Only a fresh dual publication observes GEN caches.
    let record = proof.verified().verified_record_id();
    if matches!(path, SubmissionPath::FreshDual) {
        work.charge(2, 8 * 32, stop)
            .map_err(|e| formal(S::ObserveCaches, e))?;
        if endpoints
            .primary
            .accepted_genesis_record_id()
            .map_err(|e| formal(S::ObserveCaches, e))?
            != Some(record)
            || endpoints
                .replica
                .accepted_genesis_record_id()
                .map_err(|e| formal(S::ObserveCaches, e))?
                != Some(record)
        {
            return Err(formal(
                S::ObserveCaches,
                FormalError::Invalid(
                    "GEN new durable result lacks exact local accepted source caches".into(),
                ),
            ));
        }
    }
    work.charge(1, 64, stop)
        .map_err(|e| formal(S::ObserveCaches, e))?;
    let encoded = blake3::Hash::from_bytes(record).to_hex();
    let n = primary
        .head()
        .record_id
        .len()
        .checked_add(replica.head().record_id.len())
        .and_then(|n| n.checked_add(128))
        .ok_or_else(|| {
            formal(
                S::ObserveCaches,
                FormalError::Invalid("GEN accepted head comparison layout".into()),
            )
        })?;
    work.charge(2, n as u64, stop)
        .map_err(|e| formal(S::ObserveCaches, e))?;
    if primary.head().position != proof.verified().record().position
        || replica.head().position != proof.verified().record().position
        || primary.head().record_id != encoded.as_str()
        || replica.head().record_id != encoded.as_str()
    {
        return Err(formal(
            S::ObserveCaches,
            FormalError::Invalid("GEN exact dual accepted heads".into()),
        ));
    }
    Ok(PaidGenesisSubmissionV1 {
        proof,
        primary,
        replica,
        _outer: outer,
    })
}
