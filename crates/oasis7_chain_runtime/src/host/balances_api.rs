use std::path::Path;

use oasis7_runtime::World as RuntimeWorld;

use super::execution_bridge;
use super::{ChainBalancesResponse, DEFAULT_RECENT_MINT_RECORD_LIMIT, now_unix_ms};

pub(super) fn build_chain_balances_payload(
    node_id: &str,
    world_id: &str,
    execution_world_dir: &Path,
) -> ChainBalancesResponse {
    match execution_bridge::load_execution_world(execution_world_dir) {
        Ok(world) => {
            build_chain_balances_payload_from_world(node_id, world_id, execution_world_dir, &world)
        }
        Err(err) => ChainBalancesResponse {
            ok: true,
            observed_at_unix_ms: now_unix_ms(),
            node_id: node_id.to_string(),
            world_id: world_id.to_string(),
            execution_world_dir: execution_world_dir.display().to_string(),
            load_error: Some(err),
            node_asset_balance: None,
            node_power_credit_balance: 0,
            node_main_token_account: None,
            node_main_token_liquid_balance: 0,
            reward_mint_record_count: 0,
            recent_reward_mint_records: Vec::new(),
        },
    }
}

pub(super) fn build_chain_balances_payload_from_world(
    node_id: &str,
    world_id: &str,
    execution_world_dir: &Path,
    world: &RuntimeWorld,
) -> ChainBalancesResponse {
    let node_asset_balance = world.node_asset_balance(node_id).cloned();
    let node_power_credit_balance = world.node_power_credit_balance(node_id);
    let node_main_token_account = world
        .node_main_token_account(node_id)
        .map(|value| value.to_string());
    let node_main_token_liquid_balance = node_main_token_account
        .as_deref()
        .map(|account_id| world.main_token_liquid_balance(account_id))
        .unwrap_or(0);

    let all_records = world.reward_mint_records();
    let record_count = all_records.len();
    let keep = record_count.min(DEFAULT_RECENT_MINT_RECORD_LIMIT);
    let recent_reward_mint_records = all_records
        .iter()
        .skip(record_count.saturating_sub(keep))
        .cloned()
        .collect::<Vec<_>>();

    ChainBalancesResponse {
        ok: true,
        observed_at_unix_ms: now_unix_ms(),
        node_id: node_id.to_string(),
        world_id: world_id.to_string(),
        execution_world_dir: execution_world_dir.display().to_string(),
        load_error: None,
        node_asset_balance,
        node_power_credit_balance,
        node_main_token_account,
        node_main_token_liquid_balance,
        reward_mint_record_count: record_count,
        recent_reward_mint_records,
    }
}
