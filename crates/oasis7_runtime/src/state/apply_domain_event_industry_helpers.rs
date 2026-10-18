use super::*;

#[cfg(test)]
const SETTLED_INDUSTRY_HISTORY_LIMIT: usize = 64;

#[cfg(test)]
impl WorldState {
    fn is_current_factory_failure_disposition(&self, job_id: ActionId) -> bool {
        let Some(disposition) = self.factory_production_failure_dispositions.get(&job_id) else {
            return false;
        };
        let Some(factory) = self.factories.get(&disposition.factory_id) else {
            return false;
        };
        let production = &factory.production;
        production.current_job_id.is_none()
            && production.current_recipe_id.is_none()
            && production
                .current_blocker_action_id
                .is_none_or(|id| id == job_id)
            && production.current_blocker_kind.as_deref() == Some(&disposition.blocker_kind)
            && production.current_blocker_detail.as_deref() == Some(&disposition.blocker_detail)
            && !self.pending_recipe_jobs.contains_key(&job_id)
    }

    pub(super) fn compact_settled_industry_history(&mut self) {
        let mut protected = BTreeSet::new();
        let mut ordered = BTreeSet::new();
        for job_id in self
            .product_validation_attempts
            .keys()
            .chain(self.product_validation_receipts.keys())
            .chain(self.recipe_completion_receipts.keys())
            .chain(self.factory_production_failure_dispositions.keys())
        {
            if self.settled_recipe_job_ids.contains(job_id)
                && !self.pending_recipe_jobs.contains_key(job_id)
            {
                if self.is_current_factory_failure_disposition(*job_id) {
                    protected.insert(*job_id);
                } else if let Some(order) = self.industry_settlement_orders.get(job_id) {
                    ordered.insert((*order, *job_id));
                }
            }
        }
        let excess = ordered
            .len()
            .saturating_sub(SETTLED_INDUSTRY_HISTORY_LIMIT.saturating_sub(protected.len()));
        for (_, job_id) in ordered.into_iter().take(excess) {
            self.product_validation_attempts.remove(&job_id);
            self.product_validation_receipts.remove(&job_id);
            self.recipe_completion_receipts.remove(&job_id);
            self.factory_production_failure_dispositions.remove(&job_id);
            self.industry_settlement_orders.remove(&job_id);
        }
    }
}

pub(super) fn validate_recipe_material_stacks(
    label: &str,
    stacks: &[MaterialStack],
) -> Result<(), String> {
    for stack in stacks {
        if stack.kind.trim().is_empty() || stack.amount <= 0 {
            return Err(format!(
                "{label} stack must have a non-empty kind and amount > 0"
            ));
        }
    }
    Ok(())
}

pub(super) fn aggregate_recipe_material_stacks(
    stacks: &[MaterialStack],
) -> Result<BTreeMap<String, i64>, String> {
    let mut totals: BTreeMap<String, i64> = BTreeMap::new();
    for stack in stacks {
        let entry = totals.entry(stack.kind.clone()).or_insert(0);
        *entry = entry
            .checked_add(stack.amount)
            .ok_or_else(|| format!("material amount overflow for kind={}", stack.kind))?;
    }
    Ok(totals)
}

pub(super) fn validate_recipe_output_capacity(
    ledgers: &BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    ledger: &MaterialLedgerId,
    produce: &[MaterialStack],
    byproducts: &[MaterialStack],
) -> Result<(), String> {
    validate_recipe_material_stacks("produce", produce)?;
    validate_recipe_material_stacks("byproduct", byproducts)?;

    let mut totals = aggregate_recipe_material_stacks(produce)?;
    for (kind, amount) in aggregate_recipe_material_stacks(byproducts)? {
        let entry = totals.entry(kind).or_insert(0);
        *entry = entry
            .checked_add(amount)
            .ok_or_else(|| "combined output amount overflow".to_string())?;
    }

    let current_balances = ledgers.get(ledger);
    for (kind, amount) in totals {
        let current = current_balances
            .and_then(|balances| balances.get(&kind))
            .copied()
            .unwrap_or(0);
        if current < 0 {
            return Err(format!(
                "negative existing material balance: ledger={ledger} kind={kind} balance={current}"
            ));
        }
        current.checked_add(amount).ok_or_else(|| {
            format!("material output balance overflow: ledger={ledger} kind={kind}")
        })?;
    }
    Ok(())
}
