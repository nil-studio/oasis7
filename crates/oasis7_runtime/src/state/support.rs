use super::*;

pub(super) fn integer_sqrt_u64(value: u64) -> u64 {
    if value < 2 {
        return value;
    }
    let mut x0 = value;
    let mut x1 = (x0 + value / x0) / 2;
    while x1 < x0 {
        x0 = x1;
        x1 = (x0 + value / x0) / 2;
    }
    x0
}

pub(super) fn preflight_material_balance_additions(
    ledgers: &BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    ledger: &MaterialLedgerId,
    stacks: &[MaterialStack],
) -> Result<(), String> {
    let mut additions = BTreeMap::<String, i64>::new();
    for stack in stacks {
        if stack.amount < 0 {
            return Err(format!(
                "negative material amount not allowed: {}",
                stack.amount
            ));
        }
        let total = additions.entry(stack.kind.clone()).or_default();
        *total = total.checked_add(stack.amount).ok_or_else(|| {
            format!(
                "material balance addition overflow: kind={} amount={}",
                stack.kind, stack.amount
            )
        })?;
    }
    let balances = ledgers.get(ledger);
    for (kind, amount) in additions {
        let current = balances
            .and_then(|entries| entries.get(&kind))
            .copied()
            .unwrap_or(0);
        current.checked_add(amount).ok_or_else(|| {
            format!("material balance overflow: kind={kind} current={current} amount={amount}")
        })?;
    }
    Ok(())
}

pub(super) fn add_material_balance_for_ledger(
    ledgers: &mut BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    ledger: &MaterialLedgerId,
    kind: &str,
    amount: i64,
) -> Result<(), String> {
    let current = ledgers
        .get(ledger)
        .and_then(|balances| balances.get(kind))
        .copied()
        .unwrap_or(0);
    let next = current.checked_add(amount).ok_or_else(|| {
        format!("material balance overflow: kind={kind} current={current} amount={amount}")
    })?;
    let balances = ledgers.entry(ledger.clone()).or_default();
    if next == 0 {
        balances.remove(kind);
    } else {
        balances.insert(kind.to_string(), next);
    }
    Ok(())
}

fn remove_material_balance(
    balances: &mut BTreeMap<String, i64>,
    kind: &str,
    amount: i64,
) -> Result<(), String> {
    if amount < 0 {
        return Err(format!("negative material amount not allowed: {amount}"));
    }
    let current = balances.get(kind).copied().unwrap_or(0);
    if current < amount {
        return Err(format!(
            "insufficient material {kind}: requested={amount} available={current}"
        ));
    }
    let next = current - amount;
    if next == 0 {
        balances.remove(kind);
    } else {
        balances.insert(kind.to_string(), next);
    }
    Ok(())
}

pub(super) fn remove_material_balance_for_ledger(
    ledgers: &mut BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    ledger: &MaterialLedgerId,
    kind: &str,
    amount: i64,
) -> Result<(), String> {
    let balances = ledgers.entry(ledger.clone()).or_default();
    remove_material_balance(balances, kind, amount)
}

pub(super) fn sync_compat_world_materials(
    ledgers: &BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    compat_world_materials_cache: &mut BTreeMap<String, i64>,
) {
    let world_materials = ledgers
        .get(&MaterialLedgerId::world())
        .cloned()
        .unwrap_or_default();
    *compat_world_materials_cache = world_materials;
}

pub(crate) fn verify_reward_mint_record_signature_with_state(
    state: &WorldState,
    record: &NodeRewardMintRecord,
) -> Result<(), String> {
    let signer_public_key = state
        .node_identity_bindings
        .get(record.signer_node_id.as_str())
        .map(String::as_str)
        .ok_or_else(|| {
            format!(
                "reward mint signer identity is not bound: {}",
                record.signer_node_id
            )
        })?;
    if record
        .signature
        .starts_with(REWARD_MINT_SIGNATURE_V2_PREFIX)
    {
        return verify_reward_mint_signature_v2(
            record.signature.as_str(),
            record.epoch_index,
            record.node_id.as_str(),
            record.source_awarded_points,
            record.minted_power_credits,
            record.settlement_hash.as_str(),
            record.signer_node_id.as_str(),
            signer_public_key,
        );
    }
    if record
        .signature
        .starts_with(REWARD_MINT_SIGNATURE_V1_PREFIX)
    {
        if !state
            .reward_signature_governance_policy
            .allow_mintsig_v1_fallback
        {
            return Err("mintsig:v1 is disabled by governance policy".to_string());
        }
        let expected_signature = reward_mint_signature_v1(
            record.epoch_index,
            record.node_id.as_str(),
            record.source_awarded_points,
            record.minted_power_credits,
            record.settlement_hash.as_str(),
            record.signer_node_id.as_str(),
            signer_public_key,
        );
        if expected_signature != record.signature {
            return Err(format!(
                "reward mint signature mismatch for node {} at epoch {}",
                record.node_id, record.epoch_index
            ));
        }
        return Ok(());
    }
    Err(format!(
        "unsupported reward mint signature version for node {} at epoch {}",
        record.node_id, record.epoch_index
    ))
}

pub(crate) fn ensure_system_order_budget_caps_for_epoch(
    report: &EpochSettlementReport,
    budget: &mut SystemOrderPoolBudget,
) {
    if !budget.node_credit_caps.is_empty() {
        return;
    }
    if budget.total_credit_budget == 0 || report.settlements.is_empty() {
        return;
    }

    let total_awarded_points = report
        .settlements
        .iter()
        .map(|settlement| settlement.awarded_points)
        .sum::<u64>();
    if total_awarded_points == 0 {
        return;
    }

    let mut distributed = 0_u64;
    for settlement in &report.settlements {
        let cap = budget
            .total_credit_budget
            .saturating_mul(settlement.awarded_points)
            / total_awarded_points;
        distributed = distributed.saturating_add(cap);
        budget
            .node_credit_caps
            .insert(settlement.node_id.clone(), cap);
    }

    let mut remainder = budget.total_credit_budget.saturating_sub(distributed);
    if remainder == 0 {
        return;
    }
    let mut ranked = report
        .settlements
        .iter()
        .map(|settlement| (settlement.node_id.as_str(), settlement.awarded_points))
        .collect::<Vec<_>>();
    ranked.sort_by(|(a_node_id, a_points), (b_node_id, b_points)| {
        b_points
            .cmp(a_points)
            .then_with(|| a_node_id.cmp(b_node_id))
    });
    let mut index = 0_usize;
    while remainder > 0 && !ranked.is_empty() {
        let node_id = ranked[index % ranked.len()].0;
        if let Some(cap) = budget.node_credit_caps.get_mut(node_id) {
            *cap = cap.saturating_add(1);
            remainder -= 1;
        }
        index = index.saturating_add(1);
    }
}
