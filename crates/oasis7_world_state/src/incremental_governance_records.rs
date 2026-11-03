//! Original governance identity, proposal and ballot storage. No voting authority.
use crate::{
    GovernanceIdentityProfileState, GovernanceIdentityStatus, GovernanceProposalState,
    GovernanceProposalStatus, GovernanceVoteBallotState, GovernanceVoteState,
    GovernanceVoteWeightSnapshotState,
};
use crate::{incremental_field_member::*, incremental_projection::WORLD_STATE_FIELDS};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
#[path = "incremental_governance_records_schema.rs"]
mod schema;
pub use schema::*;
#[derive(Debug, Clone, Copy)]
pub enum GovernanceRecordRefV1<'a> {
    Identity(&'a GovernanceIdentityProfileState),
    Proposal(&'a GovernanceProposalState),
    Vote(&'a GovernanceVoteState),
}
impl GovernanceRecordRefV1<'_> {
    pub fn kind(self) -> GovernanceRecordKindV1 {
        match self {
            Self::Identity(_) => GovernanceRecordKindV1::Identity,
            Self::Proposal(_) => GovernanceRecordKindV1::Proposal,
            Self::Vote(_) => GovernanceRecordKindV1::Vote,
        }
    }
}
fn identity_status(value: GovernanceIdentityStatus) -> u64 {
    match value {
        GovernanceIdentityStatus::Active => 0,
        GovernanceIdentityStatus::Frozen => 1,
        GovernanceIdentityStatus::Expelled => 2,
    }
}
macro_rules! with_governance_cell_key {
    ($cell:expr,$key:ident=>$body:expr) => {{
        match $cell {
            GovernanceCellV1::Record => {
                let $key = &();
                $body
            }
            GovernanceCellV1::Column(c) => {
                let $key = &(((), "record"), c.name());
                $body
            }
            GovernanceCellV1::Count(c) => {
                let $key = &((((), "record"), c.name()), ());
                $body
            }
            GovernanceCellV1::Option(index) => {
                let $key = &((((), "record"), "options"), index);
                $body
            }
            GovernanceCellV1::Snapshot(id, c) => {
                let $key = &(((((), "record"), "vote_weight_snapshot"), id), c.name());
                $body
            }
            GovernanceCellV1::Ballot(id, c) => {
                let $key = &(((((), "record"), "votes_by_agent"), id), c.name());
                $body
            }
            GovernanceCellV1::Tally(id) => {
                let $key = &((((), "record"), "tallies"), id);
                $body
            }
        }
    }};
}
pub const RECORD_FORMAT_KEY: &str = "governance_record_closure_v1";
fn write_cell<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter, C: FnMut() -> bool>(
    key: GovernanceRecordKeyV1,
    cell: GovernanceCellV1,
    payload: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let codec = match payload {
        MemberPayloadRefV1::Unit
            if cell.codec() == MemberPayloadCodecV1::Unit || cell.nullable() =>
        {
            MemberPayloadCodecV1::Unit
        }
        MemberPayloadRefV1::U64(v) if v <= cell.integer_max() => MemberPayloadCodecV1::U64,
        MemberPayloadRefV1::I64(_) => MemberPayloadCodecV1::I64,
        MemberPayloadRefV1::Text(_) => MemberPayloadCodecV1::Text,
        _ => return Err(MemberRecordError::Invalid("governance payload")),
    };
    if !cell.accepts(key.kind)
        || (codec != cell.codec() && !(codec == MemberPayloadCodecV1::Unit && cell.nullable()))
    {
        return Err(MemberRecordError::Invalid("governance profile"));
    }
    with_governance_cell_key!(cell,k=>write_member_record_v1(key.descriptor()?,WorldFieldRecordKindV1::Entry,k,payload,ctx))
}
#[path = "incremental_governance_records_visit.rs"]
mod visit;
pub use visit::visit_governance_record_v1;
#[path = "incremental_governance_records_genesis.rs"]
mod genesis;
pub use genesis::{GovernanceWorkBudgetV1, prepare_governance_records_genesis_v1};

#[cfg(test)]
#[path = "incremental_governance_records_tests.rs"]
mod tests;

#[path = "incremental_governance_records_read.rs"]
mod read;
pub use read::{
    GovernanceCellRequestV1, GovernanceValueV1, inspect_governance_record_root_v1,
    read_qualified_governance_cell_v1,
};
#[path = "incremental_governance_records_closure.rs"]
mod closure;
#[path = "incremental_governance_records_key_read.rs"]
mod key_read;
pub use closure::{GovernanceRecordQualificationV1, qualify_governance_body_v1};

#[path = "incremental_governance_records_family.rs"]
mod family;
pub use family::{
    GovernanceFamilyQualificationV1, open_qualified_governance_record_v1,
    qualify_governance_family_v1,
};

#[path = "incremental_governance_records_point.rs"]
mod point;
pub use point::{
    GovernanceCellChangeV1, GovernanceValueRefV1, change_qualified_governance_cell_v1,
    read_qualified_governance_cell_overlay_v1,
};

#[path = "incremental_governance_records_write.rs"]
mod write;

#[path = "incremental_governance_records_members.rs"]
mod members;
pub use members::{
    GovernanceMemberChangeV1, GovernanceMemberEditV1, GovernanceMemberKeyV1,
    GovernanceMemberValueV1, change_qualified_governance_member_v1,
};

#[path = "incremental_governance_records_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::{
    GovernanceRecordCreateV1, GovernanceRecordDeleteV1, create_qualified_governance_record_v1,
    delete_qualified_governance_record_v1,
};

#[path = "incremental_governance_records_advance.rs"]
mod advance;
#[path = "incremental_governance_records_receiver.rs"]
mod receiver;
pub use receiver::accept_governance_transition_v1;

#[path = "incremental_governance_records_pages.rs"]
mod pages;
pub use pages::{
    GovernanceFamilyPagesV1, GovernancePagesWorkV1, mark_governance_family_pages_v1,
    transfer_governance_family_pages_v1,
};
