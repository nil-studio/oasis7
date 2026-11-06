//! Exhaustive original industrial-work and substitution record shapes.
//! Called by the closed typed record codec; no DTO serialization.
use super::*;

dto!(oasis7_agent_api::world_action::IndustrialRefV1, {world_binding, domain, schema, id, revision, digest});
dto!(oasis7_agent_api::world_action::IndustrialLotV1, {lot_ref, owner_id, source_ledger, material, unit, spec, quality, custody, provenance_ref, quantity, remaining});
dto!(oasis7_agent_api::world_action::IndustrialParentV1, {lot_ref, settlement_ref, quantity});
dto!(oasis7_agent_api::world_action::IndustrialInputSlotV1, {material_profile, material, unit, spec, quality, custody, quantity, binding_revision, parents, consumed});
dto!(oasis7_agent_api::world_action::RootOperationV1, {owner_binding_digest, allowed_dispositions, output_ledger, terminal_capacities, started_job_id, root_ref, owner_id, factory_id, pinned_recipe_id, accepted_batches, pinned_profile_digest, causal_digest, work, first_sink, slots});
dto!(oasis7_agent_api::world_action::ParentBindingRevisionV1, {root_ref, slot_id, expected_revision, decision_ref, parents});
dto!(oasis7_agent_api::world_action::IndustrialReceiptV1, {receipt_ref, root_ref, child_ref, effect_role, payload_digest, input, output});
dto!(oasis7_agent_api::world_action::DispositionPlanV1, {root_ref, plan_ref, expected_causal_digest, disposition});
dto!(oasis7_agent_api::world_action::IndustrialLotAuthorityV1, {owner_binding_digest, expires_at, escrow_capacity, material_profile});
dto!(oasis7_agent_api::world_action::IndustrialWorkStateV1, {roots, lots, imported_lot_authorities, receipts, sink_keys, bindings, dispositions});
dto!(oasis7_agent_api::world_action::SubstitutionValuationV1, {unit, held, consumed, expected_output});
dto!(oasis7_agent_api::world_action::SubstitutionExecutionAuthorityV1, {authority_ref, owner_id, owner_binding_digest, factory_digest, root_ref, slot_id, expected_binding_revision, profile_digest, selection, target_recipe, baseline_impacts, candidate_impacts, output_material, output_unit, output_spec, output_quality, output_custody, owner_ledger_path_policy, input_state, costs, power_cost, duration_ticks, terminal_capacity, valid_until, valuation, risks, next_recheck});
dto!(oasis7_agent_api::world_action::SubstitutionPreviewRequestV1, {root_ref, slot_id, authority_refs});
dto!(oasis7_agent_api::world_action::SubstitutionCandidateV1, {candidate_digest, authority, rule, kind, plan, native_source_quantity, source_material_kind, candidate_root, cutover_plan, cutover_plan_digest, recipe_plan, causal_equivalent, conversion_required, source_remaining, binding_revision, resource_balances});
dto!(oasis7_agent_api::world_action::SubstitutionPreviewV1, {preview_digest, status, blocker, next_action, next_recheck, request, candidates, evaluated_at});
dto!(oasis7_agent_api::world_action::ConfirmSubstitutionV1, {preview, candidate_digest, scope, maximum_source, maximum_costs, maximum_power, cutover_plan_digest});
dto!(oasis7_agent_api::world_action::SubstitutionDecisionV1, {decision_ref, owner_id, confirmation_key, intent, candidate, confirmed_at});
dto!(oasis7_agent_api::world_action::ConversionSettlementV1, {decision_ref, conversion_ref, receipt_ref, output_lot, source_consumed, source_loss, residual_numerator, residual_denominator, committed_at});
dto!(oasis7_agent_api::world_action::SubstitutionResidualBalanceV1, {owner_id, domain_key, source_lot, settlement_ref, unit, numerator, denominator, next_action});
dto!(oasis7_agent_api::world_action::SubstitutionStateV1, {authorities, decisions, settlements, conversion_domains, terminal_used, retained_residuals});
dto!(oasis7_wasm_abi::SubstitutionProfileRefV1, {id, version, digest});
dto!(oasis7_wasm_abi::SubstitutionImpactV1, {outputs, quality, power, time, byproducts, terminal, value_class, logistics, buffer});
dto!(oasis7_wasm_abi::SubstitutionRuleV1, {rule, recipe, input_edge, required_material, allowed_materials, accepted_specs, accepted_quality, accepted_custody, owner_ledger_path_policy, source_unit, target_unit, ratio_numerator, ratio_denominator, executable_target_quantum, rounding, residual, aggregation_domain, source_loss_numerator, source_loss_denominator, impacts, conversion, applicable_input_states, disposition_policy, recheck_policy});
dto!(oasis7_wasm_abi::SubstitutionQuantityPlanV1, {rule, domain, domain_key, source_unit, target_unit, source_required, source_loss, target_executable, residual_numerator, residual_denominator, unmet_target});
dto!(oasis7_wasm_abi::MaterialStack, {kind, amount});
dto!(oasis7_wasm_abi::RecipeExecutionPlan, {accepted_batches, consume, produce, byproducts, power_required, duration_ticks, reject_reason});
