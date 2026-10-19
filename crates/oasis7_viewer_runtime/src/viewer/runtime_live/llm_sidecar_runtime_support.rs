use super::super::mapping::runtime_state_to_simulator_model;
use super::*;
use crate::simulator::{WorldJournal, WorldSnapshot};
use oasis7_simulator::{CHUNK_GENERATION_SCHEMA_VERSION, SNAPSHOT_VERSION};

impl RuntimeLlmSidecar {
    /// Rebuild the simulator shadow from the authoritative Runtime snapshot
    /// before provider-backed cognition reads or queues any decision.
    pub(in crate::viewer::runtime_live) fn sync_shadow_kernel(
        &mut self,
        world: &RuntimeWorld,
        config: &WorldConfig,
    ) -> Result<(), String> {
        let runtime_snapshot = (!self.provider_service_required).then(|| world.snapshot());
        let state = if self.provider_service_required {
            &self
                .provider_service_projection
                .as_ref()
                .ok_or("canonical Agent observation unavailable")?
                .state
        } else {
            world.state()
        };
        let snapshot = WorldSnapshot {
            version: SNAPSHOT_VERSION,
            chunk_generation_schema_version: CHUNK_GENERATION_SCHEMA_VERSION,
            time: state.time,
            config: config.clone(),
            model: runtime_state_to_simulator_model(state, self, self.runtime_seed_model.as_ref()),
            runtime_snapshot: runtime_snapshot.clone(),
            player_gameplay: None,
            chain_resource_manifest: Default::default(),
            latest_chain_resource_delta: Default::default(),
            chunk_runtime: ChunkRuntimeConfig::default(),
            intel_ttl_ticks: 0,
            next_event_id: 0,
            next_action_id: runtime_snapshot
                .as_ref()
                .map(|snapshot| snapshot.next_action_id.max(1))
                .unwrap_or(1),
            pending_actions: Vec::new(),
            journal_len: 0,
        };
        self.shadow_kernel = Some(
            WorldKernel::from_snapshot(snapshot, WorldJournal::new())
                .map_err(|err| format!("runtime live shadow kernel rebuild failed: {err:?}"))?,
        );
        Ok(())
    }
}

pub(super) fn runtime_provider_check_now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub(in crate::viewer::runtime_live) fn resolve_runtime_live_llm_timeout_ms(
    configured_timeout_ms: u64,
) -> u64 {
    let configured_timeout_ms = configured_timeout_ms.max(1);
    let runtime_timeout_ceiling_ms = std::env::var(super::ENV_RUNTIME_LIVE_LLM_TIMEOUT_MS)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| value.max(1))
        .unwrap_or(super::DEFAULT_RUNTIME_LIVE_LLM_TIMEOUT_MS);
    configured_timeout_ms.min(runtime_timeout_ceiling_ms)
}

pub(super) fn runtime_provider_check_cache_key(settings: &ProviderDecisionSettings) -> String {
    // Private exact settings fence, including execution mode and fallback selection.
    format!("{settings:?}")
}

pub(in crate::viewer::runtime_live) fn normalize_optional_public_key(
    value: Option<&str>,
) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn hash_chat_message(message: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(message.as_bytes());
    hex::encode(hasher.finalize())
}

pub(in crate::viewer::runtime_live) fn simulator_action_label(action: &SimulatorAction) -> String {
    format!("{action:?}")
}

pub(in crate::viewer::runtime_live) fn simulator_action_to_runtime(
    action: &SimulatorAction,
    world: &RuntimeWorld,
) -> Option<RuntimeAction> {
    match action {
        SimulatorAction::RegisterAgent {
            agent_id,
            location_id,
        } => Some(RuntimeAction::RegisterAgent {
            agent_id: agent_id.clone(),
            pos: resolve_runtime_location(world, location_id)?,
        }),
        SimulatorAction::MoveAgent { agent_id, to } => Some(RuntimeAction::MoveAgent {
            agent_id: agent_id.clone(),
            to: resolve_runtime_location(world, to)?,
        }),
        SimulatorAction::TransferResource {
            from,
            to,
            kind,
            amount,
        } => match (from, to) {
            (
                ResourceOwner::Agent {
                    agent_id: from_agent_id,
                },
                ResourceOwner::Agent {
                    agent_id: to_agent_id,
                },
            ) => Some(RuntimeAction::TransferResource {
                from_agent_id: from_agent_id.clone(),
                to_agent_id: to_agent_id.clone(),
                kind: *kind,
                amount: *amount,
            }),
            _ => None,
        },
        SimulatorAction::FormAlliance {
            proposer_agent_id,
            alliance_id,
            members,
            charter,
        } => Some(RuntimeAction::FormAlliance {
            proposer_agent_id: proposer_agent_id.clone(),
            alliance_id: alliance_id.clone(),
            members: members.clone(),
            charter: charter.clone(),
        }),
        SimulatorAction::JoinAlliance {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => Some(RuntimeAction::JoinAlliance {
            operator_agent_id: operator_agent_id.clone(),
            alliance_id: alliance_id.clone(),
            member_agent_id: member_agent_id.clone(),
        }),
        SimulatorAction::LeaveAlliance {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => Some(RuntimeAction::LeaveAlliance {
            operator_agent_id: operator_agent_id.clone(),
            alliance_id: alliance_id.clone(),
            member_agent_id: member_agent_id.clone(),
        }),
        SimulatorAction::DissolveAlliance {
            operator_agent_id,
            alliance_id,
            reason,
        } => Some(RuntimeAction::DissolveAlliance {
            operator_agent_id: operator_agent_id.clone(),
            alliance_id: alliance_id.clone(),
            reason: reason.clone(),
        }),
        SimulatorAction::DeclareWar {
            initiator_agent_id,
            war_id,
            aggressor_alliance_id,
            defender_alliance_id,
            objective,
            intensity,
        } => Some(RuntimeAction::DeclareWar {
            initiator_agent_id: initiator_agent_id.clone(),
            war_id: war_id.clone(),
            aggressor_alliance_id: aggressor_alliance_id.clone(),
            defender_alliance_id: defender_alliance_id.clone(),
            objective: objective.clone(),
            intensity: *intensity,
        }),
        SimulatorAction::OpenGovernanceProposal {
            proposer_agent_id,
            proposal_key,
            title,
            description,
            options,
            voting_window_ticks,
            quorum_weight,
            pass_threshold_bps,
        } => Some(RuntimeAction::OpenGovernanceProposal {
            proposer_agent_id: proposer_agent_id.clone(),
            proposal_key: proposal_key.clone(),
            title: title.clone(),
            description: description.clone(),
            options: options.clone(),
            voting_window_ticks: *voting_window_ticks,
            quorum_weight: *quorum_weight,
            pass_threshold_bps: *pass_threshold_bps,
        }),
        SimulatorAction::CastGovernanceVote {
            voter_agent_id,
            proposal_key,
            option,
            weight,
        } => Some(RuntimeAction::CastGovernanceVote {
            voter_agent_id: voter_agent_id.clone(),
            proposal_key: proposal_key.clone(),
            option: option.clone(),
            weight: *weight,
        }),
        SimulatorAction::ResolveCrisis {
            resolver_agent_id,
            crisis_id,
            strategy,
            success,
        } => Some(RuntimeAction::ResolveCrisis {
            resolver_agent_id: resolver_agent_id.clone(),
            crisis_id: crisis_id.clone(),
            strategy: strategy.clone(),
            success: *success,
        }),
        SimulatorAction::GrantMetaProgress {
            operator_agent_id,
            target_agent_id,
            track,
            points,
            achievement_id,
        } => Some(RuntimeAction::GrantMetaProgress {
            operator_agent_id: operator_agent_id.clone(),
            target_agent_id: target_agent_id.clone(),
            track: track.clone(),
            points: *points,
            achievement_id: achievement_id.clone(),
        }),
        SimulatorAction::OpenEconomicContract {
            creator_agent_id,
            contract_id,
            counterparty_agent_id,
            settlement_kind,
            settlement_amount,
            reputation_stake,
            expires_at,
            description,
        } => Some(RuntimeAction::OpenEconomicContract {
            creator_agent_id: creator_agent_id.clone(),
            contract_id: contract_id.clone(),
            counterparty_agent_id: counterparty_agent_id.clone(),
            fulfillment_kind: crate::runtime::EconomicContractFulfillmentKind::AtomicExchange,
            settlement_kind: *settlement_kind,
            settlement_amount: *settlement_amount,
            reputation_stake: *reputation_stake,
            expires_at: *expires_at,
            description: description.clone(),
        }),
        SimulatorAction::AcceptEconomicContract {
            accepter_agent_id,
            contract_id,
        } => Some(RuntimeAction::AcceptEconomicContract {
            accepter_agent_id: accepter_agent_id.clone(),
            contract_id: contract_id.clone(),
        }),
        SimulatorAction::SettleEconomicContract {
            operator_agent_id,
            contract_id,
            success,
            notes,
        } => Some(RuntimeAction::SettleEconomicContract {
            operator_agent_id: operator_agent_id.clone(),
            contract_id: contract_id.clone(),
            success: *success,
            notes: notes.clone(),
        }),
        SimulatorAction::CompileModuleArtifactFromSource {
            publisher_agent_id,
            module_id,
            manifest_path,
            source_files,
        } => Some(RuntimeAction::CompileModuleArtifactFromSource {
            publisher_agent_id: publisher_agent_id.clone(),
            module_id: module_id.clone(),
            source_package: ModuleSourcePackage {
                manifest_path: manifest_path.clone(),
                files: source_files.clone(),
            },
        }),
        SimulatorAction::DeployModuleArtifact {
            publisher_agent_id,
            wasm_hash,
            wasm_bytes,
            ..
        } => Some(RuntimeAction::DeployModuleArtifact {
            publisher_agent_id: publisher_agent_id.clone(),
            wasm_hash: wasm_hash.clone(),
            wasm_bytes: wasm_bytes.clone(),
        }),
        SimulatorAction::ListModuleArtifactForSale {
            seller_agent_id,
            wasm_hash,
            price_kind,
            price_amount,
        } => Some(RuntimeAction::ListModuleArtifactForSale {
            seller_agent_id: seller_agent_id.clone(),
            wasm_hash: wasm_hash.clone(),
            price_kind: *price_kind,
            price_amount: *price_amount,
        }),
        SimulatorAction::BuyModuleArtifact {
            buyer_agent_id,
            wasm_hash,
        } => Some(RuntimeAction::BuyModuleArtifact {
            buyer_agent_id: buyer_agent_id.clone(),
            wasm_hash: wasm_hash.clone(),
        }),
        SimulatorAction::BuildFactory {
            owner: ResourceOwner::Agent { agent_id },
            location_id,
            factory_id,
            factory_kind,
        } => crate::viewer::gameplay_actions::runtime_factory_build_action(
            agent_id,
            location_id,
            factory_id,
            factory_kind,
        ),
        SimulatorAction::ScheduleRecipe {
            owner: ResourceOwner::Agent { agent_id },
            factory_id,
            recipe_id,
            batches,
        } => crate::viewer::gameplay_actions::runtime_schedule_recipe_action(
            agent_id,
            factory_id,
            recipe_id,
            (*batches).try_into().ok()?,
        ),
        SimulatorAction::DelistModuleArtifact {
            seller_agent_id,
            wasm_hash,
        } => Some(RuntimeAction::DelistModuleArtifact {
            seller_agent_id: seller_agent_id.clone(),
            wasm_hash: wasm_hash.clone(),
        }),
        SimulatorAction::DestroyModuleArtifact {
            owner_agent_id,
            wasm_hash,
            reason,
        } => Some(RuntimeAction::DestroyModuleArtifact {
            owner_agent_id: owner_agent_id.clone(),
            wasm_hash: wasm_hash.clone(),
            reason: reason.clone(),
        }),
        SimulatorAction::PlaceModuleArtifactBid {
            bidder_agent_id,
            wasm_hash,
            price_kind,
            price_amount,
        } => Some(RuntimeAction::PlaceModuleArtifactBid {
            bidder_agent_id: bidder_agent_id.clone(),
            wasm_hash: wasm_hash.clone(),
            price_kind: *price_kind,
            price_amount: *price_amount,
        }),
        SimulatorAction::CancelModuleArtifactBid {
            bidder_agent_id,
            wasm_hash,
            bid_order_id,
        } => Some(RuntimeAction::CancelModuleArtifactBid {
            bidder_agent_id: bidder_agent_id.clone(),
            wasm_hash: wasm_hash.clone(),
            bid_order_id: *bid_order_id,
        }),
        _ => None,
    }
}

fn resolve_runtime_location(world: &RuntimeWorld, location_id: &str) -> Option<GeoPos> {
    if let Some(pos) = parse_runtime_location_id(location_id) {
        return Some(pos);
    }
    world
        .state()
        .agents
        .values()
        .map(|cell| cell.state.pos)
        .find(|pos| location_id_for_pos(*pos) == location_id)
}

fn parse_runtime_location_id(location_id: &str) -> Option<GeoPos> {
    let raw = location_id.strip_prefix("runtime:")?;
    let mut parts = raw.split(':');
    let x = parts.next()?.parse::<i64>().ok()?;
    let y = parts.next()?.parse::<i64>().ok()?;
    let z = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(GeoPos::new(x, y, z))
}

pub(super) fn restore_behavior_long_term_memory_from_model(
    behavior: &mut LlmAgentBehavior<OpenAiChatCompletionClient>,
    kernel: &WorldKernel,
    agent_id: &str,
) {
    if let Some(entries) = kernel.long_term_memory_for_agent(agent_id) {
        behavior.restore_long_term_memory_entries(entries);
    } else {
        behavior.restore_long_term_memory_entries(&[]);
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn sync_llm_runner_long_term_memory(
    kernel: &mut WorldKernel,
    runner: &AgentRunner<LlmAgentBehavior<OpenAiChatCompletionClient>>,
) {
    for agent_id in runner.agent_ids() {
        let Some(agent) = runner.get(agent_id.as_str()) else {
            continue;
        };
        let entries = agent.behavior.export_long_term_memory_entries();
        if let Err(message) = kernel.set_agent_long_term_memory(agent_id.as_str(), entries) {
            crate::observability::emit_stderr_or_event(
                tracing::Level::WARN,
                format!(
                    "viewer runtime live: skip long-term memory sync for {}: {}",
                    agent_id, message
                )
                .as_str(),
                "viewer runtime live skipped long-term memory sync",
            );
        }
    }
}
