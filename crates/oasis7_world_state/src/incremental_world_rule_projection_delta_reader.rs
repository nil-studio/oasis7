//! Original paid WorldRule projection traversal over the parent's qualified source.
use super::*;

#[allow(clippy::too_many_arguments)]
pub fn read_current_world_rule_projection_delta_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRulesQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    pending_entries: &mut usize,
    processing_work: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentWorldRuleProjectionDeltaV1, WorldFieldOverlayErrorV1> {
    let processing = Processing {
        bank: std::cell::RefCell::new(processing_work),
    };
    let p = &processing;
    let wrapped = ProcessingState {
        source: state,
        processing: p,
        first: std::cell::RefCell::new(None),
    };
    let state = &wrapped;
    let original_context = ctx;
    let _processing_phase = original_context
        .values
        .memory
        .reserve(
            size_of::<ProcessingState<'_, '_, S>>() + size_of::<Processing<'_>>() + 4096,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let attempt = {
        let mut take_error = || {
            if let Some(error) = (original_context.take_state_error)() {
                Some(error)
            } else {
                wrapped.first.borrow_mut().take()
            }
        };
        let mut current = WorldFieldOverlayContextV1 {
            transaction: &mut *original_context.transaction,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: original_context.values.source,
                memory: original_context.values.memory,
                class: original_context.values.class,
                io: &mut *original_context.values.io,
            },
            key_work: &mut *original_context.key_work,
            semantic_bytes: original_context.semantic_bytes,
            record_bytes: original_context.record_bytes,
            take_state_error: &mut take_error,
            cancelled: &mut original_context.cancelled,
        };
        let ctx = &mut current;
        (|| {
            ctx.transaction.validate_read_source_guard(guard, live)?;
            if !parent.uses_budget(ctx.values.memory)
                || ctx.values.class != MemoryClass::Ordinary
                || !ctx.transaction.uses_budget(ctx.values.memory)
            {
                return Err(bad().into());
            }
            if roots.world_id.len() > 256
                || roots.state.iter().any(|root| root.node_root.len() != 64)
            {
                return Err(bad().into());
            }
            // The complete six-branch cohort can execute fourteen bounded RootSet
            // binding/digest paths, rather than a single selected authorization read.
            let root_bytes = roots.state.iter().try_fold(
                roots.world_id.len().checked_add(512).ok_or_else(limit)?,
                |n, root| n.checked_add(root.node_root.len() + 128).ok_or_else(limit),
            )?;
            spend(
                p,
                root_bytes.checked_mul(16).ok_or_else(limit)?,
                &mut ctx.cancelled,
            )?;
            parent.validate_binding(roots)?;
            let parent_digest = ctx.transaction.parent_fence().parent_roots;
            if roots.digest().map_err(|_| bad())? != parent_digest {
                return Err(bad().into());
            }
            let memory = ctx.values.memory.clone();
            let fee = memory
                .reserve(
                    size_of::<CurrentWorldRuleProjectionDeltaV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let stop = std::cell::RefCell::new(&mut ctx.cancelled);
            let (mut count, mut key_bytes) = (0usize, 0usize);
            ctx.transaction.visit_pending_writes(
                StateNamespaceV1::Authorization,
                pending_entries,
                || {
                    spend(p, 0, &mut || (stop.borrow_mut())())
                        .map_err(WorldFieldOverlayErrorV1::from)
                },
                |key, before, after| {
                    spend(p, key.len(), &mut || (stop.borrow_mut())())?;
                    if before != after && relevant(key)? {
                        count = count.checked_add(1).ok_or_else(limit)?;
                        key_bytes = key_bytes.checked_add(key.len()).ok_or_else(limit)?;
                    }
                    Ok::<_, WorldFieldOverlayErrorV1>(())
                },
            )?;
            let pending_fee = memory
                .reserve(
                    count
                        .checked_mul(size_of::<Pending>())
                        .and_then(|n| n.checked_add(key_bytes))
                        .ok_or_else(limit)?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let mut pending = Vec::new();
            pending.try_reserve_exact(count).map_err(|_| limit())?;
            if pending.capacity() > count {
                return Err(limit().into());
            }
            ctx.transaction.visit_pending_writes(
                StateNamespaceV1::Authorization,
                pending_entries,
                || {
                    spend(p, 0, &mut || (stop.borrow_mut())())
                        .map_err(WorldFieldOverlayErrorV1::from)
                },
                |key, before, after| {
                    spend(p, key.len(), &mut || (stop.borrow_mut())())?;
                    if before != after && relevant(key)? {
                        if pending.len() == count {
                            return Err(bad().into());
                        }
                        pending.push(Pending {
                            key: key.to_vec(),
                            before,
                            after,
                        });
                    }
                    Ok::<_, WorldFieldOverlayErrorV1>(())
                },
            )?;
            if pending.len() != count {
                return Err(bad().into());
            }
            drop(stop);
            let source_cap = count.checked_add(6).ok_or_else(limit)?;
            spend(
                p,
                source_cap
                    .checked_mul(size_of::<Source>())
                    .ok_or_else(limit)?,
                &mut ctx.cancelled,
            )?;
            let source_slots = memory
                .reserve(
                    source_cap
                        .checked_mul(size_of::<Source>())
                        .ok_or_else(limit)?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let mut sources = Vec::new();
            sources.try_reserve_exact(source_cap).map_err(|_| limit())?;
            if sources.capacity() > source_cap {
                return Err(limit().into());
            }
            let mut build = Build {
                emission: [None, None],
                rows: Vec::new(),
                slots: memory
                    .reserve(0, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?,
                memory: memory.clone(),
                sources,
                source_slots,
            };
            let rules_present = {
                let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
                    values: &mut ctx.values,
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    cancelled: &mut ctx.cancelled,
                };
                crate::incremental_world_rule_parent::read_parent_overlay(
                    ctx.transaction,
                    &mut records,
                    &mut ctx.take_state_error,
                )?
                .1
            };
            let mut dirty = [false; 6];
            for row in &pending {
                spend(p, row.key.len(), &mut ctx.cancelled)?;
                let parsed =
                    WorldFieldKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?;
                if parsed.kind == WorldFieldRecordKindV1::Metadata {
                    continue;
                }
                if parsed.kind != WorldFieldRecordKindV1::Entry {
                    return Err(bad().into());
                }
                let fixed = WorldFieldKeyV1::new(
                    crate::incremental_world_rule_parent::descriptor()?,
                    parsed.kind,
                    parsed.semantic_key,
                )
                .map_err(ValueTreeError::Invalid)?;
                if ctx
                    .transaction
                    .read(StateNamespaceV1::Authorization, &row.key)?
                    != row.after
                {
                    return Err(bad().into());
                }
                let id = row.after.or(row.before).ok_or_else(bad)?;
                let limits = MemberRecordLimitsV1 {
                    semantic_bytes: ctx.semantic_bytes,
                    payload_bytes: ctx.record_bytes,
                    record_bytes: ctx.record_bytes,
                };
                let h = read_member_record_header_v1(
                    &id,
                    &fixed,
                    &mut ctx.values,
                    limits,
                    &mut ctx.cancelled,
                )?;
                let n = usize::try_from(h.semantic_bytes).map_err(|_| limit())?;
                spend(p, n.checked_mul(16).ok_or_else(limit)?, &mut ctx.cancelled)?;
                let _key_fee = memory
                    .reserve(
                        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                let mut semantic = Vec::new();
                semantic.try_reserve_exact(n).map_err(|_| limit())?;
                if semantic.capacity() > n {
                    return Err(limit().into());
                }
                read_member_record_key_v1(
                    &id,
                    &fixed,
                    &mut ctx.values,
                    limits,
                    &mut semantic,
                    &mut ctx.cancelled,
                )?;
                if semantic.len() != n {
                    return Err(bad().into());
                }
                let child =
                    crate::incremental_world_rule_effects::world_rule_child_from_semantic_v1(
                        &semantic,
                    )?;
                if let Some(index) = Branch::ALL.iter().position(|b| Some(b.name()) == child) {
                    dirty[index] = true;
                } else {
                    flat::project(
                        row, &semantic, child, roots, state, ctx, scan, work, p, &mut build, live,
                    )?;
                }
            }
            for (index, branch) in Branch::ALL.into_iter().enumerate() {
                spend(p, 0, &mut ctx.cancelled)?;
                if !dirty[index] || !rules_present {
                    continue;
                }
                let source = match branch {
                    Branch::Requests => Source::Requests(project_branch(
                        parent.requests(),
                        roots,
                        state,
                        ctx,
                        scan,
                        work,
                        p,
                        &mut build,
                        live,
                    )?),
                    Branch::AgentOrders => Source::Orders(project_branch(
                        parent.orders(),
                        roots,
                        state,
                        ctx,
                        scan,
                        work,
                        p,
                        &mut build,
                        live,
                    )?),
                    Branch::OrganizationContinuity => Source::Continuity(project_branch(
                        parent.continuity(),
                        roots,
                        state,
                        ctx,
                        scan,
                        work,
                        p,
                        &mut build,
                        live,
                    )?),
                    Branch::EmergencySupply => Source::Emergency(project_branch(
                        parent.emergency(),
                        roots,
                        state,
                        ctx,
                        scan,
                        work,
                        p,
                        &mut build,
                        live,
                    )?),
                    Branch::IndustrialWork => Source::Industrial(project_branch(
                        parent.industrial(),
                        roots,
                        state,
                        ctx,
                        scan,
                        work,
                        p,
                        &mut build,
                        live,
                    )?),
                    Branch::Substitution => Source::Substitution(project_branch(
                        parent.substitution(),
                        roots,
                        state,
                        ctx,
                        scan,
                        work,
                        p,
                        &mut build,
                        live,
                    )?),
                };
                build.sources.push(source);
            }
            drop(pending_fee);
            ctx.transaction
                .validate_guard(&ctx.transaction.guard(), live)?;
            Ok(CurrentWorldRuleProjectionDeltaV1 {
                rules_present,
                branch_emission: build.emission,
                rows: build.rows,
                _slots: build.slots,
                _sources: build.sources,
                _source_slots: build.source_slots,
                guard: ctx.transaction.guard(),
                parent: parent_digest,
                memory,
                _fee: fee,
            })
        })()
    };
    let attempt = match wrapped.first.borrow_mut().take() {
        Some(error) => Err(WorldFieldOverlayErrorV1::from(error)),
        None => attempt,
    };
    if attempt.is_err() {
        let _: Result<(), ExecutionError> = original_context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    attempt
}
