//! Explicit, protected dynamic-set BFT V2 selector and public observations.

use std::sync::Arc;

use oasis7_consensus::tendermint_v2::BftHeightProposerScheduleV2;
use oasis7_distfs::protected_bft_v2::{BftSignerBindingV2, RemoteBftSignerProviderV2};
use oasis7_proto::distributed_bft::v2::{BftCommitCertificateV2, BftProposalHeaderV2};
use serde::Serialize;

use crate::runtime_util::lock_state;
use crate::{NodeError, NodeRuntime};

#[path = "node_bft_v2_startup.rs"]
mod startup;
#[path = "node_bft_v2_worker.rs"]
mod worker;

#[cfg(all(test, unix))]
#[path = "node_bft_v2_action_tests.rs"]
mod action_tests;

#[cfg(all(test, unix))]
#[path = "node_bft_v2_worker_transition_tests.rs"]
mod worker_transition_tests;

#[derive(Clone)]
pub(crate) struct SelectedNodeBftConfigV2 {
    pub binding: BftSignerBindingV2,
    pub provider: Arc<RemoteBftSignerProviderV2>,
}

impl std::fmt::Debug for SelectedNodeBftConfigV2 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SelectedNodeBftConfigV2")
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NodeBftStatusV2 {
    pub running: bool,
    pub height: u64,
    pub round: u64,
    pub step: String,
    pub committed_height: u64,
    pub active_validator_set_hash: String,
    pub pending_transition_height: Option<u64>,
    pub local_validator_active: bool,
    pub durable_event_count: usize,
    pub proposal_equivocation_evidence: Vec<(BftProposalHeaderV2, BftProposalHeaderV2)>,
    pub rejected_inbound_messages: u64,
    pub last_inbound_rejection: Option<String>,
    pub last_transport_error: Option<String>,
    pub blocked_reason: Option<String>,
}

fn invalid(reason: impl Into<String>) -> NodeError {
    NodeError::InvalidConfig {
        reason: reason.into(),
    }
}

fn consensus_error(reason: impl Into<String>) -> NodeError {
    NodeError::Consensus {
        reason: reason.into(),
    }
}

impl NodeRuntime {
    /// Select the protected dynamic-set protocol. The immutable signer binding
    /// fixes genesis/scope and the remote provider owns requester authentication.
    /// This cannot be combined with V1 or a local consensus key.
    pub fn with_tendermint_v2(
        mut self,
        binding: BftSignerBindingV2,
        provider: Arc<RemoteBftSignerProviderV2>,
    ) -> Result<Self, NodeError> {
        if self.running.load(std::sync::atomic::Ordering::SeqCst) || self.worker.is_some() {
            return Err(NodeError::AlreadyRunning {
                node_id: self.config.node_id.clone(),
            });
        }
        if self.bft_config.is_some() || self.bft_v2_config.is_some() || self.bft_v3_config.is_some()
        {
            return Err(invalid("a BFT configuration is already selected"));
        }
        binding
            .validate()
            .map_err(|error| invalid(format!("invalid protected BFT V2 binding: {error}")))?;
        if binding.scope.world_id != self.config.world_id {
            return Err(invalid(
                "protected BFT V2 world does not match NodeRuntime world",
            ));
        }
        if binding.local_validator_id != self.config.node_id {
            return Err(invalid(
                "protected BFT V2 local validator id must equal NodeRuntime node id",
            ));
        }
        self.config
            .network_policy
            .validate_for_runtime_role(self.config.role)?;
        if self.config.network_policy.node_role_claim
            != oasis7_proto::distributed_dht::PeerNodeRole::ValidatorCore
        {
            return Err(invalid(
                "BFT V2 requires network node_role_claim=validator_core",
            ));
        }
        self.validate_bft_v2_pending_admission()?;
        self.bft_v2_config = Some(SelectedNodeBftConfigV2 { binding, provider });
        Ok(self)
    }

    pub fn bft_v2_status(&self) -> Option<NodeBftStatusV2> {
        self.bft_v2_config.as_ref()?;
        let state = lock_state(&self.state);
        let mut status = state.bft_v2_status.clone().unwrap_or_default();
        status.running = self.running.load(std::sync::atomic::Ordering::SeqCst);
        status.blocked_reason = state
            .last_error
            .clone()
            .or_else(|| (!status.running).then(|| "BFT V2 runtime is not serving".into()));
        Some(status)
    }

    /// Return bounded original QC evidence. It is not a serving or execution
    /// proof until replayed from the operator-pinned V2 genesis configuration.
    pub fn raw_bft_v2_commit_history_page(
        &self,
        after_height: u64,
        max_entries: usize,
    ) -> Result<Vec<BftCommitCertificateV2>, NodeError> {
        let selected = self
            .bft_v2_config
            .as_ref()
            .ok_or_else(|| consensus_error("BFT V2 mode was not selected"))?;
        if max_entries == 0
            || max_entries > oasis7_proto::distributed_bft::BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1
        {
            return Err(consensus_error("BFT V2 evidence page exceeds its bound"));
        }
        let hook = self
            .execution_hook
            .as_ref()
            .ok_or_else(|| consensus_error("BFT V2 execution hook is absent"))?;
        let config = execution_config(&selected.binding);
        let page = hook
            .lock()
            .map_err(|_| consensus_error("BFT V2 execution hook lock poisoned"))?
            .load_bft_v2_commit_history_page(&config, after_height, max_entries)
            .map_err(consensus_error)?;
        validate_page(&selected.binding.scope, after_height, max_entries, &page)?;
        Ok(page)
    }

    pub(crate) fn validate_bft_v2_payload_admission(
        &self,
        payload: &[u8],
    ) -> Result<(), NodeError> {
        let hook = self
            .execution_hook
            .as_ref()
            .ok_or_else(|| consensus_error("BFT V2 action admission requires execution hook"))?;
        hook.lock()
            .map_err(|_| consensus_error("BFT V2 execution hook lock poisoned"))?
            .validate_bft_v2_action_payload(payload)
            .map_err(consensus_error)
    }

    fn validate_bft_v2_pending_admission(&self) -> Result<(), NodeError> {
        let pending = self
            .pending_consensus_actions
            .lock()
            .map_err(|_| consensus_error("BFT V2 action queue lock poisoned"))?;
        let mut seen = std::collections::BTreeMap::<u64, &crate::NodeConsensusAction>::new();
        for action in pending.iter() {
            if let Some(previous) = seen.get(&action.action_id) {
                if *previous != action {
                    return Err(consensus_error(format!(
                        "conflicting preexisting BFT V2 action id {}",
                        action.action_id
                    )));
                }
                continue;
            }
            seen.insert(action.action_id, action);
            self.validate_bft_v2_payload_admission(&action.payload_cbor)?;
        }
        Ok(())
    }
}

pub(super) fn execution_config(binding: &BftSignerBindingV2) -> crate::NodeBftExecutionConfigV2 {
    crate::NodeBftExecutionConfigV2 {
        scope: binding.scope.clone(),
        authority_epoch: binding.genesis_authority_epoch,
        initial_validators: binding.genesis_validators.clone(),
        schedule_policy: binding.schedule_policy.clone(),
        genesis_state_root: binding.genesis_state_root.clone(),
        genesis_manifest_hash: binding.genesis_manifest_hash.clone(),
    }
}

pub(super) fn genesis_schedule(
    binding: &BftSignerBindingV2,
) -> Result<BftHeightProposerScheduleV2, NodeError> {
    let set = binding
        .validate()
        .map_err(|error| consensus_error(format!("invalid BFT V2 binding: {error}")))?;
    BftHeightProposerScheduleV2::genesis(
        set,
        binding.schedule_policy.clone(),
        binding.genesis_authority_epoch,
        binding.genesis_state_root.clone(),
        binding.genesis_manifest_hash.clone(),
    )
    .map_err(consensus_error)
}

fn validate_page(
    scope: &oasis7_proto::distributed_bft::v2::BftScopeV2,
    after_height: u64,
    max_entries: usize,
    page: &[BftCommitCertificateV2],
) -> Result<(), NodeError> {
    if page.len() > max_entries {
        return Err(consensus_error("BFT V2 history source exceeded page bound"));
    }
    let mut expected = after_height;
    for certificate in page {
        expected = expected
            .checked_add(1)
            .ok_or_else(|| consensus_error("BFT V2 history cursor overflow"))?;
        certificate
            .validate_wire_bounds()
            .map_err(consensus_error)?;
        let block = &certificate.proposal.block;
        if block.height != expected || block.scope != *scope {
            return Err(consensus_error(
                "BFT V2 history page is not contiguous/in-scope",
            ));
        }
    }
    Ok(())
}

pub(super) fn start(runtime: &mut NodeRuntime) -> Result<(), NodeError> {
    startup::start(runtime)
}
