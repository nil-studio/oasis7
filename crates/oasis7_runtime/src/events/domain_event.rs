use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LogisticsOwnerPayout {
    pub owner_agent_id: String,
    #[serde(default)]
    pub electricity: i64,
}

/// Domain events that describe state changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DomainEvent {
    WorldRuleCommitted {
        action_id: u64,
        actor_id: String,
        request_id: String,
        command: oasis7_agent_api::world_action::WorldRuleCommandV1,
        governance_epoch: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        world_binding: Option<String>,
    },
    AgentRegistered {
        #[serde(default, skip_serializing_if = "is_legacy_claim_contract_version")]
        registration_contract_version: u8,
        agent_id: String,
        pos: GeoPos,
    },
    AgentMoved {
        agent_id: String,
        from: GeoPos,
        to: GeoPos,
    },
    /// Replayable authority update for an agent's canonical location.
    AgentLocationAuthorityUpdated {
        authority: AgentLocationAuthorityV1,
    },
    /// Replayable exact location identity update.
    LocationAnchorUpdated {
        anchor: LocationAnchorV1,
    },
    /// Replayable site registration/access/readiness update.
    FactorySiteAuthorityUpdated {
        authority: FactorySiteAuthorityV1,
    },
    /// Replayable M4 construction electricity profile update.
    FactoryConstructionPowerProfileUpdated {
        profile: FactoryConstructionPowerProfileV1,
    },
    ActionAccepted {
        action_id: ActionId,
        action_kind: String,
        actor_id: String,
        #[serde(default)]
        eta_ticks: u64,
        #[serde(default)]
        notes: Vec<String>,
    },
    ActionRejected {
        action_id: ActionId,
        reason: RejectReason,
    },
    AgentIntentProposed {
        intent: AgentIntentV2,
    },
    AgentIntentSubmitted {
        intent: AgentIntentV2,
    },
    AgentIntentAccepted {
        intent: AgentIntentV2,
    },
    AgentIntentReplaced {
        intent: AgentIntentV2,
    },
    AgentIntentTransitioned {
        intent: AgentIntentV2,
    },
    Observation {
        observation: Observation,
    },
    BodyAttributesUpdated {
        agent_id: String,
        view: BodyKernelView,
        reason: String,
    },
    BodyAttributesRejected {
        agent_id: String,
        reason: String,
    },
    BodyInterfaceExpanded {
        agent_id: String,
        slot_capacity: u16,
        expansion_level: u16,
        consumed_item_id: String,
        new_slot_id: String,
        slot_type: BodySlotType,
    },
    BodyInterfaceExpandRejected {
        agent_id: String,
        consumed_item_id: String,
        reason: String,
    },
    ModuleArtifactDeployed {
        publisher_agent_id: String,
        wasm_hash: String,
        bytes_len: u64,
        #[serde(default = "default_module_action_fee_kind")]
        fee_kind: ResourceKind,
        #[serde(default)]
        fee_amount: i64,
    },
    ModuleInstalled {
        installer_agent_id: String,
        #[serde(default)]
        instance_id: String,
        module_id: String,
        module_version: String,
        #[serde(default)]
        wasm_hash: String,
        #[serde(default)]
        install_target: ModuleInstallTarget,
        active: bool,
        proposal_id: ProposalId,
        manifest_hash: String,
        #[serde(default = "default_module_action_fee_kind")]
        fee_kind: ResourceKind,
        #[serde(default)]
        fee_amount: i64,
    },
    ModuleUpgraded {
        upgrader_agent_id: String,
        instance_id: String,
        module_id: String,
        from_module_version: String,
        to_module_version: String,
        wasm_hash: String,
        #[serde(default)]
        install_target: ModuleInstallTarget,
        active: bool,
        proposal_id: ProposalId,
        manifest_hash: String,
        #[serde(default = "default_module_action_fee_kind")]
        fee_kind: ResourceKind,
        #[serde(default)]
        fee_amount: i64,
    },
    ModuleReleaseRequested {
        request_id: u64,
        requester_agent_id: String,
        manifest: ModuleManifest,
        activate: bool,
        #[serde(default)]
        install_target: ModuleInstallTarget,
        #[serde(default)]
        required_roles: Vec<String>,
        #[serde(default)]
        profile_changes: ModuleProfileChanges,
    },
    ModuleReleaseShadowed {
        request_id: u64,
        operator_agent_id: String,
        manifest_hash: String,
    },
    ModuleReleaseAttested {
        request_id: u64,
        operator_agent_id: String,
        signer_node_id: String,
        platform: String,
        build_manifest_hash: String,
        source_hash: String,
        wasm_hash: String,
        proof_cid: String,
        #[serde(default)]
        builder_image_digest: String,
        #[serde(default)]
        container_platform: String,
        #[serde(default)]
        canonicalizer_version: String,
    },
    ModuleReleaseRoleApproved {
        request_id: u64,
        approver_agent_id: String,
        role: String,
    },
    ModuleReleaseRolesBound {
        operator_agent_id: String,
        target_agent_id: String,
        #[serde(default)]
        roles: Vec<String>,
    },
    ModuleReleaseRejected {
        request_id: u64,
        rejector_agent_id: String,
        reason: String,
    },
    ModuleReleaseApplied {
        request_id: u64,
        operator_agent_id: String,
        installer_agent_id: String,
        instance_id: String,
        module_id: String,
        module_version: String,
        proposal_id: ProposalId,
        manifest_hash: String,
    },
    ModuleRollbackApplied {
        operator_agent_id: String,
        instance_id: String,
        module_id: String,
        from_module_version: String,
        to_module_version: String,
        wasm_hash: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_wasm_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_release_request_id: Option<u64>,
        #[serde(default)]
        install_target: ModuleInstallTarget,
        active: bool,
        proposal_id: ProposalId,
        manifest_hash: String,
        #[serde(default = "default_module_action_fee_kind")]
        fee_kind: ResourceKind,
        #[serde(default)]
        fee_amount: i64,
    },
    ModuleArtifactListed {
        seller_agent_id: String,
        wasm_hash: String,
        price_kind: ResourceKind,
        price_amount: i64,
        #[serde(default)]
        order_id: u64,
        #[serde(default = "default_module_action_fee_kind")]
        fee_kind: ResourceKind,
        #[serde(default)]
        fee_amount: i64,
    },
    ModuleArtifactDelisted {
        seller_agent_id: String,
        wasm_hash: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        order_id: Option<u64>,
        #[serde(default = "default_module_action_fee_kind")]
        fee_kind: ResourceKind,
        #[serde(default)]
        fee_amount: i64,
    },
    ModuleArtifactDestroyed {
        owner_agent_id: String,
        wasm_hash: String,
        reason: String,
        #[serde(default = "default_module_action_fee_kind")]
        fee_kind: ResourceKind,
        #[serde(default)]
        fee_amount: i64,
    },
    ModuleArtifactBidPlaced {
        bidder_agent_id: String,
        wasm_hash: String,
        order_id: u64,
        price_kind: ResourceKind,
        price_amount: i64,
    },
    ModuleArtifactBidCancelled {
        bidder_agent_id: String,
        wasm_hash: String,
        order_id: u64,
        reason: String,
    },
    ModuleArtifactSaleCompleted {
        buyer_agent_id: String,
        seller_agent_id: String,
        wasm_hash: String,
        price_kind: ResourceKind,
        price_amount: i64,
        #[serde(default)]
        sale_id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        listing_order_id: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bid_order_id: Option<u64>,
    },
    ResourceTransferred {
        from_agent_id: String,
        to_agent_id: String,
        kind: ResourceKind,
        amount: i64,
    },
    DataCollected {
        collector_agent_id: String,
        electricity_cost: i64,
        data_amount: i64,
    },
    DataCollectedAuthenticated {
        collector_agent_id: String,
        electricity_cost: i64,
        data_amount: i64,
        player_id: String,
        public_key: String,
        nonce: u64,
        signature: String,
    },
    DataAccessGranted {
        owner_agent_id: String,
        grantee_agent_id: String,
    },
    DataAccessRevoked {
        owner_agent_id: String,
        grantee_agent_id: String,
    },
    PowerRedeemed {
        node_id: String,
        target_agent_id: String,
        burned_credits: u64,
        granted_power_units: i64,
        reserve_remaining: i64,
        nonce: u64,
    },
    PowerRedeemRejected {
        node_id: String,
        target_agent_id: String,
        redeem_credits: u64,
        nonce: u64,
        reason: String,
    },
    NodePointsSettlementApplied {
        report: EpochSettlementReport,
        signer_node_id: String,
        settlement_hash: String,
        minted_records: Vec<NodeRewardMintRecord>,
        #[serde(default)]
        main_token_bridge_total_amount: u64,
        #[serde(default)]
        main_token_bridge_distributions: Vec<MainTokenNodePointsBridgeDistribution>,
    },
    MainTokenGenesisInitialized {
        total_supply: u64,
        allocations: Vec<MainTokenGenesisAllocationBucketState>,
    },
    MainTokenVestingClaimed {
        bucket_id: String,
        beneficiary: String,
        amount: u64,
        nonce: u64,
    },
    MainTokenTransferred {
        from_account_id: String,
        to_account_id: String,
        amount: u64,
        nonce: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asset_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        memo: Option<String>,
    },
    MainTokenEpochIssued {
        epoch_index: u64,
        inflation_rate_bps: u32,
        issued_amount: u64,
        staking_reward_amount: u64,
        node_service_reward_amount: u64,
        ecosystem_pool_amount: u64,
        security_reserve_amount: u64,
    },
    MainTokenFeeSettled {
        fee_kind: MainTokenFeeKind,
        amount: u64,
        burn_amount: u64,
        treasury_amount: u64,
    },
    MainTokenPolicyUpdateScheduled {
        proposal_id: ProposalId,
        effective_epoch: u64,
        next: MainTokenConfig,
    },
    MainTokenTreasuryDistributed {
        proposal_id: ProposalId,
        distribution_id: String,
        bucket_id: String,
        total_amount: u64,
        distributions: Vec<MainTokenTreasuryDistribution>,
    },
    RestrictedStarterClaimLiveopsPoolToppedUp {
        controller_account_id: String,
        top_up_id: String,
        source_treasury_bucket_id: String,
        target_treasury_bucket_id: String,
        amount: u64,
        topped_up_at_epoch: u64,
    },
    RestrictedStarterClaimGrantIssued {
        issuer_id: String,
        beneficiary_account_id: String,
        source_treasury_bucket_id: String,
        amount: u64,
        issuance_reason: String,
        spend_scope: String,
        issued_at_epoch: u64,
        expires_at_epoch: u64,
    },
    RestrictedStarterClaimGrantExpired {
        beneficiary_account_id: String,
        issuer_id: String,
        issuance_reason: String,
        spend_scope: String,
        source_treasury_bucket_id: String,
        issued_amount: u64,
        expired_amount: u64,
        issued_at_epoch: u64,
        expired_at_epoch: u64,
        configured_expires_at_epoch: u64,
    },
    RestrictedStarterClaimGrantRevoked {
        beneficiary_account_id: String,
        issuer_id: String,
        issuance_reason: String,
        spend_scope: String,
        source_treasury_bucket_id: String,
        issued_amount: u64,
        revoked_amount: u64,
        issued_at_epoch: u64,
        revoked_at_epoch: u64,
        configured_expires_at_epoch: u64,
        revoke_reason: String,
    },
    StarterOcClaimed {
        agent_id: String,
        player_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        public_key: Option<String>,
        amount: u64,
        claimed_at: WorldTime,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_treasury_bucket_id: Option<String>,
    },
    AgentClaimed {
        #[serde(default, skip_serializing_if = "is_legacy_claim_contract_version")]
        acquisition_contract_version: u8,
        claimer_agent_id: String,
        target_agent_id: String,
        reputation_tier: u8,
        slot_index: u8,
        activation_fee_amount: u64,
        activation_fee_burn_amount: u64,
        activation_fee_treasury_amount: u64,
        claim_bond_amount: u64,
        #[serde(default)]
        upfront_restricted_spent_amount: u64,
        #[serde(default)]
        upfront_liquid_spent_amount: u64,
        #[serde(default)]
        auto_issued_restricted_amount: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auto_issued_restricted_source_treasury_bucket_id: Option<String>,
        #[serde(default)]
        claim_bond_locked_restricted_amount: u64,
        #[serde(default)]
        claim_bond_locked_liquid_amount: u64,
        upkeep_per_epoch: u64,
        claimed_at_epoch: u64,
        upkeep_paid_through_epoch: u64,
        release_cooldown_epochs: u64,
        grace_epochs: u64,
        idle_warning_epochs: u64,
        forced_idle_reclaim_epochs: u64,
        forced_reclaim_penalty_bps: u16,
    },
    AgentClaimReleaseRequested {
        claimer_agent_id: String,
        target_agent_id: String,
        requested_at_epoch: u64,
        ready_at_epoch: u64,
    },
    AgentClaimUpkeepSettled {
        #[serde(default, skip_serializing_if = "is_legacy_claim_contract_version")]
        activity_contract_version: u8,
        claimer_agent_id: String,
        target_agent_id: String,
        settled_at_epoch: u64,
        charged_epochs: u64,
        amount: u64,
        #[serde(default)]
        restricted_spent_amount: u64,
        #[serde(default)]
        liquid_spent_amount: u64,
        upkeep_paid_through_epoch: u64,
    },
    AgentClaimEnteredGrace {
        claimer_agent_id: String,
        target_agent_id: String,
        delinquent_since_epoch: u64,
        grace_deadline_epoch: u64,
        upkeep_arrears_amount: u64,
    },
    AgentClaimIdleWarning {
        claimer_agent_id: String,
        target_agent_id: String,
        warning_emitted_at_epoch: u64,
        forced_reclaim_at_epoch: u64,
    },
    AgentClaimReleased {
        claimer_agent_id: String,
        target_agent_id: String,
        released_at_epoch: u64,
        refunded_bond_amount: u64,
        #[serde(default)]
        refunded_bond_restricted_amount: u64,
        #[serde(default)]
        refunded_bond_liquid_amount: u64,
        #[serde(default)]
        refunded_bond_restricted_sink: RestrictedStarterClaimRefundSink,
        #[serde(default)]
        refunded_bond_restricted_sink_bucket_id: String,
    },
    AgentClaimReclaimed {
        claimer_agent_id: String,
        target_agent_id: String,
        reclaimed_at_epoch: u64,
        reason: String,
        upkeep_arrears_amount: u64,
        collected_upkeep_amount: u64,
        penalty_amount: u64,
        refunded_bond_amount: u64,
        #[serde(default)]
        refunded_bond_restricted_amount: u64,
        #[serde(default)]
        refunded_bond_liquid_amount: u64,
        #[serde(default)]
        refunded_bond_restricted_sink: RestrictedStarterClaimRefundSink,
        #[serde(default)]
        refunded_bond_restricted_sink_bucket_id: String,
    },
    MaterialTransferred {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transfer_id: Option<ActionId>,
        requester_agent_id: String,
        from_ledger: MaterialLedgerId,
        to_ledger: MaterialLedgerId,
        kind: String,
        amount: i64,
        distance_km: i64,
        #[serde(default = "default_material_transit_priority")]
        priority: MaterialTransitPriority,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        route_id: Option<String>,
    },
    LogisticsRouteRegistered {
        requester_agent_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        route_id: Option<String>,
        from_ledger: MaterialLedgerId,
        to_ledger: MaterialLedgerId,
        kind: String,
        distance_km: i64,
        #[serde(default = "default_material_transit_priority")]
        priority: MaterialTransitPriority,
        #[serde(default)]
        owner_agent_id: String,
        #[serde(default = "default_logistics_route_available")]
        available: bool,
        #[serde(default = "default_logistics_capacity_units")]
        capacity_units: i64,
        #[serde(default)]
        tariff_electricity_per_unit: i64,
    },
    LogisticsRouteAvailabilityChanged {
        requester_agent_id: String,
        route_id: String,
        available: bool,
        #[serde(default)]
        owner_agent_id: String,
    },
    LogisticsPathRerouted {
        requester_agent_id: String,
        job_id: ActionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original_path_id: Option<String>,
        #[serde(default)]
        original_route_ids: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effective_path_id: Option<String>,
        #[serde(default)]
        effective_route_ids: Vec<String>,
        reason: String,
        #[serde(default)]
        reroute_count: u32,
        #[serde(default)]
        tariff_electricity_total: i64,
        #[serde(default)]
        owner_payouts: Vec<LogisticsOwnerPayout>,
        #[serde(default)]
        governance_tax_electricity: i64,
    },
    MaterialTransitStarted {
        job_id: ActionId,
        requester_agent_id: String,
        from_ledger: MaterialLedgerId,
        to_ledger: MaterialLedgerId,
        kind: String,
        amount: i64,
        distance_km: i64,
        loss_bps: i64,
        ready_at: WorldTime,
        #[serde(default = "default_material_transit_priority")]
        priority: MaterialTransitPriority,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        route_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path_id: Option<String>,
        #[serde(default)]
        route_ids: Vec<String>,
        #[serde(default)]
        tariff_electricity_total: i64,
        #[serde(default)]
        reroute_count: u32,
    },
    MaterialTransitCompleted {
        job_id: ActionId,
        requester_agent_id: String,
        from_ledger: MaterialLedgerId,
        to_ledger: MaterialLedgerId,
        kind: String,
        sent_amount: i64,
        received_amount: i64,
        loss_amount: i64,
        distance_km: i64,
        #[serde(default = "default_material_transit_priority")]
        priority: MaterialTransitPriority,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        route_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path_id: Option<String>,
        #[serde(default)]
        route_ids: Vec<String>,
        #[serde(default)]
        tariff_electricity_total: i64,
        #[serde(default)]
        reroute_count: u32,
    },
    FactoryBuildStarted {
        job_id: ActionId,
        builder_agent_id: String,
        site_id: String,
        spec: FactoryModuleSpec,
        #[serde(default = "default_world_material_ledger")]
        consume_ledger: MaterialLedgerId,
        ready_at: WorldTime,
        /// An omitted version is distinct from an explicitly classified
        /// historical (`Some(0)`) event. Current producers must emit
        /// `Some(1)` with complete admission facts; this distinction prevents
        /// serde omission from silently downgrading a modern event.
        #[serde(default)]
        contract_version: Option<u8>,
        /// Optional for backward-compatible replay of pre-authority events.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        site_authority_revision: Option<u64>,
        /// Optional for backward-compatible replay of pre-authority events.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        site_location_id: Option<String>,
        /// Optional for backward-compatible replay of pre-location-anchor events.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location_anchor_revision: Option<u64>,
        /// Optional for backward-compatible replay of pre-power-obligation events.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        construction_power_obligation: Option<FactoryBuildPowerObligationV1>,
    },
    FactoryBuilt {
        job_id: ActionId,
        builder_agent_id: String,
        site_id: String,
        spec: FactoryModuleSpec,
    },
    FactoryDurabilityChanged {
        factory_id: String,
        previous_durability_ppm: i64,
        durability_ppm: i64,
        reason: String,
    },
    FactoryMaintained {
        operator_agent_id: String,
        factory_id: String,
        #[serde(default = "default_world_material_ledger")]
        consume_ledger: MaterialLedgerId,
        consumed_parts: i64,
        durability_ppm: i64,
    },
    FactoryRecycled {
        operator_agent_id: String,
        factory_id: String,
        #[serde(default = "default_world_material_ledger")]
        recycle_ledger: MaterialLedgerId,
        recovered: Vec<MaterialStack>,
        durability_ppm: i64,
    },
    RecipeStarted {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emergency_power_obligation: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        committed_recipe_origin: Option<super::CommittedRecipeOrigin>,
        job_id: ActionId,
        requester_agent_id: String,
        factory_id: String,
        recipe_id: String,
        accepted_batches: u32,
        consume: Vec<MaterialStack>,
        produce: Vec<MaterialStack>,
        byproducts: Vec<MaterialStack>,
        power_required: i64,
        /// Explicit electricity payer for the recipe commitment.  Legacy
        /// events omit this field and are rejected by the reducer rather than
        /// implicitly charging the world pool.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        power_owner_agent_id: Option<String>,
        duration_ticks: u32,
        #[serde(default = "default_world_material_ledger")]
        consume_ledger: MaterialLedgerId,
        #[serde(default = "default_world_material_ledger")]
        output_ledger: MaterialLedgerId,
        #[serde(default)]
        bottleneck_tags: Vec<String>,
        #[serde(default)]
        market_quotes: Vec<MaterialMarketQuote>,
        #[serde(default)]
        logistics_route_ids: Vec<String>,
        #[serde(default)]
        logistics_path_ids: Vec<String>,
        ready_at: WorldTime,
    },
    RecipeCompleted {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        committed_recipe_origin: Option<super::CommittedRecipeOrigin>,
        job_id: ActionId,
        requester_agent_id: String,
        factory_id: String,
        recipe_id: String,
        accepted_batches: u32,
        produce: Vec<MaterialStack>,
        byproducts: Vec<MaterialStack>,
        #[serde(default = "default_world_material_ledger")]
        output_ledger: MaterialLedgerId,
        #[serde(default)]
        bottleneck_tags: Vec<String>,
        #[serde(default)]
        logistics_route_ids: Vec<String>,
        #[serde(default)]
        logistics_path_ids: Vec<String>,
    },
    FactoryProductionBlocked {
        action_id: ActionId,
        requester_agent_id: String,
        factory_id: String,
        recipe_id: String,
        blocker_kind: String,
        blocker_detail: String,
    },
    FactoryProductionResumed {
        job_id: ActionId,
        requester_agent_id: String,
        factory_id: String,
        recipe_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        previous_blocked_at: Option<WorldTime>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        previous_blocker_kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        previous_blocker_detail: Option<String>,
    },
    FactoryProductionPaused {
        action_id: ActionId,
        requester_agent_id: String,
        factory_id: String,
        reason: String,
    },
    GameplayPolicyUpdated {
        operator_agent_id: String,
        electricity_tax_bps: u16,
        data_tax_bps: u16,
        power_trade_fee_bps: u16,
        max_open_contracts_per_agent: u16,
        blocked_agents: Vec<String>,
        forbidden_location_ids: Vec<String>,
    },
    EconomicContractOpened {
        creator_agent_id: String,
        contract_id: String,
        counterparty_agent_id: String,
        #[serde(default)]
        fulfillment_kind: EconomicContractFulfillmentKind,
        settlement_kind: ResourceKind,
        settlement_amount: i64,
        reputation_stake: i64,
        expires_at: WorldTime,
        description: String,
    },
    EconomicContractAccepted {
        accepter_agent_id: String,
        contract_id: String,
    },
    EconomicContractSettled {
        operator_agent_id: String,
        contract_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        world_binding: Option<String>,
        success: bool,
        transfer_amount: i64,
        tax_amount: i64,
        notes: String,
        creator_reputation_delta: i64,
        counterparty_reputation_delta: i64,
    },
    EconomicContractExpired {
        contract_id: String,
        creator_agent_id: String,
        counterparty_agent_id: String,
        creator_reputation_delta: i64,
        counterparty_reputation_delta: i64,
    },
    AllianceFormed {
        proposer_agent_id: String,
        alliance_id: String,
        members: Vec<String>,
        charter: String,
    },
    AllianceJoined {
        operator_agent_id: String,
        alliance_id: String,
        member_agent_id: String,
    },
    AllianceLeft {
        operator_agent_id: String,
        alliance_id: String,
        member_agent_id: String,
    },
    AllianceDissolved {
        operator_agent_id: String,
        alliance_id: String,
        reason: String,
        #[serde(default)]
        former_members: Vec<String>,
    },
    WarDeclared {
        initiator_agent_id: String,
        war_id: String,
        aggressor_alliance_id: String,
        defender_alliance_id: String,
        objective: String,
        intensity: u32,
        #[serde(default)]
        mobilization_electricity_cost: i64,
        #[serde(default)]
        mobilization_data_cost: i64,
    },
    WarConcluded {
        war_id: String,
        winner_alliance_id: String,
        #[serde(default)]
        loser_alliance_id: String,
        aggressor_score: i64,
        defender_score: i64,
        summary: String,
        #[serde(default)]
        participant_outcomes: Vec<WarParticipantOutcome>,
    },
    GovernanceProposalOpened {
        proposer_agent_id: String,
        proposal_key: String,
        title: String,
        description: String,
        options: Vec<String>,
        voting_window_ticks: u64,
        closes_at: WorldTime,
        quorum_weight: u64,
        pass_threshold_bps: u16,
    },
    GovernanceVoteCast {
        voter_agent_id: String,
        proposal_key: String,
        option: String,
        weight: u32,
    },
    GovernanceProposalFinalized {
        proposal_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        winning_option: Option<String>,
        winning_weight: u64,
        total_weight: u64,
        passed: bool,
    },
    CrisisSpawned {
        crisis_id: String,
        kind: String,
        severity: u32,
        expires_at: WorldTime,
    },
    CrisisResolved {
        resolver_agent_id: String,
        crisis_id: String,
        strategy: String,
        success: bool,
        impact: i64,
    },
    CrisisTimedOut {
        crisis_id: String,
        penalty_impact: i64,
    },
    MetaProgressGranted {
        operator_agent_id: String,
        target_agent_id: String,
        track: String,
        points: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        achievement_id: Option<String>,
    },
    ProductValidated {
        requester_agent_id: String,
        module_id: String,
        stack: MaterialStack,
        stack_limit: u32,
        tradable: bool,
        quality_levels: Vec<String>,
        notes: Vec<String>,
    },
    /// Durable product-module decision for a rejected output. The generic
    /// ActionRejected event remains in the journal for the caller, while this
    /// runtime event makes crash/retry reuse the settled module decision.
    ProductValidationRecorded {
        receipt: ProductValidationReceiptV1,
    },
    /// Durable pre-call intent for job-scoped product validation. A retry
    /// seeing this without a receipt must fail closed instead of invoking the
    /// validator twice.
    ProductValidationAttemptStarted {
        attempt: ProductValidationAttemptV1,
    },
    MaterialProfileGoverned {
        operator_agent_id: String,
        proposal_id: ProposalId,
        profile: MaterialProfileV1,
    },
    ProductProfileGoverned {
        operator_agent_id: String,
        proposal_id: ProposalId,
        profile: ProductProfileV1,
    },
    RecipeProfileGoverned {
        operator_agent_id: String,
        proposal_id: ProposalId,
        profile: RecipeProfileV1,
    },
    FactoryProfileGoverned {
        operator_agent_id: String,
        proposal_id: ProposalId,
        profile: FactoryProfileV1,
    },
}

#[path = "domain_event_rejections.rs"]
mod domain_event_rejections;
pub use domain_event_rejections::{CausedBy, RejectReason};
