//! Shared fee arithmetic. Root-backed storage and invocation authority remain
//! separate consumers; amounts preserve the original saturating calculation.
use crate::ModuleLimits;
use oasis7_wasm_abi::ModuleOutput;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ModuleRuntimeFeeAmountsV1 {
    pub compute: i64,
    pub electricity: i64,
}
fn units(bytes: u64) -> i64 {
    if bytes == 0 {
        return 0;
    }
    (bytes.saturating_add(1023) / 1024).min(i64::MAX as u64) as i64
}
pub(super) fn upper_bound(limits: &ModuleLimits, input_bytes: u64) -> ModuleRuntimeFeeAmountsV1 {
    ModuleRuntimeFeeAmountsV1 {
        compute: units(input_bytes)
            .saturating_add(units(limits.max_output_bytes))
            .saturating_add(i64::from(limits.max_effects).saturating_mul(2))
            .saturating_add(i64::from(limits.max_emits)),
        electricity: 1_i64
            .saturating_add(i64::from(limits.max_effects))
            .saturating_add(i64::from(limits.max_emits))
            .saturating_add(1),
    }
}
pub(super) fn actual(input_bytes: u64, output: &ModuleOutput) -> ModuleRuntimeFeeAmountsV1 {
    let effects = i64::try_from(output.effects.len()).unwrap_or(i64::MAX);
    let emits = i64::try_from(output.emits.len()).unwrap_or(i64::MAX);
    ModuleRuntimeFeeAmountsV1 {
        compute: units(input_bytes)
            .saturating_add(units(output.output_bytes))
            .saturating_add(effects.saturating_mul(2))
            .saturating_add(emits),
        electricity: 1_i64
            .saturating_add(effects)
            .saturating_add(emits)
            .saturating_add(i64::from(output.new_state.is_some())),
    }
}

use crate::{ModuleRuntimeChargeEvent, WorldError};
use oasis7_agent_api::{world_action::ResourceKind, world_resource::ResourceStock};
use std::collections::BTreeMap;
pub(super) fn validate_amounts(charge: &ModuleRuntimeChargeEvent) -> Result<(), WorldError> {
    if charge.compute_fee_amount < 0 || charge.electricity_fee_amount < 0 {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "module runtime fee must be >= 0: compute={} electricity={}",
                charge.compute_fee_amount, charge.electricity_fee_amount
            ),
        });
    }
    Ok(())
}
/// Original sequential stock removal and treasury saturation. The caller owns
/// the provisional stock; no partial result may be installed on error.
pub(super) fn prepare_balances(
    charge: &ModuleRuntimeChargeEvent,
    stock: &mut ResourceStock,
    mut treasury: impl FnMut(ResourceKind) -> Option<i64>,
) -> Result<BTreeMap<ResourceKind, i64>, WorldError> {
    validate_amounts(charge)?;
    let mut resources = BTreeMap::new();
    for (label, kind, amount) in [
        (
            "compute",
            charge.compute_fee_kind,
            charge.compute_fee_amount,
        ),
        (
            "electricity",
            charge.electricity_fee_kind,
            charge.electricity_fee_amount,
        ),
    ] {
        if amount > 0 {
            stock
                .remove(kind, amount)
                .map_err(|err| WorldError::ResourceBalanceInvalid {
                    reason: format!(
                        "module runtime {} fee debit failed: payer={} kind={:?} amount={} err={:?}",
                        label, charge.payer_agent_id, kind, amount, err
                    ),
                })?;
            let balance = resources
                .entry(kind)
                .or_insert_with(|| treasury(kind).unwrap_or(0));
            *balance = balance.saturating_add(amount);
        }
    }
    Ok(resources)
}
