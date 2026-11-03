//! Borrowed, exhaustive industry job and terminal receipt traversal.
use crate::incremental_field_member::*;
use crate::incremental_world_field_key::WorldFieldRecordKindV1;
#[path = "incremental_industry_jobs_schema.rs"]
mod schema;
pub use schema::*;
#[path = "incremental_industry_jobs_visit.rs"]
mod visit;
use crate::{
    FactoryProductionFailureDispositionV1, LogisticsSettlementReceiptV1, MaterialTransitJobState,
    RecipeCompletionReceiptV1, RecipeJobState,
};
/// Explicit full-record scope only. Normal updates use column/member point APIs.
#[derive(Debug, Clone, Copy)]
pub enum IndustryRecordRefV1<'a> {
    Recipe(&'a RecipeJobState),
    Transit(&'a MaterialTransitJobState),
    Completion(&'a RecipeCompletionReceiptV1),
    Failure(&'a FactoryProductionFailureDispositionV1),
    Settlement(&'a LogisticsSettlementReceiptV1),
}
impl IndustryRecordRefV1<'_> {
    pub fn kind(self) -> IndustryRecordKindV1 {
        match self {
            Self::Recipe(_) => IndustryRecordKindV1::Recipe,
            Self::Transit(_) => IndustryRecordKindV1::Transit,
            Self::Completion(_) => IndustryRecordKindV1::Completion,
            Self::Failure(_) => IndustryRecordKindV1::Failure,
            Self::Settlement(_) => IndustryRecordKindV1::Settlement,
        }
    }
}
/// Exhaustively enumerate original borrowed fields. Each emission is provisional;
/// the caller owns continuous work/cancellation and retains no record-sized blob.
pub fn visit_industry_record_v1<'a, E: From<MemberRecordError>>(
    record: IndustryRecordRefV1<'a>,
    mut emit: impl FnMut(IndustryCellV1<'a>, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(IndustryCellV1::Record, MemberPayloadRefV1::Unit)?;
    match record {
        IndustryRecordRefV1::Recipe(v) => visit::recipe(v, &mut emit),
        IndustryRecordRefV1::Transit(v) => visit::transit(v, &mut emit),
        IndustryRecordRefV1::Completion(v) => visit::completion(v, &mut emit),
        IndustryRecordRefV1::Failure(v) => visit::failure(v, &mut emit),
        IndustryRecordRefV1::Settlement(v) => visit::settlement(v, &mut emit),
    }
}

#[path = "incremental_industry_jobs_genesis.rs"]
mod genesis;
pub use genesis::{IndustryWorkBudgetV1, prepare_industry_jobs_genesis_v1};
const RECORD_FORMAT_KEY: &str = "industry_record_closure_v1";
macro_rules! with_industry_cell_key {
    ($cell:expr,$key:ident => $body:expr) => {{
        match $cell {
            IndustryCellV1::Record => {
                let $key = &();
                $body
            }
            IndustryCellV1::Column(c) => {
                let (section, name) = c.path();
                let $key = &(((), section.name()), name);
                $body
            }
            IndustryCellV1::Collection(c) => {
                let $key = &((((), "record"), c.name()), ());
                $body
            }
            IndustryCellV1::TextItem(c, index) => {
                let $key = &((((), "record"), c.name()), index);
                $body
            }
            IndustryCellV1::StackItem(c, index, part) => {
                let part = match part {
                    IndustryStackColumnV1::Kind => "kind",
                    IndustryStackColumnV1::Amount => "amount",
                };
                let $key = &(((((), "record"), c.name()), index), part);
                $body
            }
            IndustryCellV1::Balance(c, member) => {
                let $key = &((((), "record"), c.name()), member);
                $body
            }
        }
    }};
}
fn valid_cell(
    key: IndustryRecordKeyV1,
    cell: IndustryCellV1<'_>,
) -> Result<(), oasis7_formal_protocol::incremental_value::ValueTreeError> {
    use IndustryCollectionTypeV1 as T;
    let valid = match cell {
        IndustryCellV1::Record => true,
        IndustryCellV1::Column(c) => c.accepts(key.kind),
        IndustryCellV1::Collection(c) => c.accepts(key.kind),
        IndustryCellV1::TextItem(c, _) => c.accepts(key.kind) && c.member_type() == T::Text,
        IndustryCellV1::StackItem(c, _, _) => c.accepts(key.kind) && c.member_type() == T::Stack,
        IndustryCellV1::Balance(c, _) => c.accepts(key.kind) && c.member_type() == T::Balance,
    };
    if valid {
        Ok(())
    } else {
        Err(
            oasis7_formal_protocol::incremental_value::ValueTreeError::Invalid(
                "industry cell type",
            ),
        )
    }
}
fn write_cell<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter, C: FnMut() -> bool>(
    key: IndustryRecordKeyV1,
    cell: IndustryCellV1<'_>,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    valid_cell(key, cell)?;
    overlay::validate_payload(cell, overlay::IndustryValueRefV1::from_payload(payload)?)?;
    let field = key.descriptor()?;
    with_industry_cell_key!(cell,encoded=>write_member_record_v1(field,WorldFieldRecordKindV1::Entry,encoded,payload,context))
}

#[cfg(test)]
#[path = "incremental_industry_jobs_tests.rs"]
mod tests;

#[path = "incremental_industry_jobs_read.rs"]
mod read;
pub use read::{IndustryCellReadV1, IndustryValueV1, read_industry_cell_v1};

use crate::incremental_world_field_key::WorldFieldKeyV1;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::ReservedValue;
#[path = "incremental_industry_jobs_closure.rs"]
mod closure;
#[path = "incremental_industry_jobs_family.rs"]
mod family;
#[path = "incremental_industry_jobs_key_read.rs"]
mod key_read;
#[path = "incremental_industry_jobs_tree.rs"]
mod tree;
pub use family::{
    IndustryFamilyQualificationV1, IndustryFamilyReadContextV1, qualify_industry_family_v1,
};

#[path = "incremental_industry_jobs_container.rs"]
mod container;
#[path = "incremental_industry_jobs_overlay.rs"]
mod overlay;
pub use overlay::{
    IndustryCellChangeV1, IndustryRecordCreateV1, IndustryValueRefV1,
    change_industry_cell_overlay_v1, create_industry_record_overlay_v1,
    read_industry_cell_overlay_v1,
};

pub use overlay::{
    IndustryRecordDeleteV1, delete_industry_record_overlay_v1, read_industry_record_root_overlay_v1,
};

#[path = "incremental_industry_jobs_advance.rs"]
mod advance;
#[path = "incremental_industry_jobs_diff.rs"]
mod diff;
#[path = "incremental_industry_jobs_receiver.rs"]
mod receiver;
pub use receiver::{
    IndustryPreparedTransitionV1, IndustryTransitionReadContextV1, verify_industry_transition_v1,
};
#[path = "incremental_industry_jobs_pages.rs"]
mod pages;
pub use pages::{
    IndustryFamilyPagesV1, mark_industry_family_pages_v1, transfer_industry_family_pages_v1,
};
