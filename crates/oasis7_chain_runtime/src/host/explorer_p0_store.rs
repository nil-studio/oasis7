use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use oasis7::consensus_action_payload::{
    ConsensusActionPayloadBody, decode_consensus_action_payload,
};
use oasis7_node::NodeCommittedActionBatch;
use oasis7_runtime::Action;
use serde::{Deserialize, Serialize};

use super::super::transfer_submit_api::{ChainTransferSubmitRequest, TransferLifecycleStatus};
use super::explorer_p0_api_support::build_tx_hash;
use super::{
    EXPLORER_ERROR_NOT_FOUND, EXPLORER_INDEX_FILE, EXPLORER_INDEX_VERSION,
    EXPLORER_MAX_SEARCH_RESULTS, EXPLORER_MAX_TRACKED_BLOCKS, EXPLORER_MAX_TRACKED_TXS,
    ExplorerBlockItem, ExplorerBlockResponse, ExplorerBlocksResponse, ExplorerSearchHit,
    ExplorerSearchResponse, ExplorerTxItem, ExplorerTxResponse, ExplorerTxsResponse,
};

pub(super) static EXPLORER_STORE: OnceLock<Mutex<ExplorerStore>> = OnceLock::new();

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ExplorerStoreSnapshot {
    version: u32,
    #[serde(default)]
    blocks: Vec<ExplorerBlockItem>,
    #[serde(default)]
    txs: Vec<ExplorerTxItem>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ExplorerStore {
    pub(super) persistence_path: Option<PathBuf>,
    pub(super) loaded: bool,
    pub(super) blocks_by_height: BTreeMap<u64, ExplorerBlockItem>,
    pub(super) txs_by_hash: BTreeMap<String, ExplorerTxItem>,
    pub(super) tx_hash_by_action_id: BTreeMap<u64, String>,
}

impl ExplorerStore {
    pub(super) fn configure_persistence_path(&mut self, execution_world_dir: &Path) {
        let path = execution_world_dir.join(EXPLORER_INDEX_FILE);
        if let Some(existing) = self.persistence_path.as_ref() {
            if existing == &path {
                return;
            }
            return;
        }
        self.persistence_path = Some(path.clone());
        self.loaded = false;
        self.blocks_by_height.clear();
        self.txs_by_hash.clear();
        self.tx_hash_by_action_id.clear();
    }

    pub(super) fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }

        self.blocks_by_height.clear();
        self.txs_by_hash.clear();
        self.tx_hash_by_action_id.clear();

        let Some(path) = self.persistence_path.as_ref() else {
            self.loaded = true;
            return;
        };
        if !path.exists() {
            self.loaded = true;
            return;
        }

        let Ok(bytes) = std::fs::read(path) else {
            self.loaded = true;
            return;
        };
        let Ok(snapshot) = serde_json::from_slice::<ExplorerStoreSnapshot>(bytes.as_slice()) else {
            self.loaded = true;
            return;
        };
        if snapshot.version != EXPLORER_INDEX_VERSION {
            self.loaded = true;
            return;
        }

        for block in snapshot.blocks {
            self.blocks_by_height.insert(block.height, block);
        }
        for tx in snapshot.txs {
            self.tx_hash_by_action_id
                .insert(tx.action_id, tx.tx_hash.clone());
            self.txs_by_hash.insert(tx.tx_hash.clone(), tx);
        }
        self.prune();
        self.loaded = true;
    }

    fn persist(&self) -> Result<(), String> {
        let Some(path) = self.persistence_path.as_ref() else {
            return Ok(());
        };
        let snapshot = ExplorerStoreSnapshot {
            version: EXPLORER_INDEX_VERSION,
            blocks: self
                .blocks_by_height
                .values()
                .cloned()
                .collect::<Vec<ExplorerBlockItem>>(),
            txs: self
                .txs_by_hash
                .values()
                .cloned()
                .collect::<Vec<ExplorerTxItem>>(),
        };
        let body = serde_json::to_vec(&snapshot)
            .map_err(|err| format!("encode explorer index snapshot failed: {err}"))?;
        let Some(parent) = path.parent() else {
            return Err(format!(
                "explorer index path has no parent: {}",
                path.display()
            ));
        };
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "create explorer index parent directory failed (path={}): {err}",
                parent.display()
            )
        })?;

        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(tmp_path.as_path(), body.as_slice()).map_err(|err| {
            format!(
                "write explorer index temp file failed (path={}): {err}",
                tmp_path.display()
            )
        })?;
        std::fs::rename(tmp_path.as_path(), path.as_path()).map_err(|err| {
            format!(
                "rename explorer index file failed (path={}): {err}",
                path.display()
            )
        })?;
        Ok(())
    }

    pub(super) fn record_transfer_accepted(
        &mut self,
        action_id: u64,
        request: &ChainTransferSubmitRequest,
        now_ms: i64,
    ) {
        self.ensure_loaded();
        let tx_hash = self
            .tx_hash_by_action_id
            .get(&action_id)
            .cloned()
            .unwrap_or_else(|| {
                build_tx_hash(
                    action_id,
                    request.from_account_id.as_str(),
                    request.to_account_id.as_str(),
                    request.amount,
                    request.nonce,
                )
            });
        self.tx_hash_by_action_id.insert(action_id, tx_hash.clone());
        let mut item = self
            .txs_by_hash
            .remove(tx_hash.as_str())
            .unwrap_or_else(|| ExplorerTxItem {
                tx_hash: tx_hash.clone(),
                action_id,
                from_account_id: request.from_account_id.clone(),
                to_account_id: request.to_account_id.clone(),
                amount: request.amount,
                nonce: request.nonce,
                asset_id: request.asset_id.clone(),
                memo: request.memo.clone(),
                chain_id: request.chain_id.clone(),
                network_id: request.network_id.clone(),
                tx_version: request.tx_version,
                tx_type: request.tx_type.clone(),
                valid_until_unix_ms: request.valid_until_unix_ms,
                max_fee: request.max_fee,
                fee_asset_id: request.fee_asset_id.clone(),
                application_payload_hash: request.application_payload_hash.clone(),
                client_request_id: request.client_request_id.clone(),
                status: TransferLifecycleStatus::Accepted,
                submitted_at_unix_ms: now_ms,
                updated_at_unix_ms: now_ms,
                block_height: None,
                block_hash: None,
                error_code: None,
                error: None,
            });
        item.status = TransferLifecycleStatus::Accepted;
        item.asset_id = request.asset_id.clone();
        item.memo = request.memo.clone();
        item.chain_id = request.chain_id.clone();
        item.network_id = request.network_id.clone();
        item.tx_version = request.tx_version;
        item.tx_type = request.tx_type.clone();
        item.valid_until_unix_ms = request.valid_until_unix_ms;
        item.max_fee = request.max_fee;
        item.fee_asset_id = request.fee_asset_id.clone();
        item.application_payload_hash = request.application_payload_hash.clone();
        item.client_request_id = request.client_request_id.clone();
        item.updated_at_unix_ms = now_ms;
        if item.submitted_at_unix_ms <= 0 {
            item.submitted_at_unix_ms = now_ms;
        }
        self.txs_by_hash.insert(tx_hash, item);
        self.prune();
        let _ = self.persist();
    }

    pub(super) fn ingest_batches(&mut self, batches: &[NodeCommittedActionBatch]) {
        self.ensure_loaded();
        if batches.is_empty() {
            return;
        }

        for batch in batches {
            let mut block_item = self
                .blocks_by_height
                .get(&batch.height)
                .cloned()
                .unwrap_or_else(|| ExplorerBlockItem {
                    height: batch.height,
                    slot: batch.slot,
                    epoch: batch.epoch,
                    block_hash: batch.block_hash.clone(),
                    action_root: batch.action_root.clone(),
                    action_count: batch.actions.len(),
                    committed_at_unix_ms: batch.committed_at_unix_ms,
                    tx_hashes: Vec::new(),
                });
            block_item.slot = batch.slot;
            block_item.epoch = batch.epoch;
            block_item.block_hash = batch.block_hash.clone();
            block_item.action_root = batch.action_root.clone();
            block_item.action_count = batch.actions.len();
            block_item.committed_at_unix_ms = batch.committed_at_unix_ms;

            let mut known_hashes = block_item
                .tx_hashes
                .iter()
                .cloned()
                .collect::<BTreeSet<String>>();
            for action in &batch.actions {
                let decoded = match decode_consensus_action_payload(action.payload_cbor.as_slice())
                {
                    Ok(decoded) => decoded,
                    Err(_) => continue,
                };
                let ConsensusActionPayloadBody::RuntimeAction {
                    action: runtime_action,
                } = decoded
                else {
                    continue;
                };
                let Action::TransferMainToken {
                    from_account_id,
                    to_account_id,
                    amount,
                    nonce,
                    asset_id,
                    memo,
                    chain_id,
                    network_id,
                    tx_version,
                    tx_type,
                    valid_until_unix_ms,
                    max_fee,
                    fee_asset_id,
                    application_payload_hash,
                    client_request_id,
                    ..
                } = runtime_action
                else {
                    continue;
                };

                let tx_hash = self
                    .tx_hash_by_action_id
                    .get(&action.action_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        build_tx_hash(
                            action.action_id,
                            from_account_id.as_str(),
                            to_account_id.as_str(),
                            amount,
                            nonce,
                        )
                    });
                self.tx_hash_by_action_id
                    .insert(action.action_id, tx_hash.clone());
                if known_hashes.insert(tx_hash.clone()) {
                    block_item.tx_hashes.push(tx_hash.clone());
                }

                let mut tx_item = self
                    .txs_by_hash
                    .remove(tx_hash.as_str())
                    .unwrap_or_else(|| ExplorerTxItem {
                        tx_hash: tx_hash.clone(),
                        action_id: action.action_id,
                        from_account_id: from_account_id.clone(),
                        to_account_id: to_account_id.clone(),
                        amount,
                        nonce,
                        asset_id: asset_id.clone(),
                        memo: memo.clone(),
                        chain_id: chain_id.clone(),
                        network_id: network_id.clone(),
                        tx_version,
                        tx_type: tx_type.clone(),
                        valid_until_unix_ms,
                        max_fee,
                        fee_asset_id: fee_asset_id.clone(),
                        application_payload_hash: application_payload_hash.clone(),
                        client_request_id: client_request_id.clone(),
                        status: TransferLifecycleStatus::Confirmed,
                        submitted_at_unix_ms: batch.committed_at_unix_ms,
                        updated_at_unix_ms: batch.committed_at_unix_ms,
                        block_height: Some(batch.height),
                        block_hash: Some(batch.block_hash.clone()),
                        error_code: None,
                        error: None,
                    });
                tx_item.from_account_id = from_account_id;
                tx_item.to_account_id = to_account_id;
                tx_item.amount = amount;
                tx_item.nonce = nonce;
                tx_item.asset_id = asset_id;
                tx_item.memo = memo;
                tx_item.chain_id = chain_id;
                tx_item.network_id = network_id;
                tx_item.tx_version = tx_version;
                tx_item.tx_type = tx_type;
                tx_item.valid_until_unix_ms = valid_until_unix_ms;
                tx_item.max_fee = max_fee;
                tx_item.fee_asset_id = fee_asset_id;
                tx_item.application_payload_hash = application_payload_hash;
                tx_item.client_request_id = client_request_id;
                tx_item.status = TransferLifecycleStatus::Confirmed;
                tx_item.updated_at_unix_ms = batch.committed_at_unix_ms;
                if tx_item.submitted_at_unix_ms <= 0 {
                    tx_item.submitted_at_unix_ms = batch.committed_at_unix_ms;
                }
                tx_item.block_height = Some(batch.height);
                tx_item.block_hash = Some(batch.block_hash.clone());
                tx_item.error_code = None;
                tx_item.error = None;
                self.txs_by_hash.insert(tx_hash, tx_item);
            }

            self.blocks_by_height.insert(batch.height, block_item);
        }

        self.prune();
        let _ = self.persist();
    }

    pub(super) fn refresh_lifecycle_by_time(&mut self, now_ms: i64) {
        self.ensure_loaded();
        const TRANSFER_PENDING_AFTER_MS: i64 = 800;
        const TRANSFER_TIMEOUT_MS: i64 = 30_000;

        for tx in self.txs_by_hash.values_mut() {
            match tx.status {
                TransferLifecycleStatus::Accepted => {
                    if now_ms.saturating_sub(tx.submitted_at_unix_ms) >= TRANSFER_TIMEOUT_MS {
                        tx.status = TransferLifecycleStatus::Timeout;
                        tx.updated_at_unix_ms = now_ms;
                    } else if now_ms.saturating_sub(tx.submitted_at_unix_ms)
                        >= TRANSFER_PENDING_AFTER_MS
                    {
                        tx.status = TransferLifecycleStatus::Pending;
                        tx.updated_at_unix_ms = now_ms;
                    }
                }
                TransferLifecycleStatus::Pending => {
                    if now_ms.saturating_sub(tx.submitted_at_unix_ms) >= TRANSFER_TIMEOUT_MS {
                        tx.status = TransferLifecycleStatus::Timeout;
                        tx.updated_at_unix_ms = now_ms;
                    }
                }
                TransferLifecycleStatus::Confirmed
                | TransferLifecycleStatus::Failed
                | TransferLifecycleStatus::Timeout => {}
            }
        }
    }

    pub(super) fn query_blocks(&self, limit: usize, cursor: usize) -> ExplorerBlocksResponse {
        let total = self.blocks_by_height.len();
        let bounded_cursor = cursor.min(total);
        let items = self
            .blocks_by_height
            .iter()
            .rev()
            .skip(bounded_cursor)
            .take(limit)
            .map(|(_, block)| block.clone())
            .collect::<Vec<ExplorerBlockItem>>();
        let next_cursor = if bounded_cursor + items.len() < total {
            Some(bounded_cursor + items.len())
        } else {
            None
        };
        ExplorerBlocksResponse {
            ok: true,
            observed_at_unix_ms: super::super::now_unix_ms(),
            limit,
            cursor: bounded_cursor,
            total,
            next_cursor,
            items,
            error_code: None,
            error: None,
        }
    }

    pub(super) fn query_block(
        &self,
        height: Option<u64>,
        hash: Option<&str>,
    ) -> ExplorerBlockResponse {
        let block = if let Some(height) = height {
            self.blocks_by_height.get(&height).cloned()
        } else if let Some(hash) = hash {
            self.blocks_by_height
                .values()
                .find(|item| item.block_hash == hash)
                .cloned()
        } else {
            None
        };
        match block {
            Some(block) => ExplorerBlockResponse {
                ok: true,
                observed_at_unix_ms: super::super::now_unix_ms(),
                height,
                block_hash: hash.map(ToOwned::to_owned),
                block: Some(block),
                error_code: None,
                error: None,
            },
            None => ExplorerBlockResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                height,
                block_hash: hash.map(ToOwned::to_owned),
                block: None,
                error_code: Some(EXPLORER_ERROR_NOT_FOUND.to_string()),
                error: Some("block not found".to_string()),
            },
        }
    }

    pub(super) fn query_txs(
        &self,
        account_filter: Option<&str>,
        status_filter: Option<TransferLifecycleStatus>,
        action_filter: Option<u64>,
        limit: usize,
        cursor: usize,
    ) -> ExplorerTxsResponse {
        let mut txs = self
            .txs_by_hash
            .values()
            .filter(|item| {
                if let Some(action_filter) = action_filter {
                    return item.action_id == action_filter;
                }
                if let Some(account_filter) = account_filter
                    && item.from_account_id != account_filter
                    && item.to_account_id != account_filter
                {
                    return false;
                }
                if let Some(status_filter) = status_filter
                    && item.status != status_filter
                {
                    return false;
                }
                true
            })
            .collect::<Vec<&ExplorerTxItem>>();
        txs.sort_by(|left, right| {
            right
                .submitted_at_unix_ms
                .cmp(&left.submitted_at_unix_ms)
                .then_with(|| right.tx_hash.cmp(&left.tx_hash))
        });

        let total = txs.len();
        let bounded_cursor = cursor.min(total);
        let items = txs
            .into_iter()
            .skip(bounded_cursor)
            .take(limit)
            .cloned()
            .collect::<Vec<ExplorerTxItem>>();
        let next_cursor = if bounded_cursor + items.len() < total {
            Some(bounded_cursor + items.len())
        } else {
            None
        };
        ExplorerTxsResponse {
            ok: true,
            observed_at_unix_ms: super::super::now_unix_ms(),
            account_filter: account_filter.map(ToOwned::to_owned),
            status_filter,
            action_filter,
            limit,
            cursor: bounded_cursor,
            total,
            next_cursor,
            items,
            error_code: None,
            error: None,
        }
    }

    pub(super) fn query_tx(
        &self,
        tx_hash: Option<&str>,
        action_id: Option<u64>,
    ) -> ExplorerTxResponse {
        let resolved_tx_hash = tx_hash.map(ToOwned::to_owned).or_else(|| {
            action_id.and_then(|action_id| self.tx_hash_by_action_id.get(&action_id).cloned())
        });

        let tx = resolved_tx_hash
            .as_ref()
            .and_then(|tx_hash| self.txs_by_hash.get(tx_hash.as_str()).cloned());
        match tx {
            Some(tx) => ExplorerTxResponse {
                ok: true,
                observed_at_unix_ms: super::super::now_unix_ms(),
                tx_hash: resolved_tx_hash,
                action_id,
                tx: Some(tx),
                error_code: None,
                error: None,
            },
            None => ExplorerTxResponse {
                ok: false,
                observed_at_unix_ms: super::super::now_unix_ms(),
                tx_hash: resolved_tx_hash,
                action_id,
                tx: None,
                error_code: Some(EXPLORER_ERROR_NOT_FOUND.to_string()),
                error: Some("tx not found".to_string()),
            },
        }
    }

    pub(super) fn query_search(&self, q: &str) -> ExplorerSearchResponse {
        let query = q.trim();
        let query_lower = query.to_ascii_lowercase();
        let mut dedup = BTreeSet::<String>::new();
        let mut items = Vec::<ExplorerSearchHit>::new();

        if let Ok(height) = query.parse::<u64>()
            && let Some(block) = self.blocks_by_height.get(&height)
        {
            let dedup_key = format!("block:{}", block.height);
            if dedup.insert(dedup_key) {
                items.push(ExplorerSearchHit {
                    item_type: "block".to_string(),
                    key: block.height.to_string(),
                    summary: format!("height={} hash={}", block.height, block.block_hash),
                });
            }
        }

        if let Ok(action_id) = query.parse::<u64>()
            && let Some(tx_hash) = self.tx_hash_by_action_id.get(&action_id)
            && let Some(tx) = self.txs_by_hash.get(tx_hash.as_str())
        {
            let dedup_key = format!("tx:{}", tx.tx_hash);
            if dedup.insert(dedup_key) {
                items.push(ExplorerSearchHit {
                    item_type: "tx".to_string(),
                    key: tx.tx_hash.clone(),
                    summary: format!(
                        "action_id={} status={:?} {}->{}",
                        tx.action_id, tx.status, tx.from_account_id, tx.to_account_id
                    ),
                });
            }
        }

        for block in self.blocks_by_height.values() {
            if block.block_hash.eq_ignore_ascii_case(query)
                || block
                    .block_hash
                    .to_ascii_lowercase()
                    .contains(query_lower.as_str())
            {
                let dedup_key = format!("block:{}", block.height);
                if dedup.insert(dedup_key) {
                    items.push(ExplorerSearchHit {
                        item_type: "block".to_string(),
                        key: block.block_hash.clone(),
                        summary: format!("height={} txs={}", block.height, block.tx_hashes.len()),
                    });
                }
            }
        }

        for tx in self.txs_by_hash.values() {
            let tx_hash_lc = tx.tx_hash.to_ascii_lowercase();
            let from_lc = tx.from_account_id.to_ascii_lowercase();
            let to_lc = tx.to_account_id.to_ascii_lowercase();
            let matches = tx_hash_lc.contains(query_lower.as_str())
                || from_lc.contains(query_lower.as_str())
                || to_lc.contains(query_lower.as_str());
            if !matches {
                continue;
            }
            let dedup_key = format!("tx:{}", tx.tx_hash);
            if dedup.insert(dedup_key) {
                items.push(ExplorerSearchHit {
                    item_type: "tx".to_string(),
                    key: tx.tx_hash.clone(),
                    summary: format!(
                        "action_id={} status={:?} {}->{}",
                        tx.action_id, tx.status, tx.from_account_id, tx.to_account_id
                    ),
                });
            }
            if items.len() >= EXPLORER_MAX_SEARCH_RESULTS {
                break;
            }
        }

        items.truncate(EXPLORER_MAX_SEARCH_RESULTS);
        ExplorerSearchResponse {
            ok: true,
            observed_at_unix_ms: super::super::now_unix_ms(),
            q: query.to_string(),
            total: items.len(),
            items,
            error_code: None,
            error: None,
        }
    }

    fn prune(&mut self) {
        if self.blocks_by_height.len() > EXPLORER_MAX_TRACKED_BLOCKS {
            let overflow = self.blocks_by_height.len() - EXPLORER_MAX_TRACKED_BLOCKS;
            let remove_heights = self
                .blocks_by_height
                .keys()
                .copied()
                .take(overflow)
                .collect::<Vec<u64>>();
            for height in remove_heights {
                self.blocks_by_height.remove(&height);
            }
        }

        if self.txs_by_hash.len() > EXPLORER_MAX_TRACKED_TXS {
            let overflow = self.txs_by_hash.len() - EXPLORER_MAX_TRACKED_TXS;
            let mut order = self
                .txs_by_hash
                .values()
                .map(|item| {
                    (
                        item.submitted_at_unix_ms,
                        item.action_id,
                        item.tx_hash.as_str(),
                    )
                })
                .collect::<Vec<(i64, u64, &str)>>();
            order.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
            let remove_txs = order
                .into_iter()
                .take(overflow)
                .map(|(_, action_id, tx_hash)| (action_id, tx_hash.to_string()))
                .collect::<Vec<(u64, String)>>();
            for (action_id, tx_hash) in remove_txs {
                self.txs_by_hash.remove(tx_hash.as_str());
                self.tx_hash_by_action_id.remove(&action_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(height: u64) -> ExplorerBlockItem {
        ExplorerBlockItem {
            height,
            slot: height,
            epoch: 0,
            block_hash: format!("block-{height}"),
            action_root: format!("root-{height}"),
            action_count: 0,
            committed_at_unix_ms: height as i64,
            tx_hashes: Vec::new(),
        }
    }

    fn tx(
        tx_hash: &str,
        action_id: u64,
        from_account_id: &str,
        to_account_id: &str,
        status: TransferLifecycleStatus,
        submitted_at_unix_ms: i64,
    ) -> ExplorerTxItem {
        ExplorerTxItem {
            tx_hash: tx_hash.to_string(),
            action_id,
            from_account_id: from_account_id.to_string(),
            to_account_id: to_account_id.to_string(),
            amount: action_id,
            nonce: action_id,
            asset_id: None,
            memo: None,
            chain_id: None,
            network_id: None,
            tx_version: None,
            tx_type: None,
            valid_until_unix_ms: None,
            max_fee: None,
            fee_asset_id: None,
            application_payload_hash: None,
            client_request_id: None,
            status,
            submitted_at_unix_ms,
            updated_at_unix_ms: submitted_at_unix_ms,
            block_height: None,
            block_hash: None,
            error_code: None,
            error: None,
        }
    }

    fn temp_dir(label: &str) -> PathBuf {
        let unique = format!(
            "{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        );
        std::env::temp_dir().join(format!("oasis7-explorer-p0-store-{label}-{unique}"))
    }

    #[test]
    fn persists_compact_json_and_reloads_block_transfer_and_action_index() {
        let dir = temp_dir("persist-reload");
        let _ = std::fs::remove_dir_all(&dir);

        let mut persisted = ExplorerStore::default();
        persisted.configure_persistence_path(dir.as_path());
        let persisted_block = block(42);
        let persisted_tx = tx(
            "tx-persisted",
            7,
            "alice",
            "bob",
            TransferLifecycleStatus::Confirmed,
            1_234,
        );
        persisted
            .blocks_by_height
            .insert(persisted_block.height, persisted_block.clone());
        persisted
            .tx_hash_by_action_id
            .insert(persisted_tx.action_id, persisted_tx.tx_hash.clone());
        persisted
            .txs_by_hash
            .insert(persisted_tx.tx_hash.clone(), persisted_tx.clone());
        persisted.persist().expect("persist explorer store");

        let index_path = dir.join(EXPLORER_INDEX_FILE);
        let compact_json = std::fs::read(index_path).expect("read explorer index");
        let _: serde_json::Value =
            serde_json::from_slice(compact_json.as_slice()).expect("explorer index is valid JSON");

        let mut reloaded = ExplorerStore::default();
        reloaded.configure_persistence_path(dir.as_path());
        reloaded.ensure_loaded();

        assert_eq!(reloaded.blocks_by_height.get(&42), Some(&persisted_block));
        assert_eq!(
            reloaded.txs_by_hash.get("tx-persisted"),
            Some(&persisted_tx)
        );
        assert_eq!(
            reloaded.tx_hash_by_action_id.get(&7),
            Some(&"tx-persisted".to_string())
        );

        std::fs::remove_dir_all(dir).expect("clean explorer store temp dir");
    }

    #[test]
    fn query_blocks_pages_descending_heights_without_full_sort() {
        let mut store = ExplorerStore::default();
        for height in [7, 3, 9, 4] {
            store.blocks_by_height.insert(height, block(height));
        }

        let first_page = store.query_blocks(2, 0);
        assert!(first_page.ok);
        assert_eq!(first_page.total, 4);
        assert_eq!(first_page.cursor, 0);
        assert_eq!(first_page.next_cursor, Some(2));
        assert_eq!(
            first_page
                .items
                .iter()
                .map(|item| item.height)
                .collect::<Vec<_>>(),
            vec![9, 7]
        );

        let second_page = store.query_blocks(2, 2);
        assert_eq!(second_page.cursor, 2);
        assert_eq!(second_page.next_cursor, None);
        assert_eq!(
            second_page
                .items
                .iter()
                .map(|item| item.height)
                .collect::<Vec<_>>(),
            vec![4, 3]
        );

        let empty_page = store.query_blocks(2, 99);
        assert_eq!(empty_page.cursor, 4);
        assert_eq!(empty_page.next_cursor, None);
        assert!(empty_page.items.is_empty());
    }

    #[test]
    fn query_txs_pages_sorted_refs_and_clones_only_page() {
        let mut store = ExplorerStore::default();
        for item in [
            tx(
                "tx-a",
                1,
                "alice",
                "bob",
                TransferLifecycleStatus::Accepted,
                100,
            ),
            tx(
                "tx-c",
                3,
                "alice",
                "carol",
                TransferLifecycleStatus::Confirmed,
                120,
            ),
            tx(
                "tx-b",
                2,
                "dora",
                "alice",
                TransferLifecycleStatus::Confirmed,
                120,
            ),
            tx(
                "tx-d",
                4,
                "erin",
                "frank",
                TransferLifecycleStatus::Failed,
                90,
            ),
        ] {
            store
                .tx_hash_by_action_id
                .insert(item.action_id, item.tx_hash.clone());
            store.txs_by_hash.insert(item.tx_hash.clone(), item);
        }

        let first_page = store.query_txs(
            Some("alice"),
            Some(TransferLifecycleStatus::Confirmed),
            None,
            1,
            0,
        );
        assert!(first_page.ok);
        assert_eq!(first_page.total, 2);
        assert_eq!(first_page.cursor, 0);
        assert_eq!(first_page.next_cursor, Some(1));
        assert_eq!(
            first_page
                .items
                .iter()
                .map(|item| item.tx_hash.as_str())
                .collect::<Vec<_>>(),
            vec!["tx-c"]
        );

        let second_page = store.query_txs(
            Some("alice"),
            Some(TransferLifecycleStatus::Confirmed),
            None,
            1,
            1,
        );
        assert_eq!(second_page.cursor, 1);
        assert_eq!(second_page.next_cursor, None);
        assert_eq!(
            second_page
                .items
                .iter()
                .map(|item| item.tx_hash.as_str())
                .collect::<Vec<_>>(),
            vec!["tx-b"]
        );

        let empty_page = store.query_txs(
            Some("alice"),
            Some(TransferLifecycleStatus::Confirmed),
            None,
            1,
            99,
        );
        assert_eq!(empty_page.cursor, 2);
        assert_eq!(empty_page.next_cursor, None);
        assert!(empty_page.items.is_empty());
    }

    #[test]
    fn prune_removes_oldest_txs_without_cloning_all_hashes() {
        let mut store = ExplorerStore::default();
        for index in 0..(EXPLORER_MAX_TRACKED_TXS + 2) {
            let item = tx(
                format!("tx-{index:04}").as_str(),
                index as u64,
                "alice",
                "bob",
                TransferLifecycleStatus::Accepted,
                index as i64,
            );
            store
                .tx_hash_by_action_id
                .insert(item.action_id, item.tx_hash.clone());
            store.txs_by_hash.insert(item.tx_hash.clone(), item);
        }

        store.prune();

        assert_eq!(store.txs_by_hash.len(), EXPLORER_MAX_TRACKED_TXS);
        assert!(!store.txs_by_hash.contains_key("tx-0000"));
        assert!(!store.txs_by_hash.contains_key("tx-0001"));
        assert!(!store.tx_hash_by_action_id.contains_key(&0));
        assert!(!store.tx_hash_by_action_id.contains_key(&1));
        assert!(store.txs_by_hash.contains_key("tx-0002"));
        assert!(
            store
                .txs_by_hash
                .contains_key(format!("tx-{:04}", EXPLORER_MAX_TRACKED_TXS + 1).as_str())
        );
    }
}
