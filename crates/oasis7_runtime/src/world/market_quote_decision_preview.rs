use serde::{Deserialize, Serialize};

use super::super::{MaterialLedgerId, MaterialMarketQuote, MaterialStack};
use super::World;
use super::event_processing::action_to_event_economy::build_material_market_quotes;

/// A conditional, read-only explanation of the material market inputs that a
/// recipe submission would use at the current world state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketQuoteDecisionPreview {
    pub market_quotes: Vec<MaterialMarketQuote>,
    pub local_vs_world_delta: Vec<MarketQuoteSupplyDelta>,
    pub total_unsatisfied_shortfall: i64,
    pub market_pressure: String,
    pub recommendation: String,
    pub rationale: String,
    pub next_reduction_action: String,
    pub conditional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketQuoteSupplyDelta {
    pub kind: String,
    pub local_deficit_amount: i64,
    pub world_cover_amount: i64,
    pub unsatisfied_shortfall_amount: i64,
}

impl World {
    /// Derives the existing market quote formula without reserving inventory or
    /// submitting an action. The preview is conditional on current balances and
    /// policy at eventual submission time.
    pub fn market_quote_decision_preview(
        &self,
        preferred_consume_ledger: &MaterialLedgerId,
        consume: &[MaterialStack],
    ) -> MarketQuoteDecisionPreview {
        let market_quotes = build_material_market_quotes(self, preferred_consume_ledger, consume);
        let uses_local_ledger = market_quotes
            .iter()
            .all(|quote| quote.local_available_amount >= quote.requested_amount);
        let mut total_unsatisfied_shortfall = 0_i64;
        let local_vs_world_delta = market_quotes
            .iter()
            .map(|quote| {
                let world_cover_amount = if uses_local_ledger {
                    0
                } else {
                    quote.world_available_amount.min(quote.requested_amount)
                };
                let selected_ledger_available = if uses_local_ledger {
                    quote.local_available_amount
                } else {
                    quote.world_available_amount
                };
                let unsatisfied_shortfall_amount = quote
                    .requested_amount
                    .saturating_sub(selected_ledger_available);
                total_unsatisfied_shortfall =
                    total_unsatisfied_shortfall.saturating_add(unsatisfied_shortfall_amount);
                MarketQuoteSupplyDelta {
                    kind: quote.kind.clone(),
                    local_deficit_amount: quote.local_deficit_amount,
                    world_cover_amount,
                    unsatisfied_shortfall_amount,
                }
            })
            .collect::<Vec<_>>();
        let (market_pressure, recommendation, rationale, next_reduction_action) =
            if total_unsatisfied_shortfall > 0 {
                (
                    "unsatisfied_shortfall",
                    "reduce_or_source_materials",
                    "neither the preferred ledger nor the world fallback can satisfy the request",
                    "reduce_requested_amount",
                )
            } else if !uses_local_ledger {
                (
                    "world_supply_pressure",
                    "submit_with_world_supply",
                    "the world fallback ledger covers the full request; tax and transit remain conditional",
                    "use_local_materials",
                )
            } else {
                (
                    "local_supply_ready",
                    "submit_with_local_supply",
                    "local inventory covers the requested materials",
                    "submit_recipe",
                )
            };

        MarketQuoteDecisionPreview {
            market_quotes,
            local_vs_world_delta,
            total_unsatisfied_shortfall,
            market_pressure: market_pressure.to_string(),
            recommendation: recommendation.to_string(),
            rationale: rationale.to_string(),
            next_reduction_action: next_reduction_action.to_string(),
            conditional: true,
        }
    }
}
