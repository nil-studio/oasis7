//! Exhaustive original pending-action dependency shapes.
use super::*;
use oasis7_wasm_abi::*;

tagged_codec!(MaterialTransitPriority, units {Urgent = 1, Standard = 2}, tuples {}, records {});
dto!(Observation, {time, agent_id, pos, visibility_range_cm, visible_agents});
dto!(ObservedAgent, {agent_id, pos, distance_cm});
dto!(GeoPos, {x_cm, y_cm, z_cm});
tagged_codec!(MainTokenFeeKind, units {GasBaseFee = 1, SlashPenalty = 2, ModuleFee = 3}, tuples {}, records {});
dto!(BodyKernelView, {mass_kg, radius_cm, thrust_limit, cross_section_cm2});
dto!(MainTokenGenesisAllocationPlan, {bucket_id, ratio_bps, recipient, cliff_epochs, linear_unlock_epochs, start_epoch});
dto!(MaterialProfileV1, {kind, tier, category, stack_limit, transport_loss_class, decay_bps_per_tick, default_priority});
tagged_codec!(MaterialDefaultPriority, units {Urgent = 1, Standard = 2}, tuples {}, records {});
tagged_codec!(MaterialTransportLossClass, units {Low = 1, Medium = 2, High = 3}, tuples {}, records {});
dto!(MainTokenConfig, {symbol, decimals, initial_supply, max_supply, inflation_policy, issuance_split, burn_policy});
dto!(MainTokenBurnPolicy, {gas_base_fee_burn_bps, slash_burn_bps, module_fee_burn_bps});
dto!(MainTokenIssuanceSplitPolicy, {staking_reward_bps, node_service_reward_bps, ecosystem_pool_bps, security_reserve_bps});
dto!(MainTokenInflationPolicy, {base_rate_bps, min_rate_bps, max_rate_bps, target_stake_ratio_bps, stake_feedback_gain_bps, epochs_per_year});
dto!(FactoryModuleSpec, {factory_id, display_name, tier, tags, build_cost, build_time_ticks, base_power_draw, recipe_slots, throughput_bps, maintenance_per_tick});
tagged_codec!(ResourceKind, units {Electricity = 1, Data = 2}, tuples {}, records {});
tagged_codec!(EconomicContractFulfillmentKind, units {AtomicExchange = 1, Service = 2}, tuples {}, records {});
dto!(NodeRewardMintRecord, {epoch_index, node_id, source_awarded_points, minted_power_credits, settlement_hash, signer_node_id, signature});
dto!(EpochSettlementReport, {epoch_index, pool_points, storage_pool_points, distributed_points, storage_distributed_points, total_distributed_points, settlements});
dto!(NodeSettlement, {node_id, obligation_met, compute_score, storage_score, uptime_score, reliability_score, storage_reward_score, rewardable_storage_bytes, penalty_score, total_score, main_awarded_points, storage_awarded_points, awarded_points, cumulative_points});
