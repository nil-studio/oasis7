//! One exclusive Common loan for all legacy GEN/cold tail-directory readers.
//! Only real consumption is charged; no admitted balance becomes a new budget.
use super::*;
use crate::world::incremental_factory_phase::io::FactoryPhaseMeterLaneV1;

const MAX_LANES: usize = 96;

struct Snapshot {
    values: [u64; MAX_LANES],
    bytes: [bool; MAX_LANES],
    len: usize,
}

trait Lanes {
    fn visit_lanes(&mut self, visit: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>));
}

impl<M, R, E, C> Lanes for Loan<'_, '_, '_, '_, '_, '_, M, R, E, C> {
    fn visit_lanes(&mut self, visitor: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        visit(self, visitor);
    }
}

fn visit<M, R, E, C>(
    loan: &mut Loan<'_, '_, '_, '_, '_, '_, M, R, E, C>,
    visit: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>),
) {
    use FactoryPhaseMeterLaneV1::{Bytes as B, Entries as N, Steps as S};
    fn io(
        io: &mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
        visit: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>),
    ) {
        visit(S(&mut io.nodes_remaining));
        visit(B(&mut io.read_bytes_remaining));
        visit(B(&mut io.write_bytes_remaining));
    }
    fn scan(
        scan: &mut StateScanBudget,
        visit: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>),
    ) {
        visit(S(&mut scan.nodes_remaining));
        visit(S(&mut scan.leaves_remaining));
        visit(B(&mut scan.read_bytes_remaining));
    }
    io(loan.read.values.io, visit);
    visit(B(&mut loan.read.key_work.bytes_remaining));
    io(&mut loan.work.factory_read.io, visit);
    visit(B(&mut loan.work.factory_read.keys.bytes_remaining));
    io(&mut loan.work.recipe_read.io, visit);
    visit(B(&mut loan.work.recipe_read.keys.bytes_remaining));
    scan(loan.work.factory_scan, visit);
    scan(loan.work.recipe_scan, visit);
    scan(loan.work.outer_scan, visit);
    scan(loan.work.inner_scan, visit);
    visit(S(&mut loan.factory.cells_remaining));
    scan(&mut loan.factory.scan, visit);
    visit(S(&mut loan.industry.cells_remaining));
    scan(&mut loan.industry.scan, visit);
    visit(B(&mut loan.work.recipes.comparisons_remaining));
    visit(N(&mut loan.work.recipes.pending_entries_remaining));
    visit(S(&mut loan.work.native_read.json.cells_remaining));
    visit(S(&mut loan.work.native_read.json.json_nodes_remaining));
    visit(S(&mut loan.work.claims.visits_remaining));
    visit(B(&mut loan.work.claims.comparison_bytes_remaining));
    visit(N(&mut loan.work.claims.pending_entries_remaining));
    visit(S(&mut loan.work.gameplay.visits_remaining));
    visit(B(&mut loan.work.gameplay.comparison_bytes_remaining));
    visit(N(&mut loan.work.gameplay.pending_entries_remaining));
    visit(S(loan.work.grant_cells));
    visit(S(&mut loan.work.threats.visits_remaining));
    visit(B(&mut loan.work.threats.comparison_bytes_remaining));
    visit(N(&mut loan.work.threats.pending_entries_remaining));
}

fn snapshot<L: Lanes>(loan: &mut L) -> Result<Snapshot, BodyMutationError> {
    let mut state = Snapshot {
        values: [0; MAX_LANES],
        bytes: [false; MAX_LANES],
        len: 0,
    };
    let mut failed = false;
    loan.visit_lanes(&mut |lane| {
        let (value, bytes) = match lane {
            FactoryPhaseMeterLaneV1::Steps(n) => (*n, false),
            FactoryPhaseMeterLaneV1::Bytes(n) => (*n, true),
            FactoryPhaseMeterLaneV1::Entries(n) => match u64::try_from(*n) {
                Ok(n) => (n, false),
                Err(_) => {
                    failed = true;
                    return;
                }
            },
        };
        if state.len == MAX_LANES {
            failed = true;
            return;
        }
        state.values[state.len] = value;
        state.bytes[state.len] = bytes;
        state.len += 1;
    });
    if failed || state.len == 0 {
        return Err(bad());
    }
    Ok(state)
}

fn set<L: Lanes>(loan: &mut L, state: &Snapshot) -> Result<(), BodyMutationError> {
    let mut at = 0usize;
    let mut failed = false;
    loan.visit_lanes(&mut |lane| {
        if at >= state.len {
            failed = true;
            return;
        }
        let value = state.values[at];
        match lane {
            FactoryPhaseMeterLaneV1::Steps(n) if !state.bytes[at] => *n = value,
            FactoryPhaseMeterLaneV1::Bytes(n) if state.bytes[at] => *n = value,
            FactoryPhaseMeterLaneV1::Entries(n) if !state.bytes[at] => {
                match usize::try_from(value) {
                    Ok(value) => *n = value,
                    Err(_) => failed = true,
                }
            }
            _ => failed = true,
        }
        at += 1;
    });
    if failed || at != state.len {
        return Err(bad());
    }
    Ok(())
}

pub(super) fn run<M, R, E, C, T>(
    loan: &mut Loan<'_, '_, '_, '_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        &mut Loan<'_, '_, '_, '_, '_, '_, M, R, E, C>,
        &mut dyn FnMut() -> bool,
    ) -> Result<T, BodyMutationError>,
) -> Result<T, BodyMutationError> {
    run_lanes(loan, common, run)
}

fn run_lanes<L: Lanes, T>(
    loan: &mut L,
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(&mut L, &mut dyn FnMut() -> bool) -> Result<T, BodyMutationError>,
) -> Result<T, BodyMutationError> {
    let layout = 3 * std::mem::size_of::<Snapshot>();
    common.charge(1, layout as u64).map_err(formal)?;
    let _layout = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut original = snapshot(loan)?;
    let mut admitted = snapshot(loan)?;
    let byte_lanes = original.bytes[..original.len]
        .iter()
        .filter(|bytes| **bytes)
        .count();
    let step_lanes = original.len - byte_lanes;
    let (_, work, stop) = common.parts();
    let child = work.begin_child().map_err(formal)?;
    let step_cap = child.remaining_steps() / step_lanes.max(1) as u64;
    let byte_cap = child.remaining_read_bytes() / byte_lanes.max(1) as u64;
    for at in 0..admitted.len {
        admitted.values[at] = admitted.values[at].min(if admitted.bytes[at] {
            byte_cap
        } else {
            step_cap
        });
    }
    set(loan, &admitted)?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(loan, stop)));
    // Snapshot failure must not replace a producer panic or its first error.
    // The exclusive child is abandoned in that case, so the original Common
    // cannot be reused after an unaccounted reader.
    let after = snapshot(loan);
    let accounting = (|| {
        let after = after?;
        if admitted.len != after.len || admitted.bytes != after.bytes {
            return Err(bad());
        }
        let mut steps = 0u64;
        let mut bytes = 0u64;
        for at in 0..admitted.len {
            let consumed = admitted.values[at]
                .checked_sub(after.values[at])
                .ok_or_else(bad)?;
            let total = if admitted.bytes[at] {
                &mut bytes
            } else {
                &mut steps
            };
            *total = total.checked_add(consumed).ok_or_else(bad)?;
            original.values[at] = original.values[at].checked_sub(consumed).ok_or_else(bad)?;
        }
        // Restore only the portion that was never admitted to the child. A
        // rejected or cancelled reader keeps every attempted debit.
        set(loan, &original)?;
        child.consume(steps, bytes).map_err(formal)
    })();
    match result {
        Err(payload) => {
            // Abandonment burns the original Common balance, including when
            // the child happened to reconcile its scalar deltas successfully.
            drop(child);
            std::panic::resume_unwind(payload)
        }
        Ok(result) => {
            if accounting.is_ok() {
                child.finish();
            } else {
                drop(child);
            }
            let value = result?;
            accounting?;
            common.consume_steps(0).map_err(formal)?;
            Ok(value)
        }
    }
}

#[cfg(test)]
#[path = "incremental_normal_host_resident_tail_meter_tests.rs"]
mod tests;
