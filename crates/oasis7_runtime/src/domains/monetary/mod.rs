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

use crate::world::bootstrap_economy::{
    m4_default_material_profiles, m4_default_product_profiles, m4_default_recipe_profiles,
};
use crate::world::event_processing::publication::staged_monetary::MonetaryPublicationDelta;
use crate::world::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedRootTransaction,
    StagedTransition, SubsystemDelta, TransitionContext, TransitionReject,
};
use crate::{
    CognitionEconomyError, CognitionEconomyStateV1, CognitionLeaseRequestV1, CognitionLeaseV1,
    CognitionProvisioningReceiptV1, CognitionReceiptV1, MaterialProfileV1, ProductProfileV1,
    RecipeProfileV1, ResourceDelta, World, WorldError, WorldTime,
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

impl<'a> StagedRuleFeeSettlement<'a> {
    pub(crate) fn selected_delta(&self) -> &'a ResourceDelta {
        AuthoritativeDebit
            .quote(RuleSettlementInput {
                general: self.general,
                with_distance: self.with_distance,
                disposition: self.disposition,
            })
            .selected
    }

    /// The existing live Runtime consumer and the Stage 1 root share the same
    /// selected original cost, with no fee clone, re-encoding or second owner.
    pub(crate) fn plan_checked(
        &self,
        balances: &BTreeMap<ResourceKind, i64>,
    ) -> Result<Vec<(ResourceKind, i64)>, WorldError> {
        let selected = self.selected_delta();
        // The live consumer and Stage 1 must reject the very same selected
        // debit before planning any balance assignments. In particular an
        // unpriced rejected movement must not require distance-only funds.
        let deficits = selected.deficits(balances);
        if !deficits.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!("staged rule fee insufficient resources: {deficits:?}"),
            });
        }
        plan_resource_delta(balances, selected)
    }

    pub(crate) fn prepare_delta(
        &self,
        balances: &BTreeMap<ResourceKind, i64>,
    ) -> Result<PreparedRuleFeeDelta, WorldError> {
        Ok(PreparedRuleFeeDelta {
            planned: self.plan_checked(balances)?,
        })
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "composed by #6150 root tick"))]
    pub(crate) fn stage_in_root(
        &self,
        root: &mut StagedRootTransaction<'_, MonetaryDomainDelta>,
    ) -> Result<(), TransitionReject> {
        let mut overlaid = root.view().state().resources.clone();
        for delta in root.deltas() {
            if let MonetaryDeltaInner::RuleFee(prev) = &delta.0 {
                overlaid.extend(prev.planned.iter().copied());
            }
        }
        let planned = self.plan_checked(&overlaid)?;
        root.stage_delta(MonetaryDomainDelta(MonetaryDeltaInner::RuleFee(
            PreparedRuleFeeDelta { planned },
        )));
        Ok(())
    }
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

type PreparedBase = (
    CognitionEconomyOutput,
    CognitionEconomyStateV1,
    CognitionEconomyDelta,
);
type StagedCognitionPrepared = (
    CognitionEconomyOutput,
    PreparedStagedCommit<CognitionEconomyDelta>,
);

impl StagedCognitionEconomyOp<'_> {
    fn prepare_with_base(
        &self,
        world: &World,
        tick: WorldTime,
        prior_econ: Option<&CognitionEconomyStateV1>,
    ) -> Result<PreparedBase, WorldError> {
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
        let mut econ = match prior_econ {
            Some(prev) => prev.clone(),
            None => world.cognition_economy()?,
        };
        let output = match self {
            Self::SetBalance(acct, res, amt) => {
                econ.set_resource_balance_staged(*acct, *res, *amt)
                    .map_err(map_err)?;
                CognitionEconomyOutput::Unit
            }
            Self::ProvisionForAgent(..) => CognitionEconomyOutput::ProvisionReceipt(
                econ.provision_staged(provision_req.expect("built above"), tick)
                    .map_err(map_err)?,
            ),
            Self::ReserveLease(req) => {
                let lease = match world.cognition_provisioning_binding_for_request(req, &econ)? {
                    Some(key) => econ.reserve_for_binding_staged((*req).clone(), key, tick),
                    None => econ.reserve_staged((*req).clone(), tick),
                }
                .map_err(map_err)?;
                CognitionEconomyOutput::Lease(lease)
            }
            Self::SettleLease(id, amt) => {
                CognitionEconomyOutput::Receipt(econ.settle_staged(id, *amt, tick).map_err(map_err)?)
            }
            Self::ReleaseLease(id) => {
                CognitionEconomyOutput::Receipt(econ.release_staged(id, tick).map_err(map_err)?)
            }
            Self::ExpireLease(id) => {
                CognitionEconomyOutput::Receipt(econ.expire_staged(id, tick).map_err(map_err)?)
            }
            Self::RefundLease(id) => {
                CognitionEconomyOutput::Receipt(econ.refund_staged(id, tick).map_err(map_err)?)
            }
            Self::RefundSettledLease(id, amt, parent, reason) => CognitionEconomyOutput::Receipt(
                econ.refund_settled_staged(id, *amt, parent, reason, tick)
                    .map_err(map_err)?,
            ),
        };
        let projection = econ.snapshot_json().map_err(map_err)?;
        Ok((output, econ, CognitionEconomyDelta { projection }))
    }

    pub(crate) fn prepare_on_world(
        &self,
        world: &World,
        tick: WorldTime,
    ) -> Result<(CognitionEconomyOutput, CognitionEconomyDelta), WorldError> {
        self.prepare_with_base(world, tick, None)
            .map(|(out, _, delta)| (out, delta))
    }

    pub(crate) fn prepare_with_output(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<StagedCognitionPrepared, TransitionReject> {
        let (out, _, delta) = self.prepare_with_base(view.world(), ctx.tick, None)?;
        Ok((out, PreparedStagedCommit::new(view.base_head().clone(), None, delta)))
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "composed by #6150 root tick"))]
    pub(crate) fn stage_in_root(
        &self,
        root: &mut StagedRootTransaction<'_, MonetaryDomainDelta>,
    ) -> Result<CognitionEconomyOutput, TransitionReject> {
        let prior = root.deltas().iter().rev().find_map(|d| match &d.0 {
            MonetaryDeltaInner::Cognition(state, _) => Some(state.as_ref()),
            MonetaryDeltaInner::RuleFee(_)
            | MonetaryDeltaInner::Publication(_)
            | MonetaryDeltaInner::BootstrapCatalog(_) => None,
        });
        let (out, state, delta) =
            self.prepare_with_base(root.view().world(), root.context().tick, prior)?;
        root.stage_delta(MonetaryDomainDelta(MonetaryDeltaInner::Cognition(
            Box::new(state),
            delta,
        )));
        Ok(out)
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

pub(crate) struct StagedBootstrapCatalogDefaults;

pub(crate) struct PreparedBootstrapCatalogDelta {
    materials: Vec<MaterialProfileV1>,
    products: Vec<ProductProfileV1>,
    recipes: Vec<RecipeProfileV1>,
}

impl StagedBootstrapCatalogDefaults {
    pub(crate) fn prepare_delta(
        &self,
        world: &World,
    ) -> Result<PreparedBootstrapCatalogDelta, WorldError> {
        let mut materials = Vec::new();
        for profile in m4_default_material_profiles() {
            if world.material_profile(profile.kind.as_str()).is_none() {
                if profile.kind.trim().is_empty() {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: "material profile kind cannot be empty".to_string(),
                    });
                }
                if profile.tier == 0 {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!("material profile tier must be >= 1: {}", profile.kind),
                    });
                }
                materials.push(profile);
            }
        }
        let mut products = Vec::new();
        for profile in m4_default_product_profiles() {
            if world.product_profile(profile.product_id.as_str()).is_none() {
                if profile.product_id.trim().is_empty() {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: "product profile product_id cannot be empty".to_string(),
                    });
                }
                products.push(profile);
            }
        }
        let mut recipes = Vec::new();
        for profile in m4_default_recipe_profiles() {
            if world.recipe_profile(profile.recipe_id.as_str()).is_none() {
                profile.validate_substitution_rules().map_err(|error| {
                    WorldError::ResourceBalanceInvalid {
                        reason: format!("invalid recipe substitution profile: {error}"),
                    }
                })?;
                if profile.recipe_id.trim().is_empty() {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: "recipe profile recipe_id cannot be empty".to_string(),
                    });
                }
                recipes.push(profile);
            }
        }
        Ok(PreparedBootstrapCatalogDelta {
            materials,
            products,
            recipes,
        })
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "composed by #6150 root tick"))]
    pub(crate) fn stage_in_root(
        &self,
        root: &mut StagedRootTransaction<'_, MonetaryDomainDelta>,
    ) -> Result<(), TransitionReject> {
        let delta = self.prepare_delta(root.view().world())?;
        root.stage_delta(MonetaryDomainDelta(MonetaryDeltaInner::BootstrapCatalog(
            delta,
        )));
        Ok(())
    }
}

impl SubsystemDelta for PreparedBootstrapCatalogDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        let state = ctx.state_mut();
        for profile in self.materials {
            state.material_profiles.insert(profile.kind.clone(), profile);
        }
        for profile in self.products {
            state
                .product_profiles
                .insert(profile.product_id.clone(), profile);
        }
        for profile in self.recipes {
            state
                .recipe_profiles
                .insert(profile.recipe_id.clone(), profile);
        }
    }
}

impl StagedTransition for StagedBootstrapCatalogDefaults {
    type Delta = PreparedBootstrapCatalogDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        _ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let delta = self.prepare_delta(view.world())?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            None,
            delta,
        ))
    }
}

enum MonetaryDeltaInner {
    RuleFee(PreparedRuleFeeDelta),
    Cognition(Box<CognitionEconomyStateV1>, CognitionEconomyDelta),
    Publication(MonetaryPublicationDelta),
    BootstrapCatalog(PreparedBootstrapCatalogDelta),
}

pub(crate) struct MonetaryDomainDelta(MonetaryDeltaInner);

impl MonetaryDomainDelta {
    pub(crate) fn from_publication(delta: MonetaryPublicationDelta) -> Self {
        Self(MonetaryDeltaInner::Publication(delta))
    }
}

impl SubsystemDelta for MonetaryDomainDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        match self.0 {
            MonetaryDeltaInner::RuleFee(delta) => delta.apply_infallible(ctx),
            MonetaryDeltaInner::Cognition(_, delta) => delta.apply_infallible(ctx),
            MonetaryDeltaInner::Publication(delta) => delta.apply_infallible(ctx),
            MonetaryDeltaInner::BootstrapCatalog(delta) => delta.apply_infallible(ctx),
        }
    }
}

#[cfg(test)]
mod tests;
