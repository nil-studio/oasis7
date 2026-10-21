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

#[derive(Clone, Default)]
pub struct DevLocalParticipantConfig {
    pub authority: Option<oasis7_runtime::DevLocalParticipantAuthority>,
    #[cfg(feature = "node-libp2p")]
    pub(crate) verified_origin: Option<VerifiedDevLocalParticipantOrigin>,
    pub startup: Option<oasis7_client_api::world_service::SignedReadRequest<
        oasis7_runtime::service_admission::participant_policy::DevLocalParticipantPolicyAuthorizationV1,
    >>,
}

/// Created only after Host verifies the original node-signed preview origin.
/// This cannot be decoded from a client or an administrator JSON request.
#[cfg(feature = "node-libp2p")]
#[derive(Clone)]
pub(crate) struct VerifiedDevLocalParticipantOrigin {
    identity: oasis7_client_api::world_service::WorldIdentity,
}
#[cfg(feature = "node-libp2p")]
impl VerifiedDevLocalParticipantOrigin {
    pub(crate) fn identity(&self) -> &oasis7_client_api::world_service::WorldIdentity {
        &self.identity
    }
}
#[cfg(feature = "node-libp2p")]
impl DevLocalParticipantConfig {
    pub(crate) fn set_verified_origin(
        &mut self,
        identity: oasis7_client_api::world_service::WorldIdentity,
    ) -> Result<(), String> {
        identity.validate().map_err(|e| e.to_string())?;
        let signed = self
            .startup
            .as_ref()
            .ok_or("origin binding requires the explicit signed startup policy")?;
        if self.authority.is_none()
            || signed.request.policy.world_id != identity.world_id
            || signed.request.policy.genesis_digest != identity.genesis_digest
        {
            return Err("verified origin differs from signed local participant policy".into());
        }
        self.verified_origin = Some(VerifiedDevLocalParticipantOrigin { identity });
        Ok(())
    }
}

/// Values needed by an execution driver; interpreting the host profile is separate.
pub struct ExecutionDriverConfig {
    pub release_security_policy: ReleaseSecurityPolicy,
    pub devlocal_participant: DevLocalParticipantConfig,
    pub hot_window_heights: u64,
    pub checkpoint_interval_heights: u64,
    pub checkpoint_keep_latest: usize,
}

impl From<&StorageProfileConfig> for ExecutionDriverConfig {
    fn from(profile: &StorageProfileConfig) -> Self {
        Self {
            release_security_policy: release_security_policy_for_storage_profile(profile.profile),
            devlocal_participant: DevLocalParticipantConfig::default(),
            hot_window_heights: profile.execution_hot_head_heights,
            checkpoint_interval_heights: profile.execution_checkpoint_interval,
            checkpoint_keep_latest: profile.execution_checkpoint_keep as usize,
        }
    }
}

/// The local pin is supplied before history validation and tail replay.
#[cfg(feature = "node-libp2p")]
pub(crate) fn restore_execution_world(
    snapshot: oasis7_runtime::Snapshot,
    journal: oasis7_runtime::Journal,
    release: ReleaseSecurityPolicy,
    authority: Option<&oasis7_runtime::DevLocalParticipantAuthority>,
) -> Result<oasis7_runtime::World, oasis7_runtime::WorldError> {
    if let Some(authority) = authority {
        if authority.release_security_policy() != &release {
            return Err(oasis7_runtime::WorldError::DistributedValidationFailed {
                reason: "local participant recovery release context mismatch".into(),
            });
        }
        oasis7::runtime_host::world::from_snapshot_with_devlocal_participant_authority(
            snapshot,
            journal,
            authority.clone(),
        )
    } else {
        oasis7::runtime_host::world::from_snapshot_with_release_security_policy(
            snapshot, journal, release,
        )
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
