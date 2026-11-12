//! Two-phase original material preflight followed by typed Current CAS.
use super::*;
use oasis7_world_state::state::support::{
    checked_material_addition_total_v1, checked_material_balance_add_v1,
    checked_material_balance_remove_v1,
};
#[derive(Debug)]
struct StateError {
    cause: crate::WorldError,
    _fee: MemoryReservation,
}
impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.cause)
    }
}
impl std::error::Error for StateError {}
fn error_fee(memory: &MemoryBudget, n: usize) -> Result<MemoryReservation, BodyMutationError> {
    Ok(memory
        .reserve(
            n.checked_add(512)
                .and_then(|n| n.checked_mul(4))
                .and_then(|n| n.checked_add(std::mem::size_of::<StateError>()))
                .ok_or(ValueTreeError::ResourceLimited("factory error bound"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?)
}
pub(super) fn invalid_state(
    memory: &MemoryBudget,
    reason: &str,
) -> Result<BodyMutationError, BodyMutationError> {
    let fee = error_fee(memory, reason.len())?;
    Ok(BodyMutationError::Original(Box::new(StateError {
        cause: crate::WorldError::ResourceBalanceInvalid {
            reason: reason.to_owned(),
        },
        _fee: fee,
    })))
}
fn scalar(
    memory: &MemoryBudget,
    kind: &str,
    current: i64,
    amount: i64,
    prefix: &str,
    operation: fn(&str, i64, i64) -> Result<i64, String>,
) -> Result<i64, BodyMutationError> {
    let fee = error_fee(
        memory,
        kind.len()
            .checked_add(prefix.len())
            .ok_or(ValueTreeError::ResourceLimited(
                "factory material error text",
            ))?,
    )?;
    match operation(kind, current, amount) {
        Ok(next) => Ok(next),
        Err(reason) => Err(BodyMutationError::Original(Box::new(StateError {
            cause: crate::WorldError::ResourceBalanceInvalid {
                reason: format!("{prefix}{reason}"),
            },
            _fee: fee,
        }))),
    }
}
struct Change<'a> {
    kind: &'a str,
    before: i64,
    after: i64,
}
pub(in crate::world) struct Plan<'a> {
    changes: Vec<Change<'a>>,
    _fee: MemoryReservation,
}
fn quantity(
    scope: &CurrentInventoryMaterialScopeV1,
    kind: &str,
    canonical: bool,
    work: &mut FactoryLifecycleWorkV1,
) -> Result<LedgerQuantityV1, BodyMutationError> {
    let world = scope.world_materials().len() != 0;
    let mut found = None;
    if canonical && !world {
        for entry in scope.legacy_materials() {
            work.spend(entry.material().len().checked_add(kind.len()).ok_or(
                ValueTreeError::ResourceLimited("factory material comparison"),
            )?)?;
            if entry.material() == kind {
                found = Some(entry.quantity());
                break;
            }
        }
    } else {
        for entry in scope.world_materials() {
            work.spend(entry.material().len().checked_add(kind.len()).ok_or(
                ValueTreeError::ResourceLimited("factory material comparison"),
            )?)?;
            if entry.material() == kind {
                found = Some(entry.quantity());
                break;
            }
        }
    }
    Ok(match found {
        Some(n) => LedgerQuantityV1::Value(n),
        None if !canonical && !scope.world_ledger_present() => LedgerQuantityV1::LedgerAbsent,
        None => LedgerQuantityV1::MaterialAbsent,
    })
}
#[allow(clippy::too_many_arguments)]
fn scope<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentInventoryMaterialScopeV1>, BodyMutationError> {
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
        cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
    };
    let guard = ctx.transaction.guard();
    open_current_inventory_material_scope_v1(
        materials.parent,
        materials.roots,
        materials.pages,
        &mut ctx,
        materials.family_work,
        materials.scope_work,
        &guard,
        live,
    )
    .map_err(overlay_error)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare<
    'a,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    ledger: &MaterialLedgerId,
    stacks: &'a [MaterialStack],
    remove: bool,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
    live: &ExecutionFence,
) -> Result<Plan<'a>, BodyMutationError> {
    prepare_with_prefixes_v1(
        step,
        materials,
        ledger,
        stacks,
        remove,
        read,
        work,
        live,
        "factory recycle material preflight failed: ",
        "factory maintenance consume failed: ",
        "factory recycle material add failed: ",
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_with_prefixes_v1<
    'a,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    ledger: &MaterialLedgerId,
    stacks: &'a [MaterialStack],
    remove: bool,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
    live: &ExecutionFence,
    preflight_prefix: &str,
    remove_prefix: &str,
    add_prefix: &str,
) -> Result<Plan<'a>, BodyMutationError> {
    let n = stacks.len();
    if n > work.max_material_entries {
        return Err(ValueTreeError::ResourceLimited("factory material plan count").into());
    }
    let bytes = n
        .checked_mul(std::mem::size_of::<Change<'a>>() + std::mem::size_of::<(&str, i64)>())
        .ok_or(ValueTreeError::ResourceLimited(
            "factory material plan slots",
        ))?;
    let fee = step
        .mutation
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut aggregate: Vec<(&str, i64)> = Vec::with_capacity(n);
    for stack in stacks {
        if (read.cancelled)() || (step.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut position = None;
        for (i, (kind, _)) in aggregate.iter().enumerate() {
            work.spend(
                kind.len()
                    .checked_add(stack.kind.len())
                    .ok_or(ValueTreeError::ResourceLimited("factory aggregation bytes"))?,
            )?;
            if *kind == stack.kind {
                position = Some(i);
                break;
            }
        }
        let total = position.map_or(0, |i| aggregate[i].1);
        let next = scalar(
            step.mutation.memory,
            &stack.kind,
            total,
            stack.amount,
            preflight_prefix,
            checked_material_addition_total_v1,
        )?;
        if let Some(i) = position {
            aggregate[i].1 = next
        } else {
            aggregate.push((&stack.kind, next));
        }
    }
    // Fallible heapsort preserves original BTreeMap lexical preflight ordering.
    fn down(
        a: &mut [(&str, i64)],
        mut i: usize,
        end: usize,
        w: &mut FactoryLifecycleWorkV1,
    ) -> Result<(), BodyMutationError> {
        loop {
            w.spend(0)?;
            let left = i
                .checked_mul(2)
                .and_then(|n| n.checked_add(1))
                .ok_or(ValueTreeError::ResourceLimited("factory sort index"))?;
            if left >= end {
                return Ok(());
            }
            let mut child = left;
            if left + 1 < end {
                w.spend(
                    a[left]
                        .0
                        .len()
                        .checked_add(a[left + 1].0.len())
                        .ok_or(ValueTreeError::ResourceLimited("factory sort bytes"))?,
                )?;
                if a[left].0 < a[left + 1].0 {
                    child += 1;
                }
            }
            w.spend(
                a[i].0
                    .len()
                    .checked_add(a[child].0.len())
                    .ok_or(ValueTreeError::ResourceLimited("factory sort bytes"))?,
            )?;
            if a[i].0 >= a[child].0 {
                return Ok(());
            }
            a.swap(i, child);
            i = child;
        }
    }
    let len = aggregate.len();
    for i in (0..len / 2).rev() {
        if (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        down(&mut aggregate, i, len, work)?;
    }
    for end in (1..len).rev() {
        if (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        aggregate.swap(0, end);
        down(&mut aggregate, 0, end, work)?;
    }
    let mut changes = Vec::with_capacity(n);
    if *ledger != MaterialLedgerId::World {
        for (kind, total) in &aggregate {
            let raw = read::amount(read::quantity(step, ledger, kind, read, work)?);
            if !remove {
                scalar(
                    step.mutation.memory,
                    kind,
                    raw,
                    *total,
                    preflight_prefix,
                    checked_material_balance_add_v1,
                )?;
            }
            changes.push(Change {
                kind,
                before: raw,
                after: raw,
            });
        }
    }
    let current = scope(step, materials, read, live)?;
    if *ledger == MaterialLedgerId::World {
        for (kind, total) in &aggregate {
            let raw = read::amount(quantity(&current, kind, false, work)?);
            if !remove {
                scalar(
                    step.mutation.memory,
                    kind,
                    raw,
                    *total,
                    preflight_prefix,
                    checked_material_balance_add_v1,
                )?;
            }
            let canonical = read::amount(quantity(&current, kind, true, work)?);
            changes.push(Change {
                kind,
                before: canonical,
                after: canonical,
            });
        }
    }
    // Publication add errors occur in original input order, after raw preflight.
    for stack in stacks {
        if (read.cancelled)() || (step.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut index = None;
        for (i, change) in changes.iter().enumerate() {
            work.spend(
                change
                    .kind
                    .len()
                    .checked_add(stack.kind.len())
                    .ok_or(ValueTreeError::ResourceLimited("factory simulation bytes"))?,
            )?;
            if change.kind == stack.kind {
                index = Some(i);
                break;
            }
        }
        let change = &mut changes[index.ok_or(BodyMutationError::Invalid(
            "factory material aggregate missing",
        ))?];
        change.after = scalar(
            step.mutation.memory,
            &stack.kind,
            change.after,
            stack.amount,
            if remove { remove_prefix } else { add_prefix },
            if remove {
                checked_material_balance_remove_v1
            } else {
                checked_material_balance_add_v1
            },
        )?;
    }
    read.check()?;
    Ok(Plan { changes, _fee: fee })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn apply<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
>(
    plan: &Plan<'_>,
    ledger: &MaterialLedgerId,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    let guard = step.transaction.guard();
    materials.contribute(step.transaction, step.values, step.mutation, &guard, live)?;
    for change in &plan.changes {
        work.spend(change.kind.len())?;
        let current = read::quantity(step, ledger, change.kind, read, work)?;
        if read::amount(current) != change.before {
            return Err(BodyMutationError::Invalid(
                "factory material Current simulation mismatch",
            ));
        }
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
        ctx.change(
            InventoryChangeRefV1::LedgerMaterial {
                ledger,
                material: change.kind,
                expected: current,
                after: (change.after != 0).then_some(change.after),
                create_ledger: matches!(current, LedgerQuantityV1::LedgerAbsent),
            },
            &mut write,
            &guard,
            live,
        )
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if *ledger == MaterialLedgerId::World {
            // Keep both maps coherent even when this removes the last World
            // member; a later Core normalization cannot resurrect old legacy.
            let legacy = ctx.material(change.kind).map_err(overlay_error)?;
            if legacy.unwrap_or(0) != change.before {
                return Err(BodyMutationError::Invalid(
                    "factory material legacy simulation mismatch",
                ));
            }
            let guard = ctx.transaction.guard();
            ctx.change(
                InventoryChangeRefV1::Material {
                    key: change.kind,
                    expected: legacy,
                    after: (change.after != 0).then_some(change.after),
                },
                &mut write,
                &guard,
                live,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        }
    }
    read.check()?;
    if (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
