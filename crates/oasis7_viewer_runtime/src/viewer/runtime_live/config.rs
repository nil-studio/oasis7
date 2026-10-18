use std::io;
use std::path::PathBuf;
use std::time::Duration;

use super::{RuntimeWorldError, ViewerLiveDecisionMode, WorldScenario};
use crate::runtime::{MajorWorldEventVisibilityPermission, ProviderBackedBootstrapAuthorityV1};

pub(crate) const DEFAULT_PROMPT_RESULT_CACHE_CAPACITY: usize = 256;
pub(crate) const DEFAULT_PROMPT_RESULT_RECEIPT_MAX_BYTES: usize = 65_536;

pub use oasis7_client_api::viewer_protocol::ChainLinkPolicy;

#[derive(Debug, Clone)]
pub struct ViewerRuntimeLiveServerConfig {
    /// Finite protocol output limits; oversized responses close that connection.
    pub response_frame_max_bytes: usize,
    pub response_turn_max_bytes: usize,
    pub response_turn_max_frames: usize,
    pub response_write_timeout: Duration,
    pub bind_addr: String,
    pub scenario: Option<WorldScenario>,
    pub world_id: String,
    pub decision_mode: ViewerLiveDecisionMode,
    pub play_step_interval: Duration,
    pub chain_poll_interval: Duration,
    pub auto_play_on_connect: bool,
    pub hosted_public_join_mode: bool,
    pub chain_status_bind: Option<String>,
    pub chain_execution_world_dir: Option<PathBuf>,
    pub chain_submit_bind: Option<String>,
    /// Logical world endpoint and pinned service trust. Node paths are never
    /// part of this application connection.
    pub world_service: Option<crate::world_service::client::WorldServiceClientConfig>,
    pub world_service_agent_signer:
        Option<crate::world_service::client::WorldServiceAgentSignerConfig>,
    pub chain_link_policy: ChainLinkPolicy,
    pub agent_chat_echo_enabled: bool,
    /// Explicit operator/session audience decision for Major World Events.
    /// Defaults to Unknown; authentication, control, and Director access do
    /// not implicitly grant event visibility.
    pub major_world_event_visibility: MajorWorldEventVisibilityPermission,
    pub generated_world_dir: Option<PathBuf>,
    /// Optional durable provider lineage path. Generated worlds continue to
    /// default beside their sidecar; formal/synthetic worlds must opt into a
    /// stable operator-owned path explicitly.
    pub provider_lineage_store: Option<PathBuf>,
    /// Runtime-only idempotency ledger capacity. A zero value is invalid.
    pub prompt_result_cache_capacity: usize,
    /// Maximum serialized result receipt size. A zero value is invalid.
    pub prompt_result_receipt_max_bytes: usize,
    /// Explicit, pre-verified ProviderBacked authority bundles. Runtime
    /// derives and validates the payer/economy binding from these records.
    pub provider_backed_bootstrap_authorities: Vec<ProviderBackedBootstrapAuthorityV1>,
    #[cfg(test)]
    pub(crate) test_cognition_runtime_binding: Option<(String, u64, Option<String>, String, u64)>,
}

impl ViewerRuntimeLiveServerConfig {
    pub(super) fn validate_response_limits(&self) -> Result<(), String> {
        if self.response_frame_max_bytes == 0
            || self.response_turn_max_bytes < self.response_frame_max_bytes
            || self.response_turn_max_frames == 0
            || self.response_turn_max_bytes > isize::MAX as usize
            || self.response_write_timeout.is_zero()
            || self.response_write_timeout > Duration::from_secs(30)
        {
            return Err("invalid Viewer response limits: nonzero frame/turn/count, turn >= frame, and write timeout <= 30 seconds required".into());
        }
        Ok(())
    }
    /// Fresh service Agent work needs application-owned durable identity.
    /// Read-only construction and reconciliation of issued work do not call this guard.
    pub(super) fn ensure_service_agent_lineage_store(&self) -> Result<(), String> {
        if self.world_service.is_none()
            || !matches!(self.decision_mode, ViewerLiveDecisionMode::Llm)
        {
            return Ok(());
        }
        if self.provider_lineage_store.as_ref().is_some_and(|path| {
            !path.as_os_str().is_empty()
                && path.to_str().is_none_or(|value| !value.trim().is_empty())
        }) {
            return Ok(());
        }
        Err("fresh service Agent admission requires an explicit App-private provider lineage store (--provider-lineage-store); no generated/node directory fallback is allowed".into())
    }

    pub(crate) fn validate_prompt_result_limits(&self) -> Result<(), String> {
        if self.prompt_result_cache_capacity == 0 {
            return Err("prompt result cache capacity must be greater than zero".to_string());
        }
        if self.prompt_result_receipt_max_bytes == 0 {
            return Err("prompt result receipt max bytes must be greater than zero".to_string());
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum ViewerRuntimeLiveServerError {
    Io(io::Error),
    Serde(String),
    Init(String),
    Runtime(RuntimeWorldError),
}

impl From<io::Error> for ViewerRuntimeLiveServerError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<RuntimeWorldError> for ViewerRuntimeLiveServerError {
    fn from(err: RuntimeWorldError) -> Self {
        Self::Runtime(err)
    }
}

#[cfg(test)]
mod response_limit_tests {
    use super::*;

    #[test]
    fn output_limits_reject_unbounded_or_inconsistent_configuration() {
        let valid = ViewerRuntimeLiveServerConfig::formal_release_default();
        assert!(valid.validate_response_limits().is_ok());
        let mut invalid = valid.clone();
        invalid.response_frame_max_bytes = 0;
        assert!(invalid.validate_response_limits().is_err());
        invalid = valid.clone();
        invalid.response_turn_max_bytes = invalid.response_frame_max_bytes - 1;
        assert!(invalid.validate_response_limits().is_err());
        invalid = valid.clone();
        invalid.response_turn_max_frames = 0;
        assert!(invalid.validate_response_limits().is_err());
        invalid = valid.clone();
        invalid.response_write_timeout = Duration::ZERO;
        assert!(invalid.validate_response_limits().is_err());
        invalid = valid;
        invalid.response_write_timeout = Duration::from_secs(31);
        assert!(invalid.validate_response_limits().is_err());
    }
}
