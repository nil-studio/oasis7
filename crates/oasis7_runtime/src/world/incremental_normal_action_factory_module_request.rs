//! Current pre-Rule request facts, in the original economy admission order.
use super::*;
use crate::world::event_processing::factory_schedule_fixed::{
    BuildMaterialCheckV1, FactoryBuildFactsV1, FactoryRecipeFactsV1, evaluate_build_factory_v1,
};
use oasis7_wasm_abi::MaterialStack;
use oasis7_world_state::MaterialLedgerId;
fn denied(note: String) -> crate::RejectReason {
    crate::RejectReason::RuleDenied { notes: vec![note] }
}
fn reject<'s>(
    selected: &'s SelectedPendingActionV1,
    reason: crate::RejectReason,
    context: &WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    common: &FormalBoundedContextV1<'_>,
    now: u64,
    fees: current::SourceFeesV1,
    layout: MemoryReservation,
) -> PreparedFactoryModuleActionV1<'s> {
    PreparedFactoryModuleActionV1::Rejected(RejectedFactoryModuleActionV1 {
        selected,
        reason,
        guard: context.transaction.guard(),
        identity: common.work().identity(),
        now,
        _fees: fees,
        _layout: layout,
    })
}
pub(in crate::world) fn prepare_factory_module_action_v1<'s, M, P, R, V, E, C, PE, PC>(
    selected: &'s SelectedPendingActionV1,
    sources: &FactoryScheduleSourcesV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedFactoryModuleActionV1<'s>, BodyMutationError>
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
    selected.validate(context.transaction)?;
    context
        .transaction
        .validate_guard(&context.transaction.guard(), live)?;
    source_binding(sources, context.transaction, common)?;
    let layout = common
        .memory()
        .reserve(
            size_of::<PreparedFactoryModuleCallV1>() + size_of::<RejectedFactoryModuleActionV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut fees = current::SourceFeesV1::new(work.max_source_records, common)?;
    let evaluated = (|| -> Result<_, current::EvaluationErrorV1> {
        let mut facts = current::CurrentFactoryScheduleFactsV1 {
            sources,
            context,
            read: parent,
            work,
            common,
            now,
            live,
            fees: &mut fees,
        };
        let (kind, module_id, request) = match &selected.envelope().action {
            crate::Action::BuildFactoryWithModule {
                builder_agent_id,
                site_id,
                module_id,
                spec,
            } => {
                facts.fees.hold(
                    module_id.len()
                        + spec.factory_id.len()
                        + site_id.len()
                        + builder_agent_id.len()
                        + 4096,
                    facts.common,
                )?;
                if module_id.trim().is_empty() {
                    return Ok(Err(denied("factory module_id cannot be empty".into())));
                }
                let preview = evaluate_build_factory_v1(
                    &mut facts,
                    selected.envelope().id,
                    builder_agent_id,
                    site_id,
                    spec,
                    BuildMaterialCheckV1::LegacyModulePreview,
                )?;
                if let crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                    reason,
                    ..
                }) = preview
                {
                    return Ok(Err(reason));
                }
                let power = FactoryBuildFactsV1::power_profile(&mut facts, &spec.factory_id)?;
                if power.as_ref().and_then(|p| p.source_module_id.as_deref())
                    != Some(module_id.as_str())
                {
                    return Ok(Err(denied(format!(
                        "construction power profile source mismatch: factory_id={} module_id={}",
                        spec.factory_id, module_id
                    ))));
                }
                let ledger = MaterialLedgerId::agent(builder_agent_id.clone());
                let peak = spec.build_cost.iter().try_fold(
                    size_of::<FactoryBuildRequest>()
                        + spec.factory_id.len()
                        + site_id.len()
                        + builder_agent_id.len()
                        + 4096,
                    |n, s| {
                        n.checked_add(s.kind.len() + size_of::<MaterialStack>() * 2)
                            .ok_or_else(limited)
                    },
                )?;
                facts.fees.hold(peak, facts.common)?;
                let mut available_inputs = Vec::with_capacity(spec.build_cost.len());
                for stack in &spec.build_cost {
                    available_inputs.push(MaterialStack::new(
                        stack.kind.clone(),
                        FactoryBuildFactsV1::material(&mut facts, &ledger, &stack.kind)?,
                    ));
                }
                let by_ledger = all_ledgers(&mut facts)?;
                let available_power =
                    FactoryBuildFactsV1::electricity(&mut facts, builder_agent_id)?;
                (
                    FactoryModuleCallKindV1::Build,
                    module_id.as_str(),
                    FactoryModuleRequestV1::Build(FactoryBuildRequest {
                        factory_id: spec.factory_id.clone(),
                        site_id: site_id.clone(),
                        builder: builder_agent_id.clone(),
                        available_inputs,
                        available_inputs_by_ledger: Some(by_ledger),
                        available_power,
                    }),
                )
            }
            crate::Action::ScheduleRecipeWithModule {
                requester_agent_id,
                factory_id,
                recipe_id,
                module_id,
                desired_batches,
                deterministic_seed,
            } => {
                facts.fees.hold(
                    (requester_agent_id.len()
                        + factory_id.len()
                        + recipe_id.len()
                        + module_id.len())
                        * 8
                        + 4096,
                    facts.common,
                )?;
                if !FactoryRecipeFactsV1::agent_exists(&mut facts, requester_agent_id)? {
                    return Ok(Err(crate::RejectReason::AgentNotFound {
                        agent_id: requester_agent_id.clone(),
                    }));
                }
                let Some(factory) = FactoryRecipeFactsV1::factory(&mut facts, factory_id)? else {
                    return Ok(Err(crate::RejectReason::FactoryNotFound {
                        factory_id: factory_id.clone(),
                    }));
                };
                if factory.builder() != requester_agent_id {
                    return Ok(Err(denied(format!(
                        "schedule recipe requester is not factory builder: factory_id={factory_id} requester={requester_agent_id} builder={}",
                        factory.builder()
                    ))));
                }
                if module_id.trim().is_empty() {
                    return Ok(Err(denied("recipe module_id cannot be empty".into())));
                }
                if *desired_batches == 0 {
                    return Ok(Err(denied("desired_batches must be > 0".into())));
                }
                let available_inputs = selected_ledger(&mut facts, factory.input())?;
                let by_ledger = all_ledgers(&mut facts)?;
                let available_power =
                    FactoryRecipeFactsV1::electricity(&mut facts, factory.builder())?;
                (
                    FactoryModuleCallKindV1::Recipe,
                    module_id.as_str(),
                    FactoryModuleRequestV1::Recipe(RecipeExecutionRequest {
                        recipe_id: recipe_id.clone(),
                        factory_id: factory_id.clone(),
                        desired_batches: *desired_batches,
                        available_inputs,
                        available_inputs_by_ledger: Some(by_ledger),
                        available_power,
                        deterministic_seed: *deterministic_seed,
                    }),
                )
            }
            crate::Action::ValidateProductWithModule {
                requester_agent_id,
                module_id,
                stack,
                deterministic_seed,
            } => {
                facts.fees.hold(
                    (requester_agent_id.len() + module_id.len() + stack.kind.len()) * 8 + 4096,
                    facts.common,
                )?;
                if !FactoryBuildFactsV1::agent_exists(&mut facts, requester_agent_id)? {
                    return Ok(Err(crate::RejectReason::AgentNotFound {
                        agent_id: requester_agent_id.clone(),
                    }));
                }
                if module_id.trim().is_empty() {
                    return Ok(Err(denied("product module_id cannot be empty".into())));
                }
                if stack.amount <= 0 {
                    return Ok(Err(denied("product stack amount must be > 0".into())));
                }
                (
                    FactoryModuleCallKindV1::Product,
                    module_id.as_str(),
                    FactoryModuleRequestV1::Product(ProductValidationRequest {
                        product_id: stack.kind.clone(),
                        stack: stack.clone(),
                        deterministic_seed: *deterministic_seed,
                    }),
                )
            }
            _ => return Err(current::EvaluationErrorV1::Body(invalid())),
        };
        Ok(Ok((kind, module_id, request)))
    })();
    let (kind, module_id, request) = match evaluated {
        Ok(Ok(v)) => v,
        Ok(Err(reason)) => return Ok(reject(selected, reason, context, common, now, fees, layout)),
        Err(e) => return Err(evaluation_error(e, fees)),
    };
    charge(common, module_id.len() + 64)?;
    let trace_bytes = module_id.len().checked_add(64).ok_or_else(limited)?;
    let trace_fee = common
        .memory()
        .reserve(size_of::<String>() + trace_bytes * 2, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let trace = format!(
        "action-{}-{}-{}",
        selected.envelope().id,
        module_id,
        kind.label()
    );
    if trace.capacity() > trace_bytes * 2 {
        return Err(limited());
    }
    let request_fee = common
        .memory()
        .reserve(size_of::<FactoryModuleRequestV1>(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(PreparedFactoryModuleActionV1::Call(
        PreparedFactoryModuleCallV1 {
            selected,
            kind,
            module_id,
            request: request_fee.retain(request),
            trace: trace_fee.retain(trace),
            guard: context.transaction.guard(),
            identity: common.work().identity(),
            now,
            memory: common.memory().clone(),
            _source_fees: fees,
            _layout: layout,
        },
    ))
}
fn all_ledgers<M, P, R, V, E, C, PE, PC>(
    facts: &mut current::CurrentFactoryScheduleFactsV1<
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
        M,
        P,
        R,
        V,
        E,
        C,
        PE,
        PC,
    >,
) -> Result<std::collections::BTreeMap<String, Vec<MaterialStack>>, current::EvaluationErrorV1>
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
    let sources = facts.sources;
    let live = facts.live;
    let value = io::paid(
        sources.roots,
        facts.context,
        facts.read,
        facts.work,
        facts.common,
        |ctx, parent, w| {
            let guard = ctx.transaction.guard();
            Ok(oasis7_world_state::incremental_inventory::recover_current_inventory_stacks_by_ledger_v1(sources.inventory_directory,sources.lifecycle.inventory,sources.roots,ctx,parent,&mut w.inventory_family,&mut w.inventory_directory,&guard,live)?)
        },
    )?;
    // The actual result carrier already bounds its complete map/vector/String
    // layout. A second full copy stays paid after the source carrier is dropped.
    let peak = value.reserved_bytes().checked_mul(3).ok_or_else(limited)?;
    facts.fees.hold(peak, facts.common)?;
    Ok((*value).clone())
}
fn selected_ledger<M, P, R, V, E, C, PE, PC>(
    facts: &mut current::CurrentFactoryScheduleFactsV1<
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
        M,
        P,
        R,
        V,
        E,
        C,
        PE,
        PC,
    >,
    ledger: &MaterialLedgerId,
) -> Result<Vec<MaterialStack>, current::EvaluationErrorV1>
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
    let sources = facts.sources;
    let live = facts.live;
    let value = io::paid(
        sources.roots,
        facts.context,
        facts.read,
        facts.work,
        facts.common,
        |ctx, parent, w| {
            let guard = ctx.transaction.guard();
            Ok(
                oasis7_world_state::incremental_inventory::recover_current_inventory_ledger_v1(
                    ledger,
                    sources.inventory_directory,
                    sources.lifecycle.inventory,
                    sources.roots,
                    ctx,
                    parent,
                    &mut w.inventory_family,
                    &mut w.inventory_directory,
                    &guard,
                    live,
                )?,
            )
        },
    )?;
    let count = value
        .as_ref()
        .map_or(0, |v| v.values().filter(|q| **q > 0).count());
    let text = value.as_ref().map_or(0, |v| {
        v.iter()
            .filter(|(_, q)| **q > 0)
            .map(|(k, _)| k.len())
            .sum::<usize>()
    });
    let peak = count
        .checked_mul(size_of::<MaterialStack>() * 2)
        .and_then(|n| n.checked_add(text))
        .ok_or_else(limited)?;
    facts.fees.hold(peak, facts.common)?;
    Ok(value
        .as_ref()
        .into_iter()
        .flat_map(|v| v.iter())
        .filter(|(_, q)| **q > 0)
        .map(|(kind, qty)| MaterialStack::new(kind.clone(), *qty))
        .collect())
}
