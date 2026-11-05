//! Actual module effect authorization, allocator, queue and Event contributions.
//! This is consumed inside the shared output target after cap-slot/required-cap
//! and Pure-hook checks; it never replaces those checks or publishes a head.
pub use super::incremental_module_execution_event::ModuleOutputEventContextV1;
use super::{
    World, incremental_allocators::RuntimeAllocatorV1, incremental_body::BodyMutationError,
    incremental_module_execution_authorization::*,
    incremental_module_execution_event::append_module_output_event_v1,
    incremental_module_invocation::PreparedRootModuleInvocationV1,
};
use crate::{WorldError, WorldEvent, WorldEventBody};
use oasis7_formal_protocol::incremental_value::ValueReadContext;
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    effect::{EffectIntent, EffectOrigin},
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_runtime_effect_records::{
        RuntimeEffectFamilyQualificationV1, RuntimeEffectMembershipChangeV1,
        RuntimeEffectRecordsWorkV1, change_runtime_effect_membership_v1,
    },
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_runtime_module_authorization::{
        ModuleAuthorizationFamilyQualificationV1, ModuleAuthorizationReadWorkV1,
    },
    policy::PolicyDecisionRecord,
};
#[path = "incremental_module_execution_effect_heap.rs"]
pub(super) mod heap;

#[derive(Debug)]
pub enum RootModuleEffectErrorV1 {
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Body(BodyMutationError),
    Business(ReservedValue<WorldError>),
}
impl From<ValueTreeError> for RootModuleEffectErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<WorldFieldOverlayErrorV1> for RootModuleEffectErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
impl From<BodyMutationError> for RootModuleEffectErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<ExecutionError> for RootModuleEffectErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Body(e.into())
    }
}
pub struct RootModuleEffectWorkV1 {
    pub clone_nodes_remaining: u64,
    pub clone_bytes_remaining: u64,
    pub max_json_depth: usize,
    pub max_policy_rules: u64,
    pub limits: RootModuleAuthorizationLimitsV1,
}
pub struct RootModuleEffectRequestV1<'a> {
    pub parent: &'a ReservedValue<AcceptedRuntimeHistorySourceV1>,
    pub head: &'a VerifiedFormalCommitV1,
    pub selected: &'a PreparedRootModuleInvocationV1,
    pub kind: &'a str,
    pub params: &'a serde_json::Value,
    pub cap_ref: &'a str,
}
pub struct PreparedRootModuleEffectV1 {
    intent: EffectIntent,
    policy: PolicyDecisionRecord,
    guard: PreparedGuard,
    parent: [u8; 32],
    now: u64,
}
impl PreparedRootModuleEffectV1 {
    pub fn intent(&self) -> &EffectIntent {
        &self.intent
    }
    pub fn policy_record(&self) -> &PolicyDecisionRecord {
        &self.policy
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("root module effect binding")
}
fn with_overlay<M, R, E, C, T>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    action: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut C>,
    ) -> Result<T, RootModuleEffectErrorV1>,
) -> Result<T, RootModuleEffectErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let values = &mut ctx.values;
    let keys = &mut *ctx.key_work;
    let errors = &mut ctx.take_state_error;
    let cancelled = &mut ctx.cancelled;
    let semantic_bytes = ctx.semantic_bytes;
    let record_bytes = ctx.record_bytes;
    ctx.transaction.contribute(|txn| {
        let mut local = WorldFieldOverlayContextV1 {
            transaction: txn,
            values: ValueReadContext {
                source: values.source,
                memory: values.memory,
                class: values.class,
                io: &mut *values.io,
            },
            key_work: keys,
            semantic_bytes,
            record_bytes,
            take_state_error: errors,
            cancelled,
        };
        let result = action(&mut local);
        if let Some(error) = (local.take_state_error)() {
            return Err(error.into());
        }
        result
    })
}
fn clone_fee(
    request: &RootModuleEffectRequestV1<'_>,
    memory: &MemoryBudget,
    work: &mut RootModuleEffectWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<usize, ValueTreeError> {
    // Same closed JSON/ABI inventory and feature guards as real output decode.
    oasis7_world_state::incremental_world_event_decode::verify_module_output_layout_inventory_v1(
        memory,
        MemoryClass::Ordinary,
    )?;
    let mut json = heap::EffectCloneWorkV1 {
        nodes: work.clone_nodes_remaining,
        bytes: work.clone_bytes_remaining,
        max_depth: work.max_json_depth,
    };
    let measured = heap::json_clone_bound_v1(request.params, &mut json, 0, cancelled);
    work.clone_nodes_remaining = json.nodes;
    work.clone_bytes_remaining = json.bytes;
    let payload = measured?;
    let text = request
        .kind
        .len()
        .checked_add(request.cap_ref.len())
        .and_then(|n| n.checked_add(request.selected.module_id().len()))
        .ok_or_else(invalid)?;
    if request.kind.len() > work.limits.max_text_bytes
        || request.cap_ref.len() > work.limits.max_text_bytes
        || request.selected.module_id().len() > work.limits.max_text_bytes
    {
        return Err(ValueTreeError::ResourceLimited(
            "module effect identity bytes",
        ));
    }
    // Intent + Event clone coexist; policy record duplicates only identities.
    payload
        .checked_mul(4)
        .and_then(|n| text.checked_mul(8).and_then(|t| n.checked_add(t)))
        .and_then(|n| n.checked_add(16384 + 2 * std::mem::size_of::<WorldEvent>()))
        .ok_or(ValueTreeError::ResourceLimited("module effect clone heap"))
}
/// Allocate the original rolling intent identity, then perform the original
/// capability and first-match policy rules and append its exact policy Event.
/// Any rejection poisons this same transaction, preserving original step input.
#[expect(
    clippy::too_many_arguments,
    reason = "Actual parent/source/fence, separate owner IO and event persistence remain explicit."
)]
pub fn prepare_root_module_effect_v1<M, R, E, C, V, EW, EC, F>(
    request: RootModuleEffectRequestV1<'_>,
    now: u64,
    family: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    events: &mut ModuleOutputEventContextV1<'_, V, EW, EC, F>,
    read: &mut ModuleAuthorizationReadWorkV1,
    work: &mut RootModuleEffectWorkV1,
    live: &ExecutionFence,
) -> Result<ReservedValue<PreparedRootModuleEffectV1>, RootModuleEffectErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    V: ValueNodeReader,
    EW: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    EC: FnMut() -> bool,
    F: FnMut(
        &mut dyn super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
{
    with_overlay(ctx, |ctx| {
        let memory = ctx.values.memory;
        let anchor = request.parent.occurrence();
        if ctx.values.class != MemoryClass::Ordinary
            || !request.parent.uses_budget(memory)
            || request.parent.allocation_class() != MemoryClass::Ordinary
            || anchor.head_record_id() != request.head.verified_record_id()
            || anchor.head_body_id() != anchor.fact_body_id()
            || anchor.head_body_id() != ctx.transaction.parent_fence().parent_commit
            || anchor.roots_id() != ctx.transaction.parent_fence().parent_roots
            || request.selected.parent_roots_digest() != &anchor.roots_id()
            || !ctx.transaction.uses_budget(events.mutation.memory)
            || !memory.same_pool(events.mutation.memory)
        {
            return Err(invalid().into());
        }
        ctx.transaction
            .validate_transaction_identity(request.selected.guard())?;
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        super::incremental_clock::check_clock(now, ctx.transaction, &mut events.mutation)?;
        let fee = clone_fee(&request, memory, work, &mut || {
            (ctx.cancelled)() || (events.mutation.cancelled)()
        })?;
        let charge = memory
            .reserve(fee, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let seq = World::read_iws_allocator(
            RuntimeAllocatorV1::Intent,
            ctx.transaction,
            events.values,
            &mut events.mutation,
        )?;
        let (id, _) = World::contribute_iws_allocate(
            RuntimeAllocatorV1::Intent,
            seq,
            ctx.transaction,
            events.values,
            &mut events.mutation,
        )?;
        let intent = EffectIntent {
            birth_era: Some(seq.era),
            intent_id: format!("intent-{id}"),
            kind: request.kind.to_owned(),
            params: request.params.clone(),
            cap_ref: request.cap_ref.to_owned(),
            origin: EffectOrigin::Module {
                module_id: request.selected.module_id().to_owned(),
            },
        };
        let Some(grant) = read_root_module_capability_v1(
            &intent.cap_ref,
            family,
            ctx,
            read,
            request.selected.guard(),
            live,
            &work.limits,
        )?
        else {
            return Err(RootModuleEffectErrorV1::Business(charge.retain(
                WorldError::CapabilityMissing {
                    cap_ref: intent.cap_ref.clone(),
                },
            )));
        };
        if grant.is_expired(now) {
            return Err(RootModuleEffectErrorV1::Business(charge.retain(
                WorldError::CapabilityExpired {
                    cap_ref: intent.cap_ref.clone(),
                },
            )));
        }
        if !grant.allows(&intent.kind) {
            return Err(RootModuleEffectErrorV1::Business(charge.retain(
                WorldError::CapabilityNotAllowed {
                    cap_ref: intent.cap_ref.clone(),
                    kind: intent.kind.clone(),
                },
            )));
        }
        let policy_phase = memory
            .reserve(
                work.limits
                    .max_text_bytes
                    .checked_mul(8)
                    .and_then(|n| n.checked_add(1024))
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let decision = decide_root_module_effect_v1(
            &intent,
            family,
            ctx,
            read,
            request.selected.guard(),
            live,
            work.limits.max_text_bytes,
            work.max_policy_rules,
        )?;
        let policy = PolicyDecisionRecord::from_intent(&intent, (*decision).clone());
        append_module_output_event_v1(
            now,
            WorldEventBody::PolicyDecisionRecorded(policy.clone()),
            ctx.transaction,
            events,
        )?;
        if !decision.is_allowed() {
            return Err(RootModuleEffectErrorV1::Business(policy_phase.retain(
                WorldError::PolicyDenied {
                    intent_id: intent.intent_id.clone(),
                    reason: decision.reason().unwrap_or_else(|| "policy_deny".into()),
                },
            )));
        }
        if (ctx.cancelled)() || (events.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(charge.retain(PreparedRootModuleEffectV1 {
            intent,
            policy,
            guard: ctx.transaction.guard(),
            parent: anchor.roots_id(),
            now,
        }))
    })
}

/// Recheck current authorization, contribute the actual ordered pending owner,
/// then append the exact queued Event in the SAME transaction. Rechecking lets
/// ordinary unrelated reads/state writes occur between prepare and queue without
/// treating an old revision guard as permission after policy/capability changes.
#[expect(
    clippy::too_many_arguments,
    reason = "Real authorization/effect owners, finite writers and exact Event original sink stay explicit."
)]
pub fn contribute_prepared_root_module_effect_v1<M, R, E, C, V, EW, EC, F, W, WC>(
    prepared: &ReservedValue<PreparedRootModuleEffectV1>,
    authorization: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    effects: &ReservedValue<RuntimeEffectFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, WC>,
    events: &mut ModuleOutputEventContextV1<'_, V, EW, EC, F>,
    read: &mut ModuleAuthorizationReadWorkV1,
    work: &mut RootModuleEffectWorkV1,
    records: &mut RuntimeEffectRecordsWorkV1,
    live: &ExecutionFence,
) -> Result<(), RootModuleEffectErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    V: ValueNodeReader,
    EW: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    EC: FnMut() -> bool,
    F: FnMut(
        &mut dyn super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    W: StatePageWriter + ValueNodeReader,
    WC: FnMut() -> bool,
{
    with_overlay(ctx, |ctx| {
        let memory = ctx.values.memory;
        if !prepared.uses_budget(memory)
            || prepared.reserved_bytes() < std::mem::size_of::<PreparedRootModuleEffectV1>()
            || prepared.parent != ctx.transaction.parent_fence().parent_roots
            || ctx.values.class != MemoryClass::Ordinary
            || !memory.same_pool(events.mutation.memory)
            || !memory.same_pool(write.memory)
        {
            return Err(invalid().into());
        }
        ctx.transaction
            .validate_transaction_identity(&prepared.guard)?;
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        super::incremental_clock::check_clock(prepared.now, ctx.transaction, &mut events.mutation)?;
        let Some(grant) = read_root_module_capability_v1(
            &prepared.intent.cap_ref,
            authorization,
            ctx,
            read,
            &prepared.guard,
            live,
            &work.limits,
        )?
        else {
            return Err(invalid().into());
        };
        if grant.is_expired(prepared.now) || !grant.allows(&prepared.intent.kind) {
            return Err(invalid().into());
        }
        let decision = decide_root_module_effect_v1(
            &prepared.intent,
            authorization,
            ctx,
            read,
            &prepared.guard,
            live,
            work.limits.max_text_bytes,
            work.max_policy_rules,
        )?;
        if !decision.is_allowed() || *decision != prepared.policy.decision {
            return Err(invalid().into());
        }
        let point = ctx.transaction.guard();
        change_runtime_effect_membership_v1(
            RuntimeEffectMembershipChangeV1::AppendPending {
                intent: &prepared.intent,
            },
            effects,
            ctx,
            write,
            records,
            &point,
            live,
        )?;
        append_module_output_event_v1(
            prepared.now,
            WorldEventBody::EffectQueued(prepared.intent.clone()),
            ctx.transaction,
            events,
        )?;
        if (ctx.cancelled)() || (events.mutation.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        Ok(())
    })
}
