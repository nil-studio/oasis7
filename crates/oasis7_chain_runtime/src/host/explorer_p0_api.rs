use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::TcpStream;
use std::path::Path;

use oasis7_node::NodeCommittedActionBatch;

use super::transfer_submit_api::{ChainTransferSubmitRequest, TransferLifecycleStatus};
use explorer_p0_api_support::lock_store;

#[path = "explorer_p0_api_support.rs"]
mod explorer_p0_api_support;
#[path = "explorer_p0_store.rs"]
mod explorer_p0_store;

const EXPLORER_ERROR_INVALID_REQUEST: &str = "invalid_request";
const EXPLORER_ERROR_NOT_FOUND: &str = "not_found";
const EXPLORER_INDEX_FILE: &str = "explorer-index.json";
const EXPLORER_INDEX_VERSION: u32 = 1;
const EXPLORER_MAX_TRACKED_BLOCKS: usize = 2_000;
const EXPLORER_MAX_TRACKED_TXS: usize = 5_000;
const EXPLORER_DEFAULT_LIMIT: usize = 50;
const EXPLORER_MAX_LIMIT: usize = 200;
const EXPLORER_MAX_SEARCH_QUERY_LEN: usize = 128;
const EXPLORER_MAX_SEARCH_RESULTS: usize = 50;
const EXPLORER_BLOCKS_PATH: &str = "/v1/chain/explorer/blocks";
const EXPLORER_BLOCK_PATH: &str = "/v1/chain/explorer/block";
const EXPLORER_TXS_PATH: &str = "/v1/chain/explorer/txs";
const EXPLORER_TX_PATH: &str = "/v1/chain/explorer/tx";
const EXPLORER_SEARCH_PATH: &str = "/v1/chain/explorer/search";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerBlockItem {
    pub(super) height: u64,
    pub(super) slot: u64,
    pub(super) epoch: u64,
    pub(super) block_hash: String,
    pub(super) action_root: String,
    pub(super) action_count: usize,
    pub(super) committed_at_unix_ms: i64,
    #[serde(default)]
    pub(super) tx_hashes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerTxItem {
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
    pub(super) status: TransferLifecycleStatus,
    pub(super) submitted_at_unix_ms: i64,
    pub(super) updated_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) block_height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) block_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerBlocksResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    pub(super) limit: usize,
    pub(super) cursor: usize,
    pub(super) total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) next_cursor: Option<usize>,
    #[serde(default)]
    pub(super) items: Vec<ExplorerBlockItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerBlockResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) block_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) block: Option<ExplorerBlockItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerTxsResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) account_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) status_filter: Option<TransferLifecycleStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) action_filter: Option<u64>,
    pub(super) limit: usize,
    pub(super) cursor: usize,
    pub(super) total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) next_cursor: Option<usize>,
    #[serde(default)]
    pub(super) items: Vec<ExplorerTxItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerTxResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) action_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tx: Option<ExplorerTxItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerSearchHit {
    pub(super) item_type: String,
    pub(super) key: String,
    pub(super) summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExplorerSearchResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    pub(super) q: String,
    pub(super) total: usize,
    #[serde(default)]
    pub(super) items: Vec<ExplorerSearchHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

pub(super) fn configure_persistence_root(execution_world_dir: &Path) {
    let mut store = lock_store();
    store.configure_persistence_path(execution_world_dir);
    store.ensure_loaded();
}

pub(super) fn record_transfer_accepted(
    action_id: u64,
    request: &ChainTransferSubmitRequest,
    now_ms: i64,
) {
    let mut store = lock_store();
    store.record_transfer_accepted(action_id, request, now_ms);
}

pub(super) fn ingest_committed_batches(batches: &[NodeCommittedActionBatch]) {
    let mut store = lock_store();
    store.ingest_batches(batches);
}

pub(super) fn maybe_handle_explorer_p0_request(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    path: &str,
    head_only: bool,
) -> Result<bool, String> {
    match path {
        EXPLORER_BLOCKS_PATH => handle_explorer_blocks(stream, request_bytes, head_only),
        EXPLORER_BLOCK_PATH => handle_explorer_block(stream, request_bytes, head_only),
        EXPLORER_TXS_PATH => handle_explorer_txs(stream, request_bytes, head_only),
        EXPLORER_TX_PATH => handle_explorer_tx(stream, request_bytes, head_only),
        EXPLORER_SEARCH_PATH => handle_explorer_search(stream, request_bytes, head_only),
        _ => Ok(false),
    }
}

pub(super) fn handle_explorer_blocks(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    head_only: bool,
) -> Result<bool, String> {
    let target = parse_http_target(request_bytes)?;
    let params = parse_query_params(target.as_str());
    let (limit, cursor) = match parse_page_params(&params) {
        Ok(params) => params,
        Err(err) => {
            let response = ExplorerBlocksResponse {
                ok: false,
                observed_at_unix_ms: super::now_unix_ms(),
                limit: EXPLORER_DEFAULT_LIMIT,
                cursor: 0,
                total: 0,
                next_cursor: None,
                items: Vec::new(),
                error_code: Some(EXPLORER_ERROR_INVALID_REQUEST.to_string()),
                error: Some(err),
            };
            write_json(stream, 400, &response, head_only)?;
            return Ok(true);
        }
    };

    let mut store = lock_store();
    store.refresh_lifecycle_by_time(super::now_unix_ms());
    let response = store.query_blocks(limit, cursor);
    write_json(stream, 200, &response, head_only)?;
    Ok(true)
}

pub(super) fn handle_explorer_block(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    head_only: bool,
) -> Result<bool, String> {
    let target = parse_http_target(request_bytes)?;
    let params = parse_query_params(target.as_str());

    let height = params
        .get("height")
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|height| *height > 0);
    let hash = params
        .get("hash")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if height.is_none() && hash.is_none() {
        let response = ExplorerBlockResponse {
            ok: false,
            observed_at_unix_ms: super::now_unix_ms(),
            height: None,
            block_hash: None,
            block: None,
            error_code: Some(EXPLORER_ERROR_INVALID_REQUEST.to_string()),
            error: Some("query parameter height or hash is required".to_string()),
        };
        write_json(stream, 400, &response, head_only)?;
        return Ok(true);
    }

    let mut store = lock_store();
    store.refresh_lifecycle_by_time(super::now_unix_ms());
    let response = store.query_block(height, hash);
    write_json(stream, 200, &response, head_only)?;
    Ok(true)
}

pub(super) fn handle_explorer_txs(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    head_only: bool,
) -> Result<bool, String> {
    let target = parse_http_target(request_bytes)?;
    let params = parse_query_params(target.as_str());
    let (limit, cursor) = match parse_page_params(&params) {
        Ok(params) => params,
        Err(err) => {
            let response = ExplorerTxsResponse {
                ok: false,
                observed_at_unix_ms: super::now_unix_ms(),
                account_filter: None,
                status_filter: None,
                action_filter: None,
                limit: EXPLORER_DEFAULT_LIMIT,
                cursor: 0,
                total: 0,
                next_cursor: None,
                items: Vec::new(),
                error_code: Some(EXPLORER_ERROR_INVALID_REQUEST.to_string()),
                error: Some(err),
            };
            write_json(stream, 400, &response, head_only)?;
            return Ok(true);
        }
    };

    let account_filter = params
        .get("account_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let action_filter = params
        .get("action_id")
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|value| *value > 0);
    let status_filter = match params
        .get("status")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(raw) => match parse_transfer_lifecycle_status(raw) {
            Some(status) => Some(status),
            None => {
                let response = ExplorerTxsResponse {
                    ok: false,
                    observed_at_unix_ms: super::now_unix_ms(),
                    account_filter: account_filter.map(ToOwned::to_owned),
                    status_filter: None,
                    action_filter,
                    limit,
                    cursor,
                    total: 0,
                    next_cursor: None,
                    items: Vec::new(),
                    error_code: Some(EXPLORER_ERROR_INVALID_REQUEST.to_string()),
                    error: Some(
                        "query parameter status must be one of: accepted,pending,confirmed,failed,timeout"
                            .to_string(),
                    ),
                };
                write_json(stream, 400, &response, head_only)?;
                return Ok(true);
            }
        },
        None => None,
    };

    let mut store = lock_store();
    store.refresh_lifecycle_by_time(super::now_unix_ms());
    let response = store.query_txs(account_filter, status_filter, action_filter, limit, cursor);
    write_json(stream, 200, &response, head_only)?;
    Ok(true)
}

pub(super) fn handle_explorer_tx(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    head_only: bool,
) -> Result<bool, String> {
    let target = parse_http_target(request_bytes)?;
    let params = parse_query_params(target.as_str());

    let tx_hash = params
        .get("tx_hash")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let action_id = params
        .get("action_id")
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|value| *value > 0);

    if tx_hash.is_none() && action_id.is_none() {
        let response = ExplorerTxResponse {
            ok: false,
            observed_at_unix_ms: super::now_unix_ms(),
            tx_hash: None,
            action_id: None,
            tx: None,
            error_code: Some(EXPLORER_ERROR_INVALID_REQUEST.to_string()),
            error: Some("query parameter tx_hash or action_id is required".to_string()),
        };
        write_json(stream, 400, &response, head_only)?;
        return Ok(true);
    }

    let mut store = lock_store();
    store.refresh_lifecycle_by_time(super::now_unix_ms());
    let response = store.query_tx(tx_hash, action_id);
    write_json(stream, 200, &response, head_only)?;
    Ok(true)
}

pub(super) fn handle_explorer_search(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    head_only: bool,
) -> Result<bool, String> {
    let target = parse_http_target(request_bytes)?;
    let params = parse_query_params(target.as_str());

    let q = params
        .get("q")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(q) = q else {
        let response = ExplorerSearchResponse {
            ok: false,
            observed_at_unix_ms: super::now_unix_ms(),
            q: String::new(),
            total: 0,
            items: Vec::new(),
            error_code: Some(EXPLORER_ERROR_INVALID_REQUEST.to_string()),
            error: Some("query parameter q is required".to_string()),
        };
        write_json(stream, 400, &response, head_only)?;
        return Ok(true);
    };
    if q.len() > EXPLORER_MAX_SEARCH_QUERY_LEN {
        let response = ExplorerSearchResponse {
            ok: false,
            observed_at_unix_ms: super::now_unix_ms(),
            q: q.to_string(),
            total: 0,
            items: Vec::new(),
            error_code: Some(EXPLORER_ERROR_INVALID_REQUEST.to_string()),
            error: Some(format!(
                "query parameter q exceeds max length {}",
                EXPLORER_MAX_SEARCH_QUERY_LEN
            )),
        };
        write_json(stream, 400, &response, head_only)?;
        return Ok(true);
    }

    let mut store = lock_store();
    store.refresh_lifecycle_by_time(super::now_unix_ms());
    let response = store.query_search(q);
    write_json(stream, 200, &response, head_only)?;
    Ok(true)
}

fn write_json<T: Serialize>(
    stream: &mut TcpStream,
    status_code: u16,
    payload: &T,
    head_only: bool,
) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(payload)
        .map_err(|err| format!("encode explorer P0 API response failed: {err}"))?;
    super::write_json_response(stream, status_code, body.as_slice(), head_only)
        .map_err(|err| format!("write explorer P0 API response failed: {err}"))
}

fn parse_page_params(params: &BTreeMap<String, String>) -> Result<(usize, usize), String> {
    let limit = match params.get("limit") {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| "query parameter limit must be an integer".to_string())?
            .clamp(1, EXPLORER_MAX_LIMIT),
        None => EXPLORER_DEFAULT_LIMIT,
    };
    let cursor = match params.get("cursor") {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| "query parameter cursor must be an integer".to_string())?,
        None => 0,
    };
    Ok((limit, cursor))
}

fn parse_http_target(request_bytes: &[u8]) -> Result<String, String> {
    let request = std::str::from_utf8(request_bytes)
        .map_err(|_| "invalid HTTP request: non UTF-8 payload".to_string())?;
    let line = request
        .lines()
        .next()
        .ok_or_else(|| "invalid HTTP request: missing request line".to_string())?;
    let target = line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "invalid HTTP request: missing request target".to_string())?;
    Ok(target.to_string())
}

fn parse_query_params(target: &str) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    let query = match target.split_once('?') {
        Some((_, query)) => query,
        None => return params,
    };

    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        let key = percent_decode(key);
        let value = percent_decode(value);
        if key.is_empty() {
            continue;
        }
        params.insert(key, value);
    }

    params
}

fn parse_transfer_lifecycle_status(raw: &str) -> Option<TransferLifecycleStatus> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "accepted" => Some(TransferLifecycleStatus::Accepted),
        "pending" => Some(TransferLifecycleStatus::Pending),
        "confirmed" => Some(TransferLifecycleStatus::Confirmed),
        "failed" => Some(TransferLifecycleStatus::Failed),
        "timeout" => Some(TransferLifecycleStatus::Timeout),
        _ => None,
    }
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut cursor = 0_usize;
    let mut output = Vec::with_capacity(bytes.len());

    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b'+' {
            output.push(b' ');
            cursor += 1;
            continue;
        }
        if byte == b'%' && cursor + 2 < bytes.len() {
            let high = hex_value(bytes[cursor + 1]);
            let low = hex_value(bytes[cursor + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                output.push((high << 4) | low);
                cursor += 3;
                continue;
            }
        }
        output.push(byte);
        cursor += 1;
    }

    String::from_utf8(output).unwrap_or_else(|_| raw.to_string())
}

fn hex_value(raw: u8) -> Option<u8> {
    match raw {
        b'0'..=b'9' => Some(raw - b'0'),
        b'a'..=b'f' => Some(raw - b'a' + 10),
        b'A'..=b'F' => Some(raw - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
pub(super) fn reset_store_for_tests() {
    explorer_p0_api_support::reset_store_for_tests();
}
