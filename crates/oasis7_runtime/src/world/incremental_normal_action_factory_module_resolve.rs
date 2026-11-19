//! Decode only a genuine bound-worker output, then retain its provenance with
//! the resolved action. The original economy emit/effects order is unchanged.
use super::*;
use crate::world::incremental_module_invocation::PaidFactoryModuleExecutionV1;
use oasis7_wasm_abi::{
    FactoryBuildDecision, ModuleCallErrorCode, ModuleCallFailure, ProductValidationDecision,
    RecipeExecutionPlan,
};
use serde::Deserialize;
use std::io::Write;
pub(in crate::world) enum FactoryModuleDecodedV1 {
    Build(ReservedValue<Result<FactoryBuildDecision, serde_json::Error>>),
    Recipe(ReservedValue<Result<RecipeExecutionPlan, serde_json::Error>>),
    Product(ReservedValue<Result<ProductValidationDecision, serde_json::Error>>),
}
pub(in crate::world) struct ResolvedFactoryModuleActionV1<'s> {
    call: PreparedFactoryModuleCallV1<'s>,
    execution: PaidFactoryModuleExecutionV1,
    action: ReservedValue<crate::Action>,
    _decoded: FactoryModuleDecodedV1,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    _layout: MemoryReservation,
    _build_preflight_fees: Option<current::SourceFeesV1>,
}
pub(in crate::world) struct InvalidFactoryModuleOutputV1<'s> {
    call: PreparedFactoryModuleCallV1<'s>,
    execution: PaidFactoryModuleExecutionV1,
    failure: ReservedValue<ModuleCallFailure>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
}
pub(in crate::world) struct DeniedFactoryModuleOutputV1<'s> {
    call: PreparedFactoryModuleCallV1<'s>,
    execution: PaidFactoryModuleExecutionV1,
    reason: ReservedValue<crate::RejectReason>,
    _decoded: FactoryModuleDecodedV1,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    _build_preflight_fees: Option<current::SourceFeesV1>,
}
pub(in crate::world) enum ResolvedFactoryModuleOutcomeV1<'s> {
    Resolved(ResolvedFactoryModuleActionV1<'s>),
    Denied(DeniedFactoryModuleOutputV1<'s>),
    Invalid(InvalidFactoryModuleOutputV1<'s>),
}
impl ResolvedFactoryModuleActionV1<'_> {
    pub(in crate::world) fn action(&self) -> &crate::Action {
        &self.action
    }
    pub(in crate::world) fn selected(&self) -> &SelectedPendingActionV1 {
        self.call.selected()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        self.validate_evidence_identity(tx, common, now, live)
    }
    pub(in crate::world) fn validate_evidence_identity<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.call
            .validate_evidence_identity(tx, common, now, live)?;
        self.execution
            .validate_evidence_identity(&self.call, tx, common, now, live)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.identity != common.work().identity()
            || self.now != now
            || !self.action.uses_budget(common.memory())
            || !self._layout.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(in crate::world) fn into_rule_action_v1<M: MerkleNodeReader>(
        self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<ReservedValue<crate::Action>, BodyMutationError> {
        self.validate(tx, common, now, live)?;
        Ok(self.action)
    }
}
impl InvalidFactoryModuleOutputV1<'_> {
    pub(in crate::world) fn failure(&self) -> &ModuleCallFailure {
        &self.failure
    }
    pub(in crate::world) fn selected(&self) -> &SelectedPendingActionV1 {
        self.call.selected()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.call
            .validate_evidence_identity(tx, common, now, live)?;
        self.execution
            .validate_evidence_identity(&self.call, tx, common, now, live)?;
        tx.validate_guard(&self.guard, live)?;
        if self.identity != common.work().identity()
            || self.now != now
            || !self.failure.uses_budget(common.memory())
            || self.failure.module_id != self.call.module_id()
            || self.failure.trace_id != self.call.trace_id()
            || self.failure.code != ModuleCallErrorCode::InvalidOutput
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl DeniedFactoryModuleOutputV1<'_> {
    pub(in crate::world) fn reason(&self) -> &crate::RejectReason {
        &self.reason
    }
    pub(in crate::world) fn selected(&self) -> &SelectedPendingActionV1 {
        self.call.selected()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.call
            .validate_evidence_identity(tx, common, now, live)?;
        self.execution
            .validate_evidence_identity(&self.call, tx, common, now, live)?;
        tx.validate_guard(&self.guard, live)?;
        if self.identity != common.work().identity()
            || self.now != now
            || !self.reason.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn name(kind: FactoryModuleCallKindV1) -> &'static str {
    match kind {
        FactoryModuleCallKindV1::Build => crate::world::economy::FACTORY_BUILD_DECISION_EMIT_KIND,
        FactoryModuleCallKindV1::Recipe => crate::world::economy::RECIPE_EXECUTION_PLAN_EMIT_KIND,
        FactoryModuleCallKindV1::Product => crate::world::economy::PRODUCT_VALIDATION_EMIT_KIND,
    }
}
fn walk(
    value: &serde_json::Value,
    common: &mut FormalBoundedContextV1<'_>,
    nodes: &mut usize,
    text: &mut usize,
) -> Result<(), BodyMutationError> {
    charge(common, 0)?;
    *nodes = nodes.checked_add(1).ok_or_else(limited)?;
    match value {
        serde_json::Value::String(s) => {
            charge(common, s.len())?;
            *text = text.checked_add(s.len()).ok_or_else(limited)?;
        }
        serde_json::Value::Array(v) => {
            for v in v {
                walk(v, common, nodes, text)?
            }
        }
        serde_json::Value::Object(m) => {
            for (k, v) in m {
                charge(common, k.len())?;
                *text = text.checked_add(k.len()).ok_or_else(limited)?;
                walk(v, common, nodes, text)?
            }
        }
        _ => {}
    }
    Ok(())
}
fn payload_fee(
    value: &serde_json::Value,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, BodyMutationError> {
    let (mut nodes, mut text) = (0usize, 0usize);
    walk(value, common, &mut nodes, &mut text)?;
    let bytes = nodes
        .checked_mul(
            3 * (size_of::<FactoryBuildDecision>()
                + size_of::<RecipeExecutionPlan>()
                + size_of::<ProductValidationDecision>()
                + size_of::<oasis7_wasm_abi::MaterialStack>()),
        )
        .and_then(|n| n.checked_add(text.checked_mul(24)?))
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(limited)?;
    charge(common, bytes)?;
    common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|e| ValueTreeError::Memory(e).into())
}
fn error_text(
    args: std::fmt::Arguments<'_>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    struct Counter<'a, 'b> {
        common: &'a mut FormalBoundedContextV1<'b>,
        bytes: usize,
        first: Option<BodyMutationError>,
    }
    impl std::fmt::Write for Counter<'_, '_> {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            if self.first.is_some() {
                return Err(std::fmt::Error);
            }
            if let Err(e) = charge(self.common, s.len()) {
                self.first = Some(e);
                return Err(std::fmt::Error);
            }
            self.bytes = self.bytes.checked_add(s.len()).ok_or(std::fmt::Error)?;
            Ok(())
        }
    }
    use std::fmt::Write as _;
    let mut count = Counter {
        common,
        bytes: 0,
        first: None,
    };
    let encoded = count.write_fmt(args);
    if let Some(e) = count.first.take() {
        return Err(e);
    }
    encoded.map_err(|_| limited())?;
    let bytes = count.bytes;
    drop(count);
    charge(common, bytes)?;
    let fee = common
        .memory()
        .reserve(bytes + size_of::<String>(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut text = String::with_capacity(bytes);
    text.write_fmt(args).map_err(|_| limited())?;
    if text.capacity() > bytes {
        return Err(limited());
    }
    Ok(fee.retain(text))
}
fn invalid_output<'s>(
    call: PreparedFactoryModuleCallV1<'s>,
    execution: PaidFactoryModuleExecutionV1,
    detail: &str,
    tx: &ExecutionTransaction<'_, impl MerkleNodeReader>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
) -> Result<ResolvedFactoryModuleOutcomeV1<'s>, BodyMutationError> {
    let bytes = call
        .module_id()
        .len()
        .checked_add(call.trace_id().len())
        .and_then(|n| n.checked_add(detail.len() + size_of::<ModuleCallFailure>()))
        .ok_or_else(limited)?;
    charge(common, bytes)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let failure = fee.retain(ModuleCallFailure {
        module_id: call.module_id().to_owned(),
        trace_id: call.trace_id().to_owned(),
        code: ModuleCallErrorCode::InvalidOutput,
        detail: detail.to_owned(),
    });
    Ok(ResolvedFactoryModuleOutcomeV1::Invalid(
        InvalidFactoryModuleOutputV1 {
            call,
            execution,
            failure,
            guard: tx.guard(),
            identity: common.work().identity(),
            now,
        },
    ))
}
fn action_layout(
    action: &crate::Action,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, BodyMutationError> {
    struct Count<'a, 'b> {
        common: &'a mut FormalBoundedContextV1<'b>,
        bytes: usize,
        first: Option<BodyMutationError>,
    }
    impl Write for Count<'_, '_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self.first.is_some() {
                return Err(std::io::ErrorKind::Other.into());
            }
            if let Err(e) = charge(self.common, b.len()) {
                self.first = Some(e);
                return Err(std::io::ErrorKind::Other.into());
            }
            self.bytes = self
                .bytes
                .checked_add(b.len())
                .ok_or(std::io::ErrorKind::InvalidData)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Count {
        common,
        bytes: 0,
        first: None,
    };
    let encoded = serde_json::to_writer(&mut counter, action);
    if let Some(e) = counter.first.take() {
        return Err(e);
    }
    encoded.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    // Only Build/Recipe/Product resolved layouts are copied here. Their closed
    // strings/vectors have no arbitrary JSON subtree; 64x actual wire dominates
    // all DTO slots, diagnostic overlap and the copied original Action fields.
    let bytes = counter
        .bytes
        .checked_mul(64)
        .and_then(|n| n.checked_add(size_of::<crate::Action>() + 8192))
        .ok_or_else(limited)?;
    drop(counter);
    charge(common, bytes)?;
    common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|e| ValueTreeError::Memory(e).into())
}
pub(in crate::world) fn resolve_actual_factory_module_output_v1<'s, M: MerkleNodeReader>(
    call: PreparedFactoryModuleCallV1<'s>,
    execution: PaidFactoryModuleExecutionV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<ResolvedFactoryModuleOutcomeV1<'s>, BodyMutationError> {
    call.validate_evidence_identity(tx, common, now, live)?;
    execution.validate_evidence_identity(&call, tx, common, now, live)?;
    execution.validate_live_resolution()?;
    let output = execution.output().ok_or(BodyMutationError::Invalid(
        "factory actual module output missing",
    ))?;
    if !output.effects.is_empty() {
        let detail = error_text(
            format_args!(
                "{} module output must not contain effects",
                call.kind().label()
            ),
            common,
        )?;
        return invalid_output(call, execution, &detail, tx, common, now);
    }
    let expected = name(call.kind());
    let mut payload = None;
    for emit in &output.emits {
        charge(common, emit.kind.len())?;
        if emit.kind == expected {
            if payload.is_some() {
                let detail = error_text(
                    format_args!("multiple {expected} emits in module output"),
                    common,
                )?;
                return invalid_output(call, execution, &detail, tx, common, now);
            }
            payload = Some(&emit.payload);
        }
    }
    let Some(payload) = payload else {
        let detail = error_text(
            format_args!("missing {expected} emit in module output"),
            common,
        )?;
        return invalid_output(call, execution, &detail, tx, common, now);
    };
    let fee = payload_fee(payload, common)?;
    let decoded = match call.kind() {
        FactoryModuleCallKindV1::Build => {
            FactoryModuleDecodedV1::Build(fee.retain(FactoryBuildDecision::deserialize(payload)))
        }
        FactoryModuleCallKindV1::Recipe => {
            FactoryModuleDecodedV1::Recipe(fee.retain(RecipeExecutionPlan::deserialize(payload)))
        }
        FactoryModuleCallKindV1::Product => FactoryModuleDecodedV1::Product(
            fee.retain(ProductValidationDecision::deserialize(payload)),
        ),
    };
    let error = match &decoded {
        FactoryModuleDecodedV1::Build(v) => v.as_ref().err(),
        FactoryModuleDecodedV1::Recipe(v) => v.as_ref().err(),
        FactoryModuleDecodedV1::Product(v) => v.as_ref().err(),
    };
    if let Some(error) = error {
        let detail = error_text(format_args!("decode {expected} failed: {error}"), common)?;
        return invalid_output(call, execution, &detail, tx, common, now);
    }
    let mut layout = action_layout(&call.selected().envelope().action, common)?;
    let clone_fee = match &decoded {
        FactoryModuleDecodedV1::Build(v) => v.reserved_bytes(),
        FactoryModuleDecodedV1::Recipe(v) => v.reserved_bytes(),
        FactoryModuleDecodedV1::Product(v) => v.reserved_bytes(),
    };
    charge(common, clone_fee)?;
    let mut action_fee = common
        .memory()
        .reserve(
            clone_fee
                .checked_add(size_of::<crate::Action>())
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let action = match (&call.selected().envelope().action, &decoded) {
        (
            crate::Action::BuildFactoryWithModule {
                builder_agent_id,
                site_id,
                spec,
                ..
            },
            FactoryModuleDecodedV1::Build(value),
        ) => {
            let decision = value.as_ref().map_err(|_| invalid())?;
            if !decision.accepted {
                let detail = error_text(
                    format_args!(
                        "factory module denied: {}",
                        decision
                            .reject_reason
                            .as_deref()
                            .unwrap_or("build rejected")
                    ),
                    common,
                )?;
                let reason_fee = common
                    .memory()
                    .reserve(
                        size_of::<crate::RejectReason>() + size_of::<String>() + detail.len(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                let reason = reason_fee.retain(crate::RejectReason::RuleDenied {
                    notes: vec![detail.to_string()],
                });
                return Ok(ResolvedFactoryModuleOutcomeV1::Denied(
                    DeniedFactoryModuleOutputV1 {
                        call,
                        execution,
                        reason,
                        _decoded: decoded,
                        guard: tx.guard(),
                        identity: common.work().identity(),
                        now,
                        _build_preflight_fees: None,
                    },
                ));
            }
            let mut spec = spec.clone();
            if !decision.consume.is_empty() {
                spec.build_cost = decision.consume.clone()
            }
            if decision.duration_ticks > 0 {
                spec.build_time_ticks = decision.duration_ticks
            }
            crate::Action::BuildFactory {
                builder_agent_id: builder_agent_id.clone(),
                site_id: site_id.clone(),
                spec,
            }
        }
        (
            crate::Action::ScheduleRecipeWithModule {
                requester_agent_id,
                factory_id,
                recipe_id,
                desired_batches,
                ..
            },
            FactoryModuleDecodedV1::Recipe(value),
        ) => {
            let plan = value.as_ref().map_err(|_| invalid())?;
            let detail = if let Some(reason) = &plan.reject_reason {
                Some(error_text(
                    format_args!("recipe module denied: {reason}"),
                    common,
                )?)
            } else if plan.accepted_batches > *desired_batches {
                Some(error_text(
                    format_args!(
                        "recipe module accepted_batches exceeds desired_batches: accepted_batches={} desired_batches={}",
                        plan.accepted_batches, desired_batches
                    ),
                    common,
                )?)
            } else {
                None
            };
            if let Some(detail) = detail {
                let fee = common
                    .memory()
                    .reserve(
                        size_of::<crate::RejectReason>() + size_of::<String>() + detail.len(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                let reason = fee.retain(crate::RejectReason::RuleDenied {
                    notes: vec![detail.to_string()],
                });
                return Ok(ResolvedFactoryModuleOutcomeV1::Denied(
                    DeniedFactoryModuleOutputV1 {
                        call,
                        execution,
                        reason,
                        _decoded: decoded,
                        guard: tx.guard(),
                        identity: common.work().identity(),
                        now,
                        _build_preflight_fees: None,
                    },
                ));
            }
            crate::Action::ScheduleRecipe {
                requester_agent_id: requester_agent_id.clone(),
                factory_id: factory_id.clone(),
                recipe_id: recipe_id.clone(),
                plan: plan.clone(),
                logistics_route_ids: Vec::new(),
                logistics_path_ids: Vec::new(),
            }
        }
        (
            crate::Action::ValidateProductWithModule {
                requester_agent_id,
                module_id,
                stack,
                ..
            },
            FactoryModuleDecodedV1::Product(value),
        ) => {
            let decision = value.as_ref().map_err(|_| invalid())?;
            if !decision.accepted {
                let slots = decision
                    .notes
                    .len()
                    .max(1)
                    .checked_mul(size_of::<ReservedValue<String>>() * 3)
                    .ok_or_else(limited)?;
                charge(common, slots)?;
                let _notes_slots = common
                    .memory()
                    .reserve(slots, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let notes: Vec<_> = if decision.notes.is_empty() {
                    vec![error_text(
                        format_args!("product module denied: {}", decision.product_id),
                        common,
                    )?]
                } else {
                    let mut out = Vec::new();
                    for note in &decision.notes {
                        out.push(error_text(
                            format_args!("product module denied: {note}"),
                            common,
                        )?);
                    }
                    out
                };
                let bytes = notes.iter().try_fold(
                    size_of::<crate::RejectReason>() + notes.len() * size_of::<String>(),
                    |n, s| n.checked_add(s.len()).ok_or_else(limited),
                )?;
                charge(common, bytes)?;
                let fee = common
                    .memory()
                    .reserve(bytes, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let reason = fee.retain(crate::RejectReason::RuleDenied {
                    notes: notes.iter().map(|n| n.to_string()).collect(),
                });
                return Ok(ResolvedFactoryModuleOutcomeV1::Denied(
                    DeniedFactoryModuleOutputV1 {
                        call,
                        execution,
                        reason,
                        _decoded: decoded,
                        guard: tx.guard(),
                        identity: common.work().identity(),
                        now,
                        _build_preflight_fees: None,
                    },
                ));
            }
            crate::Action::ValidateProduct {
                requester_agent_id: requester_agent_id.clone(),
                module_id: module_id.clone(),
                stack: stack.clone(),
                decision: decision.clone(),
            }
        }
        _ => return Err(invalid()),
    };
    // Transfer every graph charge into the moved Action owner. The original
    // selected-field copy fee must not die with the consumed resolution wrapper.
    action_fee
        .merge(&mut layout)
        .map_err(BodyMutationError::Invalid)?;
    Ok(ResolvedFactoryModuleOutcomeV1::Resolved(
        ResolvedFactoryModuleActionV1 {
            call,
            execution,
            action: action_fee.retain(action),
            _decoded: decoded,
            guard: tx.guard(),
            identity: common.work().identity(),
            now,
            _layout: layout,
            _build_preflight_fees: None,
        },
    ))
}
pub(in crate::world) fn format_failure_rejection_v1(
    error: &crate::WorldError,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<crate::RejectReason>, BodyMutationError> {
    let detail = error_text(
        format_args!("economy module evaluation failed; action rejected: {error:?}"),
        common,
    )?;
    let bytes = size_of::<crate::RejectReason>() + size_of::<String>() + detail.len();
    charge(common, bytes)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(crate::RejectReason::RuleDenied {
        notes: vec![detail.to_string()],
    }))
}

#[path = "incremental_normal_action_factory_module_build.rs"]
mod build;
impl<'s> ResolvedFactoryModuleActionV1<'s> {
    pub(in crate::world) fn preflight_build_v1<M, P, R, V, E, C, PE, PC>(
        self,
        sources: &FactoryScheduleSourcesV1<'_>,
        context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut FactoryScheduleWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<ResolvedFactoryModuleOutcomeV1<'s>, BodyMutationError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        V: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        build::preflight(self, sources, context, parent, work, common, now, live)
    }
    pub(in crate::world) fn rejoin_build_v1<M: MerkleNodeReader>(
        &mut self,
        evidence: &oasis7_formal_protocol::incremental_execution::StagedBranchRejoinEvidenceV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        if evidence.disposition()
            != oasis7_formal_protocol::incremental_execution::StagedBranchDispositionV1::Committed
            || self._build_preflight_fees.is_none()
            || !matches!(&*self.action, crate::Action::BuildFactory { .. })
            || self.identity != common.work().identity()
            || self.now != now
            || !self.action.uses_budget(common.memory())
            || !self._layout.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        let guard = evidence
            .rebind_source_guard(&self.guard, tx, common, live)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        self.execution
            .rejoin_build_execution_v1(&self.call, evidence, tx, common, now, live)?;
        self.guard = guard;
        self.validate(tx, common, now, live)
    }
}
impl InvalidFactoryModuleOutputV1<'_> {
    pub(in crate::world) fn rejoin_build_v1<M: MerkleNodeReader>(
        &mut self,
        evidence: &oasis7_formal_protocol::incremental_execution::StagedBranchRejoinEvidenceV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        if evidence.disposition()
            != oasis7_formal_protocol::incremental_execution::StagedBranchDispositionV1::Discarded
            || self.identity != common.work().identity()
            || self.now != now
            || !self.failure.uses_budget(common.memory())
            || self.failure.module_id != self.call.module_id()
            || self.failure.trace_id != self.call.trace_id()
            || self.failure.code != ModuleCallErrorCode::InvalidOutput
        {
            return Err(invalid());
        }
        let guard = evidence
            .rebind_source_guard(&self.guard, tx, common, live)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        self.execution
            .rejoin_build_execution_v1(&self.call, evidence, tx, common, now, live)?;
        self.guard = guard;
        self.validate(tx, common, now, live)
    }
}
impl DeniedFactoryModuleOutputV1<'_> {
    pub(in crate::world) fn rejoin_build_v1<M: MerkleNodeReader>(
        &mut self,
        evidence: &oasis7_formal_protocol::incremental_execution::StagedBranchRejoinEvidenceV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        if evidence.disposition()
            != oasis7_formal_protocol::incremental_execution::StagedBranchDispositionV1::Discarded
            || self.identity != common.work().identity()
            || self.now != now
            || !self.reason.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        let guard = evidence
            .rebind_source_guard(&self.guard, tx, common, live)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        self.execution
            .rejoin_build_execution_v1(&self.call, evidence, tx, common, now, live)?;
        self.guard = guard;
        self.validate(tx, common, now, live)
    }
}
