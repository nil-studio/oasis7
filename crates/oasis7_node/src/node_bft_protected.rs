//! Explicit selection of an independently provisioned BFT signer.
use std::ops::Deref;
use std::sync::Arc;

use oasis7_distfs::protected_bft::{BftSignerBindingV1, RemoteBftSignerProviderV1};

use super::NodeBftConfigV1;
use crate::{NodeError, NodeRuntime};

#[derive(Clone)]
pub(crate) struct NodeBftProtectedSignerConfig {
    pub binding: BftSignerBindingV1,
    pub provider: Arc<RemoteBftSignerProviderV1>,
}

impl std::fmt::Debug for NodeBftProtectedSignerConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeBftProtectedSignerConfig")
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SelectedNodeBftConfigV1 {
    pub consensus: NodeBftConfigV1,
    pub protected: Option<NodeBftProtectedSignerConfig>,
}

impl Deref for SelectedNodeBftConfigV1 {
    type Target = NodeBftConfigV1;

    fn deref(&self) -> &Self::Target {
        &self.consensus
    }
}

fn invalid(reason: impl Into<String>) -> NodeError {
    NodeError::InvalidConfig {
        reason: reason.into(),
    }
}

impl NodeRuntime {
    /// Select a separately provisioned signer after selecting BFT, before start.
    /// The provider owns the validator key and complete signing journal; this
    /// configuration neither provisions custody nor establishes formal readiness.
    /// An error consumes and drops the runtime, as with other consuming builders.
    pub fn with_protected_bft_signer(
        mut self,
        binding: BftSignerBindingV1,
        provider: Arc<RemoteBftSignerProviderV1>,
    ) -> Result<Self, NodeError> {
        if self.running.load(std::sync::atomic::Ordering::SeqCst) || self.worker.is_some() {
            return Err(NodeError::AlreadyRunning {
                node_id: self.config.node_id.clone(),
            });
        }
        let selection = self
            .bft_config
            .as_mut()
            .ok_or_else(|| invalid("protected signer requires explicit BFT selection"))?;
        if selection.protected.is_some() {
            return Err(invalid("protected BFT signer was already selected"));
        }
        let configured = selection
            .consensus
            .validate(&self.config.world_id)
            .map_err(invalid)?;
        let signer_set = binding
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        if binding.scope != selection.scope
            || binding.authority_epoch != selection.authority_epoch
            || binding.local_validator_id != selection.local_validator_id
            || binding.timeouts != selection.timeouts
            || signer_set.set_hash() != configured.set_hash()
        {
            return Err(invalid(
                "protected signer binding differs from the selected BFT scope, set or policy",
            ));
        }
        selection.protected = Some(NodeBftProtectedSignerConfig { binding, provider });
        Ok(self)
    }
}
