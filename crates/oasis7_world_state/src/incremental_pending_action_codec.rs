//! Closed pending-action structural projection; tags are append-only, independent of serde wire.
use crate::events::ActionEnvelope;
use crate::incremental_typed_record_codec::*;
use oasis7_agent_api::world_action::*;

macro_rules! pending_action_field {
    (put, wasm_bytes, $profile:expr, $parent:expr, $value:expr, $ctx:expr, $work:expr) => {
        bytes::put_original($profile, $parent, "wasm_bytes", $value, $ctx, $work)
    };
    (decode, wasm_bytes, $cell:expr, $d:expr) => { bytes::decode($cell, $d) };
    (accept, wasm_bytes, $projection:expr, $profile:expr, $old:expr, $new:expr, $d:expr) => {
        bytes::accept_changed($profile, $old, $new, $d)
    };
    (put, $field:ident, $($args:expr),*) => { tagged_codec!(@default_field put, $field, $($args),*) };
    (decode, $field:ident, $($args:expr),*) => { tagged_codec!(@default_field decode, $field, $($args),*) };
    (accept, $field:ident, $($args:expr),*) => { tagged_codec!(@default_field accept, $field, $($args),*) };
}

tagged_codec!(@fields pending_action_field, Action, units {}, tuples {}, records {
    WorldRule { actor_id, request_id, command } = 1,
    WorldServiceIntent { request } = 2,
    RegisterAgent { agent_id, pos } = 3,
    MoveAgent { agent_id, to } = 4,
    QueryObservation { agent_id } = 5,
    EmitObservation { observation } = 6,
    BodyAction { agent_id, kind, payload } = 7,
    EmitBodyAttributes { agent_id, view, reason } = 8,
    ExpandBodyInterface { agent_id, interface_module_item_id } = 9,
    DeployModuleArtifact { publisher_agent_id, wasm_hash, wasm_bytes } = 10,
    CompileModuleArtifactFromSource { publisher_agent_id, module_id, source_package } = 11,
    InstallModuleFromArtifact { installer_agent_id, manifest, activate } = 12,
    InstallModuleFromArtifactWithFinality { installer_agent_id, manifest, activate, finality_certificate } = 13,
    InstallModuleToTargetFromArtifact { installer_agent_id, manifest, activate, install_target } = 14,
    InstallModuleToTargetFromArtifactWithFinality { installer_agent_id, manifest, activate, install_target, finality_certificate } = 15,
    UpgradeModuleFromArtifact { upgrader_agent_id, instance_id, from_module_version, manifest, activate } = 16,
    UpgradeModuleFromArtifactWithFinality { upgrader_agent_id, instance_id, from_module_version, manifest, activate, finality_certificate } = 17,
    ModuleReleaseSubmit { requester_agent_id, manifest, activate, install_target, required_roles, profile_changes } = 18,
    ModuleReleaseShadow { operator_agent_id, request_id } = 19,
    ModuleReleaseSubmitAttestation { operator_agent_id, request_id, signer_node_id, platform, build_manifest_hash, source_hash, wasm_hash, proof_cid, builder_image_digest, container_platform, canonicalizer_version } = 20,
    ModuleReleaseApproveRole { approver_agent_id, request_id, role } = 21,
    ModuleReleaseBindRoles { operator_agent_id, target_agent_id, roles } = 22,
    ModuleReleaseReject { rejector_agent_id, request_id, reason } = 23,
    ModuleReleaseApply { operator_agent_id, request_id } = 24,
    ModuleReleaseApplyWithFinality { operator_agent_id, request_id, finality_certificate } = 25,
    RollbackModuleInstance { operator_agent_id, instance_id, target_module_version } = 26,
    RollbackModuleInstanceWithFinality { operator_agent_id, instance_id, target_module_version, finality_certificate } = 27,
    ListModuleArtifactForSale { seller_agent_id, wasm_hash, price_kind, price_amount } = 28,
    BuyModuleArtifact { buyer_agent_id, wasm_hash } = 29,
    DelistModuleArtifact { seller_agent_id, wasm_hash } = 30,
    DestroyModuleArtifact { owner_agent_id, wasm_hash, reason } = 31,
    PlaceModuleArtifactBid { bidder_agent_id, wasm_hash, price_kind, price_amount } = 32,
    CancelModuleArtifactBid { bidder_agent_id, wasm_hash, bid_order_id } = 33,
    TransferResource { from_agent_id, to_agent_id, kind, amount } = 34,
    CollectData { collector_agent_id, electricity_cost, data_amount } = 35,
    CollectDataAuthenticated { collector_agent_id, electricity_cost, data_amount, player_id, public_key, nonce, signature } = 36,
    GrantDataAccess { owner_agent_id, grantee_agent_id } = 37,
    RevokeDataAccess { owner_agent_id, grantee_agent_id } = 38,
    RedeemPower { node_id, target_agent_id, redeem_credits, nonce } = 39,
    RedeemPowerSigned { node_id, target_agent_id, redeem_credits, nonce, signer_node_id, signature } = 40,
    ApplyNodePointsSettlementSigned { report, signer_node_id, mint_records } = 41,
    InitializeMainTokenGenesis { allocations } = 42,
    ClaimMainTokenVesting { bucket_id, beneficiary, nonce } = 43,
    TransferMainToken { from_account_id, to_account_id, amount, nonce, asset_id, memo, chain_id, network_id, tx_version, tx_type, valid_until_unix_ms, max_fee, fee_asset_id, application_payload_hash, client_request_id } = 44,
    ApplyMainTokenEpochIssuance { epoch_index, actual_stake_ratio_bps } = 45,
    SettleMainTokenFee { fee_kind, amount } = 46,
    UpdateMainTokenPolicy { proposal_id, next } = 47,
    DistributeMainTokenTreasury { proposal_id, distribution_id, bucket_id, distributions } = 48,
    TopUpRestrictedStarterClaimLiveopsPool { controller_account_id, top_up_id, amount } = 49,
    IssueRestrictedStarterClaimGrant { issuer_account_id, beneficiary_account_id, amount, issuance_reason, expires_at_epoch } = 50,
    RevokeRestrictedStarterClaimGrant { issuer_account_id, beneficiary_account_id, revoke_reason } = 51,
    ClaimAgent { claimer_agent_id, target_agent_id } = 52,
    ClaimStarterOc { agent_id, player_id, public_key } = 53,
    ReleaseAgentClaim { claimer_agent_id, target_agent_id } = 54,
    TransferMaterial { requester_agent_id, from_ledger, to_ledger, kind, amount, distance_km, priority, route_id, route_ids, auto_reroute } = 55,
    RegisterLogisticsRoute { requester_agent_id, from_ledger, to_ledger, kind, distance_km, priority, capacity_units, tariff_electricity_per_unit } = 56,
    SetLogisticsRouteAvailability { requester_agent_id, route_id, available } = 57,
    FormAlliance { proposer_agent_id, alliance_id, members, charter } = 58,
    JoinAlliance { operator_agent_id, alliance_id, member_agent_id } = 59,
    LeaveAlliance { operator_agent_id, alliance_id, member_agent_id } = 60,
    DissolveAlliance { operator_agent_id, alliance_id, reason } = 61,
    DeclareWar { initiator_agent_id, war_id, aggressor_alliance_id, defender_alliance_id, objective, intensity } = 62,
    OpenGovernanceProposal { proposer_agent_id, proposal_key, title, description, options, voting_window_ticks, quorum_weight, pass_threshold_bps } = 63,
    CastGovernanceVote { voter_agent_id, proposal_key, option, weight } = 64,
    ResolveCrisis { resolver_agent_id, crisis_id, strategy, success } = 65,
    GrantMetaProgress { operator_agent_id, target_agent_id, track, points, achievement_id } = 66,
    UpdateGameplayPolicy { operator_agent_id, electricity_tax_bps, data_tax_bps, power_trade_fee_bps, max_open_contracts_per_agent, blocked_agents, forbidden_location_ids } = 67,
    UpdateRestrictedStarterClaimAdminRegistry { controller_account_id, next_admin_account_ids } = 68,
    SubmitGovernanceValidatorAdmission { controller_account_id, candidate_id, node_id, finality_signer_public_key, stake, operator_owner, public_manifest_hash } = 69,
    ApproveGovernanceValidatorAdmission { controller_account_id, candidate_id } = 70,
    ActivateGovernanceValidatorAdmission { controller_account_id, candidate_id, activation_epoch } = 71,
    RevokeGovernanceValidatorAdmission { controller_account_id, candidate_id, node_id, reason } = 72,
    OpenEconomicContract { creator_agent_id, contract_id, counterparty_agent_id, fulfillment_kind, settlement_kind, settlement_amount, reputation_stake, expires_at, description } = 73,
    AcceptEconomicContract { accepter_agent_id, contract_id } = 74,
    SettleEconomicContract { operator_agent_id, contract_id, success, notes } = 75,
    EmitResourceTransfer { from_agent_id, to_agent_id, kind, amount } = 76,
    BuildFactory { builder_agent_id, site_id, spec } = 77,
    BuildFactoryWithModule { builder_agent_id, site_id, module_id, spec } = 78,
    MaintainFactory { operator_agent_id, factory_id, parts } = 79,
    RecycleFactory { operator_agent_id, factory_id } = 80,
    ScheduleEssentialRecipe { requester_agent_id, factory_id, recipe_id, batches, obligation_id } = 81,
    ScheduleRecipe { requester_agent_id, factory_id, recipe_id, plan, logistics_route_ids, logistics_path_ids } = 82,
    PauseFactoryProduction { requester_agent_id, factory_id, reason } = 83,
    ScheduleRecipeWithModule { requester_agent_id, factory_id, recipe_id, module_id, desired_batches, deterministic_seed } = 84,
    ValidateProduct { requester_agent_id, module_id, stack, decision } = 85,
    ValidateProductWithModule { requester_agent_id, module_id, stack, deterministic_seed } = 86,
    GovernMaterialProfile { operator_agent_id, proposal_id, profile } = 87,
    GovernProductProfile { operator_agent_id, proposal_id, profile } = 88,
    GovernRecipeProfile { operator_agent_id, proposal_id, profile } = 89,
    GovernFactoryProfile { operator_agent_id, proposal_id, profile } = 90,
    RotateGovernanceValidatorFinalityKeyV1 { controller_account_id, body, old_key_proof, new_key_proof } = 91,
    RotateGovernanceBootstrapFinalityKeyV1 { controller_account_id, body, old_key_proof, new_key_proof } = 92,
});
dto!(ActionEnvelope, {governance_authorization, committed_recipe_origin, id, action});

#[path = "incremental_pending_action_codec_dependencies.rs"]
mod dependencies;

#[cfg(test)]
#[path = "incremental_pending_action_codec_tests.rs"]
mod tests;

#[path = "incremental_pending_action_codec_files.rs"]
mod files;

#[path = "incremental_pending_action_codec_bytes.rs"]
mod bytes;

pub(crate) fn schema() -> [u8; 32] {
    *blake3::hash(b"oasis7.iws.pending-actions.columns.v1\0").as_bytes()
}
pub(crate) fn reference_schema(key: &ColumnKeyV1) -> Option<[u8; 32]> {
    Some(match key {
        ColumnKeyV1::Text(s) if s == "wasm_bytes" => bytes::schema(),
        ColumnKeyV1::Text(s) if s == "files" => files::schema(),
        _ => schema(),
    })
}
pub(crate) fn profile() -> TypedRecordProfileV1 {
    TypedRecordProfileV1 {
        field: FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: "pending_actions.columns",
            shape: FieldShape::Map,
            destination: FieldDestination::State(StateNamespaceV1::Scheduler),
        },
        schema: schema(),
        reference_schema: Some(reference_schema),
        labels: RecordTreeLabelsV1 {
            read: "pending action read",
            pending: "pending action pending",
            tree: "pending action tree",
            digest: "pending action digest",
            read_allowance: "pending action read_allowance",
            admission: "pending action admission",
            bound: "pending action bound",
            producer_pool: "pending action producer_pool",
            page: "pending action page",
            write_allowance: "pending action write_allowance",
        },
    }
}

#[path = "incremental_pending_action_codec_float.rs"]
mod float;
