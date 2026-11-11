use super::*;
use std::cell::RefCell;
pub fn qualify_world_rule_remainder_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleRemainderWorkV1,
) -> Result<ReservedValue<WorldRuleRemainderQualificationV1>, ValueTreeError> {
    let result = (|| {
        ctx.check()?;
        let memory = ctx.values.memory;
        let class = ctx.values.class;
        let _phase = memory.reserve(8192, class)?;
        if ctx.root != roots.state_root(StateNamespaceV1::Authorization)
            || ctx.state.allocation_class() != Some(class)
            || ctx
                .state
                .allocation_budget()
                .is_none_or(|m| !m.same_pool(memory))
        {
            return Err(invalid());
        }
        roots.validate().map_err(|_| invalid())?;
        let charge = memory.reserve(512, class)?;
        let (_, present) = read_parent_fixed(ctx)?;
        let mut counts = [0u64; 3];
        {
            let stop = RefCell::new(&mut ctx.cancelled);
            let mut local = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Authorization),
                state: ctx.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: ctx.values.source,
                    memory,
                    class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: || stop.borrow_mut()(),
            };
            for scanned in &roots.state {
                let result = StateScanContext {
                    source: local.state,
                    memory,
                    class,
                    io: &mut *scan,
                }
                .visit_entries(
                    scanned,
                    |entry| {
                        local.check()?;
                        let Some(key) = key(&entry.key)? else {
                            return Ok(());
                        };
                        if entry.namespace != StateNamespaceV1::Authorization {
                            return Err(invalid());
                        }
                        if let Some(i) =
                            inspect(&key, &entry.value, present, true, &mut local, work)?
                        {
                            counts[i] = counts[i].checked_add(1).ok_or_else(invalid)?;
                        }
                        local.check()
                    },
                    || stop.borrow_mut()(),
                );
                local.check()?;
                match result {
                    Ok(_) => (),
                    Err(StateScanError::Visitor(e)) => return Err(e),
                    Err(StateScanError::Memory(e)) => return Err(e.into()),
                    Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
                    Err(StateScanError::ResourceLimited) => {
                        return Err(ValueTreeError::ResourceLimited("rule remainder scan"));
                    }
                    Err(_) => return Err(invalid()),
                }
            }
        }
        if !present && counts != [0; 3] {
            return Err(invalid());
        }
        ctx.check()?;
        let roots = roots.digest().map_err(|_| invalid())?;
        ctx.check()?;
        Ok(charge.retain(WorldRuleRemainderQualificationV1 {
            roots,
            class,
            present,
            counts,
        }))
    })();
    ctx.check()?;
    result
}
pub fn readmit_world_rule_remainder_ordinary_v1(
    source: &ReservedValue<WorldRuleRemainderQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldRuleRemainderQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if source.class != MemoryClass::Recovery
        || !source.uses_budget(memory)
        || source.reserved_bytes() < 512
    {
        return Err(invalid());
    }
    let _phase = memory.reserve(4096, MemoryClass::Ordinary)?;
    roots.validate().map_err(|_| invalid())?;
    source.validate_binding(roots)?;
    let charge = memory.reserve(512, MemoryClass::Ordinary)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(WorldRuleRemainderQualificationV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
        present: source.present,
        counts: source.counts,
    }))
}
