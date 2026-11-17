//! Complete real GEN setup before entering the caller's recovery phases.
//! The returned original owners retain their fees throughout the caller.
use super::*;
use oasis7_chain_runtime::iws_genesis::{
    GenesisEndpointsV1, GenesisSubmissionErrorV1, GenesisSubmissionStageV1,
    PaidGenesisSubmissionV1, submit_staged_genesis_bounded,
};
use oasis7_distfs::controlled_authority::{
    fencing::{RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1},
    formal::endpoint::{
        FormalArchiveReadWorkV1, FormalCandidateMaterialV1, FormalQueryOutcomeV1,
        FormalRequestStateV1, IwsCurrentLimitsV1,
    },
};
use std::path::Path;

pub(super) struct Preparation<'a> {
    pub primary_root: &'a Path,
    pub replica_root: &'a Path,
    pub authority: &'a formal::VerifiedAuthorityV1,
    pub candidate: &'a FormalCandidateMaterialV1,
    pub limits: &'a IwsCurrentLimitsV1,
    pub primary_provider: &'a Arc<RemoteMonotonicEndpointProvider>,
    pub replica_provider: &'a Arc<RemoteMonotonicEndpointProvider>,
    pub policy: &'a TrustedFencingPolicyV1,
    pub memory: &'a MemoryBudget,
}

// The endpoints move into the original callback. Every other original owner
// remains here across that callback, including both paid submission results.
// Keep the remaining fields in the original reverse-local drop order.
pub(super) struct Accepted {
    pub primary: FileFormalEndpoint,
    pub replica: FileFormalEndpoint,
    _retry: PaidGenesisSubmissionV1,
    _before_replica: Vec<u8>,
    _before_primary: Vec<u8>,
    pub expected: formal::VerifiedFormalCommitV1,
    _result: PaidGenesisSubmissionV1,
    _cancelled: GenesisSubmissionErrorV1,
    pub work: FormalArchiveReadWorkV1,
    _replica_context: formal::VerifiedCandidateContextV1,
    _context: formal::VerifiedCandidateContextV1,
}

// This ordinary serial call returns before the caller starts Frozen recovery.
// Its verification/RPC temporaries must not remain below the recovery stack.
#[inline(never)]
pub(super) fn prepare(
    preparation: Preparation<'_>,
    primary_pages: &mut Option<IncrementalPageStore>,
    replica_pages: &mut Option<IncrementalPageStore>,
) -> Accepted {
    let Preparation {
        primary_root: pdir,
        replica_root: rdir,
        authority,
        candidate,
        limits,
        primary_provider,
        replica_provider,
        policy,
        memory,
    } = preparation;
    let authorization = || FormalEndpointAuthorizationV1 {
        authority: authority.clone(),
        original: authority.clone(),
        recovery: None,
        parent: None,
    };
    let mut primary = FileFormalEndpoint::open_protected(
        pdir,
        authorization(),
        EndpointRole::Primary,
        primary_provider.clone(),
        None,
    )
    .unwrap();
    let mut replica = FileFormalEndpoint::open_protected(
        rdir,
        authorization(),
        EndpointRole::Replica,
        replica_provider.clone(),
        None,
    )
    .unwrap();
    primary
        .install_iws_page_store(
            IwsPageStoreCapabilityV1::new(
                primary_pages.take().unwrap(),
                endpoint_material::budgets(),
            )
            .unwrap()
            .with_current_limits(limits.clone())
            .unwrap(),
        )
        .unwrap();
    replica
        .install_iws_page_store(
            IwsPageStoreCapabilityV1::new(
                replica_pages.take().unwrap(),
                endpoint_material::budgets(),
            )
            .unwrap()
            .with_current_limits(limits.clone())
            .unwrap(),
        )
        .unwrap();
    let request = &candidate.candidate.record.request_digest;
    primary
        .supply_iws_candidate_work(endpoint_material::source_work())
        .unwrap();
    replica
        .supply_iws_candidate_work(endpoint_material::source_work())
        .unwrap();
    // The production source API succeeds directly. No expected error
    // from old guards is used as coordinator control flow.
    let context = primary.stage_genesis_source(candidate, || false).unwrap();
    let replica_context = replica.stage_genesis_source(candidate, || false).unwrap();
    assert_eq!(context.record(), replica_context.record());
    assert!(matches!(
        primary.query(request).unwrap(),
        FormalQueryOutcomeV1::NotRecorded
    ));
    assert!(matches!(
        replica.query(request).unwrap(),
        FormalQueryOutcomeV1::NotRecorded
    ));
    let work = FormalArchiveReadWorkV1::new(1_000_000_000, 16u64 << 30).unwrap();
    let cancelled = submit_staged_genesis_bounded(
        candidate,
        &context,
        GenesisEndpointsV1 {
            primary: &mut primary,
            replica: &mut replica,
            primary_provider: primary_provider.as_ref(),
            replica_provider: replica_provider.as_ref(),
            primary_policy: policy,
            replica_policy: policy,
        },
        memory,
        &work,
        &mut || true,
    )
    .err()
    .unwrap();
    assert_eq!(cancelled.stage(), GenesisSubmissionStageV1::Admission);
    assert!(matches!(
        primary.query(request).unwrap(),
        FormalQueryOutcomeV1::NotRecorded
    ));
    assert!(matches!(
        replica.query(request).unwrap(),
        FormalQueryOutcomeV1::NotRecorded
    ));
    let result = submit_staged_genesis_bounded(
        candidate,
        &context,
        GenesisEndpointsV1 {
            primary: &mut primary,
            replica: &mut replica,
            primary_provider: primary_provider.as_ref(),
            replica_provider: replica_provider.as_ref(),
            primary_policy: policy,
            replica_policy: policy,
        },
        memory,
        &work,
        &mut || false,
    )
    .unwrap();
    let expected = formal::verify_formal_qualified(result.proof(), authority, None, None).unwrap();
    assert_eq!(result.primary().head(), &expected.head());
    assert_eq!(result.replica().head(), &expected.head());
    assert!(result.uses_budget(memory));
    for endpoint in [&mut primary, &mut replica] {
        let FormalQueryOutcomeV1::Recorded(state) = endpoint.query(request).unwrap() else {
            panic!("durable Qualified missing")
        };
        let FormalRequestStateV1::Qualified {
            proof: installed, ..
        } = state.as_ref()
        else {
            panic!("durable entry not Qualified")
        };
        assert_eq!(installed, result.proof());
    }
    assert_eq!(
        primary.accepted_genesis_record_id().unwrap(),
        Some(expected.verified_record_id())
    );
    assert_eq!(
        replica.accepted_genesis_record_id().unwrap(),
        Some(expected.verified_record_id())
    );
    let before_primary = std::fs::read(pdir.join("formal-checkpoint-v1.cbor")).unwrap();
    let before_replica = std::fs::read(rdir.join("formal-checkpoint-v1.cbor")).unwrap();
    // Accepted exact retry reads retained full proofs before prepare;
    // no source-work reset, re-signing or fresh staged lease is needed.
    let retry = submit_staged_genesis_bounded(
        candidate,
        &context,
        GenesisEndpointsV1 {
            primary: &mut primary,
            replica: &mut replica,
            primary_provider: primary_provider.as_ref(),
            replica_provider: replica_provider.as_ref(),
            primary_policy: policy,
            replica_policy: policy,
        },
        memory,
        &work,
        &mut || false,
    )
    .unwrap();
    assert_eq!(retry.proof(), result.proof());
    assert_eq!(retry.primary().head(), result.primary().head());
    assert_eq!(retry.replica().head(), result.replica().head());
    assert_eq!(
        std::fs::read(pdir.join("formal-checkpoint-v1.cbor")).unwrap(),
        before_primary
    );
    assert_eq!(
        std::fs::read(rdir.join("formal-checkpoint-v1.cbor")).unwrap(),
        before_replica
    );
    Accepted {
        primary,
        replica,
        _retry: retry,
        _before_replica: before_replica,
        _before_primary: before_primary,
        expected,
        _result: result,
        _cancelled: cancelled,
        work,
        _replica_context: replica_context,
        _context: context,
    }
}
