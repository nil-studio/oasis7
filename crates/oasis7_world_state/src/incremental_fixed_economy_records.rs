//! Required closed economic globals; no business or commit authority.
use crate::{
    WorldState,
    incremental_field_member::*,
    incremental_projection::WORLD_STATE_FIELDS,
    main_token::MainTokenSupplyState,
    reward_asset::{ProtocolPowerReserve, RewardAssetConfig, RewardSignatureGovernancePolicy},
};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedEconomyFieldV1 {
    MainTokenSupply,
    RewardAssetConfig,
    ProtocolPowerReserve,
    RewardSignatureGovernancePolicy,
}
impl FixedEconomyFieldV1 {
    pub const ALL: [Self; 4] = [
        Self::MainTokenSupply,
        Self::RewardAssetConfig,
        Self::ProtocolPowerReserve,
        Self::RewardSignatureGovernancePolicy,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::MainTokenSupply => "main_token_supply",
            Self::RewardAssetConfig => "reward_asset_config",
            Self::ProtocolPowerReserve => "protocol_power_reserve",
            Self::RewardSignatureGovernancePolicy => "reward_signature_governance_policy",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::RewardSignatureGovernancePolicy => StateNamespaceV1::Authorization,
            Self::MainTokenSupply | Self::RewardAssetConfig | Self::ProtocolPowerReserve => {
                StateNamespaceV1::Business
            }
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(b"oasis7.iws.fixed-economy.v1\0");
        hash.update(self.name().as_bytes());
        *hash.finalize().as_bytes()
    }
    pub(super) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|field| {
                field.owner == FieldOwner::WorldState
                    && field.name == self.name()
                    && field.shape == FieldShape::Value
                    && field.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid("fixed economy registration"))
    }
}
#[derive(Clone, Copy)]
pub enum FixedEconomyRecordRefV1<'a> {
    Supply(&'a MainTokenSupplyState),
    AssetConfig(&'a RewardAssetConfig),
    PowerReserve(&'a ProtocolPowerReserve),
    SignaturePolicy(&'a RewardSignatureGovernancePolicy),
}
impl FixedEconomyRecordRefV1<'_> {
    pub fn field(self) -> FixedEconomyFieldV1 {
        match self {
            Self::Supply(_) => FixedEconomyFieldV1::MainTokenSupply,
            Self::AssetConfig(_) => FixedEconomyFieldV1::RewardAssetConfig,
            Self::PowerReserve(_) => FixedEconomyFieldV1::ProtocolPowerReserve,
            Self::SignaturePolicy(_) => FixedEconomyFieldV1::RewardSignatureGovernancePolicy,
        }
    }
}
#[path = "incremental_fixed_economy_records_schema.rs"]
mod schema;
pub use schema::FixedEconomyColumnV1;
/// Borrow all original fields exhaustively; no serializer or whole-DTO clone.
pub fn visit_fixed_economy_record_v1<'a, E>(
    source: FixedEconomyRecordRefV1<'a>,
    mut emit: impl FnMut(FixedEconomyColumnV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use FixedEconomyColumnV1 as C;
    use MemberPayloadRefV1 as P;
    match source {
        FixedEconomyRecordRefV1::Supply(value) => {
            let MainTokenSupplyState {
                total_supply,
                circulating_supply,
                total_issued,
                total_burned,
            } = value;
            for (cell, value) in [
                (C::TotalSupply, *total_supply),
                (C::CirculatingSupply, *circulating_supply),
                (C::TotalIssued, *total_issued),
                (C::TotalBurned, *total_burned),
            ] {
                emit(cell, P::U64(value))?;
            }
        }
        FixedEconomyRecordRefV1::AssetConfig(value) => {
            let RewardAssetConfig {
                points_per_credit,
                credits_per_power_unit,
                max_redeem_power_per_epoch,
                min_redeem_power_unit,
            } = value;
            emit(C::PointsPerCredit, P::U64(*points_per_credit))?;
            emit(C::CreditsPerPowerUnit, P::U64(*credits_per_power_unit))?;
            emit(
                C::MaxRedeemPowerPerEpoch,
                P::I64(*max_redeem_power_per_epoch),
            )?;
            emit(C::MinRedeemPowerUnit, P::I64(*min_redeem_power_unit))?;
        }
        FixedEconomyRecordRefV1::PowerReserve(value) => {
            let ProtocolPowerReserve {
                epoch_index,
                available_power_units,
                redeemed_power_units,
            } = value;
            emit(C::EpochIndex, P::U64(*epoch_index))?;
            emit(C::AvailablePowerUnits, P::I64(*available_power_units))?;
            emit(C::RedeemedPowerUnits, P::I64(*redeemed_power_units))?;
        }
        FixedEconomyRecordRefV1::SignaturePolicy(value) => {
            let RewardSignatureGovernancePolicy {
                require_mintsig_v2,
                allow_mintsig_v1_fallback,
                require_redeem_signature,
                require_redeem_signer_match_node_id,
            } = value;
            for (cell, value) in [
                (C::RequireMintsigV2, *require_mintsig_v2),
                (C::AllowMintsigV1Fallback, *allow_mintsig_v1_fallback),
                (C::RequireRedeemSignature, *require_redeem_signature),
                (
                    C::RequireRedeemSignerMatchNodeId,
                    *require_redeem_signer_match_node_id,
                ),
            ] {
                emit(cell, P::Bool(value))?;
            }
        }
    }
    Ok(())
}
macro_rules! with_cell_key {
    ($cell:expr,$key:ident=>$body:expr) => {
        match $cell {
            None => {
                let $key = &();
                $body
            }
            Some(cell) => {
                let $key = &((), cell.name());
                $body
            }
        }
    };
}
fn write_cell<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter, C: FnMut() -> bool>(
    field: FixedEconomyFieldV1,
    cell: Option<FixedEconomyColumnV1>,
    payload: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let valid = match (cell, payload) {
        (None, MemberPayloadRefV1::Unit) => true,
        (Some(cell), MemberPayloadRefV1::U64(_)) => {
            cell.field() == field && cell.codec() == MemberPayloadCodecV1::U64
        }
        (Some(cell), MemberPayloadRefV1::I64(_)) => {
            cell.field() == field && cell.codec() == MemberPayloadCodecV1::I64
        }
        (Some(cell), MemberPayloadRefV1::Bool(_)) => {
            cell.field() == field && cell.codec() == MemberPayloadCodecV1::Bool
        }
        _ => false,
    };
    if !valid {
        return Err(MemberRecordError::Invalid("fixed economy column/domain"));
    }
    with_cell_key!(cell,key=>write_member_record_v1(field.descriptor()?,WorldFieldRecordKindV1::Entry,key,payload,ctx))
}
#[path = "incremental_fixed_economy_records_genesis.rs"]
mod genesis;
pub use genesis::{FixedEconomyWorkV1, prepare_fixed_economy_genesis_v1};

#[path = "incremental_fixed_economy_records_read.rs"]
mod read;
pub use read::{FixedEconomyValueV1, inspect_fixed_economy_root_v1};
#[path = "incremental_fixed_economy_records_closure.rs"]
mod closure;
pub use closure::{FixedEconomyRecordQualificationV1, qualify_fixed_economy_body_v1};

#[path = "incremental_fixed_economy_records_family.rs"]
mod family;
pub use family::{
    FixedEconomyFamilyQualificationV1, open_qualified_fixed_economy_record_v1,
    qualify_fixed_economy_family_v1,
};
pub use read::{FixedEconomyCellRequestV1, read_qualified_fixed_economy_cell_v1};

#[path = "incremental_fixed_economy_records_overlay.rs"]
mod overlay;
pub use overlay::read_qualified_fixed_economy_overlay_v1;
#[path = "incremental_fixed_economy_records_point.rs"]
mod point;
pub use point::{FixedEconomyCellChangeV1, change_qualified_fixed_economy_cell_v1};

#[path = "incremental_fixed_economy_records_receiver.rs"]
mod receiver;
pub use receiver::accept_fixed_economy_transition_v1;

#[path = "incremental_fixed_economy_records_pages.rs"]
mod pages;
pub use pages::{
    FixedEconomyFamilyPagesV1, FixedEconomyPagesWorkV1, mark_fixed_economy_family_pages_v1,
    transfer_fixed_economy_family_pages_v1,
};
