use serde::{Deserialize, Serialize};

use super::TransferLifecycleStatus;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChainTransferSubmitRequest {
    pub(crate) from_account_id: String,
    pub(crate) to_account_id: String,
    pub(crate) amount: u64,
    pub(crate) nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) chain_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) network_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tx_version: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tx_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) valid_until_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) max_fee: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fee_asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) application_payload_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) client_request_id: Option<String>,
    pub(crate) public_key: String,
    pub(crate) signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainTransferSubmitResponse {
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) action_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) submitted_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lifecycle_status: Option<TransferLifecycleStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

impl ChainTransferSubmitResponse {
    pub(crate) fn success(action_id: u64, submitted_at_unix_ms: i64) -> Self {
        Self {
            ok: true,
            action_id: Some(action_id),
            submitted_at_unix_ms: Some(submitted_at_unix_ms),
            lifecycle_status: Some(TransferLifecycleStatus::Accepted),
            error_code: None,
            error: None,
        }
    }

    pub(crate) fn error(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            action_id: None,
            submitted_at_unix_ms: None,
            lifecycle_status: None,
            error_code: Some(error_code.into()),
            error: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainTransferRecord {
    pub(crate) action_id: u64,
    pub(crate) from_account_id: String,
    pub(crate) to_account_id: String,
    pub(crate) amount: u64,
    pub(crate) nonce: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) memo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) chain_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) network_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tx_version: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tx_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) valid_until_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_fee: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fee_asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) application_payload_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) client_request_id: Option<String>,
    pub(crate) status: TransferLifecycleStatus,
    pub(crate) submitted_at_unix_ms: i64,
    pub(crate) updated_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainTransferStatusResponse {
    pub(crate) ok: bool,
    pub(crate) observed_at_unix_ms: i64,
    pub(crate) action_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) status: Option<ChainTransferRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

impl ChainTransferStatusResponse {
    pub(crate) fn success(action_id: u64, status: ChainTransferRecord) -> Self {
        Self {
            ok: true,
            observed_at_unix_ms: super::super::now_unix_ms(),
            action_id,
            status: Some(status),
            error_code: None,
            error: None,
        }
    }

    pub(crate) fn error(
        action_id: u64,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            ok: false,
            observed_at_unix_ms: super::super::now_unix_ms(),
            action_id,
            status: None,
            error_code: Some(code.into()),
            error: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainTransferHistoryResponse {
    pub(crate) ok: bool,
    pub(crate) observed_at_unix_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) account_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) status_filter: Option<TransferLifecycleStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) action_filter: Option<u64>,
    pub(crate) limit: usize,
    pub(crate) total: usize,
    pub(crate) items: Vec<ChainTransferRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

impl ChainTransferHistoryResponse {
    pub(crate) fn success(
        account_filter: Option<String>,
        status_filter: Option<TransferLifecycleStatus>,
        action_filter: Option<u64>,
        limit: usize,
        total: usize,
        items: Vec<ChainTransferRecord>,
    ) -> Self {
        Self {
            ok: true,
            observed_at_unix_ms: super::super::now_unix_ms(),
            account_filter,
            status_filter,
            action_filter,
            limit,
            total,
            items,
            error_code: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainTransferAccountEntry {
    pub(crate) account_id: String,
    pub(crate) liquid_balance: u64,
    pub(crate) vested_balance: u64,
    pub(crate) restricted_starter_claim_balance: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_transfer_nonce: Option<u64>,
    pub(crate) next_nonce_hint: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainTransferAccountsResponse {
    pub(crate) ok: bool,
    pub(crate) observed_at_unix_ms: i64,
    pub(crate) node_id: String,
    pub(crate) world_id: String,
    pub(crate) accounts: Vec<ChainTransferAccountEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

impl ChainTransferAccountsResponse {
    pub(crate) fn success(
        node_id: &str,
        world_id: &str,
        accounts: Vec<ChainTransferAccountEntry>,
    ) -> Self {
        Self {
            ok: true,
            observed_at_unix_ms: super::super::now_unix_ms(),
            node_id: node_id.to_string(),
            world_id: world_id.to_string(),
            accounts,
            error_code: None,
            error: None,
        }
    }

    pub(crate) fn error(
        node_id: &str,
        world_id: &str,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            ok: false,
            observed_at_unix_ms: super::super::now_unix_ms(),
            node_id: node_id.to_string(),
            world_id: world_id.to_string(),
            accounts: Vec::new(),
            error_code: Some(code.into()),
            error: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChainExplorerOverviewResponse {
    pub(crate) ok: bool,
    pub(crate) observed_at_unix_ms: i64,
    pub(crate) node_id: String,
    pub(crate) world_id: String,
    pub(crate) latest_height: u64,
    pub(crate) committed_height: u64,
    pub(crate) network_committed_height: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_block_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_execution_block_hash: Option<String>,
    pub(crate) tracked_records: usize,
    pub(crate) transfer_total: usize,
    pub(crate) transfer_accepted: usize,
    pub(crate) transfer_pending: usize,
    pub(crate) transfer_confirmed: usize,
    pub(crate) transfer_failed: usize,
    pub(crate) transfer_timeout: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ChainTransferLatencySummaryStatus {
    pub(crate) sample_count: usize,
    pub(crate) avg_latency_ms: Option<i64>,
    pub(crate) max_latency_ms: Option<i64>,
    pub(crate) p50_latency_ms: Option<i64>,
    pub(crate) p95_latency_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ChainTransferMetricsStatus {
    pub(crate) tracked_records: usize,
    pub(crate) accepted_count: usize,
    pub(crate) pending_count: usize,
    pub(crate) confirmed_count: usize,
    pub(crate) failed_count: usize,
    pub(crate) timeout_count: usize,
    pub(crate) inflight_count: usize,
    pub(crate) oldest_inflight_age_ms: Option<i64>,
    pub(crate) recent_confirmation_latency: ChainTransferLatencySummaryStatus,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TransferLifecycleCounters {
    pub(crate) total: usize,
    pub(crate) accepted: usize,
    pub(crate) pending: usize,
    pub(crate) confirmed: usize,
    pub(crate) failed: usize,
    pub(crate) timeout: usize,
}

impl ChainExplorerOverviewResponse {
    pub(crate) fn success(
        node_id: &str,
        world_id: &str,
        snapshot: &oasis7_node::NodeSnapshot,
        counters: TransferLifecycleCounters,
    ) -> Self {
        Self {
            ok: true,
            observed_at_unix_ms: super::super::now_unix_ms(),
            node_id: node_id.to_string(),
            world_id: world_id.to_string(),
            latest_height: snapshot.consensus.latest_height,
            committed_height: snapshot.consensus.committed_height,
            network_committed_height: snapshot.consensus.network_committed_height,
            last_block_hash: snapshot.consensus.last_block_hash.clone(),
            last_execution_block_hash: snapshot.consensus.last_execution_block_hash.clone(),
            tracked_records: counters.total,
            transfer_total: counters.total,
            transfer_accepted: counters.accepted,
            transfer_pending: counters.pending,
            transfer_confirmed: counters.confirmed,
            transfer_failed: counters.failed,
            transfer_timeout: counters.timeout,
            error_code: None,
            error: None,
        }
    }
}
