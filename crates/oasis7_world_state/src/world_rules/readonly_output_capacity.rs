//! The original per-native-material output runway predicate, with work-only
//! callbacks and a fallible borrowed pending reservation visitor.
use super::{industrial_work_start::IndustrialOutputReservationRefV1, invalid};
use crate::{MaterialLedgerId, WorldError};
use oasis7_agent_api::world_action::RootOperationV1;
use oasis7_wasm_abi::RecipeExecutionPlan;
use std::{collections::BTreeMap, mem::size_of, ops::ControlFlow};
fn reject<T, E>(
    reason: &str,
    work: &mut impl FnMut(u64, usize, usize) -> Result<(), E>,
) -> Result<Result<T, WorldError>, E> {
    work(1, reason.len(), reason.len() + size_of::<WorldError>())?;
    Ok(Err(invalid(reason)))
}
pub(super) fn capacity<E>(
    root: &RootOperationV1,
    plan: &RecipeExecutionPlan,
    exclude_job: Option<u64>,
    ledger: Option<&BTreeMap<String, i64>>,
    pending: &mut impl FnMut(
        &mut dyn for<'r> FnMut(IndustrialOutputReservationRefV1<'r>) -> Result<ControlFlow<()>, E>,
    ) -> Result<(), E>,
    work: &mut impl FnMut(u64, usize, usize) -> Result<(), E>,
) -> Result<Result<(), WorldError>, E> {
    let mut output = BTreeMap::<String, u64>::new();
    for stack in plan.produce.iter().chain(&plan.byproducts) {
        work(1, 0, 0)?;
        let Some(amount) = u64::try_from(stack.amount).ok().filter(|q| *q > 0) else {
            return reject("industrial_terminal_quantity_invalid", work);
        };
        let levels = (output.len() + 1).ilog2() as usize + 2;
        work(
            1,
            (stack.kind.len() + 1).saturating_mul(levels * 2 + 4) + 512 * levels,
            stack.kind.len() + size_of::<String>() + 512 * levels,
        )?;
        let entry = output.entry(stack.kind.clone()).or_default();
        let Some(next) = entry.checked_add(amount) else {
            return reject("industrial_terminal_quantity_overflow", work);
        };
        *entry = next;
    }
    if output.len() != root.terminal_capacities.len() {
        return reject("industrial_terminal_capacity_scope_mismatch", work);
    }
    for kind in output.keys() {
        work(
            1,
            (kind.len() + 1)
                .saturating_mul((root.terminal_capacities.len() + 1).ilog2() as usize * 2 + 4),
            0,
        )?;
        if !root.terminal_capacities.contains_key(kind) {
            return reject("industrial_terminal_capacity_scope_mismatch", work);
        }
    }
    for (kind, incoming) in output {
        work(1, kind.len(), 0)?;
        let limit = root.terminal_capacities[&kind];
        if limit == 0 {
            return reject("industrial_terminal_capacity_invalid", work);
        }
        work(
            1,
            (kind.len() + 1)
                .saturating_mul(ledger.map_or(1, |v| (v.len() + 1).ilog2() as usize * 2 + 4)),
            0,
        )?;
        let balance = ledger.and_then(|v| v.get(&kind)).copied().unwrap_or(0);
        let Ok(mut committed) = u64::try_from(balance) else {
            return reject("industrial_terminal_balance_invalid", work);
        };
        let mut refusal = None;
        pending(&mut |job| {
            let ledger_bytes = |v: &MaterialLedgerId| match v {
                MaterialLedgerId::World => 1,
                MaterialLedgerId::Agent(v)
                | MaterialLedgerId::Site(v)
                | MaterialLedgerId::Factory(v) => 1 + v.len(),
            };
            work(
                1,
                ledger_bytes(job.output_ledger) + ledger_bytes(&root.output_ledger),
                0,
            )?;
            if job.output_ledger == &root.output_ledger && Some(job.job_id) != exclude_job {
                for stack in job.produce.iter().chain(job.byproducts) {
                    work(1, stack.kind.len() + kind.len(), 0)?;
                    if stack.kind != kind {
                        continue;
                    }
                    let Some(amount) = u64::try_from(stack.amount).ok().filter(|q| *q > 0) else {
                        refusal = Some("industrial_terminal_quantity_invalid");
                        return Ok(ControlFlow::Break(()));
                    };
                    let Some(next) = committed.checked_add(amount) else {
                        refusal = Some("industrial_terminal_quantity_overflow");
                        return Ok(ControlFlow::Break(()));
                    };
                    committed = next;
                }
            }
            Ok(ControlFlow::Continue(()))
        })?;
        if let Some(reason) = refusal {
            return reject(reason, work);
        }
        if committed
            .checked_add(incoming)
            .is_none_or(|total| total > limit)
        {
            return reject("industrial_terminal_capacity_unavailable", work);
        }
    }
    Ok(Ok(()))
}
