//! Root-staged monetary domain transitions: signed rule-fee settlement and
//! typed cognition economy lease/balance lifecycle.
//!
//! Ordinary synchronous Runtime callers and Stage 1 root transactions share the
//! same borrowed validation and checked arithmetic without cloning [`World`].
//! Delayed or externally confirmed transactions install deltas exclusively
//! through [`RootInstallContext`] after Stage 2 confirmation.

use std::collections::BTreeMap;

use oasis7_agent_api::world_action::ResourceKind;
use oasis7_domain_types::rule_settlement::{
    AuthoritativeDebit, RuleActionDisposition, RuleSettlementInput, RuleSettlementPolicy,
};

use crate::world::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedTransition, SubsystemDelta,
    TransitionContext, TransitionReject,
};
use crate::{
    CognitionEconomyError, CognitionLeaseRequestV1, CognitionLeaseV1,
    CognitionProvisioningReceiptV1, CognitionReceiptV1, ResourceDelta, World, WorldError,
    WorldTime,
};

pub(crate) fn cognition_economy_world_error(error: CognitionEconomyError) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: error.code().to_string(),
    }
}

/// Same per-key signed, checked arithmetic as the original live
/// World::apply_resource_delta path. All entries are planned before any write.
pub(crate) fn plan_resource_delta(
    balances: &BTreeMap<ResourceKind, i64>,
    delta: &ResourceDelta,
) -> Result<Vec<(ResourceKind, i64)>, WorldError> {
    let mut planned = Vec::with_capacity(delta.entries.len());
    for (kind, amount) in &delta.entries {
        let current = balances.get(kind).copied().unwrap_or(0);
        let next =
            current
                .checked_add(*amount)
                .ok_or_else(|| WorldError::ResourceBalanceInvalid {
                    reason: format!(
                        "resource balance overflow: kind={kind:?} current={current} delta={amount}"
                    ),
                })?;
        planned.push((*kind, next));
    }
    Ok(planned)
}

/// A single *already aggregated* rule-fee scope within a larger root commit.
/// Do not stage independent plans computed against the same base for one key:
/// the caller must aggregate all applicable rules before invoking this owner.
pub(crate) struct StagedRuleFeeSettlement<'a> {
    pub general: &'a ResourceDelta,
    pub with_distance: &'a ResourceDelta,
    pub disposition: RuleActionDisposition,
}

impl StagedRuleFeeSettlement<'_> {
    /// The existing live Runtime consumer and the Stage 1 root share the same
    /// selected original cost, with no fee clone, re-encoding or second owner.
    pub(crate) fn plan_checked(
        &self,
        balances: &BTreeMap<ResourceKind, i64>,
    ) -> Result<Vec<(ResourceKind, i64)>, WorldError> {
        let quote = AuthoritativeDebit.quote(RuleSettlementInput {
            general: self.general,
            with_distance: self.with_distance,
            disposition: self.disposition,
        });
        // The live consumer and Stage 1 must reject the very same selected
        // debit before planning any balance assignments. In particular an
        // unpriced rejected movement must not require distance-only funds.
        let deficits = quote.selected.deficits(balances);
        if !deficits.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!("staged rule fee insufficient resources: {deficits:?}"),
            });
        }
        plan_resource_delta(balances, quote.selected)
    }
}

/// The Stage 3 balance assignments; no live allocations, fallible arithmetic,
/// journal append, refund, or second charge is permitted during installation.
#[derive(Debug, Clone)]
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

impl StagedTransition for StagedRuleFeeSettlement<'_> {
    type Delta = PreparedRuleFeeDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        _ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let planned = self.plan_checked(&view.state().resources)?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            None,
            PreparedRuleFeeDelta { planned },
        ))
    }
}

/// Stage 3 cognition economy projection delta installed through [`RootInstallContext`].
#[derive(Debug, Clone)]
pub(crate) struct CognitionEconomyDelta {
    projection: serde_json::Value,
}

impl SubsystemDelta for CognitionEconomyDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        ctx.world_mut()
            .install_cognition_economy_projection(self.projection);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CognitionEconomyOutput {
    Unit,
    ProvisionReceipt(CognitionProvisioningReceiptV1),
    Lease(CognitionLeaseV1),
    Receipt(CognitionReceiptV1),
}

/// Borrowed Stage 1 descriptor covering all eight cognition economy mutations.
pub(crate) enum StagedCognitionEconomyOp<'a> {
    SetBalance(&'a str, &'a str, u64),
    ProvisionForAgent(&'a str, &'a str, &'a str, u64),
    ReserveLease(&'a CognitionLeaseRequestV1),
    SettleLease(&'a str, u64),
    ReleaseLease(&'a str),
    ExpireLease(&'a str),
    RefundLease(&'a str),
    RefundSettledLease(&'a str, u64, &'a str, &'a str),
}

type StagedCognitionPrepared = (
    CognitionEconomyOutput,
    PreparedStagedCommit<CognitionEconomyDelta>,
);

impl StagedCognitionEconomyOp<'_> {
    /// Prepare the typed cognition economy mutation against a shared [`World`]
    /// borrow without cloning [`World`] or hashing unrelated domains.
    pub(crate) fn prepare_on_world(
        &self,
        world: &World,
        tick: WorldTime,
    ) -> Result<(CognitionEconomyOutput, CognitionEconomyDelta), WorldError> {
        if tick != world.state.time {
            return Err(WorldError::DistributedValidationFailed {
                reason: "staged cognition balance requires original tick context".into(),
            });
        }
        let map_err = cognition_economy_world_error;
        let provision_req = match self {
            Self::ProvisionForAgent(agent, id, ctx, amt) => {
                Some(world.build_cognition_provisioning_request(agent, id, ctx, *amt)?)
            }
            _ => None,
        };
        let mut econ = world.cognition_economy()?;
        let output = match self {
            Self::SetBalance(acct, res, amt) => {
                econ.set_resource_balance(*acct, *res, *amt)
                    .map_err(map_err)?;
                CognitionEconomyOutput::Unit
            }
            Self::ProvisionForAgent(..) => CognitionEconomyOutput::ProvisionReceipt(
                econ.provision(provision_req.expect("built above"), tick)
                    .map_err(map_err)?,
            ),
            Self::ReserveLease(req) => {
                let lease = match world.cognition_provisioning_binding_for_request(req, &econ)? {
                    Some(key) => econ.reserve_for_binding((*req).clone(), key, tick),
                    None => econ.reserve((*req).clone(), tick),
                }
                .map_err(map_err)?;
                CognitionEconomyOutput::Lease(lease)
            }
            Self::SettleLease(id, amt) => {
                CognitionEconomyOutput::Receipt(econ.settle(id, *amt, tick).map_err(map_err)?)
            }
            Self::ReleaseLease(id) => {
                CognitionEconomyOutput::Receipt(econ.release(id, tick).map_err(map_err)?)
            }
            Self::ExpireLease(id) => {
                CognitionEconomyOutput::Receipt(econ.expire(id, tick).map_err(map_err)?)
            }
            Self::RefundLease(id) => {
                CognitionEconomyOutput::Receipt(econ.refund(id, tick).map_err(map_err)?)
            }
            Self::RefundSettledLease(id, amt, parent, reason) => CognitionEconomyOutput::Receipt(
                econ.refund_settled(id, *amt, parent, reason, tick)
                    .map_err(map_err)?,
            ),
        };
        let projection = econ.snapshot_json().map_err(map_err)?;
        Ok((output, CognitionEconomyDelta { projection }))
    }

    pub(crate) fn prepare_with_output(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<StagedCognitionPrepared, TransitionReject> {
        let (output, delta) = self.prepare_on_world(view.world(), ctx.tick)?;
        Ok((
            output,
            PreparedStagedCommit::new(view.base_head().clone(), None, delta),
        ))
    }
}

impl StagedTransition for StagedCognitionEconomyOp<'_> {
    type Delta = CognitionEconomyDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        self.prepare_with_output(view, ctx)
            .map(|(_, commit)| commit)
    }
}

/// Composable monetary domain delta for [`crate::world::transition::StagedRootTransaction`],
/// allowing nested rule-fee and cognition-economy operations to stage on one root view
/// without independent commits.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "composed by five-domain root tick orchestration in #6150"
    )
)]
#[derive(Debug, Clone)]
pub(crate) enum MonetaryDomainDelta {
    RuleFee(PreparedRuleFeeDelta),
    CognitionEconomy(CognitionEconomyDelta),
}

impl SubsystemDelta for MonetaryDomainDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        match self {
            Self::RuleFee(delta) => delta.apply_infallible(ctx),
            Self::CognitionEconomy(delta) => delta.apply_infallible(ctx),
        }
    }
}

#[cfg(test)]
mod tests;
