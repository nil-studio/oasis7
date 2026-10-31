//! Closed original material-profile and immediate-transfer records.
//! These are storage records, not business or commit authority.
use crate::MaterialTransferReceiptV1;
use crate::incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1};
use crate::{incremental_field_member::*, incremental_projection::WORLD_STATE_FIELDS};
use oasis7_agent_api::world_action::MaterialTransitPriority;
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
use oasis7_wasm_abi::{MaterialDefaultPriority, MaterialProfileV1, MaterialTransportLossClass};
#[path = "incremental_material_records_schema.rs"]
mod schema;
pub use schema::*;

#[derive(Debug, Clone, Copy)]
pub enum MaterialRecordRefV1<'a> {
    Profile(&'a MaterialProfileV1),
    Transfer(&'a MaterialTransferReceiptV1),
}
impl MaterialRecordRefV1<'_> {
    pub fn kind(self) -> MaterialRecordKindV1 {
        match self {
            Self::Profile(_) => MaterialRecordKindV1::Profile,
            Self::Transfer(_) => MaterialRecordKindV1::Transfer,
        }
    }
}
/// Exhaustive borrowed projection. Emissions are provisional until the caller
/// completes its whole operation, including the final cancellation check.
pub fn visit_material_record_v1<'a, E>(
    value: MaterialRecordRefV1<'a>,
    mut emit: impl FnMut(MaterialCellV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use MaterialCellV1 as C;
    use MemberPayloadRefV1 as P;
    emit(C::Record, P::Unit)?;
    match value {
        MaterialRecordRefV1::Profile(v) => {
            let MaterialProfileV1 {
                kind,
                tier,
                category,
                stack_limit,
                transport_loss_class,
                decay_bps_per_tick,
                default_priority,
            } = v;
            emit(C::Kind, P::Text(kind))?;
            emit(C::Tier, P::U64(u64::from(*tier)))?;
            emit(C::Category, P::Text(category))?;
            emit(C::StackLimit, P::I64(*stack_limit))?;
            emit(
                C::TransportLossClass,
                P::U64(match transport_loss_class {
                    MaterialTransportLossClass::Low => 0,
                    MaterialTransportLossClass::Medium => 1,
                    MaterialTransportLossClass::High => 2,
                }),
            )?;
            emit(C::DecayBpsPerTick, P::I64(*decay_bps_per_tick))?;
            emit(
                C::DefaultPriority,
                P::U64(match default_priority {
                    MaterialDefaultPriority::Urgent => 0,
                    MaterialDefaultPriority::Standard => 1,
                }),
            )?;
        }
        MaterialRecordRefV1::Transfer(v) => {
            let MaterialTransferReceiptV1 {
                transfer_id,
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                amount,
                distance_km,
                priority,
                route_id,
            } = v;
            emit(C::TransferId, P::U64(*transfer_id))?;
            emit(C::RequesterAgentId, P::Text(requester_agent_id))?;
            emit(C::FromLedger, P::Ledger(from_ledger))?;
            emit(C::ToLedger, P::Ledger(to_ledger))?;
            emit(C::Kind, P::Text(kind))?;
            emit(C::Amount, P::I64(*amount))?;
            emit(C::DistanceKm, P::I64(*distance_km))?;
            emit(
                C::Priority,
                P::U64(match priority {
                    MaterialTransitPriority::Urgent => 0,
                    MaterialTransitPriority::Standard => 1,
                }),
            )?;
            emit(C::RoutePresent, P::Bool(route_id.is_some()))?;
            if let Some(route) = route_id {
                emit(C::RouteId, P::Text(route))?;
            }
        }
    }
    Ok(())
}

const RECORD_FORMAT_KEY: &str = "material_record_closure_v1";
macro_rules! with_material_outer_key {
    ($key:expr, $encoded:ident => $body:expr) => {{
        match $key {
            MaterialRecordKeyV1::Profile(id) => {
                let $encoded = &id;
                $body
            }
            MaterialRecordKeyV1::Transfer(id) => {
                let $encoded = &id;
                $body
            }
        }
    }};
}
fn write_cell<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter, C: FnMut() -> bool>(
    key: MaterialRecordKeyV1<'_>,
    cell: MaterialCellV1,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let codec = match payload {
        MemberPayloadRefV1::Unit => MemberPayloadCodecV1::Unit,
        MemberPayloadRefV1::U64(v) if v <= cell.integer_max() => MemberPayloadCodecV1::U64,
        MemberPayloadRefV1::I64(_) => MemberPayloadCodecV1::I64,
        MemberPayloadRefV1::Bool(_) => MemberPayloadCodecV1::Bool,
        MemberPayloadRefV1::Text(_) => MemberPayloadCodecV1::Text,
        MemberPayloadRefV1::Ledger(_) => MemberPayloadCodecV1::Ledger,
        _ => return Err(MemberRecordError::Invalid("material cell payload")),
    };
    if !cell.accepts(key.kind()) || codec != cell.codec() {
        return Err(MemberRecordError::Invalid("material cell profile"));
    }
    let field = key.kind().descriptor()?;
    if cell == MaterialCellV1::Record {
        write_member_record_v1(field, WorldFieldRecordKindV1::Entry, &(), payload, context)
    } else {
        write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            &((), cell.name()),
            payload,
            context,
        )
    }
}
#[path = "incremental_material_records_genesis.rs"]
mod genesis;
pub use genesis::{MaterialWorkBudgetV1, prepare_material_records_genesis_v1};

#[cfg(test)]
#[path = "incremental_material_records_tests.rs"]
mod tests;

#[path = "incremental_material_records_closure.rs"]
mod closure;
pub use closure::{MaterialRecordQualificationV1, qualify_material_body_v1};
#[path = "incremental_material_records_read.rs"]
mod read;
pub use read::{
    MaterialCellRequestV1, MaterialValueV1, inspect_material_record_root_v1,
    read_qualified_material_cell_v1,
};

#[path = "incremental_material_records_family.rs"]
mod family;
pub use family::{MaterialFamilyQualificationV1, qualify_material_family_v1};
