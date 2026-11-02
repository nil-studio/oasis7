use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTokenPolicyKindV1 {
    Config,
    Bucket,
    Issuance,
    Grant,
    Scheduled,
}
impl MainTokenPolicyKindV1 {
    pub const ALL: [Self; 5] = [
        Self::Config,
        Self::Bucket,
        Self::Issuance,
        Self::Grant,
        Self::Scheduled,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Config => "main_token_config",
            Self::Bucket => "main_token_genesis_buckets",
            Self::Issuance => "main_token_epoch_issuance_records",
            Self::Grant => "restricted_starter_claim_grants",
            Self::Scheduled => "main_token_scheduled_policy_updates",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::Config | Self::Bucket => StateNamespaceV1::Business,
            Self::Issuance | Self::Grant => StateNamespaceV1::Authorization,
            Self::Scheduled => StateNamespaceV1::Scheduler,
        }
    }
    pub fn shape(self) -> FieldShape {
        if self == Self::Config {
            FieldShape::Value
        } else {
            FieldShape::Map
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"oasis7.iws.main-token-policy.v1\0");
        h.update(self.name().as_bytes());
        *h.finalize().as_bytes()
    }
    pub(super) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|f| {
                f.owner == FieldOwner::WorldState
                    && f.name == self.name()
                    && f.shape == self.shape()
                    && f.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid("main token policy registration"))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTokenPolicyKeyV1<'a> {
    Config,
    Bucket(&'a str),
    Issuance(u64),
    Grant(&'a str),
    Scheduled(u64),
}
impl MainTokenPolicyKeyV1<'_> {
    pub fn kind(self) -> MainTokenPolicyKindV1 {
        match self {
            Self::Config => MainTokenPolicyKindV1::Config,
            Self::Bucket(_) => MainTokenPolicyKindV1::Bucket,
            Self::Issuance(_) => MainTokenPolicyKindV1::Issuance,
            Self::Grant(_) => MainTokenPolicyKindV1::Grant,
            Self::Scheduled(_) => MainTokenPolicyKindV1::Scheduled,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTokenConfigColumnV1 {
    Symbol,
    Decimals,
    InitialSupply,
    MaxSupply,
    BaseRateBps,
    MinRateBps,
    MaxRateBps,
    TargetStakeRatioBps,
    StakeFeedbackGainBps,
    EpochsPerYear,
    StakingRewardBps,
    NodeServiceRewardBps,
    EcosystemPoolBps,
    SecurityReserveBps,
    GasBaseFeeBurnBps,
    SlashBurnBps,
    ModuleFeeBurnBps,
}
impl MainTokenConfigColumnV1 {
    pub const ALL: [Self; 17] = [
        Self::Symbol,
        Self::Decimals,
        Self::InitialSupply,
        Self::MaxSupply,
        Self::BaseRateBps,
        Self::MinRateBps,
        Self::MaxRateBps,
        Self::TargetStakeRatioBps,
        Self::StakeFeedbackGainBps,
        Self::EpochsPerYear,
        Self::StakingRewardBps,
        Self::NodeServiceRewardBps,
        Self::EcosystemPoolBps,
        Self::SecurityReserveBps,
        Self::GasBaseFeeBurnBps,
        Self::SlashBurnBps,
        Self::ModuleFeeBurnBps,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::Decimals => "decimals",
            Self::InitialSupply => "initial_supply",
            Self::MaxSupply => "max_supply",
            Self::BaseRateBps => "base_rate_bps",
            Self::MinRateBps => "min_rate_bps",
            Self::MaxRateBps => "max_rate_bps",
            Self::TargetStakeRatioBps => "target_stake_ratio_bps",
            Self::StakeFeedbackGainBps => "stake_feedback_gain_bps",
            Self::EpochsPerYear => "epochs_per_year",
            Self::StakingRewardBps => "staking_reward_bps",
            Self::NodeServiceRewardBps => "node_service_reward_bps",
            Self::EcosystemPoolBps => "ecosystem_pool_bps",
            Self::SecurityReserveBps => "security_reserve_bps",
            Self::GasBaseFeeBurnBps => "gas_base_fee_burn_bps",
            Self::SlashBurnBps => "slash_burn_bps",
            Self::ModuleFeeBurnBps => "module_fee_burn_bps",
        }
    }
    pub fn group(self) -> Option<&'static str> {
        match self {
            Self::Symbol => None,
            Self::Decimals => None,
            Self::InitialSupply => None,
            Self::MaxSupply => None,
            Self::BaseRateBps => Some("inflation_policy"),
            Self::MinRateBps => Some("inflation_policy"),
            Self::MaxRateBps => Some("inflation_policy"),
            Self::TargetStakeRatioBps => Some("inflation_policy"),
            Self::StakeFeedbackGainBps => Some("inflation_policy"),
            Self::EpochsPerYear => Some("inflation_policy"),
            Self::StakingRewardBps => Some("issuance_split"),
            Self::NodeServiceRewardBps => Some("issuance_split"),
            Self::EcosystemPoolBps => Some("issuance_split"),
            Self::SecurityReserveBps => Some("issuance_split"),
            Self::GasBaseFeeBurnBps => Some("burn_policy"),
            Self::SlashBurnBps => Some("burn_policy"),
            Self::ModuleFeeBurnBps => Some("burn_policy"),
        }
    }
    pub(super) fn accepts(self, value: MemberPayloadRefV1<'_>) -> bool {
        match self {
            Self::Symbol => matches!(value, MemberPayloadRefV1::Text(_)),
            Self::Decimals => matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u8::MAX)),
            Self::InitialSupply => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::MaxSupply => {
                matches!(value, MemberPayloadRefV1::Unit | MemberPayloadRefV1::U64(_))
            }
            Self::BaseRateBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::MinRateBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::MaxRateBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::TargetStakeRatioBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::StakeFeedbackGainBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::EpochsPerYear => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::StakingRewardBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::NodeServiceRewardBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::EcosystemPoolBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::SecurityReserveBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::GasBaseFeeBurnBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::SlashBurnBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::ModuleFeeBurnBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTokenPolicyColumnV1 {
    BucketId,
    RatioBps,
    Recipient,
    CliffEpochs,
    LinearUnlockEpochs,
    StartEpoch,
    AllocatedAmount,
    ClaimedAmount,
    EpochIndex,
    InflationRateBps,
    IssuedAmount,
    StakingRewardAmount,
    NodeServiceRewardAmount,
    EcosystemPoolAmount,
    SecurityReserveAmount,
    BeneficiaryAccountId,
    IssuerId,
    IssuanceReason,
    SpendScope,
    SourceTreasuryBucketId,
    IssuedAtEpoch,
    ExpiresAtEpoch,
    Status,
    StatusUpdatedAtEpoch,
    StatusReason,
    ProposalId,
    EffectiveEpoch,
}
impl MainTokenPolicyColumnV1 {
    pub const ALL: [Self; 27] = [
        Self::BucketId,
        Self::RatioBps,
        Self::Recipient,
        Self::CliffEpochs,
        Self::LinearUnlockEpochs,
        Self::StartEpoch,
        Self::AllocatedAmount,
        Self::ClaimedAmount,
        Self::EpochIndex,
        Self::InflationRateBps,
        Self::IssuedAmount,
        Self::StakingRewardAmount,
        Self::NodeServiceRewardAmount,
        Self::EcosystemPoolAmount,
        Self::SecurityReserveAmount,
        Self::BeneficiaryAccountId,
        Self::IssuerId,
        Self::IssuanceReason,
        Self::SpendScope,
        Self::SourceTreasuryBucketId,
        Self::IssuedAtEpoch,
        Self::ExpiresAtEpoch,
        Self::Status,
        Self::StatusUpdatedAtEpoch,
        Self::StatusReason,
        Self::ProposalId,
        Self::EffectiveEpoch,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::BucketId => "bucket_id",
            Self::RatioBps => "ratio_bps",
            Self::Recipient => "recipient",
            Self::CliffEpochs => "cliff_epochs",
            Self::LinearUnlockEpochs => "linear_unlock_epochs",
            Self::StartEpoch => "start_epoch",
            Self::AllocatedAmount => "allocated_amount",
            Self::ClaimedAmount => "claimed_amount",
            Self::EpochIndex => "epoch_index",
            Self::InflationRateBps => "inflation_rate_bps",
            Self::IssuedAmount => "issued_amount",
            Self::StakingRewardAmount => "staking_reward_amount",
            Self::NodeServiceRewardAmount => "node_service_reward_amount",
            Self::EcosystemPoolAmount => "ecosystem_pool_amount",
            Self::SecurityReserveAmount => "security_reserve_amount",
            Self::BeneficiaryAccountId => "beneficiary_account_id",
            Self::IssuerId => "issuer_id",
            Self::IssuanceReason => "issuance_reason",
            Self::SpendScope => "spend_scope",
            Self::SourceTreasuryBucketId => "source_treasury_bucket_id",
            Self::IssuedAtEpoch => "issued_at_epoch",
            Self::ExpiresAtEpoch => "expires_at_epoch",
            Self::Status => "status",
            Self::StatusUpdatedAtEpoch => "status_updated_at_epoch",
            Self::StatusReason => "status_reason",
            Self::ProposalId => "proposal_id",
            Self::EffectiveEpoch => "effective_epoch",
        }
    }
    pub fn belongs(self, kind: MainTokenPolicyKindV1) -> bool {
        match kind {
            MainTokenPolicyKindV1::Config => false,
            MainTokenPolicyKindV1::Bucket => matches!(
                self,
                Self::BucketId
                    | Self::RatioBps
                    | Self::Recipient
                    | Self::CliffEpochs
                    | Self::LinearUnlockEpochs
                    | Self::StartEpoch
                    | Self::AllocatedAmount
                    | Self::ClaimedAmount
            ),
            MainTokenPolicyKindV1::Issuance => matches!(
                self,
                Self::EpochIndex
                    | Self::InflationRateBps
                    | Self::IssuedAmount
                    | Self::StakingRewardAmount
                    | Self::NodeServiceRewardAmount
                    | Self::EcosystemPoolAmount
                    | Self::SecurityReserveAmount
            ),
            MainTokenPolicyKindV1::Grant => matches!(
                self,
                Self::BeneficiaryAccountId
                    | Self::IssuerId
                    | Self::IssuanceReason
                    | Self::SpendScope
                    | Self::SourceTreasuryBucketId
                    | Self::IssuedAmount
                    | Self::IssuedAtEpoch
                    | Self::ExpiresAtEpoch
                    | Self::Status
                    | Self::StatusUpdatedAtEpoch
                    | Self::StatusReason
            ),
            MainTokenPolicyKindV1::Scheduled => {
                matches!(self, Self::ProposalId | Self::EffectiveEpoch)
            }
        }
    }
    pub(super) fn accepts(self, value: MemberPayloadRefV1<'_>) -> bool {
        match self {
            Self::BucketId => matches!(value, MemberPayloadRefV1::Text(_)),
            Self::RatioBps => matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX)),
            Self::Recipient => matches!(value, MemberPayloadRefV1::Text(_)),
            Self::CliffEpochs => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::LinearUnlockEpochs => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::StartEpoch => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::AllocatedAmount => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::ClaimedAmount => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::EpochIndex => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::InflationRateBps => {
                matches!(value,MemberPayloadRefV1::U64(n) if n<=u64::from(u32::MAX))
            }
            Self::IssuedAmount => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::StakingRewardAmount => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::NodeServiceRewardAmount => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::EcosystemPoolAmount => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::SecurityReserveAmount => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::BeneficiaryAccountId => matches!(value, MemberPayloadRefV1::Text(_)),
            Self::IssuerId => matches!(value, MemberPayloadRefV1::Text(_)),
            Self::IssuanceReason => matches!(value, MemberPayloadRefV1::Text(_)),
            Self::SpendScope => matches!(value, MemberPayloadRefV1::Text(_)),
            Self::SourceTreasuryBucketId => matches!(value, MemberPayloadRefV1::Text(_)),
            Self::IssuedAtEpoch => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::ExpiresAtEpoch => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::Status => matches!(
                value,
                MemberPayloadRefV1::Text("issued" | "expired" | "revoked")
            ),
            Self::StatusUpdatedAtEpoch => {
                matches!(value, MemberPayloadRefV1::Unit | MemberPayloadRefV1::U64(_))
            }
            Self::StatusReason => matches!(
                value,
                MemberPayloadRefV1::Unit | MemberPayloadRefV1::Text(_)
            ),
            Self::ProposalId => matches!(value, MemberPayloadRefV1::U64(_)),
            Self::EffectiveEpoch => matches!(value, MemberPayloadRefV1::U64(_)),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTokenPolicyCellV1 {
    Record,
    Column(MainTokenPolicyColumnV1),
    Config(MainTokenConfigColumnV1),
    NextConfig(MainTokenConfigColumnV1),
}
impl MainTokenPolicyCellV1 {
    pub(super) fn accepts(
        self,
        kind: MainTokenPolicyKindV1,
        value: MemberPayloadRefV1<'_>,
    ) -> bool {
        match self {
            Self::Record => matches!(value, MemberPayloadRefV1::Unit),
            Self::Column(c) => c.belongs(kind) && c.accepts(value),
            Self::Config(c) => kind == MainTokenPolicyKindV1::Config && c.accepts(value),
            Self::NextConfig(c) => kind == MainTokenPolicyKindV1::Scheduled && c.accepts(value),
        }
    }
}

impl MainTokenPolicyCellV1 {
    pub const ALL: [Self; 62] = [
        Self::Record,
        Self::Column(MainTokenPolicyColumnV1::BucketId),
        Self::Column(MainTokenPolicyColumnV1::RatioBps),
        Self::Column(MainTokenPolicyColumnV1::Recipient),
        Self::Column(MainTokenPolicyColumnV1::CliffEpochs),
        Self::Column(MainTokenPolicyColumnV1::LinearUnlockEpochs),
        Self::Column(MainTokenPolicyColumnV1::StartEpoch),
        Self::Column(MainTokenPolicyColumnV1::AllocatedAmount),
        Self::Column(MainTokenPolicyColumnV1::ClaimedAmount),
        Self::Column(MainTokenPolicyColumnV1::EpochIndex),
        Self::Column(MainTokenPolicyColumnV1::InflationRateBps),
        Self::Column(MainTokenPolicyColumnV1::IssuedAmount),
        Self::Column(MainTokenPolicyColumnV1::StakingRewardAmount),
        Self::Column(MainTokenPolicyColumnV1::NodeServiceRewardAmount),
        Self::Column(MainTokenPolicyColumnV1::EcosystemPoolAmount),
        Self::Column(MainTokenPolicyColumnV1::SecurityReserveAmount),
        Self::Column(MainTokenPolicyColumnV1::BeneficiaryAccountId),
        Self::Column(MainTokenPolicyColumnV1::IssuerId),
        Self::Column(MainTokenPolicyColumnV1::IssuanceReason),
        Self::Column(MainTokenPolicyColumnV1::SpendScope),
        Self::Column(MainTokenPolicyColumnV1::SourceTreasuryBucketId),
        Self::Column(MainTokenPolicyColumnV1::IssuedAtEpoch),
        Self::Column(MainTokenPolicyColumnV1::ExpiresAtEpoch),
        Self::Column(MainTokenPolicyColumnV1::Status),
        Self::Column(MainTokenPolicyColumnV1::StatusUpdatedAtEpoch),
        Self::Column(MainTokenPolicyColumnV1::StatusReason),
        Self::Column(MainTokenPolicyColumnV1::ProposalId),
        Self::Column(MainTokenPolicyColumnV1::EffectiveEpoch),
        Self::Config(MainTokenConfigColumnV1::Symbol),
        Self::Config(MainTokenConfigColumnV1::Decimals),
        Self::Config(MainTokenConfigColumnV1::InitialSupply),
        Self::Config(MainTokenConfigColumnV1::MaxSupply),
        Self::Config(MainTokenConfigColumnV1::BaseRateBps),
        Self::Config(MainTokenConfigColumnV1::MinRateBps),
        Self::Config(MainTokenConfigColumnV1::MaxRateBps),
        Self::Config(MainTokenConfigColumnV1::TargetStakeRatioBps),
        Self::Config(MainTokenConfigColumnV1::StakeFeedbackGainBps),
        Self::Config(MainTokenConfigColumnV1::EpochsPerYear),
        Self::Config(MainTokenConfigColumnV1::StakingRewardBps),
        Self::Config(MainTokenConfigColumnV1::NodeServiceRewardBps),
        Self::Config(MainTokenConfigColumnV1::EcosystemPoolBps),
        Self::Config(MainTokenConfigColumnV1::SecurityReserveBps),
        Self::Config(MainTokenConfigColumnV1::GasBaseFeeBurnBps),
        Self::Config(MainTokenConfigColumnV1::SlashBurnBps),
        Self::Config(MainTokenConfigColumnV1::ModuleFeeBurnBps),
        Self::NextConfig(MainTokenConfigColumnV1::Symbol),
        Self::NextConfig(MainTokenConfigColumnV1::Decimals),
        Self::NextConfig(MainTokenConfigColumnV1::InitialSupply),
        Self::NextConfig(MainTokenConfigColumnV1::MaxSupply),
        Self::NextConfig(MainTokenConfigColumnV1::BaseRateBps),
        Self::NextConfig(MainTokenConfigColumnV1::MinRateBps),
        Self::NextConfig(MainTokenConfigColumnV1::MaxRateBps),
        Self::NextConfig(MainTokenConfigColumnV1::TargetStakeRatioBps),
        Self::NextConfig(MainTokenConfigColumnV1::StakeFeedbackGainBps),
        Self::NextConfig(MainTokenConfigColumnV1::EpochsPerYear),
        Self::NextConfig(MainTokenConfigColumnV1::StakingRewardBps),
        Self::NextConfig(MainTokenConfigColumnV1::NodeServiceRewardBps),
        Self::NextConfig(MainTokenConfigColumnV1::EcosystemPoolBps),
        Self::NextConfig(MainTokenConfigColumnV1::SecurityReserveBps),
        Self::NextConfig(MainTokenConfigColumnV1::GasBaseFeeBurnBps),
        Self::NextConfig(MainTokenConfigColumnV1::SlashBurnBps),
        Self::NextConfig(MainTokenConfigColumnV1::ModuleFeeBurnBps),
    ];
    pub fn belongs(self, kind: MainTokenPolicyKindV1) -> bool {
        match self {
            Self::Record => true,
            Self::Column(c) => c.belongs(kind),
            Self::Config(_) => kind == MainTokenPolicyKindV1::Config,
            Self::NextConfig(_) => kind == MainTokenPolicyKindV1::Scheduled,
        }
    }
    pub(super) fn profile(self) -> (MemberPayloadCodecV1, bool, u64) {
        match self {
            Self::Record => (MemberPayloadCodecV1::Unit, false, u64::MAX),
            Self::Column(c) => match c {
                MainTokenPolicyColumnV1::BucketId => (MemberPayloadCodecV1::Text, false, u64::MAX),
                MainTokenPolicyColumnV1::RatioBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenPolicyColumnV1::Recipient => (MemberPayloadCodecV1::Text, false, u64::MAX),
                MainTokenPolicyColumnV1::CliffEpochs => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::LinearUnlockEpochs => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::StartEpoch => (MemberPayloadCodecV1::U64, false, u64::MAX),
                MainTokenPolicyColumnV1::AllocatedAmount => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::ClaimedAmount => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::EpochIndex => (MemberPayloadCodecV1::U64, false, u64::MAX),
                MainTokenPolicyColumnV1::InflationRateBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenPolicyColumnV1::IssuedAmount => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::StakingRewardAmount => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::NodeServiceRewardAmount => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::EcosystemPoolAmount => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::SecurityReserveAmount => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::BeneficiaryAccountId => {
                    (MemberPayloadCodecV1::Text, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::IssuerId => (MemberPayloadCodecV1::Text, false, u64::MAX),
                MainTokenPolicyColumnV1::IssuanceReason => {
                    (MemberPayloadCodecV1::Text, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::SpendScope => {
                    (MemberPayloadCodecV1::Text, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::SourceTreasuryBucketId => {
                    (MemberPayloadCodecV1::Text, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::IssuedAtEpoch => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::ExpiresAtEpoch => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenPolicyColumnV1::Status => (MemberPayloadCodecV1::Text, false, u64::MAX),
                MainTokenPolicyColumnV1::StatusUpdatedAtEpoch => {
                    (MemberPayloadCodecV1::U64, true, u64::MAX)
                }
                MainTokenPolicyColumnV1::StatusReason => {
                    (MemberPayloadCodecV1::Text, true, u64::MAX)
                }
                MainTokenPolicyColumnV1::ProposalId => (MemberPayloadCodecV1::U64, false, u64::MAX),
                MainTokenPolicyColumnV1::EffectiveEpoch => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
            },
            Self::Config(c) | Self::NextConfig(c) => match c {
                MainTokenConfigColumnV1::Symbol => (MemberPayloadCodecV1::Text, false, u64::MAX),
                MainTokenConfigColumnV1::Decimals => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u8::MAX))
                }
                MainTokenConfigColumnV1::InitialSupply => {
                    (MemberPayloadCodecV1::U64, false, u64::MAX)
                }
                MainTokenConfigColumnV1::MaxSupply => (MemberPayloadCodecV1::U64, true, u64::MAX),
                MainTokenConfigColumnV1::BaseRateBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::MinRateBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::MaxRateBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::TargetStakeRatioBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::StakeFeedbackGainBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::EpochsPerYear => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::StakingRewardBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::NodeServiceRewardBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::EcosystemPoolBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::SecurityReserveBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::GasBaseFeeBurnBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::SlashBurnBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
                MainTokenConfigColumnV1::ModuleFeeBurnBps => {
                    (MemberPayloadCodecV1::U64, false, u64::from(u32::MAX))
                }
            },
        }
    }
}
