use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceRecordKindV1 {
    Identity,
    Proposal,
    Vote,
}
impl GovernanceRecordKindV1 {
    pub const ALL: [Self; 3] = [Self::Identity, Self::Proposal, Self::Vote];
    pub fn field_name(self) -> &'static str {
        match self {
            Self::Identity => "governance_identity_profiles",
            Self::Proposal => "governance_proposals",
            Self::Vote => "governance_votes",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        StateNamespaceV1::Authorization
    }
    pub fn schema(self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"oasis7.iws.governance-record.v1\0");
        h.update(self.field_name().as_bytes());
        *h.finalize().as_bytes()
    }
    pub(super) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|f| {
                f.owner == FieldOwner::WorldState
                    && f.name == self.field_name()
                    && f.shape == FieldShape::Map
                    && f.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid("governance registration"))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GovernanceRecordKeyV1<'a> {
    pub kind: GovernanceRecordKindV1,
    pub id: &'a str,
}
impl GovernanceRecordKeyV1<'_> {
    pub fn namespace(self) -> StateNamespaceV1 {
        self.kind.namespace()
    }
    pub(super) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        self.kind.descriptor()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceColumnV1 {
    AgentId,
    StakeLocked,
    WarmupUntilTick,
    IdentityStatus,
    SlashCount,
    UpdatedAt,
    ProposalKey,
    ProposerAgentId,
    Title,
    Description,
    VotingWindowTicks,
    QuorumWeight,
    PassThresholdBps,
    OpenedAt,
    ClosesAt,
    ProposalStatus,
    FinalizedAt,
    WinningOption,
    WinningWeight,
    TotalWeightAtFinalize,
    SnapshotAtTick,
    TotalWeight,
    LastUpdatedAt,
}
impl GovernanceColumnV1 {
    pub const ALL: [Self; 23] = [
        Self::AgentId,
        Self::StakeLocked,
        Self::WarmupUntilTick,
        Self::IdentityStatus,
        Self::SlashCount,
        Self::UpdatedAt,
        Self::ProposalKey,
        Self::ProposerAgentId,
        Self::Title,
        Self::Description,
        Self::VotingWindowTicks,
        Self::QuorumWeight,
        Self::PassThresholdBps,
        Self::OpenedAt,
        Self::ClosesAt,
        Self::ProposalStatus,
        Self::FinalizedAt,
        Self::WinningOption,
        Self::WinningWeight,
        Self::TotalWeightAtFinalize,
        Self::SnapshotAtTick,
        Self::TotalWeight,
        Self::LastUpdatedAt,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::AgentId => "agent_id",
            Self::StakeLocked => "stake_locked",
            Self::WarmupUntilTick => "warmup_until_tick",
            Self::IdentityStatus => "status",
            Self::SlashCount => "slash_count",
            Self::UpdatedAt => "updated_at",
            Self::ProposalKey => "proposal_key",
            Self::ProposerAgentId => "proposer_agent_id",
            Self::Title => "title",
            Self::Description => "description",
            Self::VotingWindowTicks => "voting_window_ticks",
            Self::QuorumWeight => "quorum_weight",
            Self::PassThresholdBps => "pass_threshold_bps",
            Self::OpenedAt => "opened_at",
            Self::ClosesAt => "closes_at",
            Self::ProposalStatus => "status",
            Self::FinalizedAt => "finalized_at",
            Self::WinningOption => "winning_option",
            Self::WinningWeight => "winning_weight",
            Self::TotalWeightAtFinalize => "total_weight_at_finalize",
            Self::SnapshotAtTick => "snapshot_at_tick",
            Self::TotalWeight => "total_weight",
            Self::LastUpdatedAt => "last_updated_at",
        }
    }
    pub fn accepts(self, kind: GovernanceRecordKindV1) -> bool {
        match self {
            Self::AgentId => kind == GovernanceRecordKindV1::Identity,
            Self::StakeLocked => kind == GovernanceRecordKindV1::Identity,
            Self::WarmupUntilTick => kind == GovernanceRecordKindV1::Identity,
            Self::IdentityStatus => kind == GovernanceRecordKindV1::Identity,
            Self::SlashCount => kind == GovernanceRecordKindV1::Identity,
            Self::UpdatedAt => kind == GovernanceRecordKindV1::Identity,
            Self::ProposalKey => matches!(
                kind,
                GovernanceRecordKindV1::Proposal | GovernanceRecordKindV1::Vote
            ),
            Self::ProposerAgentId => kind == GovernanceRecordKindV1::Proposal,
            Self::Title => kind == GovernanceRecordKindV1::Proposal,
            Self::Description => kind == GovernanceRecordKindV1::Proposal,
            Self::VotingWindowTicks => kind == GovernanceRecordKindV1::Proposal,
            Self::QuorumWeight => kind == GovernanceRecordKindV1::Proposal,
            Self::PassThresholdBps => kind == GovernanceRecordKindV1::Proposal,
            Self::OpenedAt => kind == GovernanceRecordKindV1::Proposal,
            Self::ClosesAt => kind == GovernanceRecordKindV1::Proposal,
            Self::ProposalStatus => kind == GovernanceRecordKindV1::Proposal,
            Self::FinalizedAt => kind == GovernanceRecordKindV1::Proposal,
            Self::WinningOption => kind == GovernanceRecordKindV1::Proposal,
            Self::WinningWeight => kind == GovernanceRecordKindV1::Proposal,
            Self::TotalWeightAtFinalize => kind == GovernanceRecordKindV1::Proposal,
            Self::SnapshotAtTick => kind == GovernanceRecordKindV1::Proposal,
            Self::TotalWeight => kind == GovernanceRecordKindV1::Vote,
            Self::LastUpdatedAt => kind == GovernanceRecordKindV1::Vote,
        }
    }
    pub fn codec(self) -> MemberPayloadCodecV1 {
        match self {
            Self::AgentId => MemberPayloadCodecV1::Text,
            Self::StakeLocked => MemberPayloadCodecV1::U64,
            Self::WarmupUntilTick => MemberPayloadCodecV1::U64,
            Self::IdentityStatus => MemberPayloadCodecV1::U64,
            Self::SlashCount => MemberPayloadCodecV1::U64,
            Self::UpdatedAt => MemberPayloadCodecV1::U64,
            Self::ProposalKey => MemberPayloadCodecV1::Text,
            Self::ProposerAgentId => MemberPayloadCodecV1::Text,
            Self::Title => MemberPayloadCodecV1::Text,
            Self::Description => MemberPayloadCodecV1::Text,
            Self::VotingWindowTicks => MemberPayloadCodecV1::U64,
            Self::QuorumWeight => MemberPayloadCodecV1::U64,
            Self::PassThresholdBps => MemberPayloadCodecV1::U64,
            Self::OpenedAt => MemberPayloadCodecV1::U64,
            Self::ClosesAt => MemberPayloadCodecV1::U64,
            Self::ProposalStatus => MemberPayloadCodecV1::U64,
            Self::FinalizedAt => MemberPayloadCodecV1::U64,
            Self::WinningOption => MemberPayloadCodecV1::Text,
            Self::WinningWeight => MemberPayloadCodecV1::U64,
            Self::TotalWeightAtFinalize => MemberPayloadCodecV1::U64,
            Self::SnapshotAtTick => MemberPayloadCodecV1::U64,
            Self::TotalWeight => MemberPayloadCodecV1::U64,
            Self::LastUpdatedAt => MemberPayloadCodecV1::U64,
        }
    }
    pub fn nullable(self) -> bool {
        matches!(self, Self::FinalizedAt | Self::WinningOption)
    }
    pub fn integer_max(self) -> u64 {
        match self {
            Self::SlashCount => u32::MAX as u64,
            Self::PassThresholdBps => u16::MAX as u64,
            Self::IdentityStatus | Self::ProposalStatus => 2,
            _ => u64::MAX,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceCollectionV1 {
    Options,
    Snapshot,
    Ballots,
    Tallies,
}
impl GovernanceCollectionV1 {
    pub const ALL: [Self; 4] = [Self::Options, Self::Snapshot, Self::Ballots, Self::Tallies];
    pub fn name(self) -> &'static str {
        match self {
            Self::Options => "options",
            Self::Snapshot => "vote_weight_snapshot",
            Self::Ballots => "votes_by_agent",
            Self::Tallies => "tallies",
        }
    }
    pub fn accepts(self, kind: GovernanceRecordKindV1) -> bool {
        match self {
            Self::Options | Self::Snapshot => kind == GovernanceRecordKindV1::Proposal,
            Self::Ballots | Self::Tallies => kind == GovernanceRecordKindV1::Vote,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceSnapshotColumnV1 {
    AgentId,
    ReputationScore,
    StakeLocked,
    Status,
    VoteWeightCap,
}
impl GovernanceSnapshotColumnV1 {
    pub const ALL: [Self; 5] = [
        Self::AgentId,
        Self::ReputationScore,
        Self::StakeLocked,
        Self::Status,
        Self::VoteWeightCap,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::AgentId => "agent_id",
            Self::ReputationScore => "reputation_score",
            Self::StakeLocked => "stake_locked",
            Self::Status => "status",
            Self::VoteWeightCap => "vote_weight_cap",
        }
    }
    pub fn codec(self) -> MemberPayloadCodecV1 {
        match self {
            Self::AgentId => MemberPayloadCodecV1::Text,
            Self::ReputationScore => MemberPayloadCodecV1::I64,
            _ => MemberPayloadCodecV1::U64,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceBallotColumnV1 {
    Option,
    Weight,
    VotedAt,
}
impl GovernanceBallotColumnV1 {
    pub const ALL: [Self; 3] = [Self::Option, Self::Weight, Self::VotedAt];
    pub fn name(self) -> &'static str {
        match self {
            Self::Option => "option",
            Self::Weight => "weight",
            Self::VotedAt => "voted_at",
        }
    }
    pub fn codec(self) -> MemberPayloadCodecV1 {
        if self == Self::Option {
            MemberPayloadCodecV1::Text
        } else {
            MemberPayloadCodecV1::U64
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceCellV1<'a> {
    Record,
    Column(GovernanceColumnV1),
    Count(GovernanceCollectionV1),
    Option(u64),
    Snapshot(&'a str, GovernanceSnapshotColumnV1),
    Ballot(&'a str, GovernanceBallotColumnV1),
    Tally(&'a str),
}
impl GovernanceCellV1<'_> {
    pub fn is_member(self) -> bool {
        matches!(
            self,
            Self::Option(_) | Self::Snapshot(..) | Self::Ballot(..) | Self::Tally(_)
        )
    }
    pub fn accepts(self, kind: GovernanceRecordKindV1) -> bool {
        match self {
            Self::Record => true,
            Self::Column(c) => c.accepts(kind),
            Self::Count(c) => c.accepts(kind),
            Self::Option(_) | Self::Snapshot(..) => kind == GovernanceRecordKindV1::Proposal,
            Self::Ballot(..) | Self::Tally(_) => kind == GovernanceRecordKindV1::Vote,
        }
    }
    pub fn codec(self) -> MemberPayloadCodecV1 {
        match self {
            Self::Record => MemberPayloadCodecV1::Unit,
            Self::Column(c) => c.codec(),
            Self::Count(_) | Self::Tally(_) => MemberPayloadCodecV1::U64,
            Self::Option(_) => MemberPayloadCodecV1::Text,
            Self::Snapshot(_, c) => c.codec(),
            Self::Ballot(_, c) => c.codec(),
        }
    }
    pub fn nullable(self) -> bool {
        matches!(self,Self::Column(c) if c.nullable())
    }
    pub fn integer_max(self) -> u64 {
        match self {
            Self::Column(c) => c.integer_max(),
            Self::Snapshot(_, GovernanceSnapshotColumnV1::Status) => 2,
            Self::Snapshot(_, GovernanceSnapshotColumnV1::VoteWeightCap)
            | Self::Ballot(_, GovernanceBallotColumnV1::Weight) => u32::MAX as u64,
            _ => u64::MAX,
        }
    }
}
