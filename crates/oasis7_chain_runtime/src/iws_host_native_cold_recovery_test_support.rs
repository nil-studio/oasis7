//! Only startup DATA and initial finite work construction for native tests.
//! All failure recovery remains in the shipping fixed Node/Cold consumers.
use super::*;
use crate::iws_host_runtime::{
    cold_recovery::ColdEndpointReopenRefsV1, normal_commit::NormalColdRecoveryInputsV1,
};
use oasis7_distfs::controlled_authority::formal::endpoint::{
    IwsCurrentLimitsV1, IwsEndpointVerificationBudgetsV1,
};
use oasis7_formal_protocol::{
    controlled_authority::{formal::VerifiedAuthorityV1, replicated_protocol::EndpointRole},
    incremental_material::FactLocationV1,
};
use oasis7_runtime::{
    AuthorizationRootCacheLimitsV1, ReleaseSecurityPolicy, WorldRuntimeMemoryLimits,
};
use std::{collections::BTreeMap, path::PathBuf};

pub(crate) struct NativeColdRecoverySettingsV1 {
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
    pub checkpoints: [Option<
        oasis7_distfs::controlled_authority::formal::endpoint::IwsColdCheckpointSuffixV1,
    >; 2],
}
impl NativeColdRecoverySettingsV1 {
    pub(super) fn inputs<'a>(
        &'a self,
        providers: [&'a Arc<RemoteMonotonicEndpointProvider>; 2],
        policies: [&'a TrustedFencingPolicyV1; 2],
        transaction: oasis7_formal_protocol::incremental_execution::TransactionLimits,
    ) -> NormalColdRecoveryInputsV1<'a> {
        NormalColdRecoveryInputsV1 {
            endpoints: [EndpointRole::Primary, EndpointRole::Replica].map(|role| {
                let index = usize::from(role == EndpointRole::Replica);
                ColdEndpointReopenRefsV1 {
                    endpoint_path: &self.endpoint_paths[index],
                    pages_path: &self.pages_paths[index],
                    authority: &self.authority,
                    role,
                    provider: providers[index],
                    policy: policies[index],
                    budgets: &self.budgets,
                    current_limits: self.current_limits,
                    checkpoint: self.checkpoints[index],
                }
            }),
            authorization_hint_path: &self.authorization_hint_path,
            projection_limits: self.projection_limits,
            process: crate::iws_host_runtime::cold::ProcessInput::Governed(&self.process),
            configuration: &self.configuration,
            worker: &self.worker,
            transaction,
            mailbox_hints: &self.mailbox_hints,
        }
    }
}

#[cfg(feature = "iws-host-native-test-support")]
pub(crate) fn admit_initial_cold_recovery_work_v1(
    limits: &IwsCurrentLimitsV1,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
) -> Result<crate::iws_host_runtime::normal_commit::NormalColdRecoveryWorkV1, String> {
    use crate::iws_host_runtime::native_cold_test_support::ColdFixtureWorkV1;
    use crate::iws_host_runtime::normal_commit::NormalColdRecoveryWorkV1;
    let mut stop = || false;
    let mut common = archive.bounded_context(memory, &mut stop);
    let primary =
        ColdFixtureWorkV1::new_bounded(limits, &mut common).map_err(|error| error.to_string())?;
    let replica =
        ColdFixtureWorkV1::new_bounded(limits, &mut common).map_err(|error| error.to_string())?;
    NormalColdRecoveryWorkV1::new_bounded([primary, replica], &mut common)
        .map_err(|error| error.to_string())
}
