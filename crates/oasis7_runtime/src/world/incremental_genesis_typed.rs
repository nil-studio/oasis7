//! Streaming typed Runtime assembly. Output is provisional and still needs
//! raw/delegated state, original History and complete receiver acceptance.
use super::World;
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::ExecutionWriteV1,
    incremental_state::{StateNamespaceV1 as Ns, StatePageWriter, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_fields::CognitionControlFieldV1,
    incremental_cognition_json::CognitionJsonFieldV1,
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_projection::{FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS},
};
pub use oasis7_world_state::{
    incremental_json_records::JsonWriteWorkV1,
    incremental_rollback_genesis::RollbackWorkV1,
    incremental_runtime_artifact_config::ArtifactConfigWriteWorkV1,
    incremental_runtime_artifact_content::RuntimeArtifactOriginalWorkV1,
    incremental_runtime_artifact_resources::RuntimeArtifactResourcesWorkV1,
    incremental_runtime_authorization_genesis::AuthorizationWriteWorkV1,
    incremental_runtime_capability_records::RuntimeCapabilityRecordsWorkV1,
    incremental_runtime_effect_records::RuntimeEffectRecordsWorkV1,
    incremental_runtime_governance::RuntimeGovernanceWorkV1,
    incremental_runtime_module_authorization::ModuleAuthorizationWriteWorkV1,
    incremental_runtime_proposal::RuntimeProposalWriteWorkV1,
    incremental_runtime_revocation::RevocationWriteWorkV1,
    incremental_runtime_scheduler_controls::RuntimeSchedulerControlsWorkV1,
    incremental_world_state_genesis::{WorldStateGenesisSummaryV1, WorldStateGenesisWorkV1},
};
/// No defaults are invented here. Each existing family consumes its supplied
/// work; the single ctx retains the factory's global IO/key/cancel allowances.
pub struct RuntimeTypedGenesisWorkV1 {
    pub output_rows_remaining: u64,
    pub records_remaining: u64,
    pub world: WorldStateGenesisWorkV1,
    pub artifact_config: ArtifactConfigWriteWorkV1,
    pub artifact_resources: RuntimeArtifactResourcesWorkV1,
    pub artifact_originals: RuntimeArtifactOriginalWorkV1,
    pub effects: RuntimeEffectRecordsWorkV1,
    pub capabilities: RuntimeCapabilityRecordsWorkV1,
    pub module_authorization: ModuleAuthorizationWriteWorkV1,
    pub authorization: AuthorizationWriteWorkV1,
    /// Shared by grants, audits/invocations and current Cognition; never reset.
    pub json: JsonWriteWorkV1,
    pub proposals: RuntimeProposalWriteWorkV1,
    pub revocation: RevocationWriteWorkV1,
    pub governance: RuntimeGovernanceWorkV1,
    pub rollback: RollbackWorkV1,
    pub scheduler_controls: RuntimeSchedulerControlsWorkV1,
    pub pending_actions:
        oasis7_world_state::incremental_runtime_pending_action_record::PendingActionWriteWorkV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeTypedGenesisSummaryV1 {
    pub world_state: WorldStateGenesisSummaryV1,
    pub runtime_fields: u64,
    pub cognition_fields: u64,
    pub writes: u64,
}
pub use oasis7_world_state::incremental_runtime_registry::RUNTIME_TYPED_GENESIS_FIELDS_V1;
pub const RUNTIME_TYPED_RAW_FIELDS_V1: &[&str] = &[
    "next_event_id",
    "next_event_id_era",
    "next_action_id",
    "next_action_id_era",
    "next_intent_id",
    "next_intent_id_era",
    "next_proposal_id",
    "next_proposal_id_era",
    "module_tick_schedule",
];
pub const RUNTIME_TYPED_HISTORY_FIELDS_V1: &[&str] = &[
    "snapshot_catalog",
    "journal",
    "tick_consensus_records",
    "formal_archived_tick_consensus_count",
    "tick_consensus_rejection_audit_events",
];
const CHILDREN: &[&str] = &[
    "cognition",
    "state",
    "module_registry",
    "policies",
    "runtime_memory_limits",
    "runtime_backpressure_stats",
];
const PROCESS: &[&str] = &[
    "bft_governance_parent",
    "devlocal_participant_authority",
    "formal_execution_tick",
    "formal_history_reader",
    "governance_replaying",
    "module_cache",
    "persistence_dir",
    "prepared_subscription_cache",
    "receipt_signer",
    "source_compiler",
];
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime typed GEN coverage")
}
fn inventory() -> Result<(), ValueTreeError> {
    if RUNTIME_TYPED_GENESIS_FIELDS_V1.len() > 64 {
        return Err(invalid());
    }
    for (index, field) in RUNTIME_TYPED_GENESIS_FIELDS_V1.iter().enumerate() {
        if RUNTIME_TYPED_GENESIS_FIELDS_V1[..index]
            .iter()
            .any(|f| f.name == field.name)
        {
            return Err(invalid());
        }
    }
    for field in World::iws_runtime_fields() {
        if let Some(expected) = RUNTIME_TYPED_GENESIS_FIELDS_V1
            .iter()
            .find(|f| f.name == field.name)
        {
            if expected != field {
                return Err(invalid());
            }
            continue;
        }
        let known = match field.destination {
            FieldDestination::State(ns) => {
                RUNTIME_TYPED_RAW_FIELDS_V1.contains(&field.name)
                    && ((ns == Ns::Allocator && field.shape == FieldShape::Value)
                        || (field.name == "module_tick_schedule"
                            && ns == Ns::Scheduler
                            && field.shape == FieldShape::ByteMap))
            }
            FieldDestination::Children => {
                CHILDREN.contains(&field.name) && field.shape == FieldShape::Children
            }
            FieldDestination::History => {
                RUNTIME_TYPED_HISTORY_FIELDS_V1.contains(&field.name)
                    && field.shape == FieldShape::History
            }
            FieldDestination::Process => {
                PROCESS.contains(&field.name) && field.shape == FieldShape::Process
            }
            _ => false,
        };
        if !known {
            return Err(invalid());
        }
    }
    Ok(())
}
struct Coverage {
    runtime: u64,
    cognition: u32,
    cognition_expected: u32,
    writes: u64,
}
impl Coverage {
    fn new(world: &World) -> Result<Self, ValueTreeError> {
        inventory()?;
        let mut expected = 0;
        if !world.cognition.is_null() {
            let source = world
                .borrow_iws_cognition_envelope_v1()
                .map_err(ValueTreeError::Invalid)?;
            for (index, field) in CognitionControlFieldV1::ALL.into_iter().enumerate() {
                if field.value(source).is_some() {
                    expected |= 1u32 << index
                }
            }
            for (index, field) in CognitionJsonFieldV1::ALL.into_iter().enumerate() {
                if field.value(source).is_some() {
                    expected |= 1u32 << (10 + index)
                }
            }
        }
        Ok(Self {
            runtime: 0,
            cognition: 0,
            cognition_expected: expected,
            writes: 0,
        })
    }
    fn row(&mut self, row: &ExecutionWriteV1) -> Result<(), ValueTreeError> {
        if row.mutation.before.is_some() || row.mutation.after.is_none() {
            return Err(invalid());
        }
        let parsed =
            WorldFieldKeyV1::from_bytes(&row.mutation.key).map_err(ValueTreeError::Invalid)?;
        let (index, expected) = match parsed.owner {
            FieldOwner::Runtime => {
                let (index, field) = RUNTIME_TYPED_GENESIS_FIELDS_V1
                    .iter()
                    .enumerate()
                    .find(|(_, f)| f.name == parsed.name)
                    .ok_or_else(invalid)?;
                (Some(index), *field)
            }
            FieldOwner::WorldState => {
                let field = WORLD_STATE_FIELDS
                    .iter()
                    .find(|f| f.name == parsed.name && f.owner == FieldOwner::WorldState)
                    .copied()
                    .ok_or_else(invalid)?;
                (None, field)
            }
            FieldOwner::Cognition => {
                let field = CognitionControlFieldV1::ALL
                    .into_iter()
                    .map(|f| f.descriptor())
                    .chain(
                        CognitionJsonFieldV1::ALL
                            .into_iter()
                            .map(|f| f.descriptor()),
                    )
                    .enumerate()
                    .find(|(_, f)| f.name == parsed.name)
                    .ok_or_else(invalid)?;
                if self.cognition_expected & (1u32 << field.0) == 0 {
                    return Err(invalid());
                }
                (Some(field.0), field.1)
            }
        };
        let key = WorldFieldKeyV1::new(expected, parsed.kind, parsed.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        if expected.destination != FieldDestination::State(row.namespace)
            || key.as_bytes() != row.mutation.key
        {
            return Err(invalid());
        }
        if parsed.kind == WorldFieldRecordKindV1::Metadata {
            if parsed.semantic_key.is_some() {
                return Err(invalid());
            }
            match (parsed.owner, index) {
                (FieldOwner::Runtime, Some(i)) => {
                    let bit = 1u64 << i;
                    if self.runtime & bit != 0 {
                        return Err(invalid());
                    }
                    self.runtime |= bit
                }
                (FieldOwner::Cognition, Some(i)) => {
                    let bit = 1u32 << i;
                    if self.cognition & bit != 0 {
                        return Err(invalid());
                    }
                    self.cognition |= bit
                }
                (FieldOwner::WorldState, None) => {}
                _ => return Err(invalid()),
            }
        }
        self.writes = self.writes.checked_add(1).ok_or_else(invalid)?;
        Ok(())
    }
    fn finish(&self) -> Result<(), ValueTreeError> {
        let len = RUNTIME_TYPED_GENESIS_FIELDS_V1.len();
        let full = if len == 64 {
            u64::MAX
        } else {
            (1u64 << len) - 1
        };
        if self.runtime != full || self.cognition != self.cognition_expected {
            return Err(invalid());
        }
        Ok(())
    }
}
fn emit_member<
    W,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    key: WorldFieldKeyV1,
    root: [u8; 32],
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let charge = ctx
        .memory
        .reserve(1024 + key.as_bytes().len(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let bytes = key.as_bytes().to_vec();
    if bytes.capacity() > charge.bytes() {
        return Err(invalid().into());
    }
    sink(
        charge.retain(ExecutionWriteV1 {
            namespace: WorldFieldKeyV1::from_bytes(key.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .namespace,
            mutation: StateWriteV1 {
                key: bytes,
                before: None,
                after: Some(root),
            },
        }),
    )?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
impl World {
    pub fn prepare_iws_runtime_typed_genesis_v1<
        W: StatePageWriter + ValueNodeReader,
        C: FnMut() -> bool,
        P: FnMut(OriginalArtifactRefV1, &[u8]) -> Result<(), ValueTreeError>,
        A: FnMut(
            u64,
            &oasis7_world_state::ActionEnvelope,
        ) -> Result<OriginalArtifactRefV1, ValueTreeError>,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RuntimeTypedGenesisWorkV1,
        persist_artifact: &mut P,
        persist_action: &mut A,
        sink: &mut S,
    ) -> Result<RuntimeTypedGenesisSummaryV1, MemberRecordError> {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let _phase = ctx
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut coverage = Coverage::new(self)?;
        let memory = ctx.memory;
        let remaining = &mut work.output_rows_remaining;
        let mut emit = |row: ReservedValue<ExecutionWriteV1>| {
            if !row.uses_budget(memory) {
                return Err(invalid());
            }
            *remaining = remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Runtime typed GEN output rows",
                ))?;
            coverage.row(&row)?;
            sink(row)
        };
        let world_state =
            oasis7_world_state::incremental_world_state_genesis::prepare_world_state_genesis_v1(
                &self.state,
                ctx,
                &mut work.world,
                &mut emit,
            )?;
        self.prepare_incremental_artifact_config_genesis_v1(
            ctx,
            &mut work.artifact_config,
            &mut emit,
        )?;
        self.prepare_incremental_artifact_resources_genesis_v1(
            ctx,
            &mut work.artifact_resources,
            &mut emit,
        )?;
        self.prepare_iws_runtime_artifact_content_genesis_v1(
            ctx,
            &mut work.records_remaining,
            &mut work.artifact_originals,
            persist_artifact,
            &mut emit,
        )?;
        self.prepare_incremental_runtime_controls_genesis_v1(
            ctx,
            &mut work.records_remaining,
            &mut emit,
        )?;
        self.prepare_iws_runtime_business_genesis_v1(ctx, &mut work.records_remaining, &mut emit)?;
        self.write_iws_scheduler_controls_genesis(ctx, &mut work.scheduler_controls, &mut emit)?;
        self.prepare_incremental_effect_records_genesis_v1(ctx, &mut work.effects, &mut emit)?;
        self.write_iws_capability_execution_genesis(ctx, &mut work.capabilities, &mut emit)?;
        oasis7_world_state::incremental_runtime_module_authorization::prepare_module_authorization_genesis_v1(&self.capabilities,&self.policies,ctx,&mut work.module_authorization,&mut emit)?;
        oasis7_world_state::incremental_runtime_authorization_grants::prepare_authorization_grants_genesis_v1(&self.capability_grants_v2,ctx,&mut work.json,&mut emit)?;
        oasis7_world_state::incremental_runtime_authorization_family_genesis::prepare_authorization_record_families_genesis_v1(&self.capability_authorization_receipts,&self.capability_invocation_contexts,ctx,&mut work.authorization,&mut work.json,&mut emit)?;
        let field=oasis7_world_state::incremental_runtime_authorization_root::authorization_root_descriptor_v1();
        for metadata in [true, false] {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            work.records_remaining =
                work.records_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "Runtime authorization root rows",
                    ))?;
            let row = if metadata {
                let row = write_field_metadata_v1(
                    field,
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
                (row.field_key, row.value_root)
            } else {
                let row=oasis7_world_state::incremental_runtime_authorization_root::prepare_authorization_root_value_v1(&self.capability_authorization_root,ctx)?;
                (row.field_key, row.value_root)
            };
            emit_member(row.0, row.1, ctx, &mut emit)?;
        }
        self.prepare_iws_runtime_revocation_genesis_v1(ctx, &mut work.revocation, &mut emit)?;
        oasis7_world_state::incremental_runtime_proposal::prepare_runtime_proposals_genesis_v1(
            &self.proposals,
            ctx,
            &mut work.proposals,
            &mut emit,
        )?;
        oasis7_world_state::incremental_runtime_governance::prepare_runtime_governance_genesis_v1(
            &self.borrow_iws_governance_controls_v1(),
            ctx,
            &mut work.governance,
            &mut emit,
        )?;
        oasis7_world_state::incremental_rollback_family_genesis::prepare_rollback_controls_genesis_v1(&self.borrow_iws_rollback_controls_v1(),ctx,&mut work.rollback,&mut emit)?;
        oasis7_world_state::incremental_runtime_pending_actions::prepare_runtime_pending_actions_genesis_v1(&self.pending_actions,ctx,&mut work.records_remaining,&mut work.pending_actions,persist_action,&mut emit)?;
        if !self.cognition.is_null() {
            self.prepare_iws_cognition_current_genesis_v1(
                ctx,
                &mut work.records_remaining,
                &mut work.json,
                &mut emit,
            )?;
        }
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        coverage.finish()?;
        Ok(RuntimeTypedGenesisSummaryV1 {
            world_state,
            runtime_fields: u64::from(coverage.runtime.count_ones()),
            cognition_fields: u64::from(coverage.cognition.count_ones()),
            writes: coverage.writes,
        })
    }
}
#[cfg(test)]
#[path = "incremental_genesis_typed_tests.rs"]
mod tests;

#[cfg(test)]
pub(super) fn test_work() -> RuntimeTypedGenesisWorkV1 {
    tests::work()
}
