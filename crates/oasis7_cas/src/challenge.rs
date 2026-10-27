use std::collections::BTreeMap;
use std::convert::TryFrom;

use oasis7_proto::distributed::{
    StorageChallengeFailureReason, StorageChallengeProofSemantics, StorageChallengeSampleSource,
};
use oasis7_proto::world_error::WorldError;
use serde::{Deserialize, Serialize};

use super::{LocalCasStore, blake3_hex, validate_hash};

pub const STORAGE_CHALLENGE_VERSION: u64 = 1;
pub const STORAGE_CHALLENGE_PROOF_KIND_CHUNK_HASH_V1: &str = "chunk_hash:v1";
pub const STORAGE_CHALLENGE_DEFAULT_MAX_SAMPLE_BYTES: u32 = 64 * 1024;
pub const STORAGE_CHALLENGE_DEFAULTS_PER_TICK: u32 = 1;
pub const STORAGE_CHALLENGE_DEFAULT_TTL_MS: i64 = 30_000;
pub const STORAGE_CHALLENGE_DEFAULT_ALLOWED_CLOCK_SKEW_MS: i64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageChallengeRequest {
    pub challenge_id: String,
    pub world_id: String,
    pub node_id: String,
    pub content_hash: String,
    pub max_sample_bytes: u32,
    pub issued_at_unix_ms: i64,
    pub challenge_ttl_ms: i64,
    pub vrf_seed: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageChallenge {
    pub version: u64,
    pub challenge_id: String,
    pub world_id: String,
    pub node_id: String,
    pub content_hash: String,
    pub sample_offset: u64,
    pub sample_size_bytes: u32,
    pub expected_sample_hash: String,
    pub issued_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub vrf_seed: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageChallengeReceipt {
    pub version: u64,
    pub challenge_id: String,
    pub node_id: String,
    pub content_hash: String,
    pub sample_offset: u64,
    pub sample_size_bytes: u32,
    pub sample_hash: String,
    pub responded_at_unix_ms: i64,
    pub sample_source: StorageChallengeSampleSource,
    pub failure_reason: Option<StorageChallengeFailureReason>,
    pub proof_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeStorageChallengeStats {
    pub node_id: String,
    pub total_checks: u64,
    pub passed_checks: u64,
    pub failed_checks: u64,
    pub failures_by_reason: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageChallengeProbeConfig {
    pub max_sample_bytes: u32,
    pub challenges_per_tick: u32,
    pub challenge_ttl_ms: i64,
    pub allowed_clock_skew_ms: i64,
}

impl Default for StorageChallengeProbeConfig {
    fn default() -> Self {
        Self {
            max_sample_bytes: STORAGE_CHALLENGE_DEFAULT_MAX_SAMPLE_BYTES,
            challenges_per_tick: STORAGE_CHALLENGE_DEFAULTS_PER_TICK,
            challenge_ttl_ms: STORAGE_CHALLENGE_DEFAULT_TTL_MS,
            allowed_clock_skew_ms: STORAGE_CHALLENGE_DEFAULT_ALLOWED_CLOCK_SKEW_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageChallengeProbeReport {
    pub node_id: String,
    pub world_id: String,
    pub observed_at_unix_ms: i64,
    pub total_checks: u64,
    pub passed_checks: u64,
    pub failed_checks: u64,
    pub failure_reasons: BTreeMap<String, u64>,
    pub latest_proof_semantics: Option<StorageChallengeProofSemantics>,
}

impl LocalCasStore {
    pub fn list_blob_hashes(&self) -> Result<Vec<String>, WorldError> {
        let mut hashes = Vec::new();
        if !self.blobs_dir().exists() {
            return Ok(hashes);
        }
        for entry in std::fs::read_dir(self.blobs_dir())? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("blob") {
                continue;
            }
            let file_stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| WorldError::DistributedValidationFailed {
                    reason: format!("invalid blob file name: {}", path.display()),
                })?;
            validate_hash(file_stem)?;
            hashes.push(file_stem.to_string());
        }
        hashes.sort();
        hashes.dedup();
        Ok(hashes)
    }

    pub fn issue_storage_challenge(
        &self,
        request: &StorageChallengeRequest,
    ) -> Result<StorageChallenge, WorldError> {
        validate_storage_challenge_request(request)?;

        let blob = self.get_verified(request.content_hash.as_str())?;
        let (sample_offset, sample_size_bytes, expected_sample_hash) = sample_window_for_blob(
            request.content_hash.as_str(),
            blob.as_slice(),
            request.max_sample_bytes,
            request.vrf_seed.as_str(),
        )?;
        let expires_at_unix_ms = request
            .issued_at_unix_ms
            .checked_add(request.challenge_ttl_ms)
            .ok_or_else(|| WorldError::DistributedValidationFailed {
                reason: format!(
                    "storage challenge ttl overflow: issued_at={} ttl={}",
                    request.issued_at_unix_ms, request.challenge_ttl_ms
                ),
            })?;

        Ok(StorageChallenge {
            version: STORAGE_CHALLENGE_VERSION,
            challenge_id: request.challenge_id.clone(),
            world_id: request.world_id.clone(),
            node_id: request.node_id.clone(),
            content_hash: request.content_hash.clone(),
            sample_offset,
            sample_size_bytes,
            expected_sample_hash,
            issued_at_unix_ms: request.issued_at_unix_ms,
            expires_at_unix_ms,
            vrf_seed: request.vrf_seed.clone(),
        })
    }

    pub fn answer_storage_challenge(
        &self,
        challenge: &StorageChallenge,
        responded_at_unix_ms: i64,
    ) -> Result<StorageChallengeReceipt, WorldError> {
        validate_storage_challenge(challenge)?;
        let blob = self.get_verified(challenge.content_hash.as_str())?;
        let sample = extract_sample_slice(
            blob.as_slice(),
            challenge.sample_offset,
            challenge.sample_size_bytes,
        )?;
        let sample_hash = blake3_hex(sample);
        let failure_reason = if sample_hash == challenge.expected_sample_hash {
            None
        } else {
            Some(StorageChallengeFailureReason::HashMismatch)
        };

        Ok(StorageChallengeReceipt {
            version: STORAGE_CHALLENGE_VERSION,
            challenge_id: challenge.challenge_id.clone(),
            node_id: challenge.node_id.clone(),
            content_hash: challenge.content_hash.clone(),
            sample_offset: challenge.sample_offset,
            sample_size_bytes: challenge.sample_size_bytes,
            sample_hash,
            responded_at_unix_ms,
            sample_source: StorageChallengeSampleSource::LocalStoreIndex,
            failure_reason,
            proof_kind: STORAGE_CHALLENGE_PROOF_KIND_CHUNK_HASH_V1.to_string(),
        })
    }

    pub fn probe_storage_challenges(
        &self,
        world_id: &str,
        node_id: &str,
        observed_at_unix_ms: i64,
        config: &StorageChallengeProbeConfig,
    ) -> Result<StorageChallengeProbeReport, WorldError> {
        validate_storage_challenge_probe_config(config)?;
        validate_non_empty_field(world_id, "world_id")?;
        validate_non_empty_field(node_id, "node_id")?;

        let hashes = self.list_blob_hashes()?;
        if hashes.is_empty() {
            return Ok(StorageChallengeProbeReport {
                node_id: node_id.to_string(),
                world_id: world_id.to_string(),
                observed_at_unix_ms,
                total_checks: 0,
                passed_checks: 0,
                failed_checks: 0,
                failure_reasons: BTreeMap::new(),
                latest_proof_semantics: None,
            });
        }

        let mut report = StorageChallengeProbeReport {
            node_id: node_id.to_string(),
            world_id: world_id.to_string(),
            observed_at_unix_ms,
            total_checks: 0,
            passed_checks: 0,
            failed_checks: 0,
            failure_reasons: BTreeMap::new(),
            latest_proof_semantics: None,
        };

        let checks = (config.challenges_per_tick as usize).min(hashes.len());
        let start_index =
            deterministic_probe_start_index(world_id, node_id, observed_at_unix_ms, hashes.len());

        for index in 0..checks {
            let hash = hashes[(start_index + index) % hashes.len()].clone();
            report.total_checks = report.total_checks.saturating_add(1);
            let request = StorageChallengeRequest {
                challenge_id: build_probe_challenge_id(
                    node_id,
                    observed_at_unix_ms,
                    index as u32,
                    hash.as_str(),
                ),
                world_id: world_id.to_string(),
                node_id: node_id.to_string(),
                content_hash: hash,
                max_sample_bytes: config.max_sample_bytes,
                issued_at_unix_ms: observed_at_unix_ms,
                challenge_ttl_ms: config.challenge_ttl_ms,
                vrf_seed: build_probe_seed(world_id, node_id, observed_at_unix_ms, index as u32),
            };

            let challenge = match self.issue_storage_challenge(&request) {
                Ok(challenge) => challenge,
                Err(err) => {
                    report.failed_checks = report.failed_checks.saturating_add(1);
                    let reason = classify_world_error_failure_reason(&err);
                    increment_failure_reason(
                        &mut report.failure_reasons,
                        failure_reason_key(reason),
                    );
                    continue;
                }
            };

            let receipt = match self.answer_storage_challenge(&challenge, observed_at_unix_ms) {
                Ok(receipt) => receipt,
                Err(err) => {
                    report.failed_checks = report.failed_checks.saturating_add(1);
                    let reason = classify_world_error_failure_reason(&err);
                    increment_failure_reason(
                        &mut report.failure_reasons,
                        failure_reason_key(reason),
                    );
                    continue;
                }
            };

            match verify_storage_challenge_receipt(
                &challenge,
                &receipt,
                config.allowed_clock_skew_ms,
            ) {
                Ok(()) => {
                    report.passed_checks = report.passed_checks.saturating_add(1);
                    report.latest_proof_semantics = Some(
                        storage_challenge_receipt_to_proof_semantics(&challenge, &receipt),
                    );
                }
                Err(_) => {
                    report.failed_checks = report.failed_checks.saturating_add(1);
                    let reason = classify_receipt_failure_reason(
                        &challenge,
                        &receipt,
                        config.allowed_clock_skew_ms,
                    );
                    increment_failure_reason(
                        &mut report.failure_reasons,
                        failure_reason_key(reason),
                    );
                }
            }
        }

        Ok(report)
    }
}

pub fn verify_storage_challenge_receipt(
    challenge: &StorageChallenge,
    receipt: &StorageChallengeReceipt,
    allowed_clock_skew_ms: i64,
) -> Result<(), WorldError> {
    if allowed_clock_skew_ms < 0 {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "allowed_clock_skew_ms must be >= 0, got {}",
                allowed_clock_skew_ms
            ),
        });
    }
    validate_storage_challenge(challenge)?;
    validate_storage_challenge_receipt(receipt)?;

    if challenge.challenge_id != receipt.challenge_id {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "challenge_id mismatch: expected={} actual={}",
                challenge.challenge_id, receipt.challenge_id
            ),
        });
    }
    if challenge.node_id != receipt.node_id {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "node_id mismatch: expected={} actual={}",
                challenge.node_id, receipt.node_id
            ),
        });
    }
    if challenge.content_hash != receipt.content_hash {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "content_hash mismatch: expected={} actual={}",
                challenge.content_hash, receipt.content_hash
            ),
        });
    }
    if challenge.sample_offset != receipt.sample_offset {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "sample_offset mismatch: expected={} actual={}",
                challenge.sample_offset, receipt.sample_offset
            ),
        });
    }
    if challenge.sample_size_bytes != receipt.sample_size_bytes {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "sample_size_bytes mismatch: expected={} actual={}",
                challenge.sample_size_bytes, receipt.sample_size_bytes
            ),
        });
    }
    if receipt.sample_hash != challenge.expected_sample_hash {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "sample_hash mismatch: expected={} actual={}",
                challenge.expected_sample_hash, receipt.sample_hash
            ),
        });
    }
    if receipt.proof_kind != STORAGE_CHALLENGE_PROOF_KIND_CHUNK_HASH_V1 {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "unsupported proof kind: expected={} actual={}",
                STORAGE_CHALLENGE_PROOF_KIND_CHUNK_HASH_V1, receipt.proof_kind
            ),
        });
    }
    if receipt.sample_source == StorageChallengeSampleSource::Unknown {
        return Err(WorldError::DistributedValidationFailed {
            reason: "sample_source cannot be Unknown".to_string(),
        });
    }
    if let Some(reason) = receipt.failure_reason {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("receipt indicates failure: {:?}", reason),
        });
    }

    let min_time = challenge
        .issued_at_unix_ms
        .saturating_sub(allowed_clock_skew_ms);
    let max_time = challenge
        .expires_at_unix_ms
        .saturating_add(allowed_clock_skew_ms);
    if receipt.responded_at_unix_ms < min_time || receipt.responded_at_unix_ms > max_time {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "response timestamp out of challenge window: responded_at={} allowed=[{}, {}]",
                receipt.responded_at_unix_ms, min_time, max_time
            ),
        });
    }

    Ok(())
}

pub fn storage_challenge_receipt_to_proof_semantics(
    challenge: &StorageChallenge,
    receipt: &StorageChallengeReceipt,
) -> StorageChallengeProofSemantics {
    StorageChallengeProofSemantics {
        node_id: receipt.node_id.clone(),
        sample_source: receipt.sample_source,
        sample_reference: challenge_sample_reference(challenge),
        failure_reason: receipt.failure_reason,
        proof_kind_hint: receipt.proof_kind.clone(),
        vrf_seed_hint: Some(challenge.vrf_seed.clone()),
        post_commitment_hint: Some(challenge.expected_sample_hash.clone()),
    }
}

pub fn summarize_node_storage_challenge_stats(
    entries: &[(StorageChallenge, StorageChallengeReceipt)],
    allowed_clock_skew_ms: i64,
) -> Result<Vec<NodeStorageChallengeStats>, WorldError> {
    if allowed_clock_skew_ms < 0 {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "allowed_clock_skew_ms must be >= 0, got {}",
                allowed_clock_skew_ms
            ),
        });
    }
    let mut per_node: BTreeMap<String, NodeStorageChallengeStats> = BTreeMap::new();

    for (challenge, receipt) in entries {
        let stats = per_node
            .entry(challenge.node_id.clone())
            .or_insert_with(|| NodeStorageChallengeStats {
                node_id: challenge.node_id.clone(),
                total_checks: 0,
                passed_checks: 0,
                failed_checks: 0,
                failures_by_reason: BTreeMap::new(),
            });
        stats.total_checks = stats.total_checks.saturating_add(1);

        match verify_storage_challenge_receipt(challenge, receipt, allowed_clock_skew_ms) {
            Ok(()) => {
                stats.passed_checks = stats.passed_checks.saturating_add(1);
            }
            Err(_) => {
                stats.failed_checks = stats.failed_checks.saturating_add(1);
                let reason =
                    classify_receipt_failure_reason(challenge, receipt, allowed_clock_skew_ms);
                let key = failure_reason_key(reason).to_string();
                let count = stats.failures_by_reason.entry(key).or_insert(0);
                *count = count.saturating_add(1);
            }
        }
    }

    Ok(per_node.into_values().collect())
}

fn validate_storage_challenge_probe_config(
    config: &StorageChallengeProbeConfig,
) -> Result<(), WorldError> {
    if config.max_sample_bytes == 0 {
        return Err(WorldError::DistributedValidationFailed {
            reason: "probe max_sample_bytes must be >= 1".to_string(),
        });
    }
    if config.challenges_per_tick == 0 {
        return Err(WorldError::DistributedValidationFailed {
            reason: "probe challenges_per_tick must be >= 1".to_string(),
        });
    }
    if config.challenge_ttl_ms <= 0 {
        return Err(WorldError::DistributedValidationFailed {
            reason: "probe challenge_ttl_ms must be > 0".to_string(),
        });
    }
    if config.allowed_clock_skew_ms < 0 {
        return Err(WorldError::DistributedValidationFailed {
            reason: "probe allowed_clock_skew_ms must be >= 0".to_string(),
        });
    }
    Ok(())
}

fn validate_storage_challenge_request(request: &StorageChallengeRequest) -> Result<(), WorldError> {
    validate_non_empty_field(request.challenge_id.as_str(), "challenge_id")?;
    validate_non_empty_field(request.world_id.as_str(), "world_id")?;
    validate_non_empty_field(request.node_id.as_str(), "node_id")?;
    validate_non_empty_field(request.vrf_seed.as_str(), "vrf_seed")?;
    validate_hash(request.content_hash.as_str())?;
    if request.max_sample_bytes == 0 {
        return Err(WorldError::DistributedValidationFailed {
            reason: "max_sample_bytes must be >= 1".to_string(),
        });
    }
    if request.challenge_ttl_ms <= 0 {
        return Err(WorldError::DistributedValidationFailed {
            reason: "challenge_ttl_ms must be > 0".to_string(),
        });
    }
    Ok(())
}

fn validate_storage_challenge(challenge: &StorageChallenge) -> Result<(), WorldError> {
    if challenge.version != STORAGE_CHALLENGE_VERSION {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "unsupported storage challenge version: expected={} actual={}",
                STORAGE_CHALLENGE_VERSION, challenge.version
            ),
        });
    }
    validate_non_empty_field(challenge.challenge_id.as_str(), "challenge_id")?;
    validate_non_empty_field(challenge.world_id.as_str(), "world_id")?;
    validate_non_empty_field(challenge.node_id.as_str(), "node_id")?;
    validate_non_empty_field(challenge.vrf_seed.as_str(), "vrf_seed")?;
    validate_hash(challenge.content_hash.as_str())?;
    validate_hash(challenge.expected_sample_hash.as_str())?;
    if challenge.expires_at_unix_ms < challenge.issued_at_unix_ms {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "storage challenge expires_at is earlier than issued_at: issued_at={} expires_at={}",
                challenge.issued_at_unix_ms, challenge.expires_at_unix_ms
            ),
        });
    }
    Ok(())
}

fn validate_storage_challenge_receipt(receipt: &StorageChallengeReceipt) -> Result<(), WorldError> {
    if receipt.version != STORAGE_CHALLENGE_VERSION {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "unsupported storage challenge receipt version: expected={} actual={}",
                STORAGE_CHALLENGE_VERSION, receipt.version
            ),
        });
    }
    validate_non_empty_field(receipt.challenge_id.as_str(), "challenge_id")?;
    validate_non_empty_field(receipt.node_id.as_str(), "node_id")?;
    validate_non_empty_field(receipt.proof_kind.as_str(), "proof_kind")?;
    validate_hash(receipt.content_hash.as_str())?;
    validate_hash(receipt.sample_hash.as_str())?;
    Ok(())
}

fn challenge_sample_reference(challenge: &StorageChallenge) -> String {
    format!(
        "distfs://{}/challenge/{}/blob/{}?offset={}&size={}",
        challenge.node_id,
        challenge.challenge_id,
        challenge.content_hash,
        challenge.sample_offset,
        challenge.sample_size_bytes
    )
}

fn build_probe_challenge_id(
    node_id: &str,
    observed_at_unix_ms: i64,
    probe_index: u32,
    content_hash: &str,
) -> String {
    let hash_prefix_len = content_hash.len().min(12);
    let hash_prefix = &content_hash[..hash_prefix_len];
    format!(
        "probe:{}:{}:{}:{}",
        node_id, observed_at_unix_ms, probe_index, hash_prefix
    )
}

fn build_probe_seed(
    world_id: &str,
    node_id: &str,
    observed_at_unix_ms: i64,
    probe_index: u32,
) -> String {
    format!(
        "{}:{}:{}:{}",
        world_id, node_id, observed_at_unix_ms, probe_index
    )
}

fn deterministic_probe_start_index(
    world_id: &str,
    node_id: &str,
    observed_at_unix_ms: i64,
    candidate_len: usize,
) -> usize {
    if candidate_len <= 1 {
        return 0;
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(world_id.as_bytes());
    bytes.push(b':');
    bytes.extend_from_slice(node_id.as_bytes());
    bytes.push(b':');
    bytes.extend_from_slice(observed_at_unix_ms.to_string().as_bytes());
    let digest = blake3::hash(bytes.as_slice());
    let mut prefix = [0u8; 8];
    prefix.copy_from_slice(&digest.as_bytes()[..8]);
    let seed = u64::from_le_bytes(prefix);
    (seed as usize) % candidate_len
}

fn increment_failure_reason(map: &mut BTreeMap<String, u64>, reason_key: &str) {
    let entry = map.entry(reason_key.to_string()).or_insert(0);
    *entry = entry.saturating_add(1);
}

fn classify_receipt_failure_reason(
    challenge: &StorageChallenge,
    receipt: &StorageChallengeReceipt,
    allowed_clock_skew_ms: i64,
) -> StorageChallengeFailureReason {
    if let Some(reason) = receipt.failure_reason {
        return reason;
    }
    if receipt.proof_kind != STORAGE_CHALLENGE_PROOF_KIND_CHUNK_HASH_V1 {
        return StorageChallengeFailureReason::SignatureInvalid;
    }
    if challenge.sample_offset != receipt.sample_offset
        || challenge.sample_size_bytes != receipt.sample_size_bytes
    {
        return StorageChallengeFailureReason::MissingSample;
    }
    if challenge.content_hash != receipt.content_hash
        || challenge.expected_sample_hash != receipt.sample_hash
    {
        return StorageChallengeFailureReason::HashMismatch;
    }
    let min_time = challenge
        .issued_at_unix_ms
        .saturating_sub(allowed_clock_skew_ms);
    let max_time = challenge
        .expires_at_unix_ms
        .saturating_add(allowed_clock_skew_ms);
    if receipt.responded_at_unix_ms < min_time || receipt.responded_at_unix_ms > max_time {
        return StorageChallengeFailureReason::Timeout;
    }
    StorageChallengeFailureReason::Unknown
}

fn classify_world_error_failure_reason(error: &WorldError) -> StorageChallengeFailureReason {
    match error {
        WorldError::NetworkFailure { source, .. } => classify_world_error_failure_reason(source),
        WorldError::BlobNotFound { .. } => StorageChallengeFailureReason::MissingSample,
        WorldError::BlobHashMismatch { .. } | WorldError::BlobHashInvalid { .. } => {
            StorageChallengeFailureReason::HashMismatch
        }
        WorldError::Io(_) => StorageChallengeFailureReason::ReadIoError,
        WorldError::DistributedValidationFailed { reason } => {
            let lower = reason.to_ascii_lowercase();
            if lower.contains("timeout") || lower.contains("window") {
                StorageChallengeFailureReason::Timeout
            } else if lower.contains("hash") {
                StorageChallengeFailureReason::HashMismatch
            } else if lower.contains("sample") || lower.contains("missing") {
                StorageChallengeFailureReason::MissingSample
            } else if lower.contains("signature") || lower.contains("proof") {
                StorageChallengeFailureReason::SignatureInvalid
            } else {
                StorageChallengeFailureReason::Unknown
            }
        }
        WorldError::SignatureKeyInvalid => StorageChallengeFailureReason::SignatureInvalid,
        WorldError::NetworkProtocolUnavailable { .. }
        | WorldError::NetworkRequestFailed { .. }
        | WorldError::Serde(_) => StorageChallengeFailureReason::Unknown,
    }
}

fn failure_reason_key(reason: StorageChallengeFailureReason) -> &'static str {
    match reason {
        StorageChallengeFailureReason::MissingSample => "MISSING_SAMPLE",
        StorageChallengeFailureReason::HashMismatch => "HASH_MISMATCH",
        StorageChallengeFailureReason::Timeout => "TIMEOUT",
        StorageChallengeFailureReason::ReadIoError => "READ_IO_ERROR",
        StorageChallengeFailureReason::SignatureInvalid => "SIGNATURE_INVALID",
        StorageChallengeFailureReason::Unknown => "UNKNOWN",
    }
}

fn validate_non_empty_field(value: &str, field_name: &str) -> Result<(), WorldError> {
    if value.trim().is_empty() {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("storage challenge field {} cannot be empty", field_name),
        });
    }
    Ok(())
}

fn sample_window_for_blob(
    content_hash: &str,
    blob: &[u8],
    max_sample_bytes: u32,
    vrf_seed: &str,
) -> Result<(u64, u32, String), WorldError> {
    if max_sample_bytes == 0 {
        return Err(WorldError::DistributedValidationFailed {
            reason: "max_sample_bytes must be >= 1".to_string(),
        });
    }
    let blob_len = blob.len();
    let sample_size = blob_len.min(max_sample_bytes as usize);
    let sample_size_bytes =
        u32::try_from(sample_size).map_err(|_| WorldError::DistributedValidationFailed {
            reason: format!(
                "sample size conversion overflow: blob_len={} sample_size={}",
                blob_len, sample_size
            ),
        })?;

    let offset = deterministic_offset(content_hash, vrf_seed, blob_len, sample_size);
    let sample = extract_sample_slice(blob, offset, sample_size_bytes)?;
    let expected_sample_hash = blake3_hex(sample);
    Ok((offset, sample_size_bytes, expected_sample_hash))
}

fn deterministic_offset(
    content_hash: &str,
    vrf_seed: &str,
    blob_len: usize,
    sample_size: usize,
) -> u64 {
    if blob_len <= sample_size {
        return 0;
    }
    let mut seed_material = Vec::with_capacity(content_hash.len() + vrf_seed.len() + 1);
    seed_material.extend_from_slice(content_hash.as_bytes());
    seed_material.push(b':');
    seed_material.extend_from_slice(vrf_seed.as_bytes());
    let digest = blake3::hash(seed_material.as_slice());
    let mut prefix = [0u8; 8];
    prefix.copy_from_slice(&digest.as_bytes()[..8]);
    let random_value = u64::from_le_bytes(prefix);
    let max_offset = (blob_len - sample_size) as u64;
    random_value % (max_offset + 1)
}

fn extract_sample_slice(
    blob: &[u8],
    sample_offset: u64,
    sample_size_bytes: u32,
) -> Result<&[u8], WorldError> {
    let offset =
        usize::try_from(sample_offset).map_err(|_| WorldError::DistributedValidationFailed {
            reason: format!("sample_offset overflow: {}", sample_offset),
        })?;
    let size = sample_size_bytes as usize;
    let end = offset
        .checked_add(size)
        .ok_or_else(|| WorldError::DistributedValidationFailed {
            reason: format!(
                "sample window overflow: offset={} size={}",
                sample_offset, sample_size_bytes
            ),
        })?;
    if end > blob.len() {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "sample window out of bounds: offset={} size={} blob_len={}",
                sample_offset,
                sample_size_bytes,
                blob.len()
            ),
        });
    }
    Ok(&blob[offset..end])
}

#[cfg(test)]
mod tests;
