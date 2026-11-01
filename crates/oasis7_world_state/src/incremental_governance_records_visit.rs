use super::*;
/// Exhaustive borrowed field/member emission. No DTO clone or Serde channel.
pub fn visit_governance_record_v1<'a, E>(
    source: GovernanceRecordRefV1<'a>,
    mut emit: impl FnMut(GovernanceCellV1<'a>, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use GovernanceCellV1 as C;
    use GovernanceCollectionV1 as G;
    use GovernanceColumnV1 as F;
    use MemberPayloadRefV1 as P;
    emit(C::Record, P::Unit)?;
    match source {
        GovernanceRecordRefV1::Identity(v) => {
            let GovernanceIdentityProfileState {
                agent_id,
                stake_locked,
                warmup_until_tick,
                status,
                slash_count,
                updated_at,
            } = v;
            emit(C::Column(F::AgentId), P::Text(agent_id))?;
            emit(C::Column(F::StakeLocked), P::U64(*stake_locked))?;
            emit(C::Column(F::WarmupUntilTick), P::U64(*warmup_until_tick))?;
            emit(
                C::Column(F::IdentityStatus),
                P::U64(identity_status(*status)),
            )?;
            emit(C::Column(F::SlashCount), P::U64(*slash_count as u64))?;
            emit(C::Column(F::UpdatedAt), P::U64(*updated_at))?;
        }
        GovernanceRecordRefV1::Proposal(v) => {
            let GovernanceProposalState {
                proposal_key,
                proposer_agent_id,
                title,
                description,
                options,
                voting_window_ticks,
                quorum_weight,
                pass_threshold_bps,
                opened_at,
                closes_at,
                status,
                finalized_at,
                winning_option,
                winning_weight,
                total_weight_at_finalize,
                snapshot_at_tick,
                vote_weight_snapshot,
            } = v;
            for (c, t) in [
                (F::ProposalKey, proposal_key),
                (F::ProposerAgentId, proposer_agent_id),
                (F::Title, title),
                (F::Description, description),
            ] {
                emit(C::Column(c), P::Text(t))?;
            }
            emit(C::Count(G::Options), P::U64(options.len() as u64))?;
            for (i, t) in options.iter().enumerate() {
                emit(C::Option(i as u64), P::Text(t))?;
            }
            for (c, n) in [
                (F::VotingWindowTicks, *voting_window_ticks),
                (F::QuorumWeight, *quorum_weight),
                (F::PassThresholdBps, *pass_threshold_bps as u64),
                (F::OpenedAt, *opened_at),
                (F::ClosesAt, *closes_at),
                (
                    F::ProposalStatus,
                    match status {
                        GovernanceProposalStatus::Open => 0,
                        GovernanceProposalStatus::Passed => 1,
                        GovernanceProposalStatus::Rejected => 2,
                    },
                ),
            ] {
                emit(C::Column(c), P::U64(n))?;
            }
            emit(
                C::Column(F::FinalizedAt),
                finalized_at.map_or(P::Unit, P::U64),
            )?;
            emit(
                C::Column(F::WinningOption),
                winning_option.as_deref().map_or(P::Unit, P::Text),
            )?;
            for (c, n) in [
                (F::WinningWeight, *winning_weight),
                (F::TotalWeightAtFinalize, *total_weight_at_finalize),
                (F::SnapshotAtTick, *snapshot_at_tick),
            ] {
                emit(C::Column(c), P::U64(n))?;
            }
            emit(
                C::Count(G::Snapshot),
                P::U64(vote_weight_snapshot.len() as u64),
            )?;
            for (id, v) in vote_weight_snapshot {
                snapshot(id, v, &mut emit)?;
            }
        }
        GovernanceRecordRefV1::Vote(v) => {
            let GovernanceVoteState {
                proposal_key,
                votes_by_agent,
                tallies,
                total_weight,
                last_updated_at,
            } = v;
            emit(C::Column(F::ProposalKey), P::Text(proposal_key))?;
            emit(C::Count(G::Ballots), P::U64(votes_by_agent.len() as u64))?;
            for (id, v) in votes_by_agent {
                ballot(id, v, &mut emit)?;
            }
            emit(C::Count(G::Tallies), P::U64(tallies.len() as u64))?;
            for (id, n) in tallies {
                emit(C::Tally(id), P::U64(*n))?;
            }
            emit(C::Column(F::TotalWeight), P::U64(*total_weight))?;
            emit(C::Column(F::LastUpdatedAt), P::U64(*last_updated_at))?;
        }
    }
    Ok(())
}

pub(super) fn snapshot<'a, E>(
    id: &'a str,
    v: &'a GovernanceVoteWeightSnapshotState,
    emit: &mut impl FnMut(GovernanceCellV1<'a>, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use GovernanceCellV1 as C;
    use GovernanceSnapshotColumnV1 as S;
    use MemberPayloadRefV1 as P;

    let GovernanceVoteWeightSnapshotState {
        agent_id,
        reputation_score,
        stake_locked,
        status,
        vote_weight_cap,
    } = v;
    emit(C::Snapshot(id, S::AgentId), P::Text(agent_id))?;
    emit(
        C::Snapshot(id, S::ReputationScore),
        P::I64(*reputation_score),
    )?;
    emit(C::Snapshot(id, S::StakeLocked), P::U64(*stake_locked))?;
    emit(C::Snapshot(id, S::Status), P::U64(identity_status(*status)))?;
    emit(
        C::Snapshot(id, S::VoteWeightCap),
        P::U64(*vote_weight_cap as u64),
    )?;

    Ok(())
}

pub(super) fn ballot<'a, E>(
    id: &'a str,
    v: &'a GovernanceVoteBallotState,
    emit: &mut impl FnMut(GovernanceCellV1<'a>, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use GovernanceBallotColumnV1 as B;
    use GovernanceCellV1 as C;
    use MemberPayloadRefV1 as P;

    let GovernanceVoteBallotState {
        option,
        weight,
        voted_at,
    } = v;
    emit(C::Ballot(id, B::Option), P::Text(option))?;
    emit(C::Ballot(id, B::Weight), P::U64(*weight as u64))?;
    emit(C::Ballot(id, B::VotedAt), P::U64(*voted_at))?;

    Ok(())
}
