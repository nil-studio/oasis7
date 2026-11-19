//! One explicit authenticated-parent ALL read. The transaction must still be
//! bootstrap read-only; the original complete Agent/Instance reader is shared.
use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::incremental_state::{
    StateScanBudget, StateScanContext, StateScanError,
};
pub struct QualifiedAgentInstanceProjectionColumnsV1 {
    counts: [u64; 2],
    rows: Vec<Row>,
    guard: PreparedGuard,
    roots: [u8; 32],
    memory: MemoryBudget,
    _slots: MemoryReservation,
    _fee: MemoryReservation,
}
impl QualifiedAgentInstanceProjectionColumnsV1 {
    pub fn record_counts(&self) -> [u64; 2] {
        self.counts
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_bootstrap_read_only_v1(live)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        if self.roots != tx.parent_fence().parent_roots
            || !self.memory.same_pool(memory)
            || !self._slots.uses_budget(memory)
            || !self._fee.uses_budget(memory)
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub fn visit_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&str, &[u8], &[u8], &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            emit(
                row.field,
                &row.semantic,
                &row.key,
                row.value.as_ref().expect("ALL actual row"),
            )?
        }
        Ok(())
    }
    pub fn visit_complete_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&str, &[u8], &[u8], &[u8], Option<i64>, Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            emit(
                row.field,
                &row.semantic,
                &row.key,
                row.value.as_ref().expect("ALL actual row"),
                row.electricity,
                row.intent.as_deref().map(Vec::as_slice),
            )?
        }
        Ok(())
    }
    pub fn visit_agent_intents_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&[u8], &[u8], &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            if row.field == "agents" {
                emit(
                    &row.semantic,
                    &row.key,
                    row.intent.as_deref().expect("ALL actual agent intent DATA"),
                )?
            }
        }
        Ok(())
    }
    pub fn visit_agent_electricity_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&[u8], &[u8], i64) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            if row.field == "agents" {
                emit(
                    &row.semantic,
                    &row.key,
                    row.electricity.expect("ALL actual agent"),
                )?
            }
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_qualified_agent_instance_projection_columns_v1<M, R, E, C, O, P, V, PE, PC>(
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    instances: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    scan: &mut StateScanBudget,
    processing: &mut BusinessDiffProcessingWorkV1,
    objects: &mut O,
    originals: &mut FactBudget,
    decode: &mut WorldEventDecodeWorkV1,
    live: &ExecutionFence,
) -> Result<QualifiedAgentInstanceProjectionColumnsV1, AgentInstanceProjectionErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    ctx.transaction
        .validate_bootstrap_read_only_v1(live)
        .map_err(WorldFieldOverlayErrorV1::from)?;
    parent.check()?;
    let memory = ctx.values.memory.clone();
    let digest = roots.digest().map_err(|_| bad())?;
    if digest != ctx.transaction.parent_fence().parent_roots
        || !agents.uses_budget(&memory)
        || !instances.uses_budget(&memory)
        || !parent.values.memory.same_pool(&memory)
        || ctx.values.class != MemoryClass::Ordinary
        || parent.values.class != MemoryClass::Ordinary
        || agents.allocation_class() != MemoryClass::Ordinary
        || instances.allocation_class() != MemoryClass::Ordinary
    {
        return Err(bad().into());
    }
    agents.validate_binding(roots)?;
    instances
        .validate_binding(roots)
        .map_err(AgentInstanceProjectionErrorV1::Body)?;
    let count = usize::try_from(
        agents
            .agent_count()
            .checked_add(instances.instance_count())
            .ok_or_else(limit)?,
    )
    .map_err(|_| limit())?;
    let p = Processing {
        bank: std::cell::RefCell::new(processing),
    };
    spend(
        &p,
        count
            .checked_mul(size_of::<Target>() + size_of::<Row>())
            .ok_or_else(limit)?,
        &mut ctx.cancelled,
    )?;
    let slots = memory
        .reserve(
            count
                .checked_mul(size_of::<Target>() + size_of::<Row>())
                .ok_or_else(limit)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut targets = Vec::<Target>::new();
    targets.try_reserve_exact(count).map_err(|_| limit())?;
    if targets.capacity() > count {
        return Err(limit().into());
    }
    let mut reader = Reader {
        ctx,
        p: &p,
        fee: memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?,
    };
    let mut counts = [0u64; 2];
    let source = crate::incremental_world_rule_projection_delta::ProcessingState {
        source: parent.state,
        processing: &p,
        first: std::cell::RefCell::new(None),
    };
    let _scan_phase = memory
        .reserve(
            size_of::<crate::incremental_world_rule_projection_delta::ProcessingState<'_, '_, P>>()
                + 4096,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    // Only original identity scalar rows choose entities. Qualification proves
    // all other scalar/body/mailbox rows belong to those exact entities.
    let result = StateScanContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: scan,
    }
    .visit_entries(
        roots.state_root(Ns::Business),
        |entry| -> Result<(), AgentInstanceProjectionErrorV1<O::Error>> {
            spend(
                &p,
                entry
                    .key
                    .len()
                    .checked_mul(16)
                    .and_then(|n| n.checked_add(256))
                    .ok_or_else(limit)?,
                &mut reader.ctx.cancelled,
            )?;
            let selected = if entry.key.starts_with(AGENT_FIELD_KEY_DOMAIN_V1) {
                let (owner, field, ordinal) =
                    AgentFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if field != F::Identity {
                    return Ok(());
                }
                if ordinal != 0 {
                    return Err(bad().into());
                }
                let raw = reader.raw(&entry.value)?;
                let AgentScalarRefV1::Text(id) =
                    AgentScalarRefV1::from_canonical_bytes(&raw, F::Identity)
                        .map_err(ValueTreeError::Invalid)?
                else {
                    return Err(bad().into());
                };
                if AgentFieldKeyV1::new(id, F::Identity, 0)
                    .map_err(ValueTreeError::Invalid)?
                    .as_bytes()
                    != entry.key
                    || AgentFieldKeyV1::from_owner_digest(owner, F::Identity, 0)
                        .map_err(ValueTreeError::Invalid)?
                        .as_bytes()
                        != entry.key
                {
                    return Err(bad().into());
                }
                Some((Kind::Agent, reader.own(id)?))
            } else if ModuleInstanceKeyV1::is_instance_key(&entry.key) {
                let (id, field) =
                    ModuleInstanceKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if field != I::ALL[0] {
                    return Ok(());
                }
                Some((Kind::Instance, reader.own(id)?))
            } else {
                None
            };
            if let Some((kind, key)) = selected {
                if targets.len() == count {
                    return Err(bad().into());
                }
                let slot = if kind == Kind::Agent { 0 } else { 1 };
                counts[slot] = counts[slot].checked_add(1).ok_or_else(limit)?;
                targets.push(Target { kind, key });
            }
            Ok(())
        },
        || (parent.cancelled)(),
    );
    if let Some(error) = (parent.take_state_error)() {
        return Err(error.into());
    }
    if let Some(error) = source.first.borrow_mut().take() {
        return Err(error.into());
    }
    match result {
        Ok(_) => {}
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e).into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
        Err(StateScanError::ResourceLimited) => return Err(limit().into()),
        _ => return Err(bad().into()),
    }
    if counts != [agents.agent_count(), instances.instance_count()] {
        return Err(bad().into());
    }
    // Original BTree String order, independent of hashed physical key order.
    for i in 1..targets.len() {
        let mut j = i;
        while j > 0 {
            spend(
                &p,
                targets[j - 1]
                    .key
                    .len()
                    .checked_add(targets[j].key.len() + 2 * size_of::<Target>())
                    .ok_or_else(limit)?,
                &mut reader.ctx.cancelled,
            )?;
            let a = if targets[j - 1].kind == Kind::Agent {
                0
            } else {
                1
            };
            let b = if targets[j].kind == Kind::Agent { 0 } else { 1 };
            if (a, &targets[j - 1].key) <= (b, &targets[j].key) {
                break;
            }
            targets.swap(j - 1, j);
            j -= 1
        }
    }
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(|_| limit())?;
    if rows.capacity() > count {
        return Err(limit().into());
    }
    for target in &targets {
        let semantic = semantic(&target.key, &memory, &p, &mut reader.ctx.cancelled)?;
        let key = json(&target.key, &memory, &p, &mut reader.ctx.cancelled)?;
        let (field, value, electricity, intent) = if target.kind == Kind::Agent {
            let actual = reader
                .agent(&target.key, objects, originals, decode)?
                .ok_or_else(bad)?;
            let electricity = actual.state.resources.get(ResourceKind::Electricity);
            (
                "agents",
                json(&actual, &memory, &p, &mut reader.ctx.cancelled)?,
                Some(electricity),
                Some(json(
                    &actual.intent,
                    &memory,
                    &p,
                    &mut reader.ctx.cancelled,
                )?),
            )
        } else {
            let actual = reader.instance(&target.key)?.ok_or_else(bad)?;
            (
                "module_instances",
                json(&actual, &memory, &p, &mut reader.ctx.cancelled)?,
                None,
                None,
            )
        };
        rows.push(Row {
            field,
            semantic,
            key,
            value: Some(value),
            electricity,
            intent,
        });
    }
    drop(reader);
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    ctx.transaction
        .validate_bootstrap_read_only_v1(live)
        .map_err(WorldFieldOverlayErrorV1::from)?;
    let fee = memory
        .reserve(
            size_of::<QualifiedAgentInstanceProjectionColumnsV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(QualifiedAgentInstanceProjectionColumnsV1 {
        counts,
        rows,
        guard: ctx.transaction.guard(),
        roots: digest,
        memory,
        _slots: slots,
        _fee: fee,
    })
}
