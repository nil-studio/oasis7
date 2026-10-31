//! Original single-use IDs; no settlement authority or full World qualification.
use crate::{
    incremental_field_member::*, incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1, incremental_projection::WORLD_STATE_FIELDS,
    state::WorldState,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldIdSetFieldV1 {
    CompletedLogisticsRoutes,
    SettledLogisticsTransits,
    RetiredFactories,
    SettledFactoryBuilds,
    SettledRecipeJobs,
}
#[derive(Debug, Clone, Copy)]
pub enum WorldIdSetKeyRefV1<'a> {
    Text(&'a str),
    Action(u64),
}
impl WorldIdSetFieldV1 {
    pub const ALL: [Self; 5] = [
        Self::CompletedLogisticsRoutes,
        Self::SettledLogisticsTransits,
        Self::RetiredFactories,
        Self::SettledFactoryBuilds,
        Self::SettledRecipeJobs,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::CompletedLogisticsRoutes => "completed_logistics_route_ids",
            Self::SettledLogisticsTransits => "settled_logistics_transit_ids",
            Self::RetiredFactories => "retired_factory_ids",
            Self::SettledFactoryBuilds => "settled_factory_build_ids",
            Self::SettledRecipeJobs => "settled_recipe_job_ids",
        }
    }
    fn text(self) -> bool {
        matches!(
            self,
            Self::CompletedLogisticsRoutes | Self::RetiredFactories
        )
    }
    fn check_key(self, key: WorldIdSetKeyRefV1<'_>) -> Result<(), ValueTreeError> {
        if self.text() != matches!(key, WorldIdSetKeyRefV1::Text(_)) {
            return Err(ValueTreeError::Invalid("world ID set key type"));
        }
        Ok(())
    }
    fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|field| {
                field.name == self.name()
                    && field.owner == FieldOwner::WorldState
                    && field.shape == FieldShape::Set
                    && field.destination == FieldDestination::State(StateNamespaceV1::Authorization)
            })
            .ok_or(ValueTreeError::Invalid("world ID set registration"))
    }
}
fn start<W, C: FnMut() -> bool>(
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if *records == 0 {
        return Err(ValueTreeError::ResourceLimited("world ID set records").into());
    }
    *records -= 1;
    Ok(())
}
fn entry<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    field: WorldIdSetFieldV1,
    key: WorldIdSetKeyRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    field.check_key(key)?;
    start(context, records)?;
    let field = field.descriptor()?;
    let record = match key {
        WorldIdSetKeyRefV1::Text(key) => write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            key,
            MemberPayloadRefV1::Unit,
            context,
        )?,
        WorldIdSetKeyRefV1::Action(key) => write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            &key,
            MemberPayloadRefV1::Unit,
            context,
        )?,
    };
    crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)
}
/// Caller admits its source and batch/index, discarding every prefix on error.
pub fn prepare_world_id_sets_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    let mut entries = 0u64;
    for field in WorldIdSetFieldV1::ALL {
        start(context, records)?;
        let record = write_field_metadata_v1(
            field.descriptor()?,
            &mut PresenceRecordWriteContext {
                sink: &mut *context.sink,
                memory: context.memory,
                io: &mut *context.io,
                key_work: &mut *context.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: context.limits.semantic_bytes,
                    record_bytes: context.limits.record_bytes,
                },
                cancelled: &mut context.cancelled,
            },
        )?;
        crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)?;
        macro_rules! members {
            ($values:expr, $kind:ident) => {
                for key in $values {
                    entry(
                        field,
                        WorldIdSetKeyRefV1::$kind(key),
                        context,
                        records,
                        sink,
                    )?;
                    entries = entries
                        .checked_add(1)
                        .ok_or(MemberRecordError::Invalid("world ID set count"))?;
                }
            };
        }
        match field {
            WorldIdSetFieldV1::CompletedLogisticsRoutes => members!(
                source
                    .completed_logistics_route_ids
                    .iter()
                    .map(String::as_str),
                Text
            ),
            WorldIdSetFieldV1::SettledLogisticsTransits => {
                members!(source.settled_logistics_transit_ids.iter().copied(), Action)
            }
            WorldIdSetFieldV1::RetiredFactories => {
                members!(source.retired_factory_ids.iter().map(String::as_str), Text)
            }
            WorldIdSetFieldV1::SettledFactoryBuilds => {
                members!(source.settled_factory_build_ids.iter().copied(), Action)
            }
            WorldIdSetFieldV1::SettledRecipeJobs => {
                members!(source.settled_recipe_job_ids.iter().copied(), Action)
            }
        }
    }
    Ok(entries)
}
pub fn world_id_set_contains_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldIdSetFieldV1,
    key: WorldIdSetKeyRefV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<bool, ValueTreeError> {
    context.check()?;
    field.check_key(key)?;
    let field = field.descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = context
        .lookup(&metadata)?
        .ok_or(ValueTreeError::Invalid("world ID set missing metadata"))?;
    context.records().presence(&root, &metadata, 0)?;
    let (key, bytes) = match key {
        WorldIdSetKeyRefV1::Text(key) => context.key(field, WorldFieldRecordKindV1::Entry, key)?,
        WorldIdSetKeyRefV1::Action(key) => {
            context.key(field, WorldFieldRecordKindV1::Entry, &key)?
        }
    };
    let Some(root) = context.lookup(&key)? else {
        return Ok(false);
    };
    let _: [u8; 0] = context
        .records()
        .fixed(&root, &key, bytes, MemberPayloadCodecV1::Unit)?;
    Ok(true)
}

#[path = "incremental_world_id_sets_overlay.rs"]
mod overlay;
pub use overlay::*;
