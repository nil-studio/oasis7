use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardRecordKindV1 {
    Pool,
    Mint,
}
impl RewardRecordKindV1 {
    pub const ALL: [Self; 2] = [Self::Pool, Self::Mint];
    pub fn name(self) -> &'static str {
        match self {
            Self::Pool => "system_order_pool_budgets",
            Self::Mint => "reward_mint_records",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::Pool => StateNamespaceV1::Business,
            Self::Mint => StateNamespaceV1::Authorization,
        }
    }
    pub fn shape(self) -> FieldShape {
        match self {
            Self::Pool => FieldShape::Map,
            Self::Mint => FieldShape::Ordered,
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"oasis7.iws.reward-records.v1\0");
        h.update(self.name().as_bytes());
        *h.finalize().as_bytes()
    }
    pub(crate) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|f| {
                f.owner == FieldOwner::WorldState
                    && f.name == self.name()
                    && f.shape == self.shape()
                    && f.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid("reward record registration"))
    }
}
/// Original epoch map key or original Vec ordinal. Embedded epoch is independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardRecordKeyV1 {
    pub kind: RewardRecordKindV1,
    pub index: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardColumnV1 {
    EpochIndex,
    TotalCreditBudget,
    RemainingCreditBudget,
    NodeId,
    SourceAwardedPoints,
    MintedPowerCredits,
    SettlementHash,
    SignerNodeId,
    Signature,
}
impl RewardColumnV1 {
    pub const ALL: [Self; 9] = [
        Self::EpochIndex,
        Self::TotalCreditBudget,
        Self::RemainingCreditBudget,
        Self::NodeId,
        Self::SourceAwardedPoints,
        Self::MintedPowerCredits,
        Self::SettlementHash,
        Self::SignerNodeId,
        Self::Signature,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::EpochIndex => "epoch_index",
            Self::TotalCreditBudget => "total_credit_budget",
            Self::RemainingCreditBudget => "remaining_credit_budget",
            Self::NodeId => "node_id",
            Self::SourceAwardedPoints => "source_awarded_points",
            Self::MintedPowerCredits => "minted_power_credits",
            Self::SettlementHash => "settlement_hash",
            Self::SignerNodeId => "signer_node_id",
            Self::Signature => "signature",
        }
    }
    pub fn accepts(self, kind: RewardRecordKindV1) -> bool {
        match self {
            Self::EpochIndex => true,
            Self::TotalCreditBudget | Self::RemainingCreditBudget => {
                kind == RewardRecordKindV1::Pool
            }
            _ => kind == RewardRecordKindV1::Mint,
        }
    }
    pub fn codec(self) -> MemberPayloadCodecV1 {
        match self {
            Self::EpochIndex
            | Self::TotalCreditBudget
            | Self::RemainingCreditBudget
            | Self::SourceAwardedPoints
            | Self::MintedPowerCredits => MemberPayloadCodecV1::U64,
            _ => MemberPayloadCodecV1::Text,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardCollectionV1 {
    NodeCreditCaps,
    NodeCreditAllocated,
}
impl RewardCollectionV1 {
    pub const ALL: [Self; 2] = [Self::NodeCreditCaps, Self::NodeCreditAllocated];
    pub fn name(self) -> &'static str {
        match self {
            Self::NodeCreditCaps => "node_credit_caps",
            Self::NodeCreditAllocated => "node_credit_allocated",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardCellV1<'a> {
    Record,
    Column(RewardColumnV1),
    Collection(RewardCollectionV1),
    Member(RewardCollectionV1, &'a str),
}
impl RewardCellV1<'_> {
    pub fn accepts(self, kind: RewardRecordKindV1) -> bool {
        match self {
            Self::Record => true,
            Self::Column(c) => c.accepts(kind),
            Self::Collection(_) | Self::Member(..) => kind == RewardRecordKindV1::Pool,
        }
    }
    pub fn codec(self) -> MemberPayloadCodecV1 {
        match self {
            Self::Record | Self::Collection(_) => MemberPayloadCodecV1::Unit,
            Self::Column(c) => c.codec(),
            Self::Member(..) => MemberPayloadCodecV1::U64,
        }
    }
}
