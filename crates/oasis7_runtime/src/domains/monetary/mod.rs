//! Root-staged settlement of the original signed rule resource delta.
//!
//! The ordinary Runtime consumer and this stage-one adapter share the exact
//! checked arithmetic. This module does not authorize publication by itself:
//! only a root owner with a genuine durable confirmation may install a delta.

use std::collections::BTreeMap;

use oasis7_agent_api::world_action::ResourceKind;
use oasis7_domain_types::rule_settlement::{
    AuthoritativeDebit, RuleActionDisposition, RuleSettlementInput, RuleSettlementPolicy,
};

use crate::world::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedTransition, SubsystemDelta,
    TransitionContext, TransitionReject,
};
use crate::{ResourceDelta, WorldError};

/// Same per-key signed, checked arithmetic as the original live
/// World::apply_resource_delta path. All entries are planned before any write.
pub(crate) fn plan_resource_delta(
    balances: &BTreeMap<ResourceKind, i64>,
    delta: &ResourceDelta,
) -> Result<Vec<(ResourceKind, i64)>, WorldError> {
    let mut planned = Vec::with_capacity(delta.entries.len());
    for (kind, amount) in &delta.entries {
        let current = balances.get(kind).copied().unwrap_or(0);
        let next = current.checked_add(*amount).ok_or_else(|| {
            WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "resource balance overflow: kind={kind:?} current={current} delta={amount}"
                ),
            }
        })?;
        planned.push((*kind, next));
    }
    Ok(planned)
}

/// A single *already aggregated* rule-fee scope within a larger root commit.
/// Do not stage independent plans computed against the same base for one key:
/// the caller must aggregate all applicable rules before invoking this owner.
pub(crate) struct StagedRuleFeeSettlement {
    pub general: ResourceDelta,
    pub with_distance: ResourceDelta,
    pub disposition: RuleActionDisposition,
}

/// The Stage 3 balance assignments; no live allocations, fallible arithmetic,
/// journal append, refund, or second charge is permitted during installation.
pub(crate) struct PreparedRuleFeeDelta {
    planned: Vec<(ResourceKind, i64)>,
}

impl SubsystemDelta for PreparedRuleFeeDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        let balances = &mut ctx.state_mut().resources;
        for (kind, next) in self.planned {
            balances.insert(kind, next);
        }
    }
}

impl StagedTransition for StagedRuleFeeSettlement {
    type Delta = PreparedRuleFeeDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        _ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let quote = AuthoritativeDebit.quote(RuleSettlementInput {
            general: &self.general,
            with_distance: &self.with_distance,
            disposition: self.disposition,
        });
        let balances = &view.state().resources;
        // Preserve the existing Runtime priority: only the final selected
        // scope can be charged and its insufficiency is checked before debit.
        let deficits = quote.selected.deficits(balances);
        if !deficits.is_empty() {
            return Err(TransitionReject::World(
                WorldError::ResourceBalanceInvalid {
                    reason: format!("staged rule fee insufficient resources: {deficits:?}"),
                },
            ));
        }
        let planned = plan_resource_delta(balances, quote.selected)?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            None,
            PreparedRuleFeeDelta { planned },
        ))
    }
}

#[cfg(test)]
mod tests;
