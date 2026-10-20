use std::iter::Peekable;
use std::path::Path;

use oasis7_distfs::{
    StorageChallengeAdaptivePolicy, StorageChallengeProbeConfig, StorageChallengeProbeCursorState,
    StorageChallengeProbeReport,
};
use oasis7_runtime::LocalCasStore;
use serde::Serialize;

pub(super) const DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_MAX_SAMPLE_BYTES: u32 = 64 * 1024;
pub(super) const DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_PER_TICK: u32 = 1;
pub(super) const DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_TTL_MS: i64 = 30_000;
pub(super) const DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_ALLOWED_CLOCK_SKEW_MS: i64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct DistfsProbeRuntimeConfig {
    pub max_sample_bytes: u32,
    pub challenges_per_tick: u32,
    pub challenge_ttl_ms: i64,
    pub allowed_clock_skew_ms: i64,
    pub adaptive_policy: StorageChallengeAdaptivePolicy,
}

impl Default for DistfsProbeRuntimeConfig {
    fn default() -> Self {
        Self {
            max_sample_bytes: DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_MAX_SAMPLE_BYTES,
            challenges_per_tick: DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_PER_TICK,
            challenge_ttl_ms: DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_TTL_MS,
            allowed_clock_skew_ms: DEFAULT_REWARD_RUNTIME_DISTFS_PROBE_ALLOWED_CLOCK_SKEW_MS,
            adaptive_policy: StorageChallengeAdaptivePolicy::default(),
        }
    }
}

pub(super) fn parse_distfs_probe_runtime_option<'a, I: Iterator<Item = &'a str>>(
    arg: &str,
    iter: &mut Peekable<I>,
    config: &mut DistfsProbeRuntimeConfig,
) -> Result<bool, String> {
    match arg {
        "--reward-distfs-probe-max-sample-bytes" => {
            config.max_sample_bytes = parse_u32_flag(
                iter,
                "--reward-distfs-probe-max-sample-bytes",
                "a positive integer",
                |value| value > 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-probe-per-tick" => {
            config.challenges_per_tick = parse_u32_flag(
                iter,
                "--reward-distfs-probe-per-tick",
                "a positive integer",
                |value| value > 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-probe-ttl-ms" => {
            config.challenge_ttl_ms = parse_i64_flag(
                iter,
                "--reward-distfs-probe-ttl-ms",
                "a positive integer",
                |value| value > 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-probe-allowed-clock-skew-ms" => {
            config.allowed_clock_skew_ms = parse_i64_flag(
                iter,
                "--reward-distfs-probe-allowed-clock-skew-ms",
                "a non-negative integer",
                |value| value >= 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-adaptive-max-checks-per-round" => {
            config.adaptive_policy.max_checks_per_round = parse_u32_flag(
                iter,
                "--reward-distfs-adaptive-max-checks-per-round",
                "a positive integer",
                |value| value > 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-adaptive-backoff-base-ms" => {
            config.adaptive_policy.failure_backoff_base_ms = parse_i64_flag(
                iter,
                "--reward-distfs-adaptive-backoff-base-ms",
                "a non-negative integer",
                |value| value >= 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-adaptive-backoff-max-ms" => {
            config.adaptive_policy.failure_backoff_max_ms = parse_i64_flag(
                iter,
                "--reward-distfs-adaptive-backoff-max-ms",
                "a non-negative integer",
                |value| value >= 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-adaptive-multiplier-hash-mismatch" => {
            config.adaptive_policy.backoff_multiplier_hash_mismatch = parse_u32_flag(
                iter,
                "--reward-distfs-adaptive-multiplier-hash-mismatch",
                "a positive integer",
                |value| value > 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-adaptive-multiplier-missing-sample" => {
            config.adaptive_policy.backoff_multiplier_missing_sample = parse_u32_flag(
                iter,
                "--reward-distfs-adaptive-multiplier-missing-sample",
                "a positive integer",
                |value| value > 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-adaptive-multiplier-timeout" => {
            config.adaptive_policy.backoff_multiplier_timeout = parse_u32_flag(
                iter,
                "--reward-distfs-adaptive-multiplier-timeout",
                "a positive integer",
                |value| value > 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-adaptive-multiplier-read-io-error" => {
            config.adaptive_policy.backoff_multiplier_read_io_error = parse_u32_flag(
                iter,
                "--reward-distfs-adaptive-multiplier-read-io-error",
                "a positive integer",
                |value| value > 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-adaptive-multiplier-signature-invalid" => {
            config.adaptive_policy.backoff_multiplier_signature_invalid = parse_u32_flag(
                iter,
                "--reward-distfs-adaptive-multiplier-signature-invalid",
                "a positive integer",
                |value| value > 0,
            )?;
            Ok(true)
        }
        "--reward-distfs-adaptive-multiplier-unknown" => {
            config.adaptive_policy.backoff_multiplier_unknown = parse_u32_flag(
                iter,
                "--reward-distfs-adaptive-multiplier-unknown",
                "a positive integer",
                |value| value > 0,
            )?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn parse_u32_flag<'a, I: Iterator<Item = &'a str>, F: Fn(u32) -> bool>(
    iter: &mut Peekable<I>,
    flag: &str,
    requirement: &str,
    predicate: F,
) -> Result<u32, String> {
    let raw = iter
        .next()
        .ok_or_else(|| format!("{flag} requires {requirement}"))?;
    raw.parse::<u32>()
        .ok()
        .filter(|value| predicate(*value))
        .ok_or_else(|| format!("{flag} requires {requirement}"))
}

fn parse_i64_flag<'a, I: Iterator<Item = &'a str>, F: Fn(i64) -> bool>(
    iter: &mut Peekable<I>,
    flag: &str,
    requirement: &str,
    predicate: F,
) -> Result<i64, String> {
    let raw = iter
        .next()
        .ok_or_else(|| format!("{flag} requires {requirement}"))?;
    raw.parse::<i64>()
        .ok()
        .filter(|value| predicate(*value))
        .ok_or_else(|| format!("{flag} requires {requirement}"))
}

pub(super) fn load_reward_runtime_distfs_probe_state(
    path: &Path,
) -> Result<StorageChallengeProbeCursorState, String> {
    if !path.exists() {
        return Ok(StorageChallengeProbeCursorState::default());
    }
    let bytes = std::fs::read(path)
        .map_err(|err| format!("read distfs probe state {} failed: {}", path.display(), err))?;
    serde_json::from_slice::<StorageChallengeProbeCursorState>(bytes.as_slice()).map_err(|err| {
        format!(
            "parse distfs probe state {} failed: {}",
            path.display(),
            err
        )
    })
}

pub(super) fn persist_reward_runtime_distfs_probe_state(
    path: &Path,
    state: &StorageChallengeProbeCursorState,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|err| format!("serialize distfs probe state failed: {}", err))?;
    super::write_bytes_atomic(path, bytes.as_slice())
}

pub(super) fn collect_distfs_challenge_report_with_config(
    storage_root: &Path,
    world_id: &str,
    node_id: &str,
    observed_at_unix_ms: i64,
    state: &mut StorageChallengeProbeCursorState,
    config: &DistfsProbeRuntimeConfig,
) -> Result<StorageChallengeProbeReport, String> {
    let store = LocalCasStore::new(storage_root);
    store
        .probe_storage_challenges_with_policy(
            world_id,
            node_id,
            observed_at_unix_ms,
            &StorageChallengeProbeConfig {
                max_sample_bytes: config.max_sample_bytes,
                challenges_per_tick: config.challenges_per_tick,
                challenge_ttl_ms: config.challenge_ttl_ms,
                allowed_clock_skew_ms: config.allowed_clock_skew_ms,
            },
            state,
            &config.adaptive_policy,
        )
        .map_err(|err| format!("{err:?}"))
}
