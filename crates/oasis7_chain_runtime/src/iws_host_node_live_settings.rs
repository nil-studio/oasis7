//! Owned process configuration for the continuous IWS Node consumer. Paths and
//! locator hints are DATA; only the real endpoint reopen can qualify a source.
use super::*;
use crate::iws_host_runtime::{
    cold_recovery::ColdEndpointReopenRefsV1, normal_commit::NormalColdRecoveryInputsV1,
};
use oasis7_distfs::controlled_authority::formal::endpoint::{
    IwsColdCheckpointSuffixV1, IwsCurrentLimitsV1, IwsEndpointVerificationBudgetsV1,
};
use oasis7_formal_protocol::{
    controlled_authority::{formal::VerifiedAuthorityV1, replicated_protocol::EndpointRole},
    incremental_material::FactLocationV1,
};
use oasis7_runtime::{
    AuthorizationRootCacheLimitsV1, NormalCommitHeaderLimitsV1, ReleaseSecurityPolicy,
    RuntimeSourceCompiler, WorldRuntimeMemoryLimits,
};
use std::{collections::BTreeMap, path::PathBuf};

pub struct IwsLiveNodeColdSettingsV1 {
    pub endpoint_paths: [PathBuf; 2],
    pub pages_paths: [PathBuf; 2],
    pub authorization_hint_path: PathBuf,
    pub authority: VerifiedAuthorityV1,
    pub process: ReleaseSecurityPolicy,
    pub configuration: WorldRuntimeMemoryLimits,
    pub worker: WorkerLimitsV1,
    pub current_limits: IwsCurrentLimitsV1,
    pub budgets: IwsEndpointVerificationBudgetsV1,
    pub projection_limits: AuthorizationRootCacheLimitsV1,
    pub mailbox_hints: BTreeMap<[u8; 32], FactLocationV1>,
    pub checkpoints: [Option<IwsColdCheckpointSuffixV1>; 2],
}

pub struct IwsLiveNodeSettingsV1 {
    pub world_id: String,
    pub writer: ed25519_dalek::SigningKey,
    pub primary_provider: Arc<RemoteMonotonicEndpointProvider>,
    pub replica_provider: Arc<RemoteMonotonicEndpointProvider>,
    pub primary_policy: TrustedFencingPolicyV1,
    pub replica_policy: TrustedFencingPolicyV1,
    pub compiler: RuntimeSourceCompiler,
    pub input_limits: CborDataLimitsV1,
    pub action_limits: CborDataLimitsV1,
    pub policy: NormalCommitHeaderLimitsV1,
    pub max_action_bytes: usize,
    pub max_grant_payload_bytes: u64,
    pub cold: IwsLiveNodeColdSettingsV1,
}

impl IwsLiveNodeSettingsV1 {
    pub(super) fn environment<'a>(
        &'a self,
        memory: &'a MemoryBudget,
        archive: &'a FormalArchiveReadWorkV1,
        cancelled: &'a AtomicBool,
    ) -> NodeNormalExecutionEnvironmentV1<'a> {
        let cold = &self.cold;
        let providers = [&self.primary_provider, &self.replica_provider];
        let policies = [&self.primary_policy, &self.replica_policy];
        NodeNormalExecutionEnvironmentV1 {
            writer: &self.writer,
            input_limits: &self.input_limits,
            action_limits: &self.action_limits,
            compiler: &self.compiler,
            policy: NormalCommitHeaderLimitsV1 {
                transaction: self.policy.transaction,
                checkpoint: self.policy.checkpoint,
            },
            max_action_bytes: self.max_action_bytes,
            max_grant_payload_bytes: self.max_grant_payload_bytes,
            primary_provider: &self.primary_provider,
            replica_provider: &self.replica_provider,
            primary_policy: &self.primary_policy,
            replica_policy: &self.replica_policy,
            memory,
            archive,
            cancelled,
            cold: NormalColdRecoveryInputsV1 {
                endpoints: [EndpointRole::Primary, EndpointRole::Replica].map(|role| {
                    let index = usize::from(role == EndpointRole::Replica);
                    ColdEndpointReopenRefsV1 {
                        endpoint_path: &cold.endpoint_paths[index],
                        pages_path: &cold.pages_paths[index],
                        authority: &cold.authority,
                        role,
                        provider: providers[index],
                        policy: policies[index],
                        budgets: &cold.budgets,
                        current_limits: cold.current_limits,
                        checkpoint: cold.checkpoints[index],
                    }
                }),
                authorization_hint_path: &cold.authorization_hint_path,
                projection_limits: cold.projection_limits,
                process: super::super::cold::ProcessInput::Governed(&cold.process),
                configuration: &cold.configuration,
                worker: &cold.worker,
                transaction: self.policy.transaction,
                mailbox_hints: &cold.mailbox_hints,
            },
        }
    }
}
