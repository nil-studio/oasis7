//! Original Runtime business counters and sparse signed threat entries.
use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::collections::BTreeMap;

pub const LOGISTICS_SLA_COLUMNS_V1: [&str; 8] = [
    "completed_transits",
    "fulfilled_transits",
    "breached_transits",
    "total_delay_ticks",
    "urgent_completed_transits",
    "urgent_fulfilled_transits",
    "urgent_breached_transits",
    "urgent_total_delay_ticks",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBusinessFieldV1 {
    LogisticsSlaMetrics,
    ThreatHeatmap,
}
impl RuntimeBusinessFieldV1 {
    pub const ALL: [Self; 2] = [Self::LogisticsSlaMetrics, Self::ThreatHeatmap];
    pub fn descriptor(self) -> FieldDescriptor {
        let (name, shape) = match self {
            Self::LogisticsSlaMetrics => ("logistics_sla_metrics", FieldShape::Value),
            Self::ThreatHeatmap => ("threat_heatmap", FieldShape::Map),
        };
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name,
            shape,
            destination: FieldDestination::State(StateNamespaceV1::Business),
        }
    }
    fn check(self, key: &str, value: RuntimeBusinessNumberV1) -> Result<(), ValueTreeError> {
        match (self, value) {
            (Self::LogisticsSlaMetrics, RuntimeBusinessNumberV1::Unsigned(_))
                if LOGISTICS_SLA_COLUMNS_V1.contains(&key) =>
            {
                Ok(())
            }
            (Self::ThreatHeatmap, RuntimeBusinessNumberV1::Signed(_)) => Ok(()),
            _ => Err(invalid()),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBusinessNumberV1 {
    Unsigned(u64),
    Signed(i64),
}
impl RuntimeBusinessNumberV1 {
    fn payload(self) -> MemberPayloadRefV1<'static> {
        match self {
            Self::Unsigned(v) => MemberPayloadRefV1::U64(v),
            Self::Signed(v) => MemberPayloadRefV1::I64(v),
        }
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime business field profile")
}
fn admit<W, C: FnMut() -> bool>(
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
) -> Result<(), MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    *records = records
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("Runtime business records"))?;
    Ok(())
}
fn emit_member<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    field: RuntimeBusinessFieldV1,
    key: &str,
    value: RuntimeBusinessNumberV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    field.check(key, value)?;
    admit(ctx, records)?;
    let row = write_member_record_v1(
        field.descriptor(),
        WorldFieldRecordKindV1::Entry,
        key,
        value.payload(),
        ctx,
    )?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)
}
/// Fixed counters are supplied in original declaration order; maps are borrowed.
pub fn prepare_runtime_business_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    metrics: &[u64; 8],
    heatmap: &BTreeMap<String, i64>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    for field in RuntimeBusinessFieldV1::ALL {
        admit(ctx, records)?;
        let row = write_field_metadata_v1(
            field.descriptor(),
            &mut PresenceRecordWriteContext {
                sink: &mut *ctx.sink,
                memory: ctx.memory,
                io: &mut *ctx.io,
                key_work: &mut *ctx.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: ctx.limits.semantic_bytes,
                    record_bytes: ctx.limits.record_bytes,
                },
                cancelled: &mut ctx.cancelled,
            },
        )?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
        match field {
            RuntimeBusinessFieldV1::LogisticsSlaMetrics => {
                for (key, value) in LOGISTICS_SLA_COLUMNS_V1.into_iter().zip(metrics) {
                    emit_member(
                        field,
                        key,
                        RuntimeBusinessNumberV1::Unsigned(*value),
                        ctx,
                        records,
                        sink,
                    )?;
                }
            }
            RuntimeBusinessFieldV1::ThreatHeatmap => {
                for (key, value) in heatmap {
                    emit_member(
                        field,
                        key,
                        RuntimeBusinessNumberV1::Signed(*value),
                        ctx,
                        records,
                        sink,
                    )?;
                }
            }
        }
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
pub fn read_runtime_business_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: RuntimeBusinessFieldV1,
    key: &str,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<RuntimeBusinessNumberV1>, ValueTreeError> {
    ctx.check()?;
    field.check(
        key,
        match field {
            RuntimeBusinessFieldV1::LogisticsSlaMetrics => RuntimeBusinessNumberV1::Unsigned(0),
            RuntimeBusinessFieldV1::ThreatHeatmap => RuntimeBusinessNumberV1::Signed(0),
        },
    )?;
    let metadata = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let id = ctx.lookup(&metadata)?.ok_or_else(invalid)?;
    ctx.records().presence(&id, &metadata, 0)?;
    let (member, len) = ctx.key(field.descriptor(), WorldFieldRecordKindV1::Entry, key)?;
    let Some(id) = ctx.lookup(&member)? else {
        return if field == RuntimeBusinessFieldV1::LogisticsSlaMetrics {
            Err(invalid())
        } else {
            Ok(None)
        };
    };
    let result = match field {
        RuntimeBusinessFieldV1::LogisticsSlaMetrics => {
            RuntimeBusinessNumberV1::Unsigned(ctx.records().unsigned(&id, &member, len)?)
        }
        RuntimeBusinessFieldV1::ThreatHeatmap => {
            RuntimeBusinessNumberV1::Signed(ctx.records().quantity(&id, &member, len)?)
        }
    };
    ctx.check()?;
    Ok(Some(result))
}

#[path = "incremental_runtime_business_overlay.rs"]
mod overlay;
pub use overlay::*;

#[path = "incremental_runtime_business_family.rs"]
mod family;
pub use family::*;

#[cfg(test)]
#[path = "incremental_runtime_business_tests.rs"]
mod tests;

#[path = "incremental_runtime_business_recovery.rs"]
mod recovery;
pub use recovery::*;
