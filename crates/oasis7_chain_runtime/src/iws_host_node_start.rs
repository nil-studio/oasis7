//! Fixed production clean-Cold bootstrap. DATA cannot create an accepted
//! owner: both endpoints reauthenticate with the real provider, original native
//! environment and complete Runtime sources before the Node is constructed.
use super::cold_recovery::{
    ColdEndpointReopenRefsV1, HostColdRecoveryContextV1, HostColdReopenInputV1,
    reopen_host_bounded_v1,
};
use super::node_checkpoint::IwsNodeCheckpointDataV1;
use super::*;
use oasis7_formal_protocol::controlled_authority::replicated_protocol::EndpointRole;
use oasis7_node::NodeConfig;

pub(crate) struct IwsCleanNodeStartV1 {
    pub node: NodeConfig,
    pub settings: IwsLiveNodeSettingsV1,
    pub checkpoint: IwsNodeCheckpointDataV1,
    pub memory: MemoryBudget,
    pub archive: FormalArchiveReadWorkV1,
    pub initial: [HostColdRecoveryWorkV1; 2],
    pub normal: NormalCommitWorkV1,
    pub cognition_io: HostColdIoWorkV1,
    pub cognition_work: HostCognitionStartupWorkV1,
}
impl IwsCleanNodeStartV1 {
    /// One startup request only. Failure terminates this constructor without
    /// starting a worker; no maker is invoked on a failed reopen or service
    /// admission. A new process must repeat independent authentication.
    pub(crate) fn open(mut self) -> Result<IwsLiveNodeServiceV1, String> {
        let cancelled = AtomicBool::new(false);
        let mut hosts = [None, None];
        for (index, role) in [EndpointRole::Primary, EndpointRole::Replica]
            .into_iter()
            .enumerate()
        {
            let s = &self.settings;
            let provider = if index == 0 {
                &s.primary_provider
            } else {
                &s.replica_provider
            };
            let policy = if index == 0 {
                &s.primary_policy
            } else {
                &s.replica_policy
            };
            let host = reopen_host_bounded_v1(
                HostColdReopenInputV1 {
                    endpoint: ColdEndpointReopenRefsV1 {
                        endpoint_path: &s.cold.endpoint_paths[index],
                        pages_path: &s.cold.pages_paths[index],
                        authority: &s.cold.authority,
                        role,
                        provider,
                        policy,
                        budgets: &s.cold.budgets,
                        current_limits: s.cold.current_limits,
                        checkpoint: s.cold.checkpoints[index],
                    },
                    authorization_hint_path: &self.checkpoint.authorization_hint_paths[index],
                    projection_limits: s.cold.projection_limits,
                    process: super::cold::ProcessInput::Governed(&s.cold.process),
                    configuration: &s.cold.configuration,
                    worker: &s.cold.worker,
                    transaction: s.policy.transaction,
                    facts: self.checkpoint.facts[index],
                },
                &mut self.initial[index],
                &mut |original| {
                    s.cold
                        .mailbox_hints
                        .get(original)
                        .copied()
                        .ok_or_else(|| "clean Cold mailbox locator missing".into())
                },
                HostColdRecoveryContextV1 {
                    memory: &self.memory,
                    archive: &self.archive,
                    cancelled: &cancelled,
                    compiler: &s.compiler,
                },
            )
            .map_err(|e| e.to_string())?;
            if host.current_record_id() != Some(self.checkpoint.record)
                || host.current_fence() != Some(self.checkpoint.fence)
            {
                return Err(
                    "actual authenticated endpoint differs from selected clean checkpoint".into(),
                );
            }
            hosts[index] = Some(host);
        }
        let [primary, replica] = hosts;
        let primary = primary.ok_or("authenticated primary missing")?;
        let replica = replica.ok_or("authenticated replica missing")?;
        let startup = primary
            .prepare_cognition_startup_bounded(
                &mut self.cognition_io,
                &mut self.cognition_work,
                &mut self.normal.runtime,
                self.settings.policy.transaction,
                &self.memory,
                &self.archive,
                &cancelled,
            )
            .map_err(|e| e.to_string())?;
        IwsLiveNodeServiceV1::new_bounded(
            self.node,
            IwsLiveNodeOwnersV1 {
                primary,
                replica,
                memory: self.memory,
                archive: self.archive,
                work: self.normal,
            },
            self.settings,
            IwsLiveNodeCognitionV1 {
                startup,
                io: self.cognition_io,
                work: self.cognition_work,
            },
        )
        .map_err(|failure| failure.cause)
    }
}
