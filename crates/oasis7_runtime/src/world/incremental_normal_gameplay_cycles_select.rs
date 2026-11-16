//! Original fallback work selection from the complete resident Current directory.
//! The list captures a single original stage cut; business consumers still
//! reopen each real Current target and execute the original fixed publisher.
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueTreeError};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_normal_gameplay_cycles_directory::*,
};
use std::sync::Arc;
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("gameplay fallback stage list")
}
pub(super) struct DueRowV1 {
    pub(super) outer: String,
    pub(super) inner: Option<String>,
    pub(super) severity: u32,
    _heap: Arc<ReservedValue<()>>,
}
pub(super) struct DueRowsV1 {
    rows: std::vec::IntoIter<DueRowV1>,
    _heap: Arc<ReservedValue<()>>,
}
impl DueRowsV1 {
    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }
}
impl Iterator for DueRowsV1 {
    type Item = DueRowV1;
    fn next(&mut self) -> Option<Self::Item> {
        self.rows.next()
    }
}
#[derive(Clone, Copy)]
pub(super) enum FallbackStageV1 {
    Contract,
    Governance,
    CrisisTimeout,
}
/// The original stable ordering uses inner contract IDs only for contract
/// expiry. Ties retain the outer BTree order. All other lists use outer keys.
#[allow(clippy::too_many_arguments)]
pub(super) fn select_due_rows<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    stage: FallbackStageV1,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<DueRowsV1, GameplayCycleErrorV1> {
    let guard = ctx.transaction.guard();
    let directory = open_current_gameplay_cycle_directory_v1(
        sources.directory,
        sources.diplomacy,
        sources.governance,
        sources.roots,
        ctx.transaction,
        parent,
        &mut work.selected_scan,
        &mut work.directory,
        &guard,
        live,
    )?;
    directory.validate(ctx.transaction, tail.memory(), live)?;
    let matching = |row: &GameplayDirectoryRowV1| match (&stage, row.selector()) {
        (
            FallbackStageV1::Contract,
            GameplaySelectorV1::Contract {
                due_status,
                atomic,
                expires_at,
                ..
            },
        ) => *due_status && *atomic && *expires_at <= now,
        (FallbackStageV1::Governance, GameplaySelectorV1::Proposal { open, closes_at }) => {
            *open && *closes_at <= now
        }
        (
            FallbackStageV1::CrisisTimeout,
            GameplaySelectorV1::Crisis {
                active, expires_at, ..
            },
        ) => *active && *expires_at <= now,
        _ => false,
    };
    let mut count = 0usize;
    let mut strings = 0usize;
    directory.visit::<GameplayCycleErrorV1>(
        &mut work.directory,
        &mut ctx.cancelled,
        |row, _| {
            tail.charge_record()?;
            if matching(row) {
                count = count.checked_add(1).ok_or_else(limited)?;
                strings = strings
                    .checked_add(row.outer_id().len())
                    .ok_or_else(limited)?;
                if let GameplaySelectorV1::Contract { inner_id, .. } = row.selector() {
                    strings = strings.checked_add(inner_id.len()).ok_or_else(limited)?;
                }
            }
            Ok(())
        },
    )?;
    let fee = tail.reserve(
        count
            .checked_mul(std::mem::size_of::<DueRowV1>())
            .and_then(|n| n.checked_add(strings))
            .and_then(|n| {
                n.checked_add(
                    std::mem::size_of::<ReservedValue<()>>()
                        + 2 * std::mem::size_of::<usize>()
                        + 2 * std::mem::align_of::<ReservedValue<()>>(),
                )
            })
            .ok_or_else(limited)?,
        MemoryClass::Ordinary,
    )?;
    let heap = Arc::new(fee.retain(()));
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(|_| limited())?;
    if rows.capacity() > count {
        return Err(limited().into());
    }
    directory.visit::<GameplayCycleErrorV1>(
        &mut work.directory,
        &mut ctx.cancelled,
        |row, _| {
            tail.charge(row.outer_id().len())?;
            if matching(row) {
                let (inner, severity) = match row.selector() {
                    GameplaySelectorV1::Contract { inner_id, .. } => {
                        tail.charge(inner_id.len())?;
                        (Some(inner_id.clone()), 0)
                    }
                    GameplaySelectorV1::Crisis { severity, .. } => (None, (*severity).max(1)),
                    _ => (None, 0),
                };
                rows.push(DueRowV1 {
                    outer: row.outer_id().to_owned(),
                    inner,
                    severity,
                    _heap: heap.clone(),
                });
            }
            Ok(())
        },
    )?;
    // Fallible in-place heap sort. Every comparison is charged/cancellable;
    // primitive swaps move only the already-paid String headers.
    fn compare(
        a: &DueRowV1,
        b: &DueRowV1,
        contract: bool,
        tail: &mut NormalTailWorkV1<'_, '_>,
    ) -> Result<std::cmp::Ordering, GameplayCycleErrorV1> {
        let (a0, b0) = if contract {
            (
                a.inner.as_deref().ok_or_else(limited)?,
                b.inner.as_deref().ok_or_else(limited)?,
            )
        } else {
            (a.outer.as_str(), b.outer.as_str())
        };
        tail.charge(a0.len().min(b0.len()))?;
        let order = a0.cmp(b0);
        if order == std::cmp::Ordering::Equal && contract {
            tail.charge(a.outer.len().min(b.outer.len()))?;
            Ok(a.outer.cmp(&b.outer))
        } else {
            Ok(order)
        }
    }
    fn sift(
        rows: &mut [DueRowV1],
        mut root: usize,
        end: usize,
        contract: bool,
        tail: &mut NormalTailWorkV1<'_, '_>,
    ) -> Result<(), GameplayCycleErrorV1> {
        loop {
            let left = root
                .checked_mul(2)
                .and_then(|n| n.checked_add(1))
                .ok_or_else(limited)?;
            if left >= end {
                return Ok(());
            }
            let mut child = left;
            if left + 1 < end
                && compare(&rows[left], &rows[left + 1], contract, tail)?
                    == std::cmp::Ordering::Less
            {
                child = left + 1;
            }
            if compare(&rows[root], &rows[child], contract, tail)? != std::cmp::Ordering::Less {
                return Ok(());
            }
            tail.charge(std::mem::size_of::<DueRowV1>() * 2)?;
            rows.swap(root, child);
            root = child;
        }
    }
    let contract = matches!(stage, FallbackStageV1::Contract);
    let n = rows.len();
    for i in (0..n / 2).rev() {
        sift(&mut rows, i, n, contract, tail)?;
    }
    for end in (1..n).rev() {
        tail.charge(std::mem::size_of::<DueRowV1>() * 2)?;
        rows.swap(0, end);
        sift(&mut rows, 0, end, contract, tail)?;
    }
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(DueRowsV1 {
        rows: rows.into_iter(),
        _heap: heap,
    })
}

pub(super) fn select_next_war_row_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    sources: &NormalGameplayCycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<(String, String)>>, GameplayCycleErrorV1> {
    let guard = ctx.transaction.guard();
    let directory = open_current_gameplay_cycle_directory_v1(
        sources.directory,
        sources.diplomacy,
        sources.governance,
        sources.roots,
        ctx.transaction,
        parent,
        &mut work.selected_scan,
        &mut work.directory,
        &guard,
        live,
    )?;
    directory.validate(ctx.transaction, tail.memory(), live)?;
    let mut selected: Option<ReservedValue<(String, String)>> = None;
    directory.visit::<GameplayCycleErrorV1>(
        &mut work.directory,
        &mut ctx.cancelled,
        |row, _| {
            tail.charge_record()?;
            let GameplaySelectorV1::War {
                inner_id,
                active,
                declared_at,
                max_duration_ticks,
            } = row.selector()
            else {
                return Ok(());
            };
            if !active || now < declared_at.saturating_add((*max_duration_ticks).max(1)) {
                return Ok(());
            }
            let replace = if let Some(old) = &selected {
                tail.charge(
                    inner_id.len().min(old.1.len()) + row.outer_id().len().min(old.0.len()),
                )?;
                inner_id
                    .cmp(&old.1)
                    .then_with(|| row.outer_id().cmp(&old.0))
                    == std::cmp::Ordering::Less
            } else {
                true
            };
            if replace {
                let bytes = row
                    .outer_id()
                    .len()
                    .checked_add(inner_id.len())
                    .ok_or_else(limited)?;
                tail.charge(bytes)?;
                let fee = tail.reserve(
                    bytes
                        .checked_add(std::mem::size_of::<(String, String)>())
                        .ok_or_else(limited)?,
                    MemoryClass::Ordinary,
                )?;
                selected = Some(fee.retain((row.outer_id().to_owned(), inner_id.clone())));
            }
            Ok(())
        },
    )?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(selected)
}

pub(super) fn has_active_crisis_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    sources: &NormalGameplayCycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<bool, GameplayCycleErrorV1> {
    let guard = ctx.transaction.guard();
    let directory = open_current_gameplay_cycle_directory_v1(
        sources.directory,
        sources.diplomacy,
        sources.governance,
        sources.roots,
        ctx.transaction,
        parent,
        &mut work.selected_scan,
        &mut work.directory,
        &guard,
        live,
    )?;
    directory.validate(ctx.transaction, tail.memory(), live)?;
    let mut active = false;
    directory.visit::<GameplayCycleErrorV1>(
        &mut work.directory,
        &mut ctx.cancelled,
        |row, _| {
            tail.charge_record()?;
            if matches!(
                row.selector(),
                GameplaySelectorV1::Crisis { active: true, .. }
            ) {
                active = true;
            }
            Ok(())
        },
    )?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(active)
}
