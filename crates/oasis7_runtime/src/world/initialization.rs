//! World construction and initial compatibility normalization.
use super::*;

impl World {
    pub fn new() -> Self {
        Self::new_with_state(WorldState::default())
    }

    pub fn new_with_release_security_policy(policy: ReleaseSecurityPolicy) -> Self {
        Self::new().with_release_security_policy(policy)
    }

    pub fn new_production_hardened() -> Self {
        Self::new_with_release_security_policy(ReleaseSecurityPolicy::production_hardened())
    }

    pub fn new_production_hardened_with_cognition_binding(
        world_id: impl Into<String>,
        branch_id: impl Into<String>,
        finality_epoch: u64,
        finality_block_hash: Option<String>,
        finality_status: impl Into<String>,
        reorg_epoch: u64,
    ) -> Result<Self, WorldError> {
        let mut world = Self::new_production_hardened();
        world.bind_cognition_runtime(
            world_id,
            branch_id,
            finality_epoch,
            finality_block_hash,
            finality_status,
            reorg_epoch,
        )?;
        Ok(world)
    }

    pub fn new_with_state(mut state: WorldState) -> Self {
        state.migrate_compat_material_ledgers();
        state
            .node_identity_bindings
            .entry(BUILTIN_MODULE_SIGNER_NODE_ID.to_string())
            .or_insert_with(|| BUILTIN_MODULE_SIGNER_PUBLIC_KEY_HEX.to_string());
        if let Some(registry) = state.governance_finality_signer_registry.clone() {
            for (node_id, public_key_hex) in registry.signer_bindings {
                state
                    .node_identity_bindings
                    .entry(node_id)
                    .or_insert(public_key_hex);
            }
        }
        for record in state.governance_validator_admissions.values() {
            if record.node_id.trim().is_empty()
                || record.finality_signer_public_key.trim().is_empty()
            {
                continue;
            }
            state
                .node_identity_bindings
                .entry(record.node_id.clone())
                .or_insert(record.finality_signer_public_key.clone());
            state
                .node_main_token_account_bindings
                .entry(record.node_id.clone())
                .or_insert_with(|| {
                    main_token_account_id_from_node_public_key(
                        record.finality_signer_public_key.as_str(),
                    )
                });
        }
        #[cfg(any(test, feature = "test_tier_required", feature = "test_tier_full"))]
        state
            .node_identity_bindings
            .entry(TEST_MODULE_SIGNER_NODE_ID.to_string())
            .or_insert_with(test_module_signer_public_key_hex);
        if state.governance_finality_signer_registry.is_none() {
            for (node_id, public_key_hex) in
                governance::local_governance_finality_signer_public_keys()
            {
                state
                    .node_identity_bindings
                    .entry(node_id)
                    .or_insert(public_key_hex);
            }
        }
        let mut world = Self {
            source_compiler: crate::module_source_compiler::RuntimeSourceCompiler::default(),
            manifest: Manifest::default(),
            cognition: default_cognition_persistence_projection(),
            module_registry: ModuleRegistry::default(),
            module_artifacts: BTreeSet::new(),
            module_artifact_bytes: BTreeMap::new(),
            module_cache: ModuleCache::default(),
            prepared_subscription_cache: BTreeMap::new(),
            module_limits_max: ModuleLimits::unbounded(),
            snapshot_catalog: SnapshotCatalog::default(),
            chain_resource_manifest: ChainResourceManifest::default(),
            latest_chain_resource_delta: None,
            state,
            formal_history_reader: None,
            bft_governance_parent: None,
            journal: Journal::new(),
            next_event_id: 1,
            next_event_id_era: 0,
            next_action_id: 1,
            next_action_id_era: 0,
            next_intent_id: 1,
            next_intent_id_era: 0,
            next_proposal_id: 1,
            next_proposal_id_era: 0,
            pending_actions: VecDeque::new(),
            pending_effects: VecDeque::new(),
            inflight_effects: BTreeMap::new(),
            module_tick_schedule: BTreeMap::new(),
            module_tick_routing_metrics: ModuleTickRoutingMetrics::default(),
            capabilities: BTreeMap::new(),
            capability_grants_v2: BTreeMap::new(),
            capability_revocation_state: CapabilityRevocationState::default(),
            capability_nonce_records: BTreeMap::new(),
            capability_authorization_receipts: BTreeMap::new(),
            capability_invocation_contexts: BTreeMap::new(),
            capability_authorization_root: String::new(),
            capability_budget_accounts: BTreeMap::new(),
            capability_effect_receipt_links: BTreeMap::new(),
            policies: PolicySet::default(),
            proposals: BTreeMap::new(),
            scheduler_cursor: None,
            receipt_signer: None,
            persistence_dir: RefCell::new(None),
            runtime_memory_limits: WorldRuntimeMemoryLimits::default(),
            runtime_backpressure_stats: WorldRuntimeBackpressureStats::default(),
            logistics_sla_metrics: LogisticsSlaMetrics::default(),
            threat_heatmap: BTreeMap::new(),
            tick_consensus_records: Vec::new(),
            formal_archived_tick_consensus_count: 0,
            formal_execution_tick: None,
            governance_replaying: false,
            tick_consensus_authority_source: default_tick_consensus_authority_source(),
            tick_consensus_rejection_audit_events: Vec::new(),
            governance_execution_policy: GovernanceExecutionPolicy::default(),
            governance_finality_epoch_snapshots: BTreeMap::new(),
            governance_emergency_brake_until_tick: None,
            governance_identity_penalties: BTreeMap::new(),
            next_governance_identity_penalty_id: default_next_governance_identity_penalty_id(),
            builtin_release_manifest: BuiltinReleaseManifestState::default(),
            release_security_policy: ReleaseSecurityPolicy::default(),
            devlocal_participant_authority: None,
            rollback_authority_registry: crate::RollbackAuthorityRegistry::default(),
            consumed_rollback_nonces: BTreeSet::new(),
            rollback_nonce_outcomes: BTreeMap::new(),
            #[cfg(test)]
            fail_next_append_after_reducer: false,
            #[cfg(test)]
            fail_next_append_after_publication_prepare: false,
            #[cfg(test)]
            fail_append_after_publication_prepare_countdown: None,
        };
        world
            .refresh_capability_authorization_root()
            .expect("empty capability authorization root is serializable");
        world
    }
}
