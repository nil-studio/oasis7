//! Lossless original logistics route/path records with explicit child roots.
//! These adapters do not grant transit, tariff, or recipe path authority.
use crate::incremental_world_field_key::WorldFieldKeyV1;
use crate::{
    LogisticsPathAuthorityV1, LogisticsRouteV1, incremental_field_member::*,
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::ReservedValue;
#[path = "incremental_logistics_records_schema.rs"]
mod schema;
pub use schema::*;
#[path = "incremental_logistics_records_visit.rs"]
mod visit;
#[derive(Debug, Clone, Copy)]
pub enum LogisticsRecordRefV1<'a> {
    Route(&'a LogisticsRouteV1),
    Path(&'a LogisticsPathAuthorityV1),
}
impl LogisticsRecordRefV1<'_> {
    pub fn kind(self) -> LogisticsRecordKindV1 {
        match self {
            Self::Route(_) => LogisticsRecordKindV1::Route,
            Self::Path(_) => LogisticsRecordKindV1::Path,
        }
    }
}
pub fn visit_logistics_record_v1<'a, E: From<MemberRecordError>>(
    record: LogisticsRecordRefV1<'a>,
    mut emit: impl FnMut(LogisticsCellV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(LogisticsCellV1::Record, MemberPayloadRefV1::Unit)?;
    match record {
        LogisticsRecordRefV1::Route(v) => visit::route(v, &mut emit),
        LogisticsRecordRefV1::Path(v) => visit::path(v, &mut emit),
    }
}
const RECORD_FORMAT_KEY: &str = "logistics_record_closure_v1";
macro_rules! with_logistics_cell_key {
    ($cell:expr,$key:ident => $body:expr) => {{
        match $cell {
            LogisticsCellV1::Record => {
                let $key = &();
                $body
            }
            LogisticsCellV1::Column(c) => {
                let $key = &(((), c.path().0), c.path().1);
                $body
            }
            LogisticsCellV1::Collection(c) => {
                let $key = &((((), "record"), c.name()), ());
                $body
            }
            LogisticsCellV1::TextItem(c, index) => {
                let $key = &((((), "record"), c.name()), index);
                $body
            }
        }
    }};
}
fn valid_cell(key: LogisticsRecordKeyV1<'_>, cell: LogisticsCellV1) -> Result<(), ValueTreeError> {
    let valid = match cell {
        LogisticsCellV1::Record => true,
        LogisticsCellV1::Column(c) => c.accepts(key.kind),
        LogisticsCellV1::Collection(c) | LogisticsCellV1::TextItem(c, _) => c.accepts(key.kind),
    };
    if valid {
        Ok(())
    } else {
        Err(ValueTreeError::Invalid("logistics cell type"))
    }
}
fn write_cell<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter, C: FnMut() -> bool>(
    key: LogisticsRecordKeyV1<'_>,
    cell: LogisticsCellV1,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    valid_cell(key, cell)?;
    overlay::validate_payload(cell, LogisticsValueRefV1::from_payload(payload)?)?;
    with_logistics_cell_key!(cell,encoded=>write_member_record_v1(key.descriptor()?,WorldFieldRecordKindV1::Entry,encoded,payload,context))
}
#[path = "incremental_logistics_records_genesis.rs"]
mod genesis;
pub use genesis::{LogisticsWorkBudgetV1, prepare_logistics_records_genesis_v1};

#[cfg(test)]
#[path = "incremental_logistics_records_tests.rs"]
mod tests;

#[path = "incremental_logistics_records_read.rs"]
mod read;
pub use read::{LogisticsCellReadV1, LogisticsValueV1, read_logistics_cell_v1};

#[path = "incremental_logistics_records_closure.rs"]
mod closure;
#[path = "incremental_logistics_records_diff.rs"]
mod diff;
#[path = "incremental_logistics_records_key_read.rs"]
mod key_read;
#[path = "incremental_logistics_records_tree.rs"]
mod tree;

#[path = "incremental_logistics_records_family.rs"]
mod family;
pub use family::{
    LogisticsFamilyQualificationV1, LogisticsFamilyReadContextV1, qualify_logistics_family_v1,
};

#[path = "incremental_logistics_records_advance.rs"]
mod advance;

#[path = "incremental_logistics_records_container.rs"]
mod container;
#[path = "incremental_logistics_records_overlay.rs"]
mod overlay;
pub use overlay::{
    LogisticsCellChangeV1, LogisticsRecordCreateV1, LogisticsRecordDeleteV1, LogisticsValueRefV1,
    change_logistics_cell_overlay_v1, create_logistics_record_overlay_v1,
    delete_logistics_record_overlay_v1, read_logistics_cell_overlay_v1,
    read_logistics_record_root_overlay_v1,
};

#[path = "incremental_logistics_records_receiver.rs"]
mod receiver;
pub use receiver::{
    LogisticsPreparedTransitionV1, LogisticsTransitionReadContextV1, verify_logistics_transition_v1,
};

#[path = "incremental_logistics_records_pages.rs"]
mod pages;
pub use pages::{
    LogisticsFamilyPagesV1, mark_logistics_family_pages_v1, transfer_logistics_family_pages_v1,
};
