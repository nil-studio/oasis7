use std::collections::BTreeMap;
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex};

use oasis7_node::NodeRuntime;
use oasis7_runtime::{EconomicContractState, EconomicContractStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const EXPLORER_ADDRESS_PATH: &str = "/v1/chain/explorer/address";
const EXPLORER_CONTRACTS_PATH: &str = "/v1/chain/explorer/contracts";
const EXPLORER_CONTRACT_PATH: &str = "/v1/chain/explorer/contract";
const EXPLORER_ASSETS_PATH: &str = "/v1/chain/explorer/assets";
const EXPLORER_MEMPOOL_PATH: &str = "/v1/chain/explorer/mempool";
const DEFAULT_RECENT_TX_LIMIT: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerTransferTxItem {
    pub(super) tx_hash: String,
    pub(super) action_id: u64,
    pub(super) from_account_id: String,
    pub(super) to_account_id: String,
    pub(super) amount: u64,
    pub(super) nonce: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) memo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) chain_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) network_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tx_version: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tx_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) valid_until_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_fee: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) fee_asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) application_payload_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) client_request_id: Option<String>,
    pub(super) status: super::TransferLifecycleStatus,
    pub(super) submitted_at_unix_ms: i64,
    pub(super) updated_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerAddressResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) account_id: Option<String>,
    pub(super) liquid_balance: u64,
    pub(super) vested_balance: u64,
    pub(super) restricted_starter_claim_balance: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) last_transfer_nonce: Option<u64>,
    pub(super) next_nonce_hint: u64,
    pub(super) limit: usize,
    pub(super) cursor: usize,
    pub(super) total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) next_cursor: Option<usize>,
    #[serde(default)]
    pub(super) items: Vec<ExplorerTransferTxItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerContractListItem {
    pub(super) contract_id: String,
    pub(super) contract_type: String,
    pub(super) status: EconomicContractStatus,
    pub(super) creator_agent_id: String,
    pub(super) counterparty_agent_id: String,
    pub(super) settlement_kind: String,
    pub(super) settlement_amount: i64,
    pub(super) expires_at: u64,
    pub(super) summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerContractsResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    pub(super) limit: usize,
    pub(super) cursor: usize,
    pub(super) total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) next_cursor: Option<usize>,
    #[serde(default)]
    pub(super) items: Vec<ExplorerContractListItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerContractResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) contract_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) contract: Option<EconomicContractState>,
    #[serde(default)]
    pub(super) recent_txs: Vec<ExplorerTransferTxItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerAssetHolderItem {
    pub(super) account_id: String,
    pub(super) liquid_balance: u64,
    pub(super) vested_balance: u64,
    pub(super) restricted_starter_claim_balance: u64,
    pub(super) total_balance: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) last_transfer_nonce: Option<u64>,
    pub(super) next_nonce_hint: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerAssetsResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    pub(super) token_symbol: String,
    pub(super) token_decimals: u8,
    pub(super) total_supply: u64,
    pub(super) circulating_supply: u64,
    pub(super) total_issued: u64,
    pub(super) total_burned: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) account_filter: Option<String>,
    pub(super) limit: usize,
    pub(super) cursor: usize,
    pub(super) total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) next_cursor: Option<usize>,
    #[serde(default)]
    pub(super) holders: Vec<ExplorerAssetHolderItem>,
    pub(super) nft_supported: bool,
    #[serde(default)]
    pub(super) nft_collections: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerMempoolResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    pub(super) status_filter: String,
    pub(super) accepted_count: usize,
    pub(super) pending_count: usize,
    pub(super) limit: usize,
    pub(super) cursor: usize,
    pub(super) total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) next_cursor: Option<usize>,
    #[serde(default)]
    pub(super) items: Vec<ExplorerTransferTxItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MempoolStatusFilter {
    All,
    Accepted,
    Pending,
}

pub(super) fn maybe_handle_explorer_p1_request(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    path: &str,
    execution_world_dir: &Path,
    head_only: bool,
) -> Result<bool, String> {
    match path {
        EXPLORER_ADDRESS_PATH => handle_explorer_address(
            stream,
            request_bytes,
            runtime,
            execution_world_dir,
            head_only,
        ),
        EXPLORER_CONTRACTS_PATH => {
            handle_explorer_contracts(stream, request_bytes, execution_world_dir, head_only)
        }
        EXPLORER_CONTRACT_PATH => handle_explorer_contract(
            stream,
            request_bytes,
            runtime,
            execution_world_dir,
            head_only,
        ),
        EXPLORER_ASSETS_PATH => {
            handle_explorer_assets(stream, request_bytes, execution_world_dir, head_only)
        }
        EXPLORER_MEMPOOL_PATH => handle_explorer_mempool(stream, request_bytes, runtime, head_only),
        _ => Ok(false),
    }
}

fn handle_explorer_address(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    execution_world_dir: &Path,
    head_only: bool,
) -> Result<bool, String> {
    let target = super::parse_http_target(request_bytes)?;
    let params = super::parse_query_params(target.as_str());
    let (limit, cursor) = match parse_page_params(&params) {
        Ok(values) => values,
        Err(err) => {
            let response = ExplorerAddressResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                account_id: None,
                liquid_balance: 0,
                vested_balance: 0,
                restricted_starter_claim_balance: 0,
                last_transfer_nonce: None,
                next_nonce_hint: 1,
                limit: super::DEFAULT_HISTORY_LIMIT,
                cursor: 0,
                total: 0,
                next_cursor: None,
                items: Vec::new(),
                error_code: Some(super::TRANSFER_ERROR_INVALID_REQUEST.to_string()),
                error: Some(err),
            };
            super::write_transfer_json_response(stream, 400, &response, head_only)?;
            return Ok(true);
        }
    };

    let Some(raw_account_id) = params
        .get("account_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        let response = ExplorerAddressResponse {
            ok: false,
            observed_at_unix_ms: super::super::now_unix_ms(),
            account_id: None,
            liquid_balance: 0,
            vested_balance: 0,
            restricted_starter_claim_balance: 0,
            last_transfer_nonce: None,
            next_nonce_hint: 1,
            limit,
            cursor: 0,
            total: 0,
            next_cursor: None,
            items: Vec::new(),
            error_code: Some(super::TRANSFER_ERROR_INVALID_REQUEST.to_string()),
            error: Some("query parameter account_id is required".to_string()),
        };
        super::write_transfer_json_response(stream, 400, &response, head_only)?;
        return Ok(true);
    };

    let account_id = match super::normalize_account_id(raw_account_id, "account_id") {
        Ok(value) => value,
        Err(err) => {
            let response = ExplorerAddressResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                account_id: None,
                liquid_balance: 0,
                vested_balance: 0,
                restricted_starter_claim_balance: 0,
                last_transfer_nonce: None,
                next_nonce_hint: 1,
                limit,
                cursor: 0,
                total: 0,
                next_cursor: None,
                items: Vec::new(),
                error_code: Some(super::TRANSFER_ERROR_INVALID_REQUEST.to_string()),
                error: Some(err),
            };
            super::write_transfer_json_response(stream, 400, &response, head_only)?;
            return Ok(true);
        }
    };

    let world = match super::super::execution_bridge::load_execution_world(execution_world_dir) {
        Ok(world) => world,
        Err(err) => {
            let response = ExplorerAddressResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                account_id: Some(account_id),
                liquid_balance: 0,
                vested_balance: 0,
                restricted_starter_claim_balance: 0,
                last_transfer_nonce: None,
                next_nonce_hint: 1,
                limit,
                cursor: 0,
                total: 0,
                next_cursor: None,
                items: Vec::new(),
                error_code: Some(super::TRANSFER_ERROR_INTERNAL.to_string()),
                error: Some(err),
            };
            super::write_transfer_json_response(stream, 200, &response, head_only)?;
            return Ok(true);
        }
    };

    let balance = world
        .main_token_account_balance(account_id.as_str())
        .cloned();
    let last_nonce = world.main_token_last_transfer_nonce(account_id.as_str());

    let mut tracker = super::lock_transfer_tracker();
    super::sync_tracker_from_runtime(runtime, &mut tracker)?;
    tracker.refresh_lifecycle_by_time(super::super::now_unix_ms());
    let (_, records) = tracker.query_history(
        Some(account_id.as_str()),
        None,
        None,
        super::MAX_TRACKED_TRANSFER_RECORDS,
    );
    let tx_items = records
        .into_iter()
        .map(record_to_tx_item)
        .collect::<Vec<ExplorerTransferTxItem>>();

    if balance.is_none() && last_nonce.is_none() && tx_items.is_empty() {
        let response = ExplorerAddressResponse {
            ok: false,
            observed_at_unix_ms: super::super::now_unix_ms(),
            account_id: Some(account_id),
            liquid_balance: 0,
            vested_balance: 0,
            restricted_starter_claim_balance: 0,
            last_transfer_nonce: None,
            next_nonce_hint: 1,
            limit,
            cursor: 0,
            total: 0,
            next_cursor: None,
            items: Vec::new(),
            error_code: Some(super::TRANSFER_ERROR_NOT_FOUND.to_string()),
            error: Some("address not found".to_string()),
        };
        super::write_transfer_json_response(stream, 200, &response, head_only)?;
        return Ok(true);
    }

    let (cursor, total, next_cursor, page_items) = paginate(tx_items, limit, cursor);
    let balance = balance.unwrap_or_default();
    let response = ExplorerAddressResponse {
        ok: true,
        observed_at_unix_ms: super::super::now_unix_ms(),
        account_id: Some(account_id),
        liquid_balance: balance.liquid_balance,
        vested_balance: balance.vested_balance,
        restricted_starter_claim_balance: balance.restricted_starter_claim_balance,
        last_transfer_nonce: last_nonce,
        next_nonce_hint: last_nonce.unwrap_or(0).saturating_add(1),
        limit,
        cursor,
        total,
        next_cursor,
        items: page_items,
        error_code: None,
        error: None,
    };
    super::write_transfer_json_response(stream, 200, &response, head_only)?;
    Ok(true)
}

fn handle_explorer_contracts(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    execution_world_dir: &Path,
    head_only: bool,
) -> Result<bool, String> {
    let target = super::parse_http_target(request_bytes)?;
    let params = super::parse_query_params(target.as_str());
    let (limit, cursor) = match parse_page_params(&params) {
        Ok(values) => values,
        Err(err) => {
            let response = ExplorerContractsResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                limit: super::DEFAULT_HISTORY_LIMIT,
                cursor: 0,
                total: 0,
                next_cursor: None,
                items: Vec::new(),
                error_code: Some(super::TRANSFER_ERROR_INVALID_REQUEST.to_string()),
                error: Some(err),
            };
            super::write_transfer_json_response(stream, 400, &response, head_only)?;
            return Ok(true);
        }
    };

    let world = match super::super::execution_bridge::load_execution_world(execution_world_dir) {
        Ok(world) => world,
        Err(err) => {
            let response = ExplorerContractsResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                limit,
                cursor: 0,
                total: 0,
                next_cursor: None,
                items: Vec::new(),
                error_code: Some(super::TRANSFER_ERROR_INTERNAL.to_string()),
                error: Some(err),
            };
            super::write_transfer_json_response(stream, 200, &response, head_only)?;
            return Ok(true);
        }
    };

    let mut items = world
        .state()
        .economic_contracts
        .values()
        .map(contract_to_list_item)
        .collect::<Vec<ExplorerContractListItem>>();
    items.sort_by(|left, right| left.contract_id.cmp(&right.contract_id));

    let (cursor, total, next_cursor, page_items) = paginate(items, limit, cursor);
    let response = ExplorerContractsResponse {
        ok: true,
        observed_at_unix_ms: super::super::now_unix_ms(),
        limit,
        cursor,
        total,
        next_cursor,
        items: page_items,
        error_code: None,
        error: None,
    };
    super::write_transfer_json_response(stream, 200, &response, head_only)?;
    Ok(true)
}

fn handle_explorer_contract(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    execution_world_dir: &Path,
    head_only: bool,
) -> Result<bool, String> {
    let target = super::parse_http_target(request_bytes)?;
    let params = super::parse_query_params(target.as_str());

    let Some(contract_id) = params
        .get("contract_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    else {
        let response = ExplorerContractResponse {
            ok: false,
            observed_at_unix_ms: super::super::now_unix_ms(),
            contract_id: None,
            contract: None,
            recent_txs: Vec::new(),
            error_code: Some(super::TRANSFER_ERROR_INVALID_REQUEST.to_string()),
            error: Some("query parameter contract_id is required".to_string()),
        };
        super::write_transfer_json_response(stream, 400, &response, head_only)?;
        return Ok(true);
    };

    let world = match super::super::execution_bridge::load_execution_world(execution_world_dir) {
        Ok(world) => world,
        Err(err) => {
            let response = ExplorerContractResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                contract_id: Some(contract_id),
                contract: None,
                recent_txs: Vec::new(),
                error_code: Some(super::TRANSFER_ERROR_INTERNAL.to_string()),
                error: Some(err),
            };
            super::write_transfer_json_response(stream, 200, &response, head_only)?;
            return Ok(true);
        }
    };

    let contract = world
        .state()
        .economic_contracts
        .get(contract_id.as_str())
        .cloned();
    let Some(contract) = contract else {
        let response = ExplorerContractResponse {
            ok: false,
            observed_at_unix_ms: super::super::now_unix_ms(),
            contract_id: Some(contract_id),
            contract: None,
            recent_txs: Vec::new(),
            error_code: Some(super::TRANSFER_ERROR_NOT_FOUND.to_string()),
            error: Some("contract not found".to_string()),
        };
        super::write_transfer_json_response(stream, 200, &response, head_only)?;
        return Ok(true);
    };

    let mut tracker = super::lock_transfer_tracker();
    super::sync_tracker_from_runtime(runtime, &mut tracker)?;
    tracker.refresh_lifecycle_by_time(super::super::now_unix_ms());
    let (_, records) = tracker.query_history(None, None, None, super::MAX_TRACKED_TRANSFER_RECORDS);
    let mut recent_txs = records
        .into_iter()
        .filter(|record| {
            let from_matches = record.from_account_id == contract.creator_agent_id
                || record.from_account_id == contract.counterparty_agent_id;
            let to_matches = record.to_account_id == contract.creator_agent_id
                || record.to_account_id == contract.counterparty_agent_id;
            from_matches || to_matches
        })
        .take(DEFAULT_RECENT_TX_LIMIT)
        .map(record_to_tx_item)
        .collect::<Vec<ExplorerTransferTxItem>>();
    recent_txs.sort_by(|left, right| {
        right
            .submitted_at_unix_ms
            .cmp(&left.submitted_at_unix_ms)
            .then_with(|| right.tx_hash.cmp(&left.tx_hash))
    });

    let response = ExplorerContractResponse {
        ok: true,
        observed_at_unix_ms: super::super::now_unix_ms(),
        contract_id: Some(contract_id),
        contract: Some(contract),
        recent_txs,
        error_code: None,
        error: None,
    };
    super::write_transfer_json_response(stream, 200, &response, head_only)?;
    Ok(true)
}

fn handle_explorer_assets(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    execution_world_dir: &Path,
    head_only: bool,
) -> Result<bool, String> {
    let target = super::parse_http_target(request_bytes)?;
    let params = super::parse_query_params(target.as_str());
    let (limit, cursor) = match parse_page_params(&params) {
        Ok(values) => values,
        Err(err) => {
            let response = ExplorerAssetsResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                token_symbol: String::new(),
                token_decimals: 0,
                total_supply: 0,
                circulating_supply: 0,
                total_issued: 0,
                total_burned: 0,
                account_filter: None,
                limit: super::DEFAULT_HISTORY_LIMIT,
                cursor: 0,
                total: 0,
                next_cursor: None,
                holders: Vec::new(),
                nft_supported: false,
                nft_collections: Vec::new(),
                error_code: Some(super::TRANSFER_ERROR_INVALID_REQUEST.to_string()),
                error: Some(err),
            };
            super::write_transfer_json_response(stream, 400, &response, head_only)?;
            return Ok(true);
        }
    };

    let account_filter = params
        .get("account_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let world = match super::super::execution_bridge::load_execution_world(execution_world_dir) {
        Ok(world) => world,
        Err(err) => {
            let response = ExplorerAssetsResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                token_symbol: String::new(),
                token_decimals: 0,
                total_supply: 0,
                circulating_supply: 0,
                total_issued: 0,
                total_burned: 0,
                account_filter,
                limit,
                cursor: 0,
                total: 0,
                next_cursor: None,
                holders: Vec::new(),
                nft_supported: false,
                nft_collections: Vec::new(),
                error_code: Some(super::TRANSFER_ERROR_INTERNAL.to_string()),
                error: Some(err),
            };
            super::write_transfer_json_response(stream, 200, &response, head_only)?;
            return Ok(true);
        }
    };

    let mut holders = world
        .main_token_account_balances()
        .into_iter()
        .filter(|balance| {
            account_filter
                .as_deref()
                .map(|filter| balance.account_id == filter)
                .unwrap_or(true)
        })
        .map(|balance| {
            let last_nonce = world.main_token_last_transfer_nonce(balance.account_id.as_str());
            ExplorerAssetHolderItem {
                account_id: balance.account_id,
                liquid_balance: balance.liquid_balance,
                vested_balance: balance.vested_balance,
                restricted_starter_claim_balance: balance.restricted_starter_claim_balance,
                total_balance: balance
                    .liquid_balance
                    .saturating_add(balance.vested_balance),
                last_transfer_nonce: last_nonce,
                next_nonce_hint: last_nonce.unwrap_or(0).saturating_add(1),
            }
        })
        .collect::<Vec<ExplorerAssetHolderItem>>();
    holders.sort_by(|left, right| {
        right
            .liquid_balance
            .cmp(&left.liquid_balance)
            .then_with(|| left.account_id.cmp(&right.account_id))
    });

    let (cursor, total, next_cursor, page_items) = paginate(holders, limit, cursor);
    let supply = world.main_token_supply();
    let config = world.main_token_config();
    let response = ExplorerAssetsResponse {
        ok: true,
        observed_at_unix_ms: super::super::now_unix_ms(),
        token_symbol: config.symbol.clone(),
        token_decimals: config.decimals,
        total_supply: supply.total_supply,
        circulating_supply: supply.circulating_supply,
        total_issued: supply.total_issued,
        total_burned: supply.total_burned,
        account_filter,
        limit,
        cursor,
        total,
        next_cursor,
        holders: page_items,
        nft_supported: false,
        nft_collections: Vec::new(),
        error_code: None,
        error: None,
    };
    super::write_transfer_json_response(stream, 200, &response, head_only)?;
    Ok(true)
}

fn handle_explorer_mempool(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    head_only: bool,
) -> Result<bool, String> {
    let target = super::parse_http_target(request_bytes)?;
    let params = super::parse_query_params(target.as_str());
    let (limit, cursor) = match parse_page_params(&params) {
        Ok(values) => values,
        Err(err) => {
            let response = ExplorerMempoolResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                status_filter: "all".to_string(),
                accepted_count: 0,
                pending_count: 0,
                limit: super::DEFAULT_HISTORY_LIMIT,
                cursor: 0,
                total: 0,
                next_cursor: None,
                items: Vec::new(),
                error_code: Some(super::TRANSFER_ERROR_INVALID_REQUEST.to_string()),
                error: Some(err),
            };
            super::write_transfer_json_response(stream, 400, &response, head_only)?;
            return Ok(true);
        }
    };

    let status_filter_raw = params
        .get("status")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("all");
    let status_filter = match parse_mempool_status_filter(status_filter_raw) {
        Some(filter) => filter,
        None => {
            let response = ExplorerMempoolResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                status_filter: status_filter_raw.to_string(),
                accepted_count: 0,
                pending_count: 0,
                limit,
                cursor: 0,
                total: 0,
                next_cursor: None,
                items: Vec::new(),
                error_code: Some(super::TRANSFER_ERROR_INVALID_REQUEST.to_string()),
                error: Some(
                    "query parameter status must be one of: all,accepted,pending".to_string(),
                ),
            };
            super::write_transfer_json_response(stream, 400, &response, head_only)?;
            return Ok(true);
        }
    };

    let mut tracker = super::lock_transfer_tracker();
    super::sync_tracker_from_runtime(runtime, &mut tracker)?;
    tracker.refresh_lifecycle_by_time(super::super::now_unix_ms());
    let (_, records) = tracker.query_history(None, None, None, super::MAX_TRACKED_TRANSFER_RECORDS);

    let accepted_count = records
        .iter()
        .filter(|record| record.status == super::TransferLifecycleStatus::Accepted)
        .count();
    let pending_count = records
        .iter()
        .filter(|record| record.status == super::TransferLifecycleStatus::Pending)
        .count();

    let filtered = records
        .into_iter()
        .filter(|record| match status_filter {
            MempoolStatusFilter::All => {
                matches!(
                    record.status,
                    super::TransferLifecycleStatus::Accepted
                        | super::TransferLifecycleStatus::Pending
                )
            }
            MempoolStatusFilter::Accepted => {
                record.status == super::TransferLifecycleStatus::Accepted
            }
            MempoolStatusFilter::Pending => {
                record.status == super::TransferLifecycleStatus::Pending
            }
        })
        .map(record_to_tx_item)
        .collect::<Vec<ExplorerTransferTxItem>>();

    let (cursor, total, next_cursor, page_items) = paginate(filtered, limit, cursor);
    let response = ExplorerMempoolResponse {
        ok: true,
        observed_at_unix_ms: super::super::now_unix_ms(),
        status_filter: status_filter.as_str().to_string(),
        accepted_count,
        pending_count,
        limit,
        cursor,
        total,
        next_cursor,
        items: page_items,
        error_code: None,
        error: None,
    };
    super::write_transfer_json_response(stream, 200, &response, head_only)?;
    Ok(true)
}

fn parse_page_params(params: &BTreeMap<String, String>) -> Result<(usize, usize), String> {
    let limit = params
        .get("limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .map(|value| value.clamp(1, super::MAX_HISTORY_LIMIT))
        .unwrap_or(super::DEFAULT_HISTORY_LIMIT);

    let cursor = match params.get("cursor") {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| "query parameter cursor must be a non-negative integer".to_string())?,
        None => 0,
    };

    Ok((limit, cursor))
}

fn parse_mempool_status_filter(raw: &str) -> Option<MempoolStatusFilter> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "all" => Some(MempoolStatusFilter::All),
        "accepted" => Some(MempoolStatusFilter::Accepted),
        "pending" => Some(MempoolStatusFilter::Pending),
        _ => None,
    }
}

fn record_to_tx_item(record: super::ChainTransferRecord) -> ExplorerTransferTxItem {
    ExplorerTransferTxItem {
        tx_hash: build_tx_hash(
            record.action_id,
            record.from_account_id.as_str(),
            record.to_account_id.as_str(),
            record.amount,
            record.nonce,
        ),
        action_id: record.action_id,
        from_account_id: record.from_account_id,
        to_account_id: record.to_account_id,
        amount: record.amount,
        nonce: record.nonce,
        asset_id: record.asset_id,
        memo: record.memo,
        chain_id: record.chain_id,
        network_id: record.network_id,
        tx_version: record.tx_version,
        tx_type: record.tx_type,
        valid_until_unix_ms: record.valid_until_unix_ms,
        max_fee: record.max_fee,
        fee_asset_id: record.fee_asset_id,
        application_payload_hash: record.application_payload_hash,
        client_request_id: record.client_request_id,
        status: record.status,
        submitted_at_unix_ms: record.submitted_at_unix_ms,
        updated_at_unix_ms: record.updated_at_unix_ms,
        error_code: record.error_code,
        error: record.error,
    }
}

fn contract_to_list_item(contract: &EconomicContractState) -> ExplorerContractListItem {
    ExplorerContractListItem {
        contract_id: contract.contract_id.clone(),
        contract_type: "economic_contract".to_string(),
        status: contract.status,
        creator_agent_id: contract.creator_agent_id.clone(),
        counterparty_agent_id: contract.counterparty_agent_id.clone(),
        settlement_kind: format!("{:?}", contract.settlement_kind).to_ascii_lowercase(),
        settlement_amount: contract.settlement_amount,
        expires_at: contract.expires_at,
        summary: format!(
            "{} -> {} ({:?} {})",
            contract.creator_agent_id,
            contract.counterparty_agent_id,
            contract.settlement_kind,
            contract.settlement_amount
        ),
    }
}

fn paginate<T>(
    items: Vec<T>,
    limit: usize,
    cursor: usize,
) -> (usize, usize, Option<usize>, Vec<T>) {
    let total = items.len();
    let bounded_cursor = cursor.min(total);
    let mut iter = items.into_iter().skip(bounded_cursor);
    let mut page = Vec::new();
    for _ in 0..limit {
        let Some(item) = iter.next() else {
            break;
        };
        page.push(item);
    }
    let next_cursor = if bounded_cursor + page.len() < total {
        Some(bounded_cursor + page.len())
    } else {
        None
    };
    (bounded_cursor, total, next_cursor, page)
}

fn build_tx_hash(
    action_id: u64,
    from_account_id: &str,
    to_account_id: &str,
    amount: u64,
    nonce: u64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(action_id.to_be_bytes());
    hasher.update(from_account_id.as_bytes());
    hasher.update([0]);
    hasher.update(to_account_id.as_bytes());
    hasher.update([0]);
    hasher.update(amount.to_be_bytes());
    hasher.update(nonce.to_be_bytes());
    format!("0x{}", hex::encode(hasher.finalize()))
}

impl MempoolStatusFilter {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Accepted => "accepted",
            Self::Pending => "pending",
        }
    }
}
