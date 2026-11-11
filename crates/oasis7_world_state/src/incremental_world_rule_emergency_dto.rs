//! Original emergency-supply data graph; no DTO serialization or result re-creation.
use super::*;
dto!(crate::world_rules::EmergencySupplyStateV1,{world_id,minimum_services,region_lineages,observations,observation_interval_coverage,overflowed_windows,services,evidence,authorizations,batches,needs,reviews,restricted_power});
dto!(crate::world_rules::EmergencyAuthorizationV1,{id,evidence_id,provider_id,reviewer_id,reviewer_controller_id,reviewer_owner_source_id,provider_controller_id,region_id,starts_at,expires_at,remaining,active,requested});
dto!(crate::world_rules::EmergencyBatchV1,{authorization_id,frozen_at,frozen_pool,evidence_id,demands,frozen_ledger,allocation,allocated,committed_at});
dto!(crate::world_rules::EmergencyNeedLedgerV1,{obligation_id,controller_id,location_id,beneficiary_id,interval_start,minimum,normal_coverage,fulfilled,reserved,public_critical});
dto!(crate::world_rules::EmergencyRestrictedPowerV1,{beneficiary_id,obligation_id,interval_start,remaining,delivered});
dto!(crate::world_rules::EmergencyReviewV1,{batch_id,need_id,requester_id,reason_ref,opened_at,deadline,decision});
dto!(crate::world_rules::MinimumServiceContractV1,{registration,owner_source_id,operator_agent_id,region_budget_lineage,registered_at,certificate});
dto!(oasis7_agent_api::world_action::EmergencyServiceRegistrationV1,{obligation_id,beneficiary_id,controller_id,region_id,location_id,registered_at,electricity_per_interval,public_critical,recipe_ids,source_ref});
dto!(oasis7_agent_api::world_action::EmergencyObservationV1,{window_start,world_root,obligation_ids,normal_coverage});
dto!(oasis7_agent_api::world_action::EmergencyEvidenceV1,{region_id,region_budget_lineage,observations,source_ref});
