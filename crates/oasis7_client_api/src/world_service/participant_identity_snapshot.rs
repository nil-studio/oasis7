//! Offline identity snapshot DTO. A valid digest proves integrity, not issuer authority.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy)]
pub struct SnapshotError;
impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid complete identity snapshot")
    }
}
impl std::error::Error for SnapshotError {}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SnapshotIdentityNamespace {
    pub identity_service_id: String,
    pub environment_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRpConfig {
    pub rp_id: String,
    pub allowed_origins: Vec<String>,
    pub version: u64,
    pub identity_instance_epoch: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SnapshotAccount {
    pub account_id: String,
    pub status: String,
    pub security_epoch: u64,
    pub row_version: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SnapshotBinding {
    pub account_id: String,
    pub world_identity_scope: String,
    pub player_id: String,
    pub binding_version: u64,
    pub row_version: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IdentitySnapshot {
    pub api_rev: u32,
    pub identity_namespace: SnapshotIdentityNamespace,
    pub rp_config: SnapshotRpConfig,
    pub source_snapshot_epoch: u64,
    /// Full store commitment, not a guessed monotonically increasing sequence.
    pub source_watermark: String,
    pub source_snapshot_digest: String,
    pub complete: bool,
    pub source_kind: String,
    pub account_count: usize,
    pub binding_count: usize,
    pub accounts: Vec<SnapshotAccount>,
    pub bindings: Vec<SnapshotBinding>,
}
impl IdentitySnapshot {
    pub fn computed_digest(&self) -> Result<String, SnapshotError> {
        let mut candidate = self.clone();
        candidate.source_snapshot_digest.clear();
        let bytes = serde_json::to_vec(&candidate).map_err(|_| SnapshotError)?;
        Ok(format!("blake3:{}", blake3::hash(&bytes)))
    }
    pub fn verify(&self) -> Result<(), SnapshotError> {
        let digest = self.computed_digest()?;
        if !self.complete
            || self.api_rev != 1
            || self.source_snapshot_epoch == 0
            || self.rp_config.version == 0
            || self.rp_config.rp_id.trim().is_empty()
            || self.rp_config.allowed_origins.is_empty()
            || self
                .identity_namespace
                .identity_service_id
                .trim()
                .is_empty()
            || self.identity_namespace.environment_id.trim().is_empty()
            || !self.source_watermark.starts_with("sqlite-content-v1:")
            || self.source_watermark.len() > 128
            || self.source_kind != "sqlite_single_read_transaction_v1"
            || self.source_snapshot_epoch != self.rp_config.identity_instance_epoch
            || self.account_count != self.accounts.len()
            || self.binding_count != self.bindings.len()
            || self.source_snapshot_digest != digest
        {
            return Err(SnapshotError);
        }
        let watermark: Vec<_> = self.source_watermark.split(':').collect();
        if watermark.len() != 3
            || watermark[1].parse::<u64>().is_err()
            || watermark[2].len() != 64
            || !watermark[2]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self
                .accounts
                .windows(2)
                .any(|w| w[0].account_id >= w[1].account_id)
            || self.bindings.windows(2).any(|w| {
                (&w[0].account_id, &w[0].world_identity_scope)
                    >= (&w[1].account_id, &w[1].world_identity_scope)
            })
        {
            return Err(SnapshotError);
        }
        let mut ids = std::collections::BTreeSet::new();
        for a in &self.accounts {
            if a.account_id.is_empty() || a.row_version == 0 || !ids.insert(&a.account_id) {
                return Err(SnapshotError);
            }
        }
        let mut bindings = std::collections::BTreeSet::new();
        for b in &self.bindings {
            if !ids.contains(&b.account_id)
                || b.player_id.is_empty()
                || b.world_identity_scope.is_empty()
                || b.binding_version == 0
                || b.row_version == 0
                || !bindings.insert((&b.account_id, &b.world_identity_scope))
            {
                return Err(SnapshotError);
            }
        }
        Ok(())
    }
}
