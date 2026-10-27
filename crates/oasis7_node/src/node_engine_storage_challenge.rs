use super::*;
use crate::replication_probe_gate::{
    request_fetch_blob_with_route_fallback, should_fallback_provider_aware_replication_request,
};

impl PosNodeEngine {
    pub(super) fn is_storage_challenge_success_cache_height_valid(
        &self,
        checked_height: u64,
    ) -> bool {
        let min_height = self
            .committed_height
            .saturating_sub(STORAGE_CHALLENGE_SUCCESS_CACHE_MAX_AGE_HEIGHTS);
        checked_height > min_height && checked_height <= self.committed_height
    }

    pub(super) fn prune_storage_challenge_success_cache(&mut self) {
        let committed_height = self.committed_height;
        let min_height =
            committed_height.saturating_sub(STORAGE_CHALLENGE_SUCCESS_CACHE_MAX_AGE_HEIGHTS);
        self.recent_storage_challenge_successes
            .retain(|_, checked_height| {
                *checked_height > min_height && *checked_height <= committed_height
            });
    }

    pub(super) fn storage_challenge_success_cache_hit(
        &self,
        replication: &ReplicationRuntime,
        content_hash: &str,
    ) -> Result<bool, NodeError> {
        let Some(&checked_height) = self.recent_storage_challenge_successes.get(content_hash)
        else {
            return Ok(false);
        };
        if !self.is_storage_challenge_success_cache_height_valid(checked_height) {
            return Ok(false);
        }
        Ok(replication.load_blob_by_hash(content_hash)?.is_some())
    }

    pub(super) fn mark_storage_challenge_success(&mut self, content_hash: &str) {
        self.recent_storage_challenge_successes
            .insert(content_hash.to_string(), self.committed_height);
    }

    pub(super) fn clear_storage_challenge_network_degraded(&mut self) {
        self.storage_challenge_network_degraded_height = None;
        self.storage_challenge_network_degraded_reason = None;
        self.storage_challenge_network_next_probe_after_ms = None;
    }

    pub(super) fn mark_storage_challenge_network_degraded(
        &mut self,
        now_ms: i64,
        required_matches: usize,
        successful_matches: usize,
        failure_reasons: Vec<String>,
    ) {
        let latest_reason = failure_reasons
            .last()
            .cloned()
            .unwrap_or_else(|| "storage challenge network unavailable".to_string());
        self.storage_challenge_network_degraded_height = Some(self.committed_height);
        self.storage_challenge_network_degraded_reason = Some(format!(
            "storage challenge network degraded: required_matches={} successful_matches={} latest_reason={}",
            required_matches, successful_matches, latest_reason
        ));
        self.storage_challenge_network_last_probe_at_ms = Some(now_ms);
        self.storage_challenge_network_next_probe_after_ms =
            Some(now_ms.saturating_add(STORAGE_CHALLENGE_NETWORK_RETRY_COOLDOWN_MS));
    }

    pub(super) fn storage_challenge_network_probe_in_cooldown(&self, now_ms: i64) -> bool {
        self.storage_challenge_network_next_probe_after_ms
            .map(|next_probe_after_ms| now_ms < next_probe_after_ms)
            .unwrap_or(false)
    }
}

pub(super) enum StorageChallengeSampleOutcome {
    Matched,
    Unavailable { reason: String },
    HardFailure { reason: String },
}

pub(super) fn evaluate_storage_challenge_sample(
    replication: &ReplicationRuntime,
    endpoint: &ReplicationNetworkEndpoint,
    world_id: &str,
    content_hash: &str,
) -> Result<StorageChallengeSampleOutcome, NodeError> {
    let local_blob = match replication.load_blob_by_hash(content_hash)? {
        Some(blob) => blob,
        None => {
            return Ok(StorageChallengeSampleOutcome::HardFailure {
                reason: format!(
                    "storage challenge gate local blob missing for hash {}",
                    content_hash
                ),
            });
        }
    };
    let fetch_blob_request = replication.build_fetch_blob_request(content_hash)?;
    let mut provider_lookup_failure = None;
    let provider_lookup =
        match endpoint.lookup_provider_ids_for_content_hash(world_id, content_hash) {
            Ok(provider_ids) => provider_ids,
            Err(err) if storage_challenge_provider_lookup_can_fallback(&err) => {
                provider_lookup_failure = Some(format!("{:?}", err));
                None
            }
            Err(err) => {
                return Ok(StorageChallengeSampleOutcome::Unavailable {
                    reason: format!(
                        "storage challenge gate provider lookup failed for hash {}: {:?}",
                        content_hash, err
                    ),
                });
            }
        };
    let response = match if provider_lookup_failure.is_some() {
        request_fetch_blob_with_route_fallback(
            endpoint,
            world_id,
            content_hash,
            &fetch_blob_request,
            None,
        )
    } else {
        request_fetch_blob_with_storage_challenge_routes(
            endpoint,
            world_id,
            content_hash,
            &fetch_blob_request,
            provider_lookup.as_deref(),
        )
    } {
        Ok(response) => response,
        Err(err) if should_fallback_provider_aware_replication_request(&err) => {
            let reason = if let Some(provider_lookup_failure) = provider_lookup_failure.as_deref() {
                format!(
                    "storage challenge gate network request failed for hash {} after provider lookup failed: {}; {:?}",
                    content_hash, provider_lookup_failure, err
                )
            } else {
                format!(
                    "storage challenge gate network request failed for hash {}: {:?}",
                    content_hash, err
                )
            };
            return Ok(StorageChallengeSampleOutcome::Unavailable { reason });
        }
        Err(err) => {
            return Ok(StorageChallengeSampleOutcome::HardFailure {
                reason: format!(
                    "storage challenge gate invalid network response for hash {}: {:?}",
                    content_hash, err
                ),
            });
        }
    };
    if !response.found {
        return Ok(StorageChallengeSampleOutcome::Unavailable {
            reason: format!(
                "storage challenge gate network blob not found for hash {}",
                content_hash
            ),
        });
    }
    let Some(network_blob) = response.blob else {
        return Ok(StorageChallengeSampleOutcome::Unavailable {
            reason: format!(
                "storage challenge gate network blob payload missing for hash {}",
                content_hash
            ),
        });
    };
    if blake3_hex(network_blob.as_slice()) != content_hash {
        if let Some(provider_lookup_failure) = provider_lookup_failure.as_deref() {
            return Ok(StorageChallengeSampleOutcome::Unavailable {
                reason: format!(
                    "storage challenge gate fallback blob hash mismatch for hash {} after provider lookup failed: {}",
                    content_hash, provider_lookup_failure
                ),
            });
        }
        return Ok(StorageChallengeSampleOutcome::HardFailure {
            reason: format!(
                "storage challenge gate network blob hash mismatch for hash {}",
                content_hash
            ),
        });
    }
    if network_blob != local_blob {
        return Ok(StorageChallengeSampleOutcome::HardFailure {
            reason: format!(
                "storage challenge gate local blob bytes mismatch for hash {}",
                content_hash
            ),
        });
    }

    Ok(StorageChallengeSampleOutcome::Matched)
}

fn storage_challenge_provider_lookup_can_fallback(err: &NodeError) -> bool {
    use oasis7_proto::network_failure::FailureCause;
    let facts = crate::network_error_classification::replication_network_failure(err);
    facts.is_connection_gap()
        || matches!(
            facts.cause,
            FailureCause::ProtocolUnavailable
                | FailureCause::MissingHandler
                | FailureCause::Timeout
                | FailureCause::Unavailable
                | FailureCause::Busy
                | FailureCause::RateLimited
        )
}
