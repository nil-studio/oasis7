//! Paid Current Agent/Instance delta read and exact first-cause propagation.
use super::*;

/// Capture each actually changed Agent/Instance once. Hashed agent-field keys
/// are joined to the actual Identity scalar and original body key; hashes are
/// never interpreted as names. All work inputs are continuous caller banks.
#[allow(clippy::too_many_arguments)]
fn read_current_agent_instance_projection_delta_impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
>(
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    instances: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    pending_entries: &mut usize,
    processing: &mut BusinessDiffProcessingWorkV1,
    objects: &mut O,
    originals: &mut FactBudget,
    decode: &mut WorldEventDecodeWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentAgentInstanceProjectionDeltaV1, AgentInstanceProjectionErrorV1<O::Error>> {
    ctx.transaction
        .validate_read_source_guard(guard, live)
        .map_err(WorldFieldOverlayErrorV1::from)?;
    let p = Processing {
        bank: std::cell::RefCell::new(processing),
    };
    let memory = ctx.values.memory.clone();
    let _phase = memory
        .reserve(8192 + size_of::<Processing<'_>>(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if !agents.uses_budget(&memory)
        || !instances.uses_budget(&memory)
        || !ctx.transaction.uses_budget(&memory)
        || ctx.values.class != MemoryClass::Ordinary
        || agents.allocation_class() != MemoryClass::Ordinary
        || instances.allocation_class() != MemoryClass::Ordinary
    {
        return Err(bad().into());
    }
    if roots.world_id.len() > 256 || roots.state.iter().any(|r| r.node_root.len() != 64) {
        return Err(bad().into());
    }
    let root_bytes = roots
        .state
        .iter()
        .try_fold(roots.world_id.len() + 512, |n, r| {
            spend(&p, 0, &mut ctx.cancelled)?;
            n.checked_add(r.node_root.len() + 128).ok_or_else(limit)
        })?;
    spend(
        &p,
        root_bytes.checked_mul(4).ok_or_else(limit)?,
        &mut ctx.cancelled,
    )?;
    agents.validate_binding(roots)?;
    instances
        .validate_binding(roots)
        .map_err(AgentInstanceProjectionErrorV1::Body)?;
    if roots.digest().map_err(|_| bad())? != ctx.transaction.parent_fence().parent_roots {
        return Err(bad().into());
    }
    let mut count = 0usize;
    let mut bytes = 0usize;
    for root in &roots.state {
        spend(&p, 0, &mut ctx.cancelled)?;
        let stop = std::cell::RefCell::new(&mut ctx.cancelled);
        ctx.transaction.visit_pending_writes(
            root.namespace,
            pending_entries,
            || spend(&p, 0, &mut || (stop.borrow_mut())()).map_err(WorldFieldOverlayErrorV1::from),
            |key, before, after| {
                spend(&p, key.len(), &mut || (stop.borrow_mut())())?;
                if before != after && relevant(key) {
                    count = count.checked_add(1).ok_or_else(limit)?;
                    bytes = bytes.checked_add(key.len()).ok_or_else(limit)?;
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
        )?;
    }
    let pending_fee = memory
        .reserve(
            count
                .checked_mul(size_of::<Pending>())
                .and_then(|n| n.checked_add(bytes))
                .ok_or_else(limit)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut pending = Vec::new();
    pending.try_reserve_exact(count).map_err(|_| limit())?;
    if pending.capacity() > count {
        return Err(limit().into());
    }
    for root in &roots.state {
        spend(&p, 0, &mut ctx.cancelled)?;
        let stop = std::cell::RefCell::new(&mut ctx.cancelled);
        ctx.transaction.visit_pending_writes(
            root.namespace,
            pending_entries,
            || spend(&p, 0, &mut || (stop.borrow_mut())()).map_err(WorldFieldOverlayErrorV1::from),
            |key, before, after| {
                spend(&p, key.len(), &mut || (stop.borrow_mut())())?;
                if before != after && relevant(key) {
                    if pending.len() == count {
                        return Err(bad().into());
                    }
                    pending.push(Pending {
                        ns: root.namespace,
                        key: key.to_vec(),
                        before,
                        after,
                    });
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
        )?;
    }
    if pending.len() != count {
        return Err(bad().into());
    }
    let fee = memory
        .reserve(
            count
                .checked_mul(size_of::<Row>())
                .and_then(|n| n.checked_add(size_of::<CurrentAgentInstanceProjectionDeltaV1>()))
                .ok_or_else(limit)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(|_| limit())?;
    if rows.capacity() > count {
        return Err(limit().into());
    }
    let mut reader = Reader {
        ctx: &mut *ctx,
        p: &p,
        fee: memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?,
    };
    reader.pay(
        count
            .checked_mul(size_of::<Target>() + size_of::<usize>())
            .ok_or_else(limit)?,
    )?;
    let mut targets: Vec<Target> = Vec::new();
    targets.try_reserve_exact(count).map_err(|_| limit())?;
    if targets.capacity() > count {
        return Err(limit().into());
    }
    let mut owners = Vec::new();
    owners.try_reserve_exact(count).map_err(|_| limit())?;
    if owners.capacity() > count {
        return Err(limit().into());
    }
    for row in &pending {
        if reader.point(row.ns, &row.key)? != row.after {
            return Err(bad().into());
        }
        let (kind, key) = if row.key.starts_with(AGENT_BODY_KEY_DOMAIN_V1) {
            if row.ns != Ns::Business {
                return Err(bad().into());
            }
            (
                Kind::Agent,
                reader
                    .own(AgentBodyKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?)?,
            )
        } else if row.key.starts_with(MAILBOX_KEY_DOMAIN_V1) {
            if row.ns != Ns::Scheduler {
                return Err(bad().into());
            }
            let (id, ordinal) =
                MailboxKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?;
            if let Some(ordinal) = ordinal {
                if row.after.is_some() {
                    let key = MailboxKeyV1::new(id, None).map_err(ValueTreeError::Invalid)?;
                    let original = reader
                        .point(Ns::Scheduler, key.as_bytes())?
                        .ok_or_else(bad)?;
                    let frontier = MailboxFrontierV1::from_canonical_bytes(&reader.raw(&original)?)
                        .map_err(ValueTreeError::Invalid)?;
                    if ordinal < frontier.head || ordinal >= frontier.tail {
                        return Err(bad().into());
                    }
                }
            }
            (Kind::Agent, reader.own(id)?)
        } else if ModuleInstanceKeyV1::is_instance_key(&row.key) {
            if row.ns != Ns::Business {
                return Err(bad().into());
            }
            let (id, _) =
                ModuleInstanceKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?;
            (Kind::Instance, reader.own(id)?)
        } else {
            let (owner, field, ordinal) =
                AgentFieldKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?;
            if row.ns != field.namespace() {
                return Err(bad().into());
            }
            let identity = AgentFieldKeyV1::from_owner_digest(owner, F::Identity, 0)
                .map_err(ValueTreeError::Invalid)?;
            let identity_id = if let Some(id) = reader.point(Ns::Business, identity.as_bytes())? {
                id
            } else {
                let mut found = None;
                for original in &pending {
                    spend(
                        &p,
                        identity
                            .as_bytes()
                            .len()
                            .checked_add(original.key.len())
                            .ok_or_else(limit)?,
                        &mut reader.ctx.cancelled,
                    )?;
                    if original.ns == Ns::Business && original.key == identity.as_bytes() {
                        if found.is_some() {
                            return Err(bad().into());
                        }
                        found = original.before;
                    }
                }
                found.ok_or_else(bad)?
            };
            let raw = reader.raw(&identity_id)?;
            let AgentScalarRefV1::Text(id) =
                AgentScalarRefV1::from_canonical_bytes(&raw, F::Identity)
                    .map_err(ValueTreeError::Invalid)?
            else {
                return Err(bad().into());
            };
            spend(
                &p,
                id.len().checked_add(row.key.len()).ok_or_else(limit)?,
                &mut reader.ctx.cancelled,
            )?;
            if AgentFieldKeyV1::new(id, field, ordinal)
                .map_err(ValueTreeError::Invalid)?
                .as_bytes()
                != row.key
            {
                return Err(bad().into());
            }
            if row.after.is_some() {
                let next = match field {
                    F::SlotId | F::SlotType | F::SlotInstalled | F::SlotLocked => {
                        Some(reader.u64(id, F::SlotNext)?)
                    }
                    F::CargoId | F::CargoKind | F::CargoQuantity | F::CargoUnitSize => {
                        Some(reader.u64(id, F::CargoNext)?)
                    }
                    _ => None,
                };
                if next.is_some_and(|next| ordinal >= next) {
                    return Err(bad().into());
                }
            }
            (Kind::Agent, reader.own(id)?)
        };
        let mut seen = None;
        for (index, old) in targets.iter().enumerate() {
            spend(
                &p,
                old.key.len().checked_add(key.len()).ok_or_else(limit)?,
                &mut reader.ctx.cancelled,
            )?;
            if old.kind == kind && old.key == key {
                seen = Some(index);
                break;
            }
        }
        let index = if let Some(index) = seen {
            index
        } else {
            let index = targets.len();
            targets.push(Target { kind, key });
            index
        };
        owners.push(index);
    }
    for (index, target) in targets.iter().enumerate() {
        spend(&p, 0, &mut reader.ctx.cancelled)?;
        let semantic = semantic(&target.key, &memory, &p, &mut reader.ctx.cancelled)?;
        let key = json(&target.key, &memory, &p, &mut reader.ctx.cancelled)?;
        let (field, value, electricity, intent) = match target.kind {
            Kind::Agent => {
                let value = reader.agent(&target.key, objects, originals, decode)?;
                let electricity = value
                    .as_ref()
                    .map(|agent| agent.state.resources.get(ResourceKind::Electricity));
                (
                    "agents",
                    value
                        .as_ref()
                        .map(|v| json(v, &memory, &p, &mut reader.ctx.cancelled))
                        .transpose()?,
                    electricity,
                    value
                        .as_ref()
                        .map(|agent| json(&agent.intent, &memory, &p, &mut reader.ctx.cancelled))
                        .transpose()?,
                )
            }
            Kind::Instance => {
                let value = reader.instance(&target.key)?;
                (
                    "module_instances",
                    value
                        .as_ref()
                        .map(|v| json(v, &memory, &p, &mut reader.ctx.cancelled))
                        .transpose()?,
                    None,
                    None,
                )
            }
        };
        if value.is_none() {
            for (owner, row) in owners.iter().zip(&pending) {
                spend(&p, 0, &mut reader.ctx.cancelled)?;
                if *owner == index && row.after.is_some() {
                    return Err(bad().into());
                }
            }
            if target.kind == Kind::Agent {
                reader.verify_removed_agent(&target.key, &pending)?;
            }
        }
        rows.push(Row {
            field,
            semantic,
            key,
            value,
            electricity,
            intent,
        });
    }
    drop(owners);
    drop(targets);
    drop(reader);
    drop(pending);
    drop(pending_fee);
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    Ok(CurrentAgentInstanceProjectionDeltaV1 {
        rows,
        guard: ctx.transaction.guard(),
        parent: ctx.transaction.parent_fence().parent_roots,
        memory,
        fee,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn read_current_agent_instance_projection_delta_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
>(
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    instances: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    pending_entries: &mut usize,
    processing: &mut BusinessDiffProcessingWorkV1,
    objects: &mut O,
    originals: &mut FactBudget,
    decode: &mut WorldEventDecodeWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentAgentInstanceProjectionDeltaV1, AgentInstanceProjectionErrorV1<O::Error>> {
    let result = read_current_agent_instance_projection_delta_impl(
        agents,
        instances,
        roots,
        ctx,
        pending_entries,
        processing,
        objects,
        originals,
        decode,
        guard,
        live,
    );
    // MerkleReader's typed first cause takes precedence over its String
    // transport sentinel, on both success and refusal paths.
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    result
}
