//! Field-level original site and construction-power storage, not admission authority.
use crate::{
    FactoryConstructionPowerMode, FactoryConstructionPowerProfileV1, FactorySiteAuthorityV1,
};
use crate::{incremental_field_member::*, incremental_projection::WORLD_STATE_FIELDS};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
#[path = "incremental_factory_authority_records_schema.rs"]
mod schema;
pub use schema::*;

#[derive(Debug, Clone, Copy)]
pub enum FactoryAuthorityRecordRefV1<'a> {
    Site(&'a FactorySiteAuthorityV1),
    Power(&'a FactoryConstructionPowerProfileV1),
}
impl FactoryAuthorityRecordRefV1<'_> {
    pub fn kind(self) -> FactoryAuthorityRecordKindV1 {
        match self {
            Self::Site(_) => FactoryAuthorityRecordKindV1::Site,
            Self::Power(_) => FactoryAuthorityRecordKindV1::Power,
        }
    }
}
/// Borrow every original field. Ordered allowlist elements retain empty strings
/// and duplicates: storage must not silently apply business normalization.
pub fn visit_factory_authority_record_v1<'a, E>(
    source: FactoryAuthorityRecordRefV1<'a>,
    mut emit: impl FnMut(FactoryAuthorityCellV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use FactoryAuthorityCellV1 as C;
    use MemberPayloadRefV1 as P;
    emit(C::Record, P::Unit)?;
    match source {
        FactoryAuthorityRecordRefV1::Site(value) => {
            let FactorySiteAuthorityV1 {
                site_id,
                location_id,
                owner_agent_id,
                authorized_agent_ids,
                chunk_ready,
                active,
                authority_revision,
                registered_at,
            } = value;
            emit(C::SiteId, P::Text(site_id))?;
            emit(C::LocationId, P::Text(location_id))?;
            emit(C::OwnerAgentId, P::Text(owner_agent_id))?;
            emit(
                C::AuthorizedCount,
                P::U64(authorized_agent_ids.len() as u64),
            )?;
            for (index, id) in authorized_agent_ids.iter().enumerate() {
                emit(C::AuthorizedAgent(index as u64), P::Text(id))?;
            }
            emit(C::ChunkReady, P::Bool(*chunk_ready))?;
            emit(C::Active, P::Bool(*active))?;
            emit(C::AuthorityRevision, P::U64(*authority_revision))?;
            emit(C::RegisteredAt, P::U64(*registered_at))?;
        }
        FactoryAuthorityRecordRefV1::Power(value) => {
            let FactoryConstructionPowerProfileV1 {
                factory_id,
                factory_kind,
                source_module_id,
                electricity_amount,
                mode,
                authority_revision,
                active,
            } = value;
            emit(C::FactoryId, P::Text(factory_id))?;
            emit(C::FactoryKind, P::Text(factory_kind))?;
            emit(C::SourceModulePresent, P::Bool(source_module_id.is_some()))?;
            if let Some(id) = source_module_id {
                emit(C::SourceModuleId, P::Text(id))?;
            }
            emit(C::ElectricityAmount, P::I64(*electricity_amount))?;
            emit(
                C::Mode,
                P::U64(match mode {
                    FactoryConstructionPowerMode::StartOnlySink => 0,
                }),
            )?;
            emit(C::AuthorityRevision, P::U64(*authority_revision))?;
            emit(C::Active, P::Bool(*active))?;
        }
    }
    Ok(())
}
macro_rules! with_cell_key {
    ($cell:expr, $key:ident => $body:expr) => {{
        match $cell {
            FactoryAuthorityCellV1::Record => {
                let $key = &();
                $body
            }
            FactoryAuthorityCellV1::AuthorizedAgent(index) => {
                let $key = &(((), "authorized_agent_ids"), index);
                $body
            }
            cell => {
                let $key = &((), cell.name());
                $body
            }
        }
    }};
}
fn write_cell<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter, C: FnMut() -> bool>(
    kind: FactoryAuthorityRecordKindV1,
    cell: FactoryAuthorityCellV1,
    payload: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let codec = match payload {
        MemberPayloadRefV1::Unit => MemberPayloadCodecV1::Unit,
        MemberPayloadRefV1::Text(_) => MemberPayloadCodecV1::Text,
        MemberPayloadRefV1::Bool(_) => MemberPayloadCodecV1::Bool,
        MemberPayloadRefV1::U64(v) if v <= cell.integer_max() => MemberPayloadCodecV1::U64,
        MemberPayloadRefV1::I64(_) => MemberPayloadCodecV1::I64,
        _ => return Err(MemberRecordError::Invalid("factory authority payload")),
    };
    if !cell.accepts(kind) || codec != cell.codec() {
        return Err(MemberRecordError::Invalid("factory authority profile"));
    }
    with_cell_key!(cell, key => write_member_record_v1(kind.descriptor()?, WorldFieldRecordKindV1::Entry, key, payload, ctx))
}
#[path = "incremental_factory_authority_records_genesis.rs"]
mod genesis;
pub use genesis::{FactoryAuthorityWorkV1, prepare_factory_authority_genesis_v1};
#[path = "incremental_factory_authority_records_read.rs"]
mod read;
pub use read::{
    FactoryAuthorityCellRequestV1, FactoryAuthorityValueV1, inspect_factory_authority_root_v1,
    read_qualified_factory_authority_cell_v1,
};
#[path = "incremental_factory_authority_records_closure.rs"]
mod closure;
pub use closure::{FactoryAuthorityQualificationV1, qualify_factory_authority_body_v1};
#[path = "incremental_factory_authority_records_point.rs"]
mod point;
pub use point::{
    FactoryAuthorityCellChangeV1, FactoryAuthorityCellValueRefV1,
    change_qualified_factory_authority_cell_v1,
};
#[path = "incremental_factory_authority_records_overlay_read.rs"]
mod overlay_read;
pub use overlay_read::read_qualified_factory_authority_cell_overlay_v1;
#[path = "incremental_factory_authority_records_family.rs"]
mod family;
pub use family::{
    FactoryAuthorityFamilyQualificationV1, open_qualified_factory_authority_record_v1,
    qualify_factory_authority_family_v1,
};
#[path = "incremental_factory_authority_records_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::{
    FactoryAuthorityRecordCreateV1, FactoryAuthorityRecordDeleteV1,
    create_qualified_factory_authority_record_v1, delete_qualified_factory_authority_record_v1,
};
#[path = "incremental_factory_authority_records_allowlist.rs"]
mod allowlist;
pub use allowlist::{
    FactoryAuthorityAllowlistChangeV1, FactoryAuthorityAllowlistEditV1,
    change_qualified_factory_allowlist_v1,
};
#[path = "incremental_factory_authority_records_advance.rs"]
mod advance;
#[path = "incremental_factory_authority_records_key_read.rs"]
mod key_read;
#[path = "incremental_factory_authority_records_receiver.rs"]
mod receiver;
pub use receiver::accept_factory_authority_transition_v1;
#[path = "incremental_factory_authority_records_pages.rs"]
mod pages;
pub use pages::{
    FactoryAuthorityFamilyPagesV1, FactoryAuthorityPagesWorkV1,
    mark_factory_authority_family_pages_v1, transfer_factory_authority_family_pages_v1,
};
#[cfg(test)]
#[path = "incremental_factory_authority_records_tests.rs"]
mod tests;
