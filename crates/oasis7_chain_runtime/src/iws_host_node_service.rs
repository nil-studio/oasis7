//! Library service for one continuous local IWS Node and its accepted owners.
//! Admission consumes the existing Host sources and remaining work once. The
//! service never exposes hook replacement, raw execution, or a static boundary
//! setter. CLI configuration and cross-process pending recovery are separate
//! consumers; neither is implied by constructing this local service.
#[cfg(any(test, feature = "iws-host-native-test-support"))]
use super::node_live::IwsLiveNodePartsV1;
use super::node_live::{
    IwsLiveNodeCancellationV1, IwsLiveNodeCognitionV1, IwsLiveNodeHookV1, IwsLiveNodeOwnerV1,
    IwsLiveNodeOwnersV1, IwsLiveNodeSettingsV1,
};
use super::{FormalArchiveReadWorkV1, IwsHostRuntimeV1, MemoryBudget};
use oasis7_distfs::controlled_authority::formal::endpoint::IwsColdParentHintsV1;
use oasis7_node::{NodeConfig, NodeError, NodeExecutionHook, NodeRole, NodeRuntime, NodeSnapshot};
use std::sync::atomic::AtomicBool;

/// A rejected service admission returns the same inputs, including every
/// remaining work account. A configuration error never starts a Node worker.
pub struct IwsLiveNodeServiceStartFailureV1 {
    pub cause: String,
    pub config: NodeConfig,
    pub owners: IwsLiveNodeOwnersV1,
    pub settings: IwsLiveNodeSettingsV1,
    pub cognition: IwsLiveNodeCognitionV1,
}

/// One Node instance survives ordinary stop/start calls. Its hook retains the
/// original pending decision, Runtime result, work accounts and accepted cut.
/// Fields are private so a caller cannot substitute another execution hook.
pub struct IwsLiveNodeServiceV1 {
    // Drop joins the worker before releasing the service's Host owner. Normal
    // stop and Drop let the current bounded tick finish; neither implies abort.
    node: NodeRuntime,
    owner: IwsLiveNodeOwnerV1,
    aborted: bool,
}

fn validate_local_config_bounded(
    config: &NodeConfig,
    world_id: &str,
    owners: &IwsLiveNodeOwnersV1,
) -> Result<(), String> {
    let validator_bytes = config
        .pos_config
        .validators
        .first()
        .map_or(0, |validator| validator.validator_id.len());
    let bytes = [
        config.world_id.len(),
        world_id.len(),
        config.node_id.len(),
        config.player_id.len(),
        validator_bytes,
    ]
    .into_iter()
    .try_fold(0usize, |sum, length| sum.checked_add(length))
    .and_then(|bytes| bytes.checked_mul(2))
    .and_then(|bytes| bytes.checked_add(256))
    .and_then(|bytes| u64::try_from(bytes).ok())
    .ok_or("live Node service configuration length overflow")?;
    owners
        .archive
        .charge(bytes, bytes, &mut || false)
        .map_err(|error| error.to_string())?;
    if config.node_id.trim().is_empty()
        || config.player_id.trim().is_empty()
        || config.world_id != world_id
        || config.role != NodeRole::Sequencer
        || !config.allow_local_proposals
        || !config.require_execution_on_commit
        || config.require_peer_execution_hashes
    {
        return Err(
            "live Node service requires a local executing Sequencer in the accepted world".into(),
        );
    }
    if config.replication.is_some()
        || config.gossip.is_some()
        || config.feedback_p2p.is_some()
        || config.replica_maintenance.is_some()
    {
        return Err(
            "live Node service rejects ordinary replication, gossip and network maintenance".into(),
        );
    }
    if config.pos_config.validators.len() != 1
        || config.pos_config.validators[0].validator_id != config.node_id
    {
        return Err("live Node service requires its one local PoS validator".into());
    }
    Ok(())
}

fn aborted_error() -> NodeError {
    NodeError::Execution {
        reason: "live Node service was explicitly aborted; its original work cannot be restarted"
            .into(),
    }
}

impl IwsLiveNodeServiceV1 {
    /// Consume genuine accepted Hosts, paid Cognition evidence and their
    /// original remaining work. The exact authority, writer, pool, Common,
    /// dual boundary and Cognition binding are validated by the owned driver.
    pub fn new_bounded(
        config: NodeConfig,
        owners: IwsLiveNodeOwnersV1,
        settings: IwsLiveNodeSettingsV1,
        cognition: IwsLiveNodeCognitionV1,
    ) -> Result<Self, IwsLiveNodeServiceStartFailureV1> {
        Self::new_wrapped_bounded(config, owners, settings, cognition, |hook, _| hook)
    }

    fn new_wrapped_bounded<H: NodeExecutionHook + 'static>(
        config: NodeConfig,
        owners: IwsLiveNodeOwnersV1,
        settings: IwsLiveNodeSettingsV1,
        cognition: IwsLiveNodeCognitionV1,
        wrap: impl FnOnce(IwsLiveNodeHookV1, IwsLiveNodeCancellationV1) -> H,
    ) -> Result<Self, IwsLiveNodeServiceStartFailureV1> {
        if let Err(cause) = validate_local_config_bounded(&config, &settings.world_id, &owners) {
            return Err(IwsLiveNodeServiceStartFailureV1 {
                cause,
                config,
                owners,
                settings,
                cognition,
            });
        }
        let owner = match IwsLiveNodeOwnerV1::new_bounded(owners, settings, cognition) {
            Ok(owner) => owner,
            Err(failure) => {
                return Err(IwsLiveNodeServiceStartFailureV1 {
                    cause: failure.cause,
                    config,
                    owners: failure.owners,
                    settings: failure.settings,
                    cognition: failure.cognition,
                });
            }
        };
        let hook = wrap(owner.hook(), owner.cancellation_handle());
        let node = NodeRuntime::new(config).with_execution_hook(hook);
        Ok(Self {
            node,
            owner,
            aborted: false,
        })
    }

    /// Each start reads the current boundary through the installed guarded
    /// hook. It never reconstructs the Host or replaces a pending original.
    pub fn start(&mut self) -> Result<(), NodeError> {
        if self.aborted {
            return Err(aborted_error());
        }
        self.node.start()
    }

    /// Request a stop between bounded ticks and join the original worker.
    /// Runtime preparation may not be safely replayed after cancellation, so
    /// normal stop deliberately preserves the existing cancellation state.
    pub fn stop(&mut self) -> Result<(), NodeError> {
        self.node.stop()
    }

    /// Join the only Node worker, reject queued input or retained execution and
    /// persist clean recovery DATA for that actual completed cut. Missing
    /// checkpoint DATA after a crash never licenses a rollback to an older cut.
    pub(crate) fn stop_and_checkpoint_bounded(
        &mut self,
        root: &std::path::Path,
        limits: super::node_checkpoint::IwsNodeCheckpointLimitsV1,
    ) -> Result<[u8; 32], String> {
        if self.aborted {
            return Err("an aborted service cannot publish a clean checkpoint".into());
        }
        match self.node.stop() {
            Ok(()) | Err(NodeError::NotRunning { .. }) => {}
            Err(error) => return Err(error.to_string()),
        }
        let snapshot = self.node.snapshot();
        let pending = &snapshot.consensus.pending_consensus_actions;
        if snapshot.running || pending.queued_action_count != 0
            || pending.submit_buffer_action_count != 0 || pending.reserved_requeue_action_count != 0
        {
            return Err("clean checkpoint would leave queued Node input; resume the same service first".into());
        }
        self.owner.publish_clean_checkpoint_bounded(root, limits)
    }

    /// Explicitly cancel current work, then join without holding the owner
    /// lock. Cancellation can retain a terminal Runtime failure. This service
    /// will not clear the flag, restart, or accept another business submission.
    pub fn abort(&mut self) -> Result<(), NodeError> {
        self.aborted = true;
        self.owner.cancellation_handle().set_cancelled(true);
        match self.node.stop() {
            Err(NodeError::NotRunning { .. }) => Ok(()),
            result => result,
        }
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted
    }

    /// Queue an original business payload in the actual Node. As with Node's
    /// existing API, submissions may be queued while stopped. Success reports
    /// queue admission only; accepted results require the completed Host cut.
    pub fn submit_consensus_action_payload(
        &self,
        action_id: u64,
        payload_cbor: Vec<u8>,
    ) -> Result<(), NodeError> {
        if self.aborted {
            return Err(aborted_error());
        }
        self.node
            .submit_consensus_action_payload(action_id, payload_cbor)
    }

    /// Operational DATA, including a terminal execution diagnostic. The Node
    /// snapshot and its transient queues are not startup or accepted authority.
    pub fn snapshot(&self) -> NodeSnapshot {
        self.node.snapshot()
    }

    /// Observe an actual completed cut while the original owner lock and its
    /// Common/pool remain held. Borrowed Host sources cannot escape the call.
    /// The callback must not recursively start/stop this service or block on
    /// Node progress; the worker uses this same owner lock.
    pub fn with_current_bounded(
        &self,
        run: impl FnOnce(
            &IwsHostRuntimeV1,
            &IwsHostRuntimeV1,
            &MemoryBudget,
            &FormalArchiveReadWorkV1,
            &AtomicBool,
        ) -> Result<(), String>,
    ) -> Result<(), String> {
        self.owner.with_current_bounded(run)
    }

    /// Return only DATA locators selected from the actual accepted sources.
    /// A later cold reopen must authenticate the history again; these values
    /// do not reconstruct an accepted owner or an unresolved continuation.
    pub fn selected_fact_hints_bounded(&self) -> Result<[IwsColdParentHintsV1; 2], String> {
        let mut result = None;
        self.owner
            .with_current_bounded(|primary, replica, memory, archive, cancelled| {
                let read = |host| {
                    super::cold_recovery::selected_fact_hints_bounded_v1(
                        host, memory, archive, cancelled,
                    )
                    .map_err(|error| error.to_string())
                };
                result = Some([read(primary)?, read(replica)?]);
                Ok(())
            })?;
        result.ok_or_else(|| "live Node completed hint callback returned no locations".into())
    }

    /// Native scheduling instrumentation wraps the same production hook only
    /// after ordinary service admission. No fixture owner or budget is minted.
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn new_with_fixture_hook_bounded<H: NodeExecutionHook + 'static>(
        config: NodeConfig,
        owners: IwsLiveNodeOwnersV1,
        settings: IwsLiveNodeSettingsV1,
        cognition: IwsLiveNodeCognitionV1,
        wrap: impl FnOnce(IwsLiveNodeHookV1, IwsLiveNodeCancellationV1) -> H,
    ) -> Result<Self, IwsLiveNodeServiceStartFailureV1> {
        Self::new_wrapped_bounded(config, owners, settings, cognition, wrap)
    }

    /// Only acceptance tests inspect the final remaining accounts. Dropping
    /// Node first joins and releases its hook; production consumers retain the
    /// complete service across stop/start and release it by dropping it.
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn into_parts_fixture(self) -> Result<IwsLiveNodePartsV1, IwsLiveNodeOwnerV1> {
        let Self { node, owner, .. } = self;
        drop(node);
        owner.into_parts()
    }
}
