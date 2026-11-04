//! Exhaustive original receipt, order and nested intent records.
use super::*;
dto!(crate::world_rules::WorldRuleReceiptV1, {request_id, actor_id, command, committed_at, result_id});
dto!(crate::world_rules::AgentIndustrialOrderV1, {order_id, owner_id, factory_id, target_agent_id, policy_version, inputs, confirmed_at, ready_at, delivered_at, phase, quote, started_at, produced_at, blocked_at, completed_work_ticks, last_progress_tick, terminal_reason, refund_pending, refunded_at});
dto!(crate::world_rules::AgentManufacturingQuoteV1, {policy_version, world_binding, recipe_id, owner_id, owner_player_id, owner_source_id, factory_id, quoted_at, expires_at, quote_digest, dependency_digest, delivery_location_id, inputs, input_ledger, electricity, claim_cost, upfront_liquid, liquid_after_reservation, electricity_after_reservation, maintenance_per_tick});
dto!(crate::AgentClaimCostQuote, {reputation_tier, claim_cap, slot_index, activation_fee_amount, activation_fee_burn_amount, activation_fee_treasury_amount, claim_bond_amount, upkeep_per_epoch, release_cooldown_epochs, grace_epochs, idle_warning_epochs, forced_idle_reclaim_epochs, forced_reclaim_penalty_bps});
dto!(oasis7_agent_api::world_action::QualifiedIndustrialLotFactsV1, {world_binding, owner_id, owner_binding_digest, source_ledger, material, material_profile, unit, spec, quality, custody, provenance_ref, quantity, escrow_capacity, expires_at, consensus_height});
dto!(oasis7_agent_api::world_action::IndustrialInputBaselineV1, {material_profile, unit, spec, quality, custody});
dto!(oasis7_agent_api::world_action::IndustrialBaselineAuthorityV1, {world_binding, owner_binding_digest, state_digest, profile_digest, plan_digest, baseline_impacts, allowed_dispositions, terminal_capacities, quoted_at, expires_at, consensus_height});
dto!(oasis7_agent_api::world_action::EmergencyNeedV1, {obligation_id, interval_start});
