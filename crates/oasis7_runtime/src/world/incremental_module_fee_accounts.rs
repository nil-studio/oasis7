//! Selected owner and fee reads from one authenticated ordinary overlay.
use super::incremental_module_fee::{self as core, ModuleRuntimeFeeAmountsV1};
use crate::{ModuleManifest, ModuleRuntimeChargeEvent};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_wasm_abi::{ModuleCallErrorCode, ModuleCallFailure, ModuleOutput};
use oasis7_world_state::{
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1,
        AgentSubtreeStructureQualificationV1, AgentSubtreeValueV1,
        read_agent_subtree_scalar_overlay_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_semantic_key::{SemanticKeyError, hash_semantic_key_v1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    incremental_world_text_maps::{WorldTextMapFieldV1, read_world_text_map_overlay_v1},
};

#[derive(Debug)]
pub enum RootModuleFeeErrorV1 {
    Overlay(WorldFieldOverlayErrorV1),
    Semantic(SemanticKeyError),
    Body(super::incremental_body::BodyMutationError),
    World(crate::WorldError),
    Inventory(
        oasis7_world_state::incremental_inventory_overlay_change::InventoryOverlayChangeError,
    ),
}
impl From<ExecutionError> for RootModuleFeeErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Overlay(e.into())
    }
}
impl From<ValueTreeError> for RootModuleFeeErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Overlay(e.into())
    }
}
impl From<WorldFieldOverlayErrorV1> for RootModuleFeeErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
impl From<SemanticKeyError> for RootModuleFeeErrorV1 {
    fn from(e: SemanticKeyError) -> Self {
        Self::Semantic(e)
    }
}
impl From<oasis7_proto::memory_budget::MemoryLimitExceeded> for RootModuleFeeErrorV1 {
    fn from(e: oasis7_proto::memory_budget::MemoryLimitExceeded) -> Self {
        ValueTreeError::Memory(e).into()
    }
}
impl From<super::incremental_body::BodyMutationError> for RootModuleFeeErrorV1 {
    fn from(e: super::incremental_body::BodyMutationError) -> Self {
        Self::Body(e)
    }
}
fn invalid() -> RootModuleFeeErrorV1 {
    ValueTreeError::Invalid("module fee binding/type").into()
}

pub struct RootModuleFeeRequestV1<'a> {
    pub parent_roots: &'a RootSetV1,
    pub agents: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub guard: &'a PreparedGuard,
    pub live: &'a ExecutionFence,
    pub module_id: &'a str,
    pub trace_id: &'a str,
    pub manifest: &'a ModuleManifest,
    pub input_bytes: u64,
    pub max_owner_bytes: usize,
}

pub struct PreparedRootModuleFeeV1 {
    pub(super) event: ModuleRuntimeChargeEvent,
    pub(super) guard: PreparedGuard,
    pub(super) live: ExecutionFence,
    pub(super) owner_key: WorldFieldKeyV1,
    pub(super) owner_value: [u8; 32],
    pub(super) balances: [Option<i64>; 2],
}
impl PreparedRootModuleFeeV1 {
    pub fn event(&self) -> &ModuleRuntimeChargeEvent {
        &self.event
    }
}
pub enum RootModuleFeeOutcomeV1 {
    Free,
    Denied(ReservedValue<ModuleCallFailure>),
    Prepared(Box<ReservedValue<PreparedRootModuleFeeV1>>),
}

pub(super) fn with_overlay<M, R, E, C, T>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    action: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut C>,
    ) -> Result<T, RootModuleFeeErrorV1>,
) -> Result<T, RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let values = &mut ctx.values;
    let keys = &mut *ctx.key_work;
    let semantic_bytes = ctx.semantic_bytes;
    let record_bytes = ctx.record_bytes;
    let errors = &mut ctx.take_state_error;
    let cancelled = &mut ctx.cancelled;
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
            Err(error.into())
        } else {
            result
        }
    })
}
pub(super) fn finish<M, R, E, C>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<(), RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
fn check<M, R, E, C>(
    request: &RootModuleFeeRequestV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<(), RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction
        .validate_guard(request.guard, request.live)?;
    if ctx.values.class != MemoryClass::Ordinary
        || !ctx.transaction.uses_budget(ctx.values.memory)
        || !request.agents.uses_budget(ctx.values.memory)
        || request.agents.reserved_bytes() < 512
        || request.agents.allocation_class() != MemoryClass::Ordinary
        || request.parent_roots.digest()? != ctx.transaction.parent_fence().parent_roots
    {
        return Err(invalid());
    }
    request.agents.validate_binding(request.parent_roots)?;
    finish(ctx)
}
pub(super) fn balance<M, R, E, C>(
    agent: &str,
    field: AgentFieldV1,
    ordinal: u64,
    proof: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<AgentSubtreeValueV1>, RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let guard = ctx.transaction.guard();
    let live = ctx.transaction.parent_fence();
    Ok(read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent,
                field,
                ordinal,
            },
            qualification: proof,
        },
        ctx,
        ctx.record_bytes,
        &mut std::io::sink(),
        &guard,
        &live,
    )?)
}
pub(super) fn resource<M, R, E, C>(
    agent: &str,
    kind: ResourceKind,
    proof: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<i64>, RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    match balance(
        agent,
        AgentFieldV1::ResourceAmount,
        match kind {
            ResourceKind::Electricity => 0,
            ResourceKind::Data => 1,
        },
        proof,
        ctx,
    )? {
        None => Ok(None),
        Some(AgentSubtreeValueV1::I64(n)) => Ok(Some(n)),
        _ => Err(invalid()),
    }
}
struct Payer {
    id: String,
    key: WorldFieldKeyV1,
    value: [u8; 32],
    balances: [Option<i64>; 2],
}
fn payer<M, R, E, C>(
    request: &RootModuleFeeRequestV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<Payer>, RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut bytes = Vec::with_capacity(request.max_owner_bytes);
    let exists = read_world_text_map_overlay_v1(
        WorldTextMapFieldV1::ModuleArtifactOwners,
        &request.manifest.wasm_hash,
        ctx,
        request.max_owner_bytes as u64,
        &mut bytes,
    )?;
    finish(ctx)?;
    if exists.is_none() {
        return Ok(None);
    }
    let id = String::from_utf8(bytes).map_err(|_| invalid())?;
    if balance(&id, AgentFieldV1::Identity, 0, request.agents, ctx)?.is_none() {
        finish(ctx)?;
        return Ok(None);
    }
    let balances = [
        resource(&id, ResourceKind::Data, request.agents, ctx)?,
        resource(&id, ResourceKind::Electricity, request.agents, ctx)?,
    ];
    let semantic = hash_semantic_key_v1(
        request.manifest.wasm_hash.as_str(),
        ctx.values.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )?;
    let key = WorldFieldKeyV1::new(
        FieldDescriptor {
            owner: FieldOwner::WorldState,
            name: "module_artifact_owners",
            destination: FieldDestination::State(StateNamespaceV1::Business),
            shape: FieldShape::Map,
        },
        WorldFieldRecordKindV1::Entry,
        Some(semantic.digest),
    )
    .map_err(ValueTreeError::Invalid)?;
    let value = ctx
        .transaction
        .read(StateNamespaceV1::Business, key.as_bytes())?
        .ok_or_else(invalid)?;
    finish(ctx)?;
    Ok(Some(Payer {
        id,
        key,
        value,
        balances,
    }))
}
fn insufficient(
    request: &RootModuleFeeRequestV1<'_>,
    payer: &Payer,
    amounts: ModuleRuntimeFeeAmountsV1,
    upper: bool,
) -> Option<ModuleCallFailure> {
    let data = payer.balances[0].unwrap_or(0);
    let electricity = payer.balances[1].unwrap_or(0);
    (data < amounts.compute || electricity < amounts.electricity).then(|| ModuleCallFailure {
        module_id: request.module_id.into(),
        trace_id: request.trace_id.into(),
        code: ModuleCallErrorCode::PolicyDenied,
        detail: format!(
            "module runtime {} insufficient payer={} data {}/{} electricity {}/{}",
            if upper { "upper-bound fee" } else { "fee" },
            payer.id,
            data,
            amounts.compute,
            electricity,
            amounts.electricity
        ),
    })
}
pub fn check_root_module_fee_resources_v1<M, R, E, C>(
    request: RootModuleFeeRequestV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<ReservedValue<ModuleCallFailure>>, RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    with_overlay(ctx, |ctx| {
        check(&request, ctx)?;
        let charge = ctx
            .values
            .memory
            .reserve(retained_size(&request)?, MemoryClass::Ordinary)?;
        let result = payer(&request, ctx)?.and_then(|payer| {
            insufficient(
                &request,
                &payer,
                core::upper_bound(&request.manifest.limits, request.input_bytes),
                true,
            )
        });
        finish(ctx)?;
        Ok(result.map(|failure| charge.retain(failure)))
    })
}
pub fn prepare_root_module_fee_v1<M, R, E, C>(
    request: RootModuleFeeRequestV1<'_>,
    output: &ModuleOutput,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<RootModuleFeeOutcomeV1, RootModuleFeeErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    with_overlay(ctx, |ctx| {
        check(&request, ctx)?;
        let size = retained_size(&request)?;
        let charge = ctx.values.memory.reserve(size, MemoryClass::Ordinary)?;
        let Some(payer) = payer(&request, ctx)? else {
            finish(ctx)?;
            return Ok(RootModuleFeeOutcomeV1::Free);
        };
        let amounts = core::actual(request.input_bytes, output);
        if let Some(failure) = insufficient(&request, &payer, amounts, false) {
            finish(ctx)?;
            return Ok(RootModuleFeeOutcomeV1::Denied(charge.retain(failure)));
        }
        let result = charge.retain(PreparedRootModuleFeeV1 {
            event: ModuleRuntimeChargeEvent {
                module_id: request.module_id.into(),
                trace_id: request.trace_id.into(),
                payer_agent_id: payer.id,
                compute_fee_kind: ResourceKind::Data,
                compute_fee_amount: amounts.compute,
                electricity_fee_kind: ResourceKind::Electricity,
                electricity_fee_amount: amounts.electricity,
                input_bytes: request.input_bytes,
                output_bytes: output.output_bytes,
                effect_count: u32::try_from(output.effects.len()).unwrap_or(u32::MAX),
                emit_count: u32::try_from(output.emits.len()).unwrap_or(u32::MAX),
            },
            guard: ctx.transaction.guard(),
            live: *request.live,
            owner_key: payer.key,
            owner_value: payer.value,
            balances: payer.balances,
        });
        finish(ctx)?;
        Ok(RootModuleFeeOutcomeV1::Prepared(Box::new(result)))
    })
}

fn retained_size(request: &RootModuleFeeRequestV1<'_>) -> Result<usize, RootModuleFeeErrorV1> {
    request
        .max_owner_bytes
        .checked_mul(2)
        .and_then(|n| n.checked_add(request.module_id.len()))
        .and_then(|n| n.checked_add(request.trace_id.len()))
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(invalid)
}

#[cfg(test)]
#[path = "incremental_module_fee_tests.rs"]
mod tests;
