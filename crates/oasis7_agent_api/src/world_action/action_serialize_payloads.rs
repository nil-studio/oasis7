// Private borrowed payload types for Action's adjacent-tag Serialize implementation.
use super::super::*;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename = "WorldRule")]
pub(super) struct WorldRulePayload<'a> {
    pub(super) actor_id: &'a String,
    pub(super) request_id: &'a String,
    pub(super) command: &'a WorldRuleCommandV1,
}

#[derive(Serialize)]
#[serde(rename = "WorldServiceIntent")]
pub(super) struct WorldServiceIntentPayload<'a> {
    pub(super) request: &'a serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename = "RegisterAgent")]
pub(super) struct RegisterAgentPayload<'a> {
    pub(super) agent_id: &'a String,
    pub(super) pos: &'a GeoPos,
}

#[derive(Serialize)]
#[serde(rename = "MoveAgent")]
pub(super) struct MoveAgentPayload<'a> {
    pub(super) agent_id: &'a String,
    pub(super) to: &'a GeoPos,
}

#[derive(Serialize)]
#[serde(rename = "QueryObservation")]
pub(super) struct QueryObservationPayload<'a> {
    pub(super) agent_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "EmitObservation")]
pub(super) struct EmitObservationPayload<'a> {
    pub(super) observation: &'a Observation,
}

#[derive(Serialize)]
#[serde(rename = "BodyAction")]
pub(super) struct BodyActionPayload<'a> {
    pub(super) agent_id: &'a String,
    pub(super) kind: &'a String,
    pub(super) payload: &'a serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename = "EmitBodyAttributes")]
pub(super) struct EmitBodyAttributesPayload<'a> {
    pub(super) agent_id: &'a String,
    pub(super) view: &'a BodyKernelView,
    pub(super) reason: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ExpandBodyInterface")]
pub(super) struct ExpandBodyInterfacePayload<'a> {
    pub(super) agent_id: &'a String,
    pub(super) interface_module_item_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "DeployModuleArtifact")]
pub(super) struct DeployModuleArtifactPayload<'a> {
    pub(super) publisher_agent_id: &'a String,
    pub(super) wasm_hash: &'a String,
    pub(super) wasm_bytes: &'a Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename = "CompileModuleArtifactFromSource")]
pub(super) struct CompileModuleArtifactFromSourcePayload<'a> {
    pub(super) publisher_agent_id: &'a String,
    pub(super) module_id: &'a String,
    pub(super) source_package: &'a ModuleSourcePackage,
}

#[derive(Serialize)]
#[serde(rename = "InstallModuleFromArtifact")]
pub(super) struct InstallModuleFromArtifactPayload<'a> {
    pub(super) installer_agent_id: &'a String,
    pub(super) manifest: &'a ModuleManifest,
    pub(super) activate: &'a bool,
}

#[derive(Serialize)]
#[serde(rename = "InstallModuleFromArtifactWithFinality")]
pub(super) struct InstallModuleFromArtifactWithFinalityPayload<'a> {
    pub(super) installer_agent_id: &'a String,
    pub(super) manifest: &'a ModuleManifest,
    pub(super) activate: &'a bool,
    pub(super) finality_certificate: &'a GovernanceFinalityCertificate,
}

#[derive(Serialize)]
#[serde(rename = "InstallModuleToTargetFromArtifact")]
pub(super) struct InstallModuleToTargetFromArtifactPayload<'a> {
    pub(super) installer_agent_id: &'a String,
    pub(super) manifest: &'a ModuleManifest,
    pub(super) activate: &'a bool,
    pub(super) install_target: &'a ModuleInstallTarget,
}

#[derive(Serialize)]
#[serde(rename = "InstallModuleToTargetFromArtifactWithFinality")]
pub(super) struct InstallModuleToTargetFromArtifactWithFinalityPayload<'a> {
    pub(super) installer_agent_id: &'a String,
    pub(super) manifest: &'a ModuleManifest,
    pub(super) activate: &'a bool,
    pub(super) install_target: &'a ModuleInstallTarget,
    pub(super) finality_certificate: &'a GovernanceFinalityCertificate,
}

#[derive(Serialize)]
#[serde(rename = "UpgradeModuleFromArtifact")]
pub(super) struct UpgradeModuleFromArtifactPayload<'a> {
    pub(super) upgrader_agent_id: &'a String,
    pub(super) instance_id: &'a String,
    pub(super) from_module_version: &'a String,
    pub(super) manifest: &'a ModuleManifest,
    pub(super) activate: &'a bool,
}

#[derive(Serialize)]
#[serde(rename = "UpgradeModuleFromArtifactWithFinality")]
pub(super) struct UpgradeModuleFromArtifactWithFinalityPayload<'a> {
    pub(super) upgrader_agent_id: &'a String,
    pub(super) instance_id: &'a String,
    pub(super) from_module_version: &'a String,
    pub(super) manifest: &'a ModuleManifest,
    pub(super) activate: &'a bool,
    pub(super) finality_certificate: &'a GovernanceFinalityCertificate,
}

#[derive(Serialize)]
#[serde(rename = "ModuleReleaseSubmit")]
pub(super) struct ModuleReleaseSubmitPayload<'a> {
    pub(super) requester_agent_id: &'a String,
    pub(super) manifest: &'a ModuleManifest,
    pub(super) activate: &'a bool,
    pub(super) install_target: &'a ModuleInstallTarget,
    pub(super) required_roles: &'a Vec<String>,
    pub(super) profile_changes: &'a ModuleProfileChanges,
}

#[derive(Serialize)]
#[serde(rename = "ModuleReleaseShadow")]
pub(super) struct ModuleReleaseShadowPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) request_id: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "ModuleReleaseSubmitAttestation")]
pub(super) struct ModuleReleaseSubmitAttestationPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) request_id: &'a u64,
    pub(super) signer_node_id: &'a String,
    pub(super) platform: &'a String,
    pub(super) build_manifest_hash: &'a String,
    pub(super) source_hash: &'a String,
    pub(super) wasm_hash: &'a String,
    pub(super) proof_cid: &'a String,
    pub(super) builder_image_digest: &'a String,
    pub(super) container_platform: &'a String,
    pub(super) canonicalizer_version: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ModuleReleaseApproveRole")]
pub(super) struct ModuleReleaseApproveRolePayload<'a> {
    pub(super) approver_agent_id: &'a String,
    pub(super) request_id: &'a u64,
    pub(super) role: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ModuleReleaseBindRoles")]
pub(super) struct ModuleReleaseBindRolesPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) target_agent_id: &'a String,
    pub(super) roles: &'a Vec<String>,
}

#[derive(Serialize)]
#[serde(rename = "ModuleReleaseReject")]
pub(super) struct ModuleReleaseRejectPayload<'a> {
    pub(super) rejector_agent_id: &'a String,
    pub(super) request_id: &'a u64,
    pub(super) reason: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ModuleReleaseApply")]
pub(super) struct ModuleReleaseApplyPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) request_id: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "ModuleReleaseApplyWithFinality")]
pub(super) struct ModuleReleaseApplyWithFinalityPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) request_id: &'a u64,
    pub(super) finality_certificate: &'a GovernanceFinalityCertificate,
}

#[derive(Serialize)]
#[serde(rename = "RollbackModuleInstance")]
pub(super) struct RollbackModuleInstancePayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) instance_id: &'a String,
    pub(super) target_module_version: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "RollbackModuleInstanceWithFinality")]
pub(super) struct RollbackModuleInstanceWithFinalityPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) instance_id: &'a String,
    pub(super) target_module_version: &'a String,
    pub(super) finality_certificate: &'a GovernanceFinalityCertificate,
}

#[derive(Serialize)]
#[serde(rename = "ListModuleArtifactForSale")]
pub(super) struct ListModuleArtifactForSalePayload<'a> {
    pub(super) seller_agent_id: &'a String,
    pub(super) wasm_hash: &'a String,
    pub(super) price_kind: &'a ResourceKind,
    pub(super) price_amount: &'a i64,
}

#[derive(Serialize)]
#[serde(rename = "BuyModuleArtifact")]
pub(super) struct BuyModuleArtifactPayload<'a> {
    pub(super) buyer_agent_id: &'a String,
    pub(super) wasm_hash: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "DelistModuleArtifact")]
pub(super) struct DelistModuleArtifactPayload<'a> {
    pub(super) seller_agent_id: &'a String,
    pub(super) wasm_hash: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "DestroyModuleArtifact")]
pub(super) struct DestroyModuleArtifactPayload<'a> {
    pub(super) owner_agent_id: &'a String,
    pub(super) wasm_hash: &'a String,
    pub(super) reason: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "PlaceModuleArtifactBid")]
pub(super) struct PlaceModuleArtifactBidPayload<'a> {
    pub(super) bidder_agent_id: &'a String,
    pub(super) wasm_hash: &'a String,
    pub(super) price_kind: &'a ResourceKind,
    pub(super) price_amount: &'a i64,
}

#[derive(Serialize)]
#[serde(rename = "CancelModuleArtifactBid")]
pub(super) struct CancelModuleArtifactBidPayload<'a> {
    pub(super) bidder_agent_id: &'a String,
    pub(super) wasm_hash: &'a String,
    pub(super) bid_order_id: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "TransferResource")]
pub(super) struct TransferResourcePayload<'a> {
    pub(super) from_agent_id: &'a String,
    pub(super) to_agent_id: &'a String,
    pub(super) kind: &'a ResourceKind,
    pub(super) amount: &'a i64,
}

#[derive(Serialize)]
#[serde(rename = "CollectData")]
pub(super) struct CollectDataPayload<'a> {
    pub(super) collector_agent_id: &'a String,
    pub(super) electricity_cost: &'a i64,
    pub(super) data_amount: &'a i64,
}

#[derive(Serialize)]
#[serde(rename = "CollectDataAuthenticated")]
pub(super) struct CollectDataAuthenticatedPayload<'a> {
    pub(super) collector_agent_id: &'a String,
    pub(super) electricity_cost: &'a i64,
    pub(super) data_amount: &'a i64,
    pub(super) player_id: &'a String,
    pub(super) public_key: &'a String,
    pub(super) nonce: &'a u64,
    pub(super) signature: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "GrantDataAccess")]
pub(super) struct GrantDataAccessPayload<'a> {
    pub(super) owner_agent_id: &'a String,
    pub(super) grantee_agent_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "RevokeDataAccess")]
pub(super) struct RevokeDataAccessPayload<'a> {
    pub(super) owner_agent_id: &'a String,
    pub(super) grantee_agent_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "RedeemPower")]
pub(super) struct RedeemPowerPayload<'a> {
    pub(super) node_id: &'a String,
    pub(super) target_agent_id: &'a String,
    pub(super) redeem_credits: &'a u64,
    pub(super) nonce: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "RedeemPowerSigned")]
pub(super) struct RedeemPowerSignedPayload<'a> {
    pub(super) node_id: &'a String,
    pub(super) target_agent_id: &'a String,
    pub(super) redeem_credits: &'a u64,
    pub(super) nonce: &'a u64,
    pub(super) signer_node_id: &'a String,
    pub(super) signature: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ApplyNodePointsSettlementSigned")]
pub(super) struct ApplyNodePointsSettlementSignedPayload<'a> {
    pub(super) report: &'a EpochSettlementReport,
    pub(super) signer_node_id: &'a String,
    pub(super) mint_records: &'a Vec<NodeRewardMintRecord>,
}

#[derive(Serialize)]
#[serde(rename = "InitializeMainTokenGenesis")]
pub(super) struct InitializeMainTokenGenesisPayload<'a> {
    pub(super) allocations: &'a Vec<MainTokenGenesisAllocationPlan>,
}

#[derive(Serialize)]
#[serde(rename = "ClaimMainTokenVesting")]
pub(super) struct ClaimMainTokenVestingPayload<'a> {
    pub(super) bucket_id: &'a String,
    pub(super) beneficiary: &'a String,
    pub(super) nonce: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "TransferMainToken")]
pub(super) struct TransferMainTokenPayload<'a> {
    pub(super) from_account_id: &'a String,
    pub(super) to_account_id: &'a String,
    pub(super) amount: &'a u64,
    pub(super) nonce: &'a u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) asset_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) memo: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) chain_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) network_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tx_version: &'a Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tx_type: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) valid_until_unix_ms: &'a Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_fee: &'a Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) fee_asset_id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) application_payload_hash: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) client_request_id: &'a Option<String>,
}

#[derive(Serialize)]
#[serde(rename = "ApplyMainTokenEpochIssuance")]
pub(super) struct ApplyMainTokenEpochIssuancePayload<'a> {
    pub(super) epoch_index: &'a u64,
    pub(super) actual_stake_ratio_bps: &'a u32,
}

#[derive(Serialize)]
#[serde(rename = "SettleMainTokenFee")]
pub(super) struct SettleMainTokenFeePayload<'a> {
    pub(super) fee_kind: &'a MainTokenFeeKind,
    pub(super) amount: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "UpdateMainTokenPolicy")]
pub(super) struct UpdateMainTokenPolicyPayload<'a> {
    pub(super) proposal_id: &'a ProposalId,
    pub(super) next: &'a MainTokenConfig,
}

#[derive(Serialize)]
#[serde(rename = "DistributeMainTokenTreasury")]
pub(super) struct DistributeMainTokenTreasuryPayload<'a> {
    pub(super) proposal_id: &'a ProposalId,
    pub(super) distribution_id: &'a String,
    pub(super) bucket_id: &'a String,
    pub(super) distributions: &'a Vec<MainTokenTreasuryDistribution>,
}

#[derive(Serialize)]
#[serde(rename = "TopUpRestrictedStarterClaimLiveopsPool")]
pub(super) struct TopUpRestrictedStarterClaimLiveopsPoolPayload<'a> {
    pub(super) controller_account_id: &'a String,
    pub(super) top_up_id: &'a String,
    pub(super) amount: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "IssueRestrictedStarterClaimGrant")]
pub(super) struct IssueRestrictedStarterClaimGrantPayload<'a> {
    pub(super) issuer_account_id: &'a String,
    pub(super) beneficiary_account_id: &'a String,
    pub(super) amount: &'a u64,
    pub(super) issuance_reason: &'a String,
    pub(super) expires_at_epoch: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "RevokeRestrictedStarterClaimGrant")]
pub(super) struct RevokeRestrictedStarterClaimGrantPayload<'a> {
    pub(super) issuer_account_id: &'a String,
    pub(super) beneficiary_account_id: &'a String,
    pub(super) revoke_reason: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ClaimAgent")]
pub(super) struct ClaimAgentPayload<'a> {
    pub(super) claimer_agent_id: &'a String,
    pub(super) target_agent_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ClaimStarterOc")]
pub(super) struct ClaimStarterOcPayload<'a> {
    pub(super) agent_id: &'a String,
    pub(super) player_id: &'a String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) public_key: &'a Option<String>,
}

#[derive(Serialize)]
#[serde(rename = "ReleaseAgentClaim")]
pub(super) struct ReleaseAgentClaimPayload<'a> {
    pub(super) claimer_agent_id: &'a String,
    pub(super) target_agent_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "TransferMaterial")]
pub(super) struct TransferMaterialPayload<'a> {
    pub(super) requester_agent_id: &'a String,
    pub(super) from_ledger: &'a MaterialLedgerId,
    pub(super) to_ledger: &'a MaterialLedgerId,
    pub(super) kind: &'a String,
    pub(super) amount: &'a i64,
    pub(super) distance_km: &'a i64,
    pub(super) priority: &'a Option<MaterialTransitPriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) route_id: &'a Option<String>,
    pub(super) route_ids: &'a Vec<String>,
    pub(super) auto_reroute: &'a bool,
}

#[derive(Serialize)]
#[serde(rename = "RegisterLogisticsRoute")]
pub(super) struct RegisterLogisticsRoutePayload<'a> {
    pub(super) requester_agent_id: &'a String,
    pub(super) from_ledger: &'a MaterialLedgerId,
    pub(super) to_ledger: &'a MaterialLedgerId,
    pub(super) kind: &'a String,
    pub(super) distance_km: &'a i64,
    pub(super) priority: &'a MaterialTransitPriority,
    pub(super) capacity_units: &'a i64,
    pub(super) tariff_electricity_per_unit: &'a i64,
}

#[derive(Serialize)]
#[serde(rename = "SetLogisticsRouteAvailability")]
pub(super) struct SetLogisticsRouteAvailabilityPayload<'a> {
    pub(super) requester_agent_id: &'a String,
    pub(super) route_id: &'a String,
    pub(super) available: &'a bool,
}

#[derive(Serialize)]
#[serde(rename = "FormAlliance")]
pub(super) struct FormAlliancePayload<'a> {
    pub(super) proposer_agent_id: &'a String,
    pub(super) alliance_id: &'a String,
    pub(super) members: &'a Vec<String>,
    pub(super) charter: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "JoinAlliance")]
pub(super) struct JoinAlliancePayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) alliance_id: &'a String,
    pub(super) member_agent_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "LeaveAlliance")]
pub(super) struct LeaveAlliancePayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) alliance_id: &'a String,
    pub(super) member_agent_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "DissolveAlliance")]
pub(super) struct DissolveAlliancePayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) alliance_id: &'a String,
    pub(super) reason: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "DeclareWar")]
pub(super) struct DeclareWarPayload<'a> {
    pub(super) initiator_agent_id: &'a String,
    pub(super) war_id: &'a String,
    pub(super) aggressor_alliance_id: &'a String,
    pub(super) defender_alliance_id: &'a String,
    pub(super) objective: &'a String,
    pub(super) intensity: &'a u32,
}

#[derive(Serialize)]
#[serde(rename = "OpenGovernanceProposal")]
pub(super) struct OpenGovernanceProposalPayload<'a> {
    pub(super) proposer_agent_id: &'a String,
    pub(super) proposal_key: &'a String,
    pub(super) title: &'a String,
    pub(super) description: &'a String,
    pub(super) options: &'a Vec<String>,
    pub(super) voting_window_ticks: &'a u64,
    pub(super) quorum_weight: &'a u64,
    pub(super) pass_threshold_bps: &'a u16,
}

#[derive(Serialize)]
#[serde(rename = "CastGovernanceVote")]
pub(super) struct CastGovernanceVotePayload<'a> {
    pub(super) voter_agent_id: &'a String,
    pub(super) proposal_key: &'a String,
    pub(super) option: &'a String,
    pub(super) weight: &'a u32,
}

#[derive(Serialize)]
#[serde(rename = "ResolveCrisis")]
pub(super) struct ResolveCrisisPayload<'a> {
    pub(super) resolver_agent_id: &'a String,
    pub(super) crisis_id: &'a String,
    pub(super) strategy: &'a String,
    pub(super) success: &'a bool,
}

#[derive(Serialize)]
#[serde(rename = "GrantMetaProgress")]
pub(super) struct GrantMetaProgressPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) target_agent_id: &'a String,
    pub(super) track: &'a String,
    pub(super) points: &'a i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) achievement_id: &'a Option<String>,
}

#[derive(Serialize)]
#[serde(rename = "UpdateGameplayPolicy")]
pub(super) struct UpdateGameplayPolicyPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) electricity_tax_bps: &'a u16,
    pub(super) data_tax_bps: &'a u16,
    pub(super) power_trade_fee_bps: &'a u16,
    pub(super) max_open_contracts_per_agent: &'a u16,
    pub(super) blocked_agents: &'a Vec<String>,
    pub(super) forbidden_location_ids: &'a Vec<String>,
}

#[derive(Serialize)]
#[serde(rename = "UpdateRestrictedStarterClaimAdminRegistry")]
pub(super) struct UpdateRestrictedStarterClaimAdminRegistryPayload<'a> {
    pub(super) controller_account_id: &'a String,
    pub(super) next_admin_account_ids: &'a Vec<String>,
}

#[derive(Serialize)]
#[serde(rename = "SubmitGovernanceValidatorAdmission")]
pub(super) struct SubmitGovernanceValidatorAdmissionPayload<'a> {
    pub(super) controller_account_id: &'a String,
    pub(super) candidate_id: &'a String,
    pub(super) node_id: &'a String,
    pub(super) finality_signer_public_key: &'a String,
    pub(super) stake: &'a u64,
    pub(super) operator_owner: &'a String,
    pub(super) public_manifest_hash: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ApproveGovernanceValidatorAdmission")]
pub(super) struct ApproveGovernanceValidatorAdmissionPayload<'a> {
    pub(super) controller_account_id: &'a String,
    pub(super) candidate_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ActivateGovernanceValidatorAdmission")]
pub(super) struct ActivateGovernanceValidatorAdmissionPayload<'a> {
    pub(super) controller_account_id: &'a String,
    pub(super) candidate_id: &'a String,
    pub(super) activation_epoch: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "RevokeGovernanceValidatorAdmission")]
pub(super) struct RevokeGovernanceValidatorAdmissionPayload<'a> {
    pub(super) controller_account_id: &'a String,
    pub(super) candidate_id: &'a String,
    pub(super) node_id: &'a String,
    pub(super) reason: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "OpenEconomicContract")]
pub(super) struct OpenEconomicContractPayload<'a> {
    pub(super) creator_agent_id: &'a String,
    pub(super) contract_id: &'a String,
    pub(super) counterparty_agent_id: &'a String,
    pub(super) fulfillment_kind: &'a EconomicContractFulfillmentKind,
    pub(super) settlement_kind: &'a ResourceKind,
    pub(super) settlement_amount: &'a i64,
    pub(super) reputation_stake: &'a i64,
    pub(super) expires_at: &'a WorldTime,
    pub(super) description: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "AcceptEconomicContract")]
pub(super) struct AcceptEconomicContractPayload<'a> {
    pub(super) accepter_agent_id: &'a String,
    pub(super) contract_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "SettleEconomicContract")]
pub(super) struct SettleEconomicContractPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) contract_id: &'a String,
    pub(super) success: &'a bool,
    pub(super) notes: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "EmitResourceTransfer")]
pub(super) struct EmitResourceTransferPayload<'a> {
    pub(super) from_agent_id: &'a String,
    pub(super) to_agent_id: &'a String,
    pub(super) kind: &'a ResourceKind,
    pub(super) amount: &'a i64,
}

#[derive(Serialize)]
#[serde(rename = "BuildFactory")]
pub(super) struct BuildFactoryPayload<'a> {
    pub(super) builder_agent_id: &'a String,
    pub(super) site_id: &'a String,
    pub(super) spec: &'a FactoryModuleSpec,
}

#[derive(Serialize)]
#[serde(rename = "BuildFactoryWithModule")]
pub(super) struct BuildFactoryWithModulePayload<'a> {
    pub(super) builder_agent_id: &'a String,
    pub(super) site_id: &'a String,
    pub(super) module_id: &'a String,
    pub(super) spec: &'a FactoryModuleSpec,
}

#[derive(Serialize)]
#[serde(rename = "MaintainFactory")]
pub(super) struct MaintainFactoryPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) factory_id: &'a String,
    pub(super) parts: &'a i64,
}

#[derive(Serialize)]
#[serde(rename = "RecycleFactory")]
pub(super) struct RecycleFactoryPayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) factory_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ScheduleEssentialRecipe")]
pub(super) struct ScheduleEssentialRecipePayload<'a> {
    pub(super) requester_agent_id: &'a String,
    pub(super) factory_id: &'a String,
    pub(super) recipe_id: &'a String,
    pub(super) batches: &'a u32,
    pub(super) obligation_id: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ScheduleRecipe")]
pub(super) struct ScheduleRecipePayload<'a> {
    pub(super) requester_agent_id: &'a String,
    pub(super) factory_id: &'a String,
    pub(super) recipe_id: &'a String,
    pub(super) plan: &'a RecipeExecutionPlan,
    pub(super) logistics_route_ids: &'a Vec<String>,
    pub(super) logistics_path_ids: &'a Vec<String>,
}

#[derive(Serialize)]
#[serde(rename = "PauseFactoryProduction")]
pub(super) struct PauseFactoryProductionPayload<'a> {
    pub(super) requester_agent_id: &'a String,
    pub(super) factory_id: &'a String,
    pub(super) reason: &'a String,
}

#[derive(Serialize)]
#[serde(rename = "ScheduleRecipeWithModule")]
pub(super) struct ScheduleRecipeWithModulePayload<'a> {
    pub(super) requester_agent_id: &'a String,
    pub(super) factory_id: &'a String,
    pub(super) recipe_id: &'a String,
    pub(super) module_id: &'a String,
    pub(super) desired_batches: &'a u32,
    pub(super) deterministic_seed: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "ValidateProduct")]
pub(super) struct ValidateProductPayload<'a> {
    pub(super) requester_agent_id: &'a String,
    pub(super) module_id: &'a String,
    pub(super) stack: &'a MaterialStack,
    pub(super) decision: &'a ProductValidationDecision,
}

#[derive(Serialize)]
#[serde(rename = "ValidateProductWithModule")]
pub(super) struct ValidateProductWithModulePayload<'a> {
    pub(super) requester_agent_id: &'a String,
    pub(super) module_id: &'a String,
    pub(super) stack: &'a MaterialStack,
    pub(super) deterministic_seed: &'a u64,
}

#[derive(Serialize)]
#[serde(rename = "GovernMaterialProfile")]
pub(super) struct GovernMaterialProfilePayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) proposal_id: &'a ProposalId,
    pub(super) profile: &'a MaterialProfileV1,
}

#[derive(Serialize)]
#[serde(rename = "GovernProductProfile")]
pub(super) struct GovernProductProfilePayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) proposal_id: &'a ProposalId,
    pub(super) profile: &'a ProductProfileV1,
}

#[derive(Serialize)]
#[serde(rename = "GovernRecipeProfile")]
pub(super) struct GovernRecipeProfilePayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) proposal_id: &'a ProposalId,
    pub(super) profile: &'a RecipeProfileV1,
}

#[derive(Serialize)]
#[serde(rename = "GovernFactoryProfile")]
pub(super) struct GovernFactoryProfilePayload<'a> {
    pub(super) operator_agent_id: &'a String,
    pub(super) proposal_id: &'a ProposalId,
    pub(super) profile: &'a FactoryProfileV1,
}
