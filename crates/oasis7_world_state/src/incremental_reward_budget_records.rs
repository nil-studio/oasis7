//! Original epoch budgets and ordered mint records. Storage is not mint authority.
use crate::{
    NodeRewardMintRecord, SystemOrderPoolBudget,
    incremental_field_member::*,
    incremental_projection::{
        FieldDescriptor, FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
#[path = "incremental_reward_budget_records_schema.rs"]
mod schema;
pub use schema::*;
const FORMAT: &str = "reward_budget_record_closure_v1";
#[derive(Debug, Clone, Copy)]
pub enum RewardRecordRefV1<'a> {
    Pool(&'a SystemOrderPoolBudget),
    Mint(&'a NodeRewardMintRecord),
}
impl RewardRecordRefV1<'_> {
    pub fn kind(self) -> RewardRecordKindV1 {
        match self {
            Self::Pool(_) => RewardRecordKindV1::Pool,
            Self::Mint(_) => RewardRecordKindV1::Mint,
        }
    }
}
/// Complete borrowed original values; member IDs and Vec ordering are not normalized.
pub fn visit_reward_record_v1<'a, E: From<MemberRecordError>>(
    source: RewardRecordRefV1<'a>,
    mut emit: impl FnMut(RewardCellV1<'a>, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use MemberPayloadRefV1 as V;
    use RewardCellV1 as K;
    use RewardColumnV1 as C;
    emit(K::Record, V::Unit)?;
    match source {
        RewardRecordRefV1::Pool(SystemOrderPoolBudget {
            epoch_index,
            total_credit_budget,
            remaining_credit_budget,
            node_credit_caps,
            node_credit_allocated,
        }) => {
            for (c, v) in [
                (C::EpochIndex, *epoch_index),
                (C::TotalCreditBudget, *total_credit_budget),
                (C::RemainingCreditBudget, *remaining_credit_budget),
            ] {
                emit(K::Column(c), V::U64(v))?;
            }
            for (c, members) in [
                (RewardCollectionV1::NodeCreditCaps, node_credit_caps),
                (
                    RewardCollectionV1::NodeCreditAllocated,
                    node_credit_allocated,
                ),
            ] {
                emit(K::Collection(c), V::Unit)?;
                for (id, value) in members {
                    emit(K::Member(c, id), V::U64(*value))?;
                }
            }
        }
        RewardRecordRefV1::Mint(NodeRewardMintRecord {
            epoch_index,
            node_id,
            source_awarded_points,
            minted_power_credits,
            settlement_hash,
            signer_node_id,
            signature,
        }) => {
            for (c, v) in [
                (C::EpochIndex, *epoch_index),
                (C::SourceAwardedPoints, *source_awarded_points),
                (C::MintedPowerCredits, *minted_power_credits),
            ] {
                emit(K::Column(c), V::U64(v))?;
            }
            for (c, v) in [
                (C::NodeId, node_id),
                (C::SettlementHash, settlement_hash),
                (C::SignerNodeId, signer_node_id),
                (C::Signature, signature),
            ] {
                emit(K::Column(c), V::Text(v))?;
            }
        }
    }
    Ok(())
}
macro_rules! with_reward_cell_key {
    ($cell:expr,$key:ident=>$body:expr) => {{
        match $cell {
            RewardCellV1::Record => {
                let $key = &();
                $body
            }
            RewardCellV1::Column(c) => {
                let $key = &((), c.name());
                $body
            }
            RewardCellV1::Collection(c) => {
                let $key = &(((), c.name()), ());
                $body
            }
            RewardCellV1::Member(c, id) => {
                let $key = &(((), c.name()), id);
                $body
            }
        }
    }};
}
fn write_cell<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter, C: FnMut() -> bool>(
    key: RewardRecordKeyV1,
    cell: RewardCellV1<'_>,
    payload: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    if !cell.accepts(key.kind) {
        return Err(MemberRecordError::Invalid("reward cell owner"));
    }
    let codec = match payload {
        MemberPayloadRefV1::Unit => MemberPayloadCodecV1::Unit,
        MemberPayloadRefV1::U64(_) => MemberPayloadCodecV1::U64,
        MemberPayloadRefV1::Text(_) => MemberPayloadCodecV1::Text,
        _ => return Err(MemberRecordError::Invalid("reward cell codec")),
    };
    if codec != cell.codec() {
        return Err(MemberRecordError::Invalid("reward cell profile"));
    }
    with_reward_cell_key!(cell,encoded=>write_member_record_v1(key.kind.descriptor()?,WorldFieldRecordKindV1::Entry,encoded,payload,ctx))
}
#[path = "incremental_reward_budget_records_genesis.rs"]
mod genesis;
pub use genesis::{RewardRecordsWorkV1, prepare_reward_records_genesis_v1};

#[path = "incremental_reward_budget_records_key_read.rs"]
mod key_read;
#[path = "incremental_reward_budget_records_read.rs"]
mod read;
pub use read::{
    RewardCellRequestV1, RewardValueV1, inspect_reward_record_root_v1,
    read_qualified_reward_cell_v1,
};
#[path = "incremental_reward_budget_records_closure.rs"]
mod closure;
pub use closure::{RewardRecordQualificationV1, qualify_reward_body_v1};

#[path = "incremental_reward_budget_records_family.rs"]
mod family;
pub use family::{
    RewardFamilyQualificationV1, open_qualified_reward_record_v1, qualify_reward_family_v1,
};

#[path = "incremental_reward_budget_records_point.rs"]
mod point;
pub use point::{
    RewardCellChangeV1, RewardValueRefV1, change_qualified_reward_cell_v1,
    read_qualified_reward_cell_overlay_v1,
};

#[path = "incremental_reward_budget_records_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::{
    RewardMintAppendV1, RewardPoolCreateV1, RewardPoolDeleteV1, append_qualified_reward_mint_v1,
    create_qualified_reward_pool_v1, delete_qualified_reward_pool_v1,
};

#[path = "incremental_reward_budget_records_receiver.rs"]
mod receiver;
pub use receiver::accept_reward_transition_v1;

#[path = "incremental_reward_budget_records_pages.rs"]
mod pages;
pub use pages::{
    RewardFamilyPagesV1, RewardPagesWorkV1, mark_reward_family_pages_v1,
    transfer_reward_family_pages_v1,
};
