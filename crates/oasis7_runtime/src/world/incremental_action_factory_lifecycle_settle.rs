use super::*;
use oasis7_world_state::incremental_world_id_sets::{
    WorldIdSetChangeV1, WorldIdSetFieldV1, WorldIdSetKeyRefV1, change_world_id_set_overlay_v1,
};
use oasis7_world_state::incremental_world_native::{
    CurrentFactoryNativeRecordV1, change_current_factory_industry_stage_v1,
    recover_current_factory_industry_progress_v1, recover_current_factory_recycle_receipt_v1,
    recover_gameplay_policy_overlay_v1, write_current_factory_recycle_receipt_v1,
};
#[path = "incremental_action_factory_lifecycle_settle_material.rs"]
pub(in crate::world) mod material;
#[allow(clippy::too_many_arguments)]
fn receipt<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    id: &str,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    sources: &FactoryLifecycleSourcesV1<'_>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<CurrentFactoryNativeRecordV1<oasis7_world_state::FactoryRecycleReceiptV1>>,
    BodyMutationError,
> {
    let mut ctx = WorldFieldReadContextV1 {
        root: roots
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization),
        state: read.state,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: MemoryClass::Ordinary,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
    };
    let guard = step.transaction.guard();
    recover_current_factory_recycle_receipt_v1(
        id,
        sources.native,
        roots,
        step.transaction,
        &mut ctx,
        &mut work.native_scan,
        &mut work.native_read,
        &guard,
        live,
    )
    .map_err(overlay_error)
}
fn ledger_bytes(ledger: &MaterialLedgerId) -> usize {
    match ledger {
        MaterialLedgerId::World => 0,
        MaterialLedgerId::Agent(s) | MaterialLedgerId::Factory(s) | MaterialLedgerId::Site(s) => {
            s.len()
        }
    }
}
pub(super) fn event_clone_bytes(
    intent: &LifecycleIntentV1<'_>,
) -> Result<usize, BodyMutationError> {
    let mut n = intent
        .operator
        .len()
        .checked_add(intent.factory.len())
        .and_then(|n| n.checked_add(std::mem::size_of::<crate::WorldEvent>()))
        .ok_or(ValueTreeError::ResourceLimited("factory event text bound"))?;
    match &intent.effect {
        LifecycleEffectV1::Maintained { ledger, .. } => {
            n = n
                .checked_add(ledger_bytes(ledger))
                .ok_or(ValueTreeError::ResourceLimited("factory event ledger"))?
        }
        LifecycleEffectV1::Recycled {
            ledger, recovered, ..
        } => {
            n = n
                .checked_add(ledger_bytes(ledger))
                .and_then(|n| {
                    n.checked_add(
                        recovered
                            .len()
                            .checked_mul(std::mem::size_of::<MaterialStack>())?,
                    )
                })
                .ok_or(ValueTreeError::ResourceLimited("factory event stacks"))?;
            for MaterialStack { kind, amount: _ } in recovered {
                n = n
                    .checked_add(kind.len())
                    .ok_or(ValueTreeError::ResourceLimited(
                        "factory event material text",
                    ))?;
            }
        }
    }
    Ok(n)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn settle<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
>(
    intent: &LifecycleIntentV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &FactoryLifecycleSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
    live: &ExecutionFence,
) -> Result<bool, BodyMutationError> {
    step.transaction.validate_guard(&intent.identity, live)?;
    if intent.time != step.time() {
        return Err(BodyMutationError::Invalid("factory preparation time"));
    }
    match &intent.effect {
        LifecycleEffectV1::Maintained {
            ledger,
            consumed,
            before_durability,
            durability,
        } => {
            if read::retired(step, intent.factory, read, work)? {
                return Err(material::invalid_state(
                    step.mutation.memory,
                    &format!(
                        "factory maintenance targets retired identity: factory_id={}",
                        intent.factory
                    ),
                )?);
            }
            let stacks = [MaterialStack::new("hardware_part".to_owned(), *consumed)];
            let plan = material::prepare(step, materials, ledger, &stacks, true, read, work, live)?;
            material::apply(&plan, ledger, step, materials, read, work, live)?;
            let stop = RefCell::new(&mut read.cancelled);
            let mutation = RefCell::new(&mut step.mutation.cancelled);
            let mut ctx = WorldFieldOverlayContextV1 {
                transaction: step.transaction,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: step.mutation.memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *materials.read_io,
                },
                key_work: &mut *materials.read_keys,
                semantic_bytes: materials.semantic_bytes,
                record_bytes: materials.record_bytes,
                take_state_error: &mut materials.take_state_error,
                cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut *step.mutation.sink,
                memory: step.mutation.memory,
                io: &mut *step.mutation.io,
                key_work: &mut *materials.write_keys,
                limits: materials.write_limits,
                cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
            };
            let guard = ctx.transaction.guard();
            change_factory_cell_overlay_v1(
                FactoryCellChangeV1 {
                    key: FactoryRecordKeyV1::Factory(intent.factory),
                    cell: FactoryCellV1::Column(FactoryColumnV1::FactoryDurabilityPpm),
                    expected: Some(FactoryValueRefV1::Signed(*before_durability)),
                    after: Some(FactoryValueRefV1::Signed(*durability)),
                    max_current_payload_bytes: work.max_scalar_bytes as u64,
                },
                &mut ctx,
                &mut write,
                &mut work.factory,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            Ok(true)
        }
        LifecycleEffectV1::Recycled {
            ledger,
            recovered,
            durability,
            ..
        } => {
            let _receipt_phase = step
                .mutation
                .memory
                .reserve(
                    event_clone_bytes(intent)?
                        .checked_add(1024)
                        .ok_or(ValueTreeError::ResourceLimited("factory receipt clone"))?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let desired = oasis7_world_state::FactoryRecycleReceiptV1 {
                operator_agent_id: intent.operator.to_owned(),
                factory_id: intent.factory.to_owned(),
                recycle_ledger: (**ledger).clone(),
                recovered: recovered.clone(),
                durability_ppm: *durability,
            };
            let old = receipt(
                intent.factory,
                step,
                sources,
                materials.roots,
                read,
                work,
                live,
            )?;
            if old.value().is_some_and(|r| r != &desired) {
                return Err(material::invalid_state(
                    step.mutation.memory,
                    &format!(
                        "factory recycle conflicts with persisted receipt: factory_id={}",
                        intent.factory
                    ),
                )?);
            }
            if read::retired(step, intent.factory, read, work)? {
                let guard = step.transaction.guard();
                materials.contribute(step.transaction, step.values, step.mutation, &guard, live)?;
                return Ok(false);
            }
            let plan =
                material::prepare(step, materials, ledger, recovered, false, read, work, live)?;
            material::apply(&plan, ledger, step, materials, read, work, live)?;
            // Empty salvage still creates the real output group; the owner
            // derives that ledger from this genuine Current factory itself.
            {
                let stop = RefCell::new(&mut read.cancelled);
                let mutation = RefCell::new(&mut step.mutation.cancelled);
                let mut ctx = WorldFieldReadContextV1 {
                    root: materials.roots.state_root(
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                    ),
                    state: read.state,
                    values: ValueReadContext {
                        source: read.values.source,
                        memory: step.mutation.memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut *step.mutation.sink,
                    memory: step.mutation.memory,
                    io: &mut *step.mutation.io,
                    key_work: &mut *materials.write_keys,
                    limits: materials.write_limits,
                    cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
                };
                let guard = step.transaction.guard();
                oasis7_world_state::incremental_inventory::ensure_current_factory_output_ledger_group_v1(intent.factory,sources.factories,sources.inventory,materials.roots,step.transaction,&mut ctx,&mut write,
                    &mut work.factory,materials.family_work,&mut work.output_group,work.max_scalar_bytes as u64,&guard,live).map_err(overlay_error)?;
            }
            let guard = step.transaction.guard();
            let deletion = prepare_current_factory_delete_v1(
                intent.factory,
                sources.factories,
                materials.roots,
                step.transaction,
                read,
                &mut work.factory,
                work.max_scalar_bytes as u64,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            delete_prepared_current_factory_v1(&deletion, step.transaction, read, live)
                .map_err(overlay_error)?;
            {
                let stop = RefCell::new(&mut read.cancelled);
                let mutation = RefCell::new(&mut step.mutation.cancelled);
                let mut ctx = WorldFieldOverlayContextV1 {
                    transaction: step.transaction,
                    values: ValueReadContext {
                        source: read.values.source,
                        memory: step.mutation.memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *materials.read_io,
                    },
                    key_work: &mut *materials.read_keys,
                    semantic_bytes: materials.semantic_bytes,
                    record_bytes: materials.record_bytes,
                    take_state_error: &mut materials.take_state_error,
                    cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut *step.mutation.sink,
                    memory: step.mutation.memory,
                    io: &mut *step.mutation.io,
                    key_work: &mut *materials.write_keys,
                    limits: materials.write_limits,
                    cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
                };
                let guard = ctx.transaction.guard();
                change_world_id_set_overlay_v1(
                    WorldIdSetChangeV1 {
                        field: WorldIdSetFieldV1::RetiredFactories,
                        key: WorldIdSetKeyRefV1::Text(intent.factory),
                        expected: false,
                        after: true,
                    },
                    &mut ctx,
                    &mut write,
                    &guard,
                    live,
                )
                .map_err(overlay_error)?;
            }
            update_progress(intent.factory, step, materials, sources, read, work, live)?;
            // Prior writes legitimately changed revision. Reopen the real
            // unchanged receipt rather than refreshing/re-signing an old owner.
            let expected = receipt(
                intent.factory,
                step,
                sources,
                materials.roots,
                read,
                work,
                live,
            )?;
            {
                let stop = RefCell::new(&mut read.cancelled);
                let mutation = RefCell::new(&mut step.mutation.cancelled);
                let mut ctx = WorldFieldReadContextV1 {
                    root: materials.roots.state_root(
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
                    ),
                    state: read.state,
                    values: ValueReadContext {
                        source: read.values.source,
                        memory: step.mutation.memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut *step.mutation.sink,
                    memory: step.mutation.memory,
                    io: &mut *step.mutation.io,
                    key_work: &mut *materials.write_keys,
                    limits: materials.write_limits,
                    cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
                };
                write_current_factory_recycle_receipt_v1(
                    &expected,
                    intent.factory,
                    &desired,
                    step.transaction,
                    &mut ctx,
                    &mut write,
                    &mut work.records_remaining,
                    &mut work.native_write,
                    live,
                )
                .map_err(overlay_error)?;
            }
            Ok(true)
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn update_progress<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
>(
    factory: &str,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &FactoryLifecycleSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    let policy = recover_gameplay_policy_overlay_v1(
        step.transaction,
        live,
        read,
        &mut work.native_scan,
        &mut work.native_read,
    )
    .map_err(overlay_error)?;
    let enabled =
        policy.value().value().electricity_tax_bps > 0 || policy.value().value().data_tax_bps > 0;
    // Prime the exact native point before obtaining the strict index tail.
    let guard = step.transaction.guard();
    let prior = recover_current_factory_industry_progress_v1(
        sources.native,
        materials.roots,
        step.transaction,
        read,
        &mut work.native_scan,
        &mut work.native_read,
        &guard,
        live,
    )
    .map_err(overlay_error)?;
    let transits = prior
        .value()
        .ok_or(BodyMutationError::Invalid("factory progress absent"))?
        .completed_material_transits;
    let guard = step.transaction.guard();
    let current = observe_current_factory_lifecycle_v1(
        factory,
        sources.index,
        sources.factories,
        sources.industry,
        materials.roots,
        step.transaction,
        read,
        &mut work.factory,
        &mut work.industry,
        &mut work.lifecycle,
        work.max_scalar_bytes as u64,
        &guard,
        live,
    )
    .map_err(overlay_error)?;
    current
        .validate(step.transaction, step.mutation.memory, live)
        .map_err(overlay_error)?;
    let stage = oasis7_world_state::state::industry_transition::industry_stage_for_progress(
        current.stable_without_factory(),
        current.stage_completed_jobs_without_factory(),
        transits,
        enabled,
    );
    // Reopen the native owner rather than refresh an invalidated guard. Its
    // point dependencies are already in this transaction's actual read set.
    let guard = step.transaction.guard();
    let progress = recover_current_factory_industry_progress_v1(
        sources.native,
        materials.roots,
        step.transaction,
        read,
        &mut work.native_scan,
        &mut work.native_read,
        &guard,
        live,
    )
    .map_err(overlay_error)?;
    current
        .validate(step.transaction, step.mutation.memory, live)
        .map_err(overlay_error)?;
    if progress
        .value()
        .ok_or(BodyMutationError::Invalid("factory progress absent"))?
        .completed_material_transits
        != transits
    {
        return Err(BodyMutationError::Invalid(
            "factory progress Current changed",
        ));
    }
    let now = step.time();
    let stop = RefCell::new(&mut read.cancelled);
    let mutation = RefCell::new(&mut step.mutation.cancelled);
    let mut ctx = WorldFieldReadContextV1 {
        root: materials
            .roots
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business),
        state: read.state,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: MemoryClass::Ordinary,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut *step.mutation.sink,
        memory: step.mutation.memory,
        io: &mut *step.mutation.io,
        key_work: &mut *materials.write_keys,
        limits: materials.write_limits,
        cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
    };
    change_current_factory_industry_stage_v1(
        &progress,
        stage,
        now,
        step.transaction,
        &mut ctx,
        &mut write,
        &mut work.native_scan,
        &mut work.native_read,
        &mut work.native_write,
        live,
    )
    .map_err(overlay_error)?;
    Ok(())
}
