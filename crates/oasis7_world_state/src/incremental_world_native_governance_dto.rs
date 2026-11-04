//! Every original native governance/money DTO field, without DTO serialization.
use super::*;
dto!(crate::governance::GovernanceThresholdSignerPolicy,{threshold,allowed_public_keys});
dto!(crate::governance::GovernanceFinalitySignerRegistry,{slot_id,threshold,threshold_bps,signer_bindings,validator_stakes});
dto!(crate::governance::GovernanceMainTokenControllerRegistry,{genesis_controller_account_id,treasury_bucket_controller_slots,restricted_starter_claim_admin_account_ids,controller_signer_policies});
dto!(crate::governance::GovernanceValidatorAdmissionRecord,{candidate_id,node_id,finality_signer_public_key,stake,operator_owner,public_manifest_hash,requested_at_epoch,last_transition_tick,approved_at_epoch,activation_epoch,status,revoked_at_epoch,revocation_reason});
dto!(crate::main_token::MainTokenNodePointsBridgeDistribution,{node_id,account_id,amount});
dto!(crate::main_token::MainTokenNodePointsBridgeEpochRecord,{epoch_index,settlement_hash,total_amount,distributions});
dto!(crate::main_token::MainTokenTreasuryDistributionRecord,{proposal_id,distribution_id,bucket_id,total_amount,distributions,distributed_epoch});
dto!(crate::main_token::RestrictedStarterClaimLiveopsPoolTopUpRecord,{controller_account_id,top_up_id,source_treasury_bucket_id,target_treasury_bucket_id,amount,topped_up_at_epoch});
dto!(oasis7_agent_api::world_action::MainTokenTreasuryDistribution,{account_id,amount});
dto!(oasis7_agent_api::world_action::GovernanceAuthorizationV1,{schema_version,world_id,chain_id,genesis_digest,parent_position,parent_record_id,parent_state_root,policy_digest,controller_account_id,nonce,valid_from_position,valid_through_position,signatures});
dto!(oasis7_agent_api::world_action::FinalityKeyRotationBodyV1,{schema_version,authorization,rotation_id,candidate_id,node_id,expected_generation,old_public_key_hex,new_public_key_hex,decision_height,activation_height,target_epoch});
dto!(oasis7_agent_api::world_action::FinalityKeyRotationProofV1,{schema_version,role,signature_hex});
