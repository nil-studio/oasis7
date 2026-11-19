//! Complete original industry persistence records; no DTO serialization.
use super::*;
dto!(crate::ProductValidationReceiptV1, {job_id, validation_index, requester_agent_id, module_id, stack, decision, failure_detail});
dto!(oasis7_wasm_abi::ProductValidationDecision, {product_id, accepted, notes, stack_limit, tradable, quality_levels});
dto!(oasis7_wasm_abi::ProductProfileV1, {product_id, role_tag, maintenance_sink, tradable, unlock_stage});
dto!(crate::LastProductValidationState, {product_id, tradable});
dto!(oasis7_wasm_abi::RecipeProfileV1, {recipe_id, substitution_rules, bottleneck_tags, stage_gate, preferred_factory_tags});
dto!(oasis7_wasm_abi::FactoryProfileV1, {factory_id, tier, recipe_slots, tags});
dto!(oasis7_agent_api::world_quote::FactoryBuildPowerObligationV1, {payer_agent_id, profile_key, profile_revision, electricity_amount, mode, material_ledger, electricity_before, electricity_after, material_balances_before, material_balances_after});
dto!(crate::ProductValidationAttemptV1, {job_id, validation_index, requester_agent_id, module_id, stack});
dto!(crate::FactoryRecycleReceiptV1, {operator_agent_id, factory_id, recycle_ledger, recovered, durability_ppm});
dto!(crate::IndustryProgressState, {stage, stage_updated_at, completed_recipe_jobs, completed_material_transits, starter_industrial_milestone, latest_market_quotes});
dto!(crate::StarterIndustrialMilestoneV1, {settlement_summary, committed_recipe_origin, profile_id, profile_revision, factory_id, recipe_id, output_ledger, settlement_job_id, settled_at});
dto!(oasis7_agent_api::world_industry::StarterIndustrialSettlementSummaryV1, {requester_agent_id, accepted_batches, consume, power_required, produce});
dto!(crate::CommittedRecipeOrigin, {submission, consensus_action_id, consensus_submitter_player_id, action_payload_hash, committed_height, action_root});
dto!(oasis7_proto::gameplay_submission_origin::GameplaySubmissionOrigin, {verified_player_id, public_key, auth_nonce, hosted_registration_nonce, requester_agent_id, factory_id, recipe_id});
dto!(crate::events::MaterialMarketQuote, {kind, requested_amount, local_available_amount, world_available_amount, local_deficit_amount, transit_loss_bps, governance_tax_bps, effective_cost_index_ppm});
tagged_codec!(crate::events::IndustryStage,units {Bootstrap=0,ScaleOut=1,Governance=2},tuples {},records {});
tagged_codec!(oasis7_agent_api::world_quote::FactoryConstructionPowerMode,units {StartOnlySink=0},tuples {},records {});
