//! Lossless factory and pending-build records. These storage adapters do not
//! authorize building, production, electricity consumption or World activation.
use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    state::{FactoryBuildJobState, FactoryState, WorldState},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
#[path = "incremental_factory_schema.rs"]
mod schema;
pub use schema::*;
#[path = "incremental_factory_closure.rs"]
mod closure;
#[path = "incremental_factory_diff.rs"]
mod diff;
#[path = "incremental_factory_family.rs"]
mod family;
pub use family::{
    FactoryFamilyQualificationV1, FactoryFamilyReadContextV1, qualify_factory_family_v1,
    readmit_factory_family_ordinary_v1,
};
#[path = "incremental_factory_current_identity.rs"]
mod current_identity;
pub use current_identity::{
    FactoryCurrentOuterChangeV1, FactoryOuterRecordRootV1, FactoryQualifiedOuterIdentityV1,
    visit_factory_current_outer_changes_v1, visit_qualified_factory_parent_identities_v1,
};
#[path = "incremental_factory_key_read.rs"]
mod key_read;
#[path = "incremental_factory_visit.rs"]
mod visit;
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FactoryRecordRefV1<'a> {
    Factory(&'a FactoryState),
    Build(&'a FactoryBuildJobState),
}
impl FactoryRecordRefV1<'_> {
    fn matches(self, key: FactoryRecordKeyV1<'_>) -> bool {
        matches!(
            (self, key),
            (Self::Factory(_), FactoryRecordKeyV1::Factory(_))
                | (Self::Build(_), FactoryRecordKeyV1::Build(_))
        )
    }
}
// A closed tuple grammar, shared by both registered namespaces. No path joins,
// arbitrary Serialize, key normalization or record-sized intermediary occurs.
macro_rules! with_factory_cell_key {
    ($record:expr, $cell:expr, $key:ident => $body:expr) => {{
        macro_rules! path {
            ($root:expr) => {
                match $cell {
                    FactoryCellV1::Record => {
                        let $key = &$root;
                        $body
                    }
                    FactoryCellV1::Column(c) => {
                        let (section, column) = c.path();
                        let $key = &(($root, section), column);
                        $body
                    }
                    FactoryCellV1::Collection(c) => {
                        let (section, collection) = c.path();
                        let $key = &((($root, section), collection), ());
                        $body
                    }
                    FactoryCellV1::TextItem(c, index) => {
                        let (section, collection) = c.path();
                        let $key = &((($root, section), collection), index);
                        $body
                    }
                    FactoryCellV1::StackItem(c, index, part) => {
                        let (section, collection) = c.path();
                        let part = match part {
                            FactoryStackColumnV1::Kind => "kind",
                            FactoryStackColumnV1::Amount => "amount",
                        };
                        let $key = &(((($root, section), collection), index), part);
                        $body
                    }
                    FactoryCellV1::Balance(c, member) => {
                        let (section, collection) = c.path();
                        let $key = &((($root, section), collection), member);
                        $body
                    }
                }
            };
        }
        let _ = $record;
        path!(())
    }};
}
macro_rules! with_factory_outer_key {
    ($record:expr, $key:ident => $body:expr) => {
        match $record {
            FactoryRecordKeyV1::Factory(value) => {
                let $key = &value;
                $body
            }
            FactoryRecordKeyV1::Build(value) => {
                let $key = &value;
                $body
            }
        }
    };
}
fn valid_cell(key: FactoryRecordKeyV1<'_>, cell: FactoryCellV1<'_>) -> Result<(), ValueTreeError> {
    use FactoryCollectionV1 as C;
    let valid = match cell {
        FactoryCellV1::Record => true,
        FactoryCellV1::Column(c) => key.accepts(c),
        FactoryCellV1::Collection(c) => c.accepts(key),
        FactoryCellV1::TextItem(c, _) => {
            c.accepts(key)
                && matches!(
                    c,
                    C::SpecTags
                        | C::SnapshotBottleneckTags
                        | C::SnapshotLogisticsRouteIds
                        | C::SnapshotLogisticsPathIds
                )
        }
        FactoryCellV1::StackItem(c, _, _) => {
            c.accepts(key)
                && matches!(
                    c,
                    C::SpecBuildCost
                        | C::SnapshotConsume
                        | C::SnapshotProduce
                        | C::SnapshotByproducts
                )
        }
        FactoryCellV1::Balance(c, _) => {
            c.accepts(key) && matches!(c, C::ObligationBalancesBefore | C::ObligationBalancesAfter)
        }
    };
    if valid {
        Ok(())
    } else {
        Err(ValueTreeError::Invalid("factory cell type"))
    }
}
fn write_cell<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: FactoryRecordKeyV1<'_>,
    cell: FactoryCellV1<'_>,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    valid_cell(key, cell)?;
    let field = key.descriptor()?;
    with_factory_cell_key!(key, cell, encoded => write_member_record_v1(field, WorldFieldRecordKindV1::Entry, encoded, payload, context))
}
fn spend(records: &mut u64) -> Result<(), MemberRecordError> {
    *records = records
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("factory records"))?;
    Ok(())
}
/// Explicitly enumerate a requested record without cloning it. Each borrowed
/// output is provisional until the visitor succeeds; source/output admission is
/// the caller's responsibility. Normal small updates use the point API.
pub fn visit_factory_record_v1<'a, E: From<MemberRecordError>>(
    key: FactoryRecordKeyV1<'a>,
    value: FactoryRecordRefV1<'a>,
    sink: &mut impl FnMut(FactoryCellV1<'a>, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    if !value.matches(key) {
        return Err(
            MemberRecordError::Value(ValueTreeError::Invalid("factory record type")).into(),
        );
    }
    visit::visit(value, sink)
}
/// Both required map metadata and every original nested field are streamed.
/// Empty/None markers and all vector positions are retained. Any failure requires
/// discarding the complete provisional genesis prefix, including emitted pages.
pub fn prepare_factory_records_genesis_v1<
    W: StatePageWriter + oasis7_formal_protocol::incremental_value::ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    work: &mut FactoryWorkBudgetV1,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    let mut count = 0u64;
    for key in [
        FactoryRecordKeyV1::Factory(""),
        FactoryRecordKeyV1::Build(0),
    ] {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        work.cell()?;
        spend(records)?;
        let metadata = container::metadata(key, context)?;
        crate::incremental_field_write::emit(
            metadata.field_key,
            metadata.value_root,
            context,
            sink,
        )?;
        work.cell()?;
        spend(records)?;
        let marker = write_member_record_v1(
            key.descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &(RECORD_FORMAT_KEY, ()),
            MemberPayloadRefV1::Unit,
            context,
        )?;
        crate::incremental_field_write::emit(marker.field_key, marker.value_root, context, sink)?;
    }
    let mut emit_record = |key, value| -> Result<(), MemberRecordError> {
        let child = container::genesis_body(key, value, context, records, work)?;
        work.cell()?;
        spend(records)?;
        let record = container::write(key, child, context)?;
        crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)?;
        count = count
            .checked_add(1)
            .ok_or(MemberRecordError::Invalid("factory record count"))?;
        Ok(())
    };
    for (key, value) in &source.factories {
        emit_record(
            FactoryRecordKeyV1::Factory(key),
            FactoryRecordRefV1::Factory(value),
        )?;
    }
    for (&key, value) in &source.pending_factory_builds {
        emit_record(
            FactoryRecordKeyV1::Build(key),
            FactoryRecordRefV1::Build(value),
        )?;
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}
#[path = "incremental_factory_tree.rs"]
mod tree;
pub use tree::FactoryWorkBudgetV1;
#[path = "incremental_factory_container.rs"]
mod container;
#[path = "incremental_factory_read.rs"]
mod read;
pub use read::{FactoryCellReadV1, FactoryValueV1, read_factory_cell_v1};

#[path = "incremental_factory_proof.rs"]
mod proof;
pub use proof::FactoryRecordProofV1;
#[path = "incremental_factory_overlay.rs"]
mod overlay;
pub use overlay::{
    FactoryCellChangeV1, FactoryRecordChangeV1, FactoryRecordCreateV1, FactoryValueRefV1,
    change_factory_cell_overlay_v1, create_factory_record_overlay_v1, read_factory_cell_overlay_v1,
    replace_factory_record_overlay_v1, verify_factory_record_overlay_v1,
};

#[path = "incremental_factory_advance.rs"]
mod advance;
#[path = "incremental_factory_receiver.rs"]
mod receiver;
pub use receiver::{
    FactoryPreparedTransitionV1, FactoryTransitionReadContextV1, verify_factory_transition_v1,
};
#[path = "incremental_factory_pages.rs"]
mod pages;
pub use pages::{
    FactoryFamilyPagesV1, mark_factory_family_pages_v1, transfer_factory_family_pages_v1,
};

#[cfg(test)]
#[path = "incremental_factory_industry_phase_tests.rs"]
mod phase_tests;

pub(crate) use receiver::accept_factory_verified_v1;

/// Validate only the original typed child closure/delta. This produces no
/// permission or family qualification; the joint owner authenticates its source.
pub(crate) fn validate_factory_lifecycle_child_delta_v1<
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    C: FnMut() -> bool,
>(
    before: Option<&oasis7_formal_protocol::incremental_state::StateRootV1>,
    after: &oasis7_formal_protocol::incremental_state::StateRootV1,
    context: &mut FactoryTransitionReadContextV1<'_, R, C>,
    work: &mut FactoryWorkBudgetV1,
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
            FactoryRecordKindV1::Factory,
            before,
            after,
            &mut records,
            work,
            context.max_payload_bytes,
        ),
        None => closure::verify_child(
            FactoryRecordKindV1::Factory,
            after,
            &mut records,
            context.max_payload_bytes,
            work,
        ),
    }
    .map_err(Into::into)
}

#[path = "incremental_factory_lifecycle_delete.rs"]
mod lifecycle_delete;
pub use lifecycle_delete::{
    PreparedCurrentFactoryDeleteV1, delete_prepared_current_factory_v1,
    prepare_current_factory_delete_v1,
};

/// A paid decoded outer observation only. The joint lifecycle owner separately
/// binds its genuine parent/new family qualifications and verified write source.
pub(crate) fn decode_factory_lifecycle_outer_record_v1<
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    C: FnMut() -> bool,
>(
    value_root: &[u8; 32],
    key: &crate::incremental_world_field_key::WorldFieldKeyV1,
    context: &mut FactoryTransitionReadContextV1<'_, R, C>,
) -> Result<
    ReservedValue<FactoryQualifiedOuterIdentityV1>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
> {
    let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let (id, record_root, fee) =
        current_identity::capture_factory_id(value_root, key, &mut records)?;
    Ok(fee.retain(FactoryQualifiedOuterIdentityV1::Record {
        id,
        value_root: *value_root,
        record_root,
    }))
}
