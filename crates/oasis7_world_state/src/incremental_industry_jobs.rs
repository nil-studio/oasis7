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
    readmit_industry_family_ordinary_v1,
};
#[path = "incremental_industry_jobs_current_identity.rs"]
mod current_identity;
pub use current_identity::{
    IndustryCurrentOuterChangeV1, IndustryOuterRecordRootV1, IndustryQualifiedOuterIdentityV1,
    visit_completion_current_outer_changes_v1, visit_qualified_completion_parent_identities_v1,
    visit_qualified_recipe_parent_identities_v1, visit_qualified_transit_parent_identities_v1,
    visit_recipe_current_outer_changes_v1, visit_transit_current_outer_changes_v1,
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

pub(crate) use receiver::accept_industry_verified_v1;

/// Validate only the original typed child closure/delta. This produces no
/// permission or family qualification; the joint owner authenticates its source.
pub(crate) fn validate_industry_lifecycle_child_delta_v1<
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    C: FnMut() -> bool,
>(
    before: Option<&oasis7_formal_protocol::incremental_state::StateRootV1>,
    after: &oasis7_formal_protocol::incremental_state::StateRootV1,
    context: &mut IndustryTransitionReadContextV1<'_, R, C>,
    work: &mut IndustryWorkBudgetV1,
) -> Result<(), crate::incremental_field_overlay::WorldFieldOverlayErrorV1> {
    let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    match before {
        Some(before) => advance::child(
            IndustryRecordKindV1::Recipe,
            before,
            after,
            &mut records,
            work,
            context.max_payload_bytes,
        ),
        None => closure::verify_child(
            IndustryRecordKindV1::Recipe,
            after,
            &mut records,
            context.max_payload_bytes,
            work,
        ),
    }
    .map_err(Into::into)
}

/// A paid decoded outer observation only. The joint lifecycle owner separately
/// binds its genuine parent/new family qualifications and verified write source.
pub(crate) fn decode_industry_lifecycle_outer_record_v1<
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    C: FnMut() -> bool,
>(
    value_root: &[u8; 32],
    key: &crate::incremental_world_field_key::WorldFieldKeyV1,
    context: &mut IndustryTransitionReadContextV1<'_, R, C>,
) -> Result<
    ReservedValue<IndustryQualifiedOuterIdentityV1>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
> {
    let fee = context
        .values
        .memory
        .reserve(
            std::mem::size_of::<IndustryQualifiedOuterIdentityV1>() + 64,
            context.values.class,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let (id, record_root) = current_identity::decode_recipe_root(value_root, key, &mut records)?;
    Ok(fee.retain(IndustryQualifiedOuterIdentityV1::Record {
        id,
        value_root: *value_root,
        record_root,
    }))
}
