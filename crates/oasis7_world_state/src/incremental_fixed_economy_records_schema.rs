use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedEconomyColumnV1 {
    TotalSupply,
    CirculatingSupply,
    TotalIssued,
    TotalBurned,
    PointsPerCredit,
    CreditsPerPowerUnit,
    MaxRedeemPowerPerEpoch,
    MinRedeemPowerUnit,
    EpochIndex,
    AvailablePowerUnits,
    RedeemedPowerUnits,
    RequireMintsigV2,
    AllowMintsigV1Fallback,
    RequireRedeemSignature,
    RequireRedeemSignerMatchNodeId,
}
impl FixedEconomyColumnV1 {
    pub const ALL: [Self; 15] = [
        Self::TotalSupply,
        Self::CirculatingSupply,
        Self::TotalIssued,
        Self::TotalBurned,
        Self::PointsPerCredit,
        Self::CreditsPerPowerUnit,
        Self::MaxRedeemPowerPerEpoch,
        Self::MinRedeemPowerUnit,
        Self::EpochIndex,
        Self::AvailablePowerUnits,
        Self::RedeemedPowerUnits,
        Self::RequireMintsigV2,
        Self::AllowMintsigV1Fallback,
        Self::RequireRedeemSignature,
        Self::RequireRedeemSignerMatchNodeId,
    ];
    pub fn field(self) -> FixedEconomyFieldV1 {
        match self {
            Self::TotalSupply | Self::CirculatingSupply | Self::TotalIssued | Self::TotalBurned => {
                FixedEconomyFieldV1::MainTokenSupply
            }
            Self::PointsPerCredit
            | Self::CreditsPerPowerUnit
            | Self::MaxRedeemPowerPerEpoch
            | Self::MinRedeemPowerUnit => FixedEconomyFieldV1::RewardAssetConfig,
            Self::EpochIndex | Self::AvailablePowerUnits | Self::RedeemedPowerUnits => {
                FixedEconomyFieldV1::ProtocolPowerReserve
            }
            Self::RequireMintsigV2
            | Self::AllowMintsigV1Fallback
            | Self::RequireRedeemSignature
            | Self::RequireRedeemSignerMatchNodeId => {
                FixedEconomyFieldV1::RewardSignatureGovernancePolicy
            }
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::TotalSupply => "total_supply",
            Self::CirculatingSupply => "circulating_supply",
            Self::TotalIssued => "total_issued",
            Self::TotalBurned => "total_burned",
            Self::PointsPerCredit => "points_per_credit",
            Self::CreditsPerPowerUnit => "credits_per_power_unit",
            Self::MaxRedeemPowerPerEpoch => "max_redeem_power_per_epoch",
            Self::MinRedeemPowerUnit => "min_redeem_power_unit",
            Self::EpochIndex => "epoch_index",
            Self::AvailablePowerUnits => "available_power_units",
            Self::RedeemedPowerUnits => "redeemed_power_units",
            Self::RequireMintsigV2 => "require_mintsig_v2",
            Self::AllowMintsigV1Fallback => "allow_mintsig_v1_fallback",
            Self::RequireRedeemSignature => "require_redeem_signature",
            Self::RequireRedeemSignerMatchNodeId => "require_redeem_signer_match_node_id",
        }
    }
    pub(super) fn codec(self) -> MemberPayloadCodecV1 {
        match self {
            Self::MaxRedeemPowerPerEpoch
            | Self::MinRedeemPowerUnit
            | Self::AvailablePowerUnits
            | Self::RedeemedPowerUnits => MemberPayloadCodecV1::I64,
            Self::RequireMintsigV2
            | Self::AllowMintsigV1Fallback
            | Self::RequireRedeemSignature
            | Self::RequireRedeemSignerMatchNodeId => MemberPayloadCodecV1::Bool,
            Self::TotalSupply
            | Self::CirculatingSupply
            | Self::TotalIssued
            | Self::TotalBurned
            | Self::PointsPerCredit
            | Self::CreditsPerPowerUnit
            | Self::EpochIndex => MemberPayloadCodecV1::U64,
        }
    }
}
