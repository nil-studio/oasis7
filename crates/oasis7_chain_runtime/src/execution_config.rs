//! Explicit execution policy and retention values assembled by the chain host.
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::ReleaseSecurityPolicy;

pub const EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER: &str = "retention-degraded";
pub const EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER: &str = "retention-in-progress";

pub fn release_security_policy_for_storage_profile(
    profile: StorageProfile,
) -> ReleaseSecurityPolicy {
    if matches!(profile, StorageProfile::ReleaseDefault) {
        ReleaseSecurityPolicy::production_hardened()
    } else {
        ReleaseSecurityPolicy::default()
    }
}

/// Values needed by an execution driver; interpreting the host profile is separate.
pub struct ExecutionDriverConfig {
    pub release_security_policy: ReleaseSecurityPolicy,
    pub hot_window_heights: u64,
    pub checkpoint_interval_heights: u64,
    pub checkpoint_keep_latest: usize,
}

impl From<&StorageProfileConfig> for ExecutionDriverConfig {
    fn from(profile: &StorageProfileConfig) -> Self {
        Self {
            release_security_policy: release_security_policy_for_storage_profile(profile.profile),
            hot_window_heights: profile.execution_hot_head_heights,
            checkpoint_interval_heights: profile.execution_checkpoint_interval,
            checkpoint_keep_latest: profile.execution_checkpoint_keep as usize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_mapping_keeps_release_hardened_and_retention_values_explicit() {
        for profile in [
            StorageProfile::DevLocal,
            StorageProfile::ReleaseDefault,
            StorageProfile::SoakForensics,
        ] {
            let source = StorageProfileConfig::for_profile(profile);
            let config = ExecutionDriverConfig::from(&source);
            assert_eq!(
                config.release_security_policy.is_production_hardened(),
                profile == StorageProfile::ReleaseDefault
            );
            assert_eq!(config.hot_window_heights, source.execution_hot_head_heights);
            assert_eq!(
                config.checkpoint_interval_heights,
                source.execution_checkpoint_interval
            );
            assert_eq!(
                config.checkpoint_keep_latest,
                source.execution_checkpoint_keep as usize
            );
        }
    }

    #[test]
    fn custom_retention_values_are_preserved_at_host_boundary() {
        let source = StorageProfileConfig {
            execution_hot_head_heights: 7,
            execution_checkpoint_interval: 3,
            execution_checkpoint_keep: 2,
            ..StorageProfileConfig::default()
        };
        let config = ExecutionDriverConfig::from(&source);
        assert_eq!(
            (
                config.hot_window_heights,
                config.checkpoint_interval_heights,
                config.checkpoint_keep_latest
            ),
            (7, 3, 2)
        );
    }
}
