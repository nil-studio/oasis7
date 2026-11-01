//! Seven current-state Cognition cohorts. Each outer original field has an
//! explicit Unit RootRef into owner-selected JSON nodes, never a JSON blob.
use crate::{
    incremental_cognition_fields::CognitionEnvelopeRefV1,
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_json_records::{JsonRecordProfileV1, JsonWriteWorkV1, prepare_json_subtree_v1},
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_semantic_key::hash_semantic_key_v1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1 as Ns, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CognitionJsonFieldV1 {
    FeedbackOutbox,
    IdempotencyIndex,
    StagedActions,
    SchedulerState,
    Continuations,
    ContinuationContexts,
    ContinuationEvaluations,
}
impl CognitionJsonFieldV1 {
    pub const ALL: [Self; 7] = [
        Self::FeedbackOutbox,
        Self::IdempotencyIndex,
        Self::StagedActions,
        Self::SchedulerState,
        Self::Continuations,
        Self::ContinuationContexts,
        Self::ContinuationEvaluations,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::FeedbackOutbox => "feedback_outbox",
            Self::IdempotencyIndex => "idempotency_index",
            Self::StagedActions => "staged_actions",
            Self::SchedulerState => "scheduler_state",
            Self::Continuations => "continuations",
            Self::ContinuationContexts => "continuation_contexts",
            Self::ContinuationEvaluations => "continuation_evaluations",
        }
    }
    pub fn optional(self) -> bool {
        matches!(
            self,
            Self::ContinuationContexts | Self::ContinuationEvaluations
        )
    }
    pub fn descriptor(self) -> FieldDescriptor {
        let ns = match self {
            Self::IdempotencyIndex => Ns::Authorization,
            Self::SchedulerState | Self::ContinuationEvaluations => Ns::Scheduler,
            _ => Ns::Effects,
        };
        let shape = match self {
            Self::SchedulerState => FieldShape::Value,
            Self::Continuations => FieldShape::Ordered,
            _ => FieldShape::Map,
        };
        FieldDescriptor {
            owner: FieldOwner::Cognition,
            name: self.name(),
            destination: FieldDestination::State(ns),
            shape,
        }
    }
    /// Closed schema identifiers, one per cohort. They are version identifiers,
    /// not signatures or a claim about external cognition/business authority.
    pub fn profile(self) -> JsonRecordProfileV1 {
        let (name, schema) = match self {
            Self::FeedbackOutbox => ("feedback_outbox.json.v1", [0xc1; 32]),
            Self::IdempotencyIndex => ("idempotency_index.json.v1", [0xc2; 32]),
            Self::StagedActions => ("staged_actions.json.v1", [0xc3; 32]),
            Self::SchedulerState => ("scheduler_state.json.v1", [0xc4; 32]),
            Self::Continuations => ("continuations.json.v1", [0xc5; 32]),
            Self::ContinuationContexts => ("continuation_contexts.json.v1", [0xc6; 32]),
            Self::ContinuationEvaluations => ("continuation_evaluations.json.v1", [0xc7; 32]),
        };
        JsonRecordProfileV1 {
            field: FieldDescriptor {
                name,
                shape: FieldShape::Map,
                ..self.descriptor()
            },
            schema,
        }
    }
}
fn start<W, C: FnMut() -> bool>(
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    *records = records
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("cognition outer records"))?;
    Ok(())
}
/// Provisional writes only. All cohorts share one caller-owned work allowance;
/// absent optional fields emit neither metadata nor an empty substitute.
pub fn prepare_cognition_json_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: CognitionEnvelopeRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    work: &mut JsonWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    for field in CognitionJsonFieldV1::ALL {
        let Some(value) = source.field(field.name()) else {
            if field.optional() {
                continue;
            } else {
                return Err(MemberRecordError::Invalid("missing cognition JSON field"));
            }
        };
        start(context, records)?;
        let metadata = write_field_metadata_v1(
            field.descriptor(),
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
        crate::incremental_field_write::emit(
            metadata.field_key,
            metadata.value_root,
            context,
            sink,
        )?;
        let child = prepare_json_subtree_v1(field.profile(), value, context, work)?;
        start(context, records)?;
        let entry = write_root(field, child, context)?;
        crate::incremental_field_write::emit(entry.field_key, entry.value_root, context, sink)?;
    }
    Ok(())
}

pub(crate) fn write_root<W: ValueNodeWriter, C: FnMut() -> bool>(
    field: CognitionJsonFieldV1,
    child: oasis7_formal_protocol::incremental_state::StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    if field.descriptor().destination != FieldDestination::State(child.namespace) {
        return Err(MemberRecordError::Invalid("cognition JSON namespace"));
    }
    let _scratch = context
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = hash_semantic_key_v1(
        &(),
        context.memory,
        MemoryClass::Ordinary,
        context.key_work,
        &mut context.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(field.profile().schema, semantic.digest, child)
        .map_err(|_| MemberRecordError::Invalid("cognition JSON root reference"))?;
    write_member_record_v1(
        field.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::RecordRoot(&reference),
        context,
    )
}
