//! Original checked industrial maintenance expansion over actual profile rows.
//! This deliberately differs from the saturating ordinary recipe helper.
use super::{invalid, readonly_owner::IndustrialOwnerReadonlyFactsV1};
use crate::{WorldError, WorldState};
use oasis7_wasm_abi::{MaterialStack, ProductProfileV1, RecipeExecutionPlan};
use std::{collections::BTreeMap, convert::Infallible, mem::size_of};
pub trait IndustrialProductReadonlyFactsV1: IndustrialOwnerReadonlyFactsV1 {
    fn industrial_with_product<T>(
        &self,
        kind: &str,
        visit: impl FnOnce(Option<&ProductProfileV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
}
fn reject<S: IndustrialOwnerReadonlyFactsV1, T>(
    s: &S,
    reason: &str,
) -> Result<Result<T, WorldError>, S::Error> {
    s.owner_work(1, reason.len())?;
    s.owner_before_owned(reason.len() + size_of::<WorldError>())?;
    Ok(Err(invalid(reason)))
}
fn add<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    quantities: &mut BTreeMap<String, i64>,
    kind: &str,
    amount: i64,
) -> Result<Result<(), WorldError>, S::Error> {
    let levels = (quantities.len() + 1).ilog2() as usize + 2;
    s.owner_work(
        1,
        (kind.len() + 1).saturating_mul(levels * 2 + 4) + 512 * levels,
    )?;
    s.owner_before_owned(kind.len() + size_of::<String>() + 512 * levels)?;
    let entry = quantities.entry(kind.to_owned()).or_default();
    let Some(value) = entry.checked_add(amount) else {
        return reject(s, "industrial_plan_overflow");
    };
    *entry = value;
    Ok(Ok(()))
}
pub fn industrial_effective_consume_readonly_v1<S: IndustrialProductReadonlyFactsV1>(
    s: &S,
    plan: &RecipeExecutionPlan,
) -> Result<Result<Vec<MaterialStack>, WorldError>, S::Error> {
    let mut quantities = BTreeMap::new();
    for stack in &plan.consume {
        s.owner_work(1, 0)?;
        if stack.amount <= 0 {
            return reject(s, "industrial_plan_quantity_invalid");
        }
        if let Err(error) = add(s, &mut quantities, &stack.kind, stack.amount)? {
            return Ok(Err(error));
        }
    }
    for output in &plan.produce {
        s.owner_work(1, 0)?;
        if output.amount <= 0 {
            return reject(s, "industrial_plan_quantity_invalid");
        }
        let result = s.industrial_with_product(&output.kind, |profile| {
            if let Some(profile) = profile {
                for sink in &profile.maintenance_sink {
                    s.owner_work(1, 0)?;
                    if sink.amount < 0 {
                        return reject(s, "industrial_maintenance_invalid");
                    }
                    let Some(amount) = sink.amount.checked_mul(output.amount) else {
                        return reject(s, "industrial_plan_overflow");
                    };
                    if let Err(error) = add(s, &mut quantities, &sink.kind, amount)? {
                        return Ok(Err(error));
                    }
                }
            }
            Ok(Ok(()))
        })?;
        if let Err(error) = result {
            return Ok(Err(error));
        }
    }
    let mut count = 0usize;
    for amount in quantities.values() {
        s.owner_work(1, 0)?;
        if *amount > 0 {
            count += 1;
        }
    }
    s.owner_before_owned(count.saturating_mul(size_of::<MaterialStack>()))?;
    let mut out = Vec::with_capacity(count);
    assert_eq!(out.capacity(), count, "industrial consume exact capacity");
    for (kind, amount) in quantities {
        s.owner_work(1, size_of::<MaterialStack>())?;
        if amount > 0 {
            out.push(MaterialStack::new(kind, amount));
        }
    }
    Ok(Ok(out))
}
impl IndustrialProductReadonlyFactsV1 for WorldState {
    fn industrial_with_product<T>(
        &self,
        kind: &str,
        visit: impl FnOnce(Option<&ProductProfileV1>) -> Result<T, Infallible>,
    ) -> Result<T, Infallible> {
        visit(self.product_profiles.get(kind))
    }
}
