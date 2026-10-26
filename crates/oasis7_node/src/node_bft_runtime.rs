use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v1::{BftTimeoutsV1, BftValidatorSetV1, BftValidatorV1};
use oasis7_proto::distributed_bft::BftScopeV1;

use crate::{NodeError, NodeRuntime};

#[path = "node_bft_protected.rs"]
mod protected;
pub(super) use protected::SelectedNodeBftConfigV1;

/// Explicit local configuration for the fixed-set engineering BFT runtime.
/// This is operator configuration, not a governance or validator-set proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeBftConfigV1 {
    pub scope: BftScopeV1,
    pub authority_epoch: u64,
    pub validators: Vec<BftValidatorV1>,
    pub local_validator_id: String,
    pub timeouts: BftTimeoutsV1,
}

/// Engineering process status, separate from a formal authority/profile proof.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct NodeBftStatusV1 {
    pub running: bool,
    pub height: u64,
    pub round: u64,
    pub step: String,
    pub committed_height: u64,
    pub validator_set_hash: String,
    pub locked_block_id: Option<String>,
    pub locked_round: Option<u64>,
    pub valid_block_id: Option<String>,
    pub valid_round: Option<u64>,
    pub durable_event_count: usize,
    pub rejected_inbound_messages: u64,
    pub last_inbound_rejection: Option<String>,
    pub rejected_local_actions: u64,
    pub last_local_action_rejection: Option<String>,
    pub last_transport_error: Option<String>,
    pub blocked_reason: Option<String>,
}

impl NodeRuntime {
    /// Explicitly select the fixed-validator engineering BFT path. This does
    /// not authorize a formal controlled profile or change the default PoS
    /// prototype mode.
    /// Configure before starting. Rejecting an active runtime consumes it,
    /// so its existing worker is stopped by Drop rather than hot-switched.
    pub fn with_tendermint_v1(mut self, config: crate::NodeBftConfigV1) -> Result<Self, NodeError> {
        if self.running.load(std::sync::atomic::Ordering::SeqCst) || self.worker.is_some() {
            return Err(NodeError::AlreadyRunning {
                node_id: self.config.node_id.clone(),
            });
        }
        config
            .validate(self.config.world_id.as_str())
            .map_err(|reason| NodeError::InvalidConfig { reason })?;
        validate_voter_role(&self.config)?;
        if self.bft_config.is_some() {
            return Err(NodeError::InvalidConfig {
                reason: "Tendermint v1 configuration was already selected".into(),
            });
        }
        if self.bft_v2_config.is_some() {
            return Err(NodeError::InvalidConfig {
                reason: "Tendermint v2 configuration was already selected".into(),
            });
        }
        self.validate_bft_pending_admission()?;
        self.bft_config = Some(SelectedNodeBftConfigV1 {
            consensus: config,
            protected: None,
        });
        Ok(self)
    }

    pub fn bft_status(&self) -> Option<NodeBftStatusV1> {
        self.bft_config.as_ref()?;
        let state = crate::runtime_util::lock_state(&self.state);
        let mut status = state.bft_status.clone().unwrap_or_default();
        status.running = self.running.load(std::sync::atomic::Ordering::SeqCst);
        status.blocked_reason = state
            .last_error
            .clone()
            .or_else(|| (!status.running).then(|| "BFT runtime is not serving".into()));
        Some(status)
    }

    /// Original QC evidence for a verifier/consumer. A page alone grants no
    /// authority: consumers must replay the exact fixed-set genesis prefix.
    pub fn raw_bft_commit_history_page(
        &self,
        after_height: u64,
        max_entries: usize,
    ) -> Result<Vec<oasis7_proto::distributed_bft::BftCommitCertificateV1>, NodeError> {
        let bft = self
            .bft_config
            .as_ref()
            .ok_or_else(|| startup::error("BFT mode was not selected"))?;
        if max_entries == 0
            || max_entries > oasis7_proto::distributed_bft::BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1
        {
            return Err(startup::error("BFT evidence page exceeds the entry bound"));
        }
        let set = bft
            .validate(&self.config.world_id)
            .map_err(startup::error)?;
        let config = crate::NodeBftExecutionConfigV1 {
            scope: bft.scope.clone(),
            authority_epoch: bft.authority_epoch,
            validators: bft.validators.clone(),
            validator_set_hash: set.set_hash().into(),
        };
        let hook = self
            .execution_hook
            .as_ref()
            .ok_or_else(|| startup::error("BFT evidence hook is absent"))?;
        let page = startup::lock_hook(hook)?
            .load_bft_commit_history_page(&config, after_height, max_entries)
            .map_err(startup::error)?;
        if page.len() > max_entries {
            return Err(startup::error("BFT evidence source exceeded page bound"));
        }
        let mut expected = after_height;
        for cert in &page {
            expected = expected
                .checked_add(1)
                .ok_or_else(|| startup::error("BFT evidence cursor overflow"))?;
            cert.validate_wire_bounds().map_err(startup::error)?;
            if cert.proposal.block.height != expected || cert.proposal.block.scope != bft.scope {
                return Err(startup::error(
                    "BFT evidence page is not contiguous in this scope",
                ));
            }
        }
        Ok(page)
    }
}

fn validate_voter_role(config: &crate::NodeConfig) -> Result<(), NodeError> {
    config
        .network_policy
        .validate_for_runtime_role(config.role)?;
    if config.network_policy.node_role_claim
        != oasis7_proto::distributed_dht::PeerNodeRole::ValidatorCore
    {
        return Err(NodeError::InvalidConfig {
            reason: "BFT voting requires network node_role_claim=validator_core".into(),
        });
    }
    Ok(())
}

impl NodeBftConfigV1 {
    pub fn validate(&self, world_id: &str) -> Result<BftValidatorSetV1, String> {
        if self.scope.world_id != world_id {
            return Err("BFT scope world does not match NodeRuntime world".into());
        }
        self.scope.validate()?;
        self.timeouts.validate()?;
        let set = BftValidatorSetV1::new(self.scope.clone(), self.validators.clone())?;
        if set.validator(&self.local_validator_id).is_none() {
            return Err("local BFT validator is not in the configured fixed set".into());
        }
        Ok(set)
    }
}

#[path = "node_bft_startup.rs"]
mod startup;
#[path = "node_bft_worker.rs"]
mod worker;

pub(super) fn start(runtime: &mut NodeRuntime) -> Result<(), NodeError> {
    startup::start(runtime)
}

fn local_signing_key(
    replication: &crate::NodeReplicationConfig,
    config: &NodeBftConfigV1,
) -> Result<SigningKey, NodeError> {
    let (key, public_key) =
        replication
            .consensus_signer()?
            .ok_or_else(|| NodeError::InvalidConfig {
                reason: "engineering Tendermint mode requires a durable local signing key".into(),
            })?;
    let set = config
        .validate(config.scope.world_id.as_str())
        .map_err(|reason| NodeError::InvalidConfig { reason })?;
    let validator = set
        .validator(config.local_validator_id.as_str())
        .ok_or_else(|| NodeError::InvalidConfig {
            reason: "local BFT validator is not configured".into(),
        })?;
    if validator.public_key_hex != public_key {
        return Err(NodeError::InvalidConfig {
            reason: "replication signing key does not match local BFT validator".into(),
        });
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;

    fn config() -> NodeBftConfigV1 {
        let key = SigningKey::from_bytes(&[7; 32]);
        NodeBftConfigV1 {
            scope: BftScopeV1 {
                world_id: "world-a".into(),
                chain_id: "chain-a".into(),
                genesis_hash: "11".repeat(32),
            },
            authority_epoch: 1,
            validators: vec![BftValidatorV1 {
                validator_id: "validator-a".into(),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                voting_power: 1,
            }],
            local_validator_id: "validator-a".into(),
            timeouts: BftTimeoutsV1 {
                propose_ms: 100,
                prevote_ms: 100,
                precommit_ms: 100,
                round_delta_ms: 25,
            },
        }
    }

    struct ClosedActionHook;

    impl crate::NodeExecutionHook for ClosedActionHook {
        fn on_commit(
            &mut self,
            _: crate::NodeExecutionCommitContext,
        ) -> Result<crate::NodeExecutionCommitResult, String> {
            Err("unused legacy path".into())
        }

        fn validate_bft_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
            let value: serde_json::Value =
                serde_cbor::from_slice(payload).map_err(|error| error.to_string())?;
            if value == serde_json::json!({"kind": "closed-action"}) {
                Ok(())
            } else {
                Err("unsupported BFT action body".into())
            }
        }
    }

    fn admission_runtime() -> NodeRuntime {
        NodeRuntime::new(
            crate::NodeConfig::new("validator-a", "world-a", crate::NodeRole::Sequencer)
                .expect("node config"),
        )
        .with_execution_hook(ClosedActionHook)
    }

    #[test]
    fn unsupported_local_actions_are_rejected_without_poisoning_bft_queue() {
        let runtime = admission_runtime()
            .with_tendermint_v1(config())
            .expect("BFT");
        for payload in [
            serde_json::json!({"version":1,"body":{"type":"simulator_action","data":{"action":{"type":"wait"}}}}),
            serde_json::json!({"version":1,"body":{"type":"world_service_intent","data":{}}}),
            serde_json::json!({"unknown_runtime_action":{}}),
        ] {
            assert!(
                runtime
                    .submit_consensus_action_payload(
                        1,
                        serde_cbor::to_vec(&payload).expect("payload")
                    )
                    .is_err()
            );
        }
        assert!(
            runtime
                .pending_consensus_actions
                .lock()
                .expect("queue")
                .is_empty()
        );
        assert_eq!(
            runtime
                .pending_consensus_action_queue_bytes
                .load(std::sync::atomic::Ordering::Acquire),
            0
        );
        runtime
            .submit_consensus_action_payload(
                2,
                serde_cbor::to_vec(&serde_json::json!({"kind":"closed-action"})).expect("payload"),
            )
            .expect("supported action remains admissible");
        assert_eq!(
            runtime
                .pending_consensus_actions
                .lock()
                .expect("queue")
                .len(),
            1
        );
    }

    #[test]
    fn bft_selection_and_start_reject_preexisting_unsupported_queue() {
        let payload = serde_cbor::to_vec(&serde_json::json!({"unknown_runtime_action":{}}))
            .expect("legacy payload");
        let runtime = admission_runtime();
        runtime
            .submit_consensus_action_payload(1, payload.clone())
            .expect("legacy admission");
        assert!(runtime.with_tendermint_v1(config()).is_err());

        let mut runtime = admission_runtime()
            .with_tendermint_v1(config())
            .expect("BFT");
        runtime
            .pending_consensus_actions
            .lock()
            .expect("queue")
            .push(
                crate::NodeConsensusAction::from_payload(1, "validator-a", payload)
                    .expect("action"),
            );
        let error = runtime.start().expect_err("preexisting unsupported action");
        assert!(error.to_string().contains("unsupported BFT action body"));
        assert!(runtime.worker.is_none());
        assert_eq!(
            runtime
                .pending_consensus_actions
                .lock()
                .expect("queue")
                .len(),
            1
        );
    }

    #[test]
    fn explicit_config_is_fixed_to_runtime_world_and_local_member() {
        let config = config();
        assert!(config.validate("world-a").is_ok());
        assert!(config.validate("world-b").is_err());

        let mut missing_local = config;
        missing_local.local_validator_id = "unknown".into();
        assert!(missing_local.validate("world-a").is_err());
    }

    #[test]
    fn bft_voter_role_rejects_observer_and_storage_only_claims() {
        for role in [crate::NodeRole::Observer, crate::NodeRole::Storage] {
            let runtime = NodeRuntime::new(
                crate::NodeConfig::new("validator-a", "world-a", role).expect("role config"),
            );
            let error = match runtime.with_tendermint_v1(config()) {
                Ok(_) => panic!("non-validator network claim must not become a BFT voter"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("validator_core"));
        }
    }

    #[test]
    fn bft_voter_role_allows_storage_with_explicit_validator_claim() {
        use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole};
        let node_config =
            crate::NodeConfig::new("validator-a", "world-a", crate::NodeRole::Storage)
                .expect("storage config")
                .with_network_policy(crate::NodeNetworkPolicy {
                    deployment_mode: PeerDeploymentMode::Private,
                    node_role_claim: PeerNodeRole::ValidatorCore,
                })
                .expect("explicit storage validator policy");
        assert!(
            NodeRuntime::new(node_config)
                .with_tendermint_v1(config())
                .is_ok()
        );
    }

    #[test]
    fn bft_voter_role_is_rechecked_before_starting_listener_or_worker() {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("reserve address");
        let address = socket.local_addr().expect("reserved address");
        drop(socket);
        let node_config =
            crate::NodeConfig::new("validator-a", "world-a", crate::NodeRole::Sequencer)
                .expect("voter config")
                .with_gossip_optional(address, Vec::new());
        let mut runtime = NodeRuntime::new(node_config)
            .with_tendermint_v1(config())
            .expect("select voter");
        runtime.config.role = crate::NodeRole::Observer;
        runtime.config.network_policy =
            crate::NodeNetworkPolicy::for_runtime_role(crate::NodeRole::Observer);
        let error = runtime.start().expect_err("stale role must be revalidated");
        assert!(error.to_string().contains("validator_core"));
        assert!(!runtime.snapshot().running);
        assert!(runtime.worker.is_none());
        assert!(std::net::UdpSocket::bind(address).is_ok());
    }

    #[test]
    fn bft_selector_rejects_a_started_pos_runtime_without_reporting_bft() {
        let mut runtime = admission_runtime();
        runtime.start().expect("start existing PoS runtime");
        assert!(runtime.worker.is_some());
        assert!(runtime.bft_status().is_none());
        let running = std::sync::Arc::clone(&runtime.running);
        let state = std::sync::Arc::clone(&runtime.state);
        let result = runtime.with_tendermint_v1(config());
        assert!(matches!(result, Err(NodeError::AlreadyRunning { .. })));
        // The consuming builder drops the rejected runtime, joining its old
        // worker rather than returning an implicitly switched live runtime.
        assert!(!running.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            crate::runtime_util::lock_state(&state).consensus.mode,
            crate::NodeConsensusMode::Pos
        );
    }

    #[test]
    fn selected_bft_without_dependencies_never_falls_back_to_pos_or_binds() {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("reserve address");
        let address = socket.local_addr().expect("reserved address");
        drop(socket);
        let node_config =
            crate::NodeConfig::new("validator-a", "world-a", crate::NodeRole::Sequencer)
                .expect("config")
                .with_gossip_optional(address, Vec::new());
        let mut runtime = NodeRuntime::new(node_config)
            .with_tendermint_v1(config())
            .expect("select explicit BFT");
        assert!(
            runtime
                .start()
                .expect_err("missing explicit BFT dependencies")
                .to_string()
                .contains("BFT")
        );
        assert!(!runtime.snapshot().running);
        assert!(std::net::UdpSocket::bind(address).is_ok());
    }
}
