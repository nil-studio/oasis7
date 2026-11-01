//! Current-state Cognition cohorts, including optional economy/delegation. Each outer original field has an
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
    EconomyBalances,
    EconomyProvisionedBalances,
    EconomyLeaseBindingKeys,
    EconomyProvisions,
    EconomyProvisionReceipts,
    EconomyLeases,
    EconomyReceipts,
    EconomyIdempotency,
    EconomyOperations,
    DelegationGrants,
    DelegationSpent,
    DelegationDecisions,
    DelegationControls,
}
impl CognitionJsonFieldV1 {
    pub const ALL: [Self; 20] = [
        Self::FeedbackOutbox,
        Self::IdempotencyIndex,
        Self::StagedActions,
        Self::SchedulerState,
        Self::Continuations,
        Self::ContinuationContexts,
        Self::ContinuationEvaluations,
        Self::EconomyBalances,
        Self::EconomyProvisionedBalances,
        Self::EconomyLeaseBindingKeys,
        Self::EconomyProvisions,
        Self::EconomyProvisionReceipts,
        Self::EconomyLeases,
        Self::EconomyReceipts,
        Self::EconomyIdempotency,
        Self::EconomyOperations,
        Self::DelegationGrants,
        Self::DelegationSpent,
        Self::DelegationDecisions,
        Self::DelegationControls,
    ];
    fn spec(
        self,
    ) -> (
        &'static str,
        Option<&'static str>,
        &'static str,
        Ns,
        FieldShape,
        u8,
        &'static str,
    ) {
        match self {
            Self::FeedbackOutbox => (
                "feedback_outbox",
                None,
                "feedback_outbox",
                Ns::Effects,
                FieldShape::Map,
                0xc1,
                "feedback_outbox.json.v1",
            ),
            Self::IdempotencyIndex => (
                "idempotency_index",
                None,
                "idempotency_index",
                Ns::Authorization,
                FieldShape::Map,
                0xc2,
                "idempotency_index.json.v1",
            ),
            Self::StagedActions => (
                "staged_actions",
                None,
                "staged_actions",
                Ns::Effects,
                FieldShape::Map,
                0xc3,
                "staged_actions.json.v1",
            ),
            Self::SchedulerState => (
                "scheduler_state",
                None,
                "scheduler_state",
                Ns::Scheduler,
                FieldShape::Value,
                0xc4,
                "scheduler_state.json.v1",
            ),
            Self::Continuations => (
                "continuations",
                None,
                "continuations",
                Ns::Effects,
                FieldShape::Ordered,
                0xc5,
                "continuations.json.v1",
            ),
            Self::ContinuationContexts => (
                "continuation_contexts",
                None,
                "continuation_contexts",
                Ns::Effects,
                FieldShape::Map,
                0xc6,
                "continuation_contexts.json.v1",
            ),
            Self::ContinuationEvaluations => (
                "continuation_evaluations",
                None,
                "continuation_evaluations",
                Ns::Scheduler,
                FieldShape::Map,
                0xc7,
                "continuation_evaluations.json.v1",
            ),
            Self::EconomyBalances => (
                "economy.balances",
                Some("cognition_economy"),
                "balances",
                Ns::Business,
                FieldShape::NestedMap,
                0xd1,
                "economy.balances.json.v1",
            ),
            Self::EconomyProvisionedBalances => (
                "economy.provisioned_balances",
                Some("cognition_economy"),
                "provisioned_balances",
                Ns::Business,
                FieldShape::NestedMap,
                0xd2,
                "economy.provisioned_balances.json.v1",
            ),
            Self::EconomyLeaseBindingKeys => (
                "economy.lease_binding_keys",
                Some("cognition_economy"),
                "lease_binding_keys",
                Ns::Authorization,
                FieldShape::Map,
                0xd3,
                "economy.lease_binding_keys.json.v1",
            ),
            Self::EconomyProvisions => (
                "economy.provisions",
                Some("cognition_economy"),
                "provisions",
                Ns::Authorization,
                FieldShape::Map,
                0xd4,
                "economy.provisions.json.v1",
            ),
            Self::EconomyProvisionReceipts => (
                "economy.provision_receipts",
                Some("cognition_economy"),
                "provision_receipts",
                Ns::Authorization,
                FieldShape::Map,
                0xd5,
                "economy.provision_receipts.json.v1",
            ),
            Self::EconomyLeases => (
                "economy.leases",
                Some("cognition_economy"),
                "leases",
                Ns::Effects,
                FieldShape::Map,
                0xd6,
                "economy.leases.json.v1",
            ),
            Self::EconomyReceipts => (
                "economy.receipts",
                Some("cognition_economy"),
                "receipts",
                Ns::Authorization,
                FieldShape::Map,
                0xd7,
                "economy.receipts.json.v1",
            ),
            Self::EconomyIdempotency => (
                "economy.idempotency",
                Some("cognition_economy"),
                "idempotency",
                Ns::Authorization,
                FieldShape::Map,
                0xd8,
                "economy.idempotency.json.v1",
            ),
            Self::EconomyOperations => (
                "economy.operations",
                Some("cognition_economy"),
                "operations",
                Ns::Authorization,
                FieldShape::Map,
                0xd9,
                "economy.operations.json.v1",
            ),
            Self::DelegationGrants => (
                "delegation.grants",
                Some("agent_delegation"),
                "grants",
                Ns::Authorization,
                FieldShape::Map,
                0xe1,
                "delegation.grants.json.v1",
            ),
            Self::DelegationSpent => (
                "delegation.spent",
                Some("agent_delegation"),
                "spent",
                Ns::Authorization,
                FieldShape::Map,
                0xe2,
                "delegation.spent.json.v1",
            ),
            Self::DelegationDecisions => (
                "delegation.decisions",
                Some("agent_delegation"),
                "decisions",
                Ns::Authorization,
                FieldShape::Map,
                0xe3,
                "delegation.decisions.json.v1",
            ),
            Self::DelegationControls => (
                "delegation.controls",
                Some("agent_delegation"),
                "controls",
                Ns::Authorization,
                FieldShape::Map,
                0xe4,
                "delegation.controls.json.v1",
            ),
        }
    }
    pub fn name(self) -> &'static str {
        self.spec().0
    }
    pub fn optional(self) -> bool {
        !matches!(
            self,
            Self::FeedbackOutbox
                | Self::IdempotencyIndex
                | Self::StagedActions
                | Self::SchedulerState
                | Self::Continuations
        )
    }
    pub fn descriptor(self) -> FieldDescriptor {
        let (name, _, _, namespace, shape, _, _) = self.spec();
        FieldDescriptor {
            owner: FieldOwner::Cognition,
            name,
            destination: FieldDestination::State(namespace),
            shape,
        }
    }
    pub fn profile(self) -> JsonRecordProfileV1 {
        let (_, _, _, _, _, schema, name) = self.spec();
        JsonRecordProfileV1 {
            field: FieldDescriptor {
                name,
                shape: FieldShape::Map,
                ..self.descriptor()
            },
            schema: [schema; 32],
        }
    }
    pub fn value(self, source: CognitionEnvelopeRefV1<'_>) -> Option<&serde_json::Value> {
        let (_, container, key, _, _, _, _) = self.spec();
        if let Some(container) = container {
            source.field(container)?.get(key)
        } else {
            source.field(key)
        }
    }
    pub(crate) fn immediate(self) -> Option<crate::incremental_json_records::JsonImmediateShapeV1> {
        use crate::incremental_json_records::JsonImmediateShapeV1;
        match self {
            Self::EconomyBalances
            | Self::EconomyProvisionedBalances
            | Self::DelegationGrants
            | Self::DelegationDecisions
            | Self::DelegationControls => Some(JsonImmediateShapeV1::Object),
            Self::DelegationSpent => Some(JsonImmediateShapeV1::Unsigned),
            _ => None,
        }
    }
    pub(crate) fn scalar_constraint_depth(self) -> Option<usize> {
        if self == Self::DelegationSpent {
            Some(1)
        } else {
            None
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
        let Some(value) = field.value(source) else {
            if field.optional() {
                continue;
            } else {
                return Err(MemberRecordError::Invalid("missing cognition JSON field"));
            }
        };
        if let Some(shape) = field.immediate() {
            for child in value
                .as_object()
                .ok_or(MemberRecordError::Invalid("cognition JSON object source"))?
                .values()
            {
                if (context.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                work.cells_remaining =
                    work.cells_remaining
                        .checked_sub(1)
                        .ok_or(ValueTreeError::ResourceLimited(
                            "cognition JSON source shape checks",
                        ))?;
                let valid = match shape {
                    crate::incremental_json_records::JsonImmediateShapeV1::Object => {
                        child.is_object()
                    }
                    crate::incremental_json_records::JsonImmediateShapeV1::Unsigned => {
                        child.as_u64().is_some()
                    }
                };
                if !valid {
                    return Err(MemberRecordError::Invalid(
                        "cognition JSON source child shape",
                    ));
                }
            }
        }
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
