//! Borrowed original intent/location records. Integrity is not permission authority.
pub use crate::incremental_intent_fields::IntentFieldV1;
use crate::{
    AgentIntentV2, AgentLocationAuthorityV1, LocationAnchorV1,
    incremental_field_member::*,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
#[path = "incremental_agent_authority_records_schema.rs"]
mod schema;
pub use schema::*;
#[derive(Debug, Clone, Copy)]
pub enum AgentAuthorityRecordRefV1<'a> {
    IntentLedger(&'a AgentIntentV2),
    AgentLocation(&'a AgentLocationAuthorityV1),
    LocationAnchor(&'a LocationAnchorV1),
}
impl AgentAuthorityRecordRefV1<'_> {
    pub fn kind(self) -> AgentAuthorityRecordKindV1 {
        match self {
            Self::IntentLedger(_) => AgentAuthorityRecordKindV1::IntentLedger,
            Self::AgentLocation(_) => AgentAuthorityRecordKindV1::AgentLocation,
            Self::LocationAnchor(_) => AgentAuthorityRecordKindV1::LocationAnchor,
        }
    }
}
#[path = "incremental_agent_authority_records_visit.rs"]
mod visit;
pub fn visit_agent_authority_record_v1<'a, E: From<MemberRecordError>>(
    record: AgentAuthorityRecordRefV1<'a>,
    mut emit: impl FnMut(AgentAuthorityCellV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(AgentAuthorityCellV1::Record, MemberPayloadRefV1::Unit)?;
    visit::record(record, &mut emit)
}
const RECORD_FORMAT_KEY: &str = "agent_authority_record_closure_v1";
macro_rules! with_agent_authority_cell_key {
    ($cell:expr,$key:ident=>$body:expr) => {{
        match $cell {
            AgentAuthorityCellV1::Record => {
                let $key = &();
                $body
            }
            AgentAuthorityCellV1::Column(c) => {
                let $key = &(((), c.path().0), c.path().1);
                $body
            }
        }
    }};
}
fn valid_cell(
    key: AgentAuthorityRecordKeyV1<'_>,
    cell: AgentAuthorityCellV1,
) -> Result<(), ValueTreeError> {
    if matches!(cell,AgentAuthorityCellV1::Column(c) if !c.accepts(key.kind)) {
        Err(ValueTreeError::Invalid("agent authority cell owner"))
    } else {
        Ok(())
    }
}
fn write_cell<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter, C: FnMut() -> bool>(
    key: AgentAuthorityRecordKeyV1<'_>,
    cell: AgentAuthorityCellV1,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    valid_cell(key, cell)?;
    let codec = match payload {
        MemberPayloadRefV1::Unit => MemberPayloadCodecV1::Unit,
        MemberPayloadRefV1::U64(v) if v <= cell.integer_max() => MemberPayloadCodecV1::U64,
        MemberPayloadRefV1::Text(_) => MemberPayloadCodecV1::Text,
        MemberPayloadRefV1::Bool(_) => MemberPayloadCodecV1::Bool,
        _ => return Err(MemberRecordError::Invalid("agent authority payload")),
    };
    let (expected, nullable) = cell.profile();
    if !(codec == expected || nullable && codec == MemberPayloadCodecV1::Unit) {
        return Err(MemberRecordError::Invalid(
            "agent authority payload profile",
        ));
    }
    with_agent_authority_cell_key!(cell,encoded=>write_member_record_v1(key.descriptor()?,WorldFieldRecordKindV1::Entry,encoded,payload,context))
}

#[path = "incremental_agent_authority_records_genesis.rs"]
mod genesis;
pub use genesis::{AgentAuthorityWorkBudgetV1, prepare_agent_authority_records_genesis_v1};
#[cfg(test)]
#[path = "incremental_agent_authority_records_tests.rs"]
mod tests;

#[path = "incremental_agent_authority_records_read.rs"]
mod read;
pub use read::{
    AgentAuthorityCellRequestV1, AgentAuthorityValueV1, inspect_agent_authority_record_root_v1,
    read_qualified_agent_authority_cell_v1,
};

#[path = "incremental_agent_authority_records_closure.rs"]
mod closure;
pub use closure::{AgentAuthorityRecordQualificationV1, qualify_agent_authority_body_v1};

#[path = "incremental_agent_authority_records_family.rs"]
mod family;
pub use family::{
    AgentAuthorityFamilyQualificationV1, open_qualified_agent_authority_record_v1,
    qualify_agent_authority_family_v1,
};

#[path = "incremental_agent_authority_records_point.rs"]
mod point;
pub use point::{
    AgentAuthorityCellChangeV1, AgentAuthorityValueRefV1, change_qualified_agent_authority_cell_v1,
    read_qualified_agent_authority_cell_overlay_v1,
};

#[path = "incremental_agent_authority_records_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::{
    AgentAuthorityRecordCreateV1, AgentAuthorityRecordDeleteV1,
    create_qualified_agent_authority_record_v1, delete_qualified_agent_authority_record_v1,
};
#[path = "incremental_agent_authority_records_receiver.rs"]
mod receiver;
pub use receiver::accept_agent_authority_transition_v1;
#[path = "incremental_agent_authority_records_pages.rs"]
mod pages;
pub use pages::{
    AgentAuthorityFamilyPagesV1, AgentAuthorityPagesWorkV1, mark_agent_authority_family_pages_v1,
    transfer_agent_authority_family_pages_v1,
};
