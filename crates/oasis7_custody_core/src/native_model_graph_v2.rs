//! Signed physical graph description. This wire description is not proof that
//! a paid Current/price/provider implementation is qualified or dispatchable.
use crate::{native_model_proxy_v2::NativeModelCallKindV2, provisioning::ModelInferenceBudgetV1};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeModelGraphPortClassV2 {
    HostIdentityIngress,
    ProviderAbuseActor,
    RealmAbuseActor,
    IdentityOwnerService,
    OwnerAccountActor,
    AccountDirectory,
    AccountOwnerRecoveryFence,
    AccountIdentityRecoveryFence,
    IdentityNativeSource,
    IdentityNativeLifetime,
    IdentityPrivateOwnerActor,
    IdentityPrivateIdentityActor,
    AccountExecutorInitializeService,
    ExecutorInitializeActor,
    AccountExecutorSignService,
    ExecutorSignActor,
    IdentityNativeExecutorPrepare,
    IdentityNativeExecutorCapture,
    NativeWorldView,
    NativeWorldPrepare,
    NativeWorldCapture,
    EncodedAdmitHostIdentityIngress,
    EncodedAdmitProviderAbuseActor,
    EncodedAdmitRealmAbuseActor,
    EncodedAdmitIdentityOwnerService,
    EncodedAdmitOwnerAccountActor,
    NativeProviderRequest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeModelPhysicalGraphV2 {
    pub api_version: u8,
    pub first_probe: Vec<NativeModelGraphPortClassV2>,
    pub second_probe: Vec<NativeModelGraphPortClassV2>,
    pub inference: Vec<NativeModelGraphPortClassV2>,
}
impl NativeModelPhysicalGraphV2 {
    pub fn ports(&self, kind: NativeModelCallKindV2) -> &[NativeModelGraphPortClassV2] {
        match kind {
            NativeModelCallKindV2::ProbeFirst => &self.first_probe,
            NativeModelCallKindV2::ProbeSecond => &self.second_probe,
            NativeModelCallKindV2::Inference => &self.inference,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        use NativeModelGraphPortClassV2::*;
        if self.api_version != 2 {
            return Err("invalid Native model graph".into());
        }
        for ports in [&self.first_probe, &self.second_probe, &self.inference] {
            if ports.len() < 6
                || ports.len() > 400
                || ports[..5]
                    != [
                        HostIdentityIngress,
                        ProviderAbuseActor,
                        RealmAbuseActor,
                        IdentityOwnerService,
                        OwnerAccountActor,
                    ]
                || ports.last() != Some(&NativeProviderRequest)
                || ports[5..].iter().any(|p| {
                    matches!(
                        p,
                        HostIdentityIngress
                            | ProviderAbuseActor
                            | RealmAbuseActor
                            | IdentityOwnerService
                            | OwnerAccountActor
                    )
                })
                || ports
                    .iter()
                    .filter(|p| **p == NativeProviderRequest)
                    .count()
                    != 1
            {
                return Err("invalid Native model graph".into());
            }
        }
        Ok(())
    }
    pub fn validate_awaiting_captured(&self) -> Result<(), String> {
        use NativeModelGraphPortClassV2::*;
        let expected = [
            EncodedAdmitHostIdentityIngress,
            EncodedAdmitProviderAbuseActor,
            EncodedAdmitRealmAbuseActor,
            EncodedAdmitIdentityOwnerService,
            EncodedAdmitOwnerAccountActor,
        ];
        if self
            .inference
            .windows(expected.len())
            .filter(|v| *v == expected)
            .count()
            != 1
            || self
                .inference
                .iter()
                .filter(|v| {
                    matches!(
                        v,
                        EncodedAdmitHostIdentityIngress
                            | EncodedAdmitProviderAbuseActor
                            | EncodedAdmitRealmAbuseActor
                            | EncodedAdmitIdentityOwnerService
                            | EncodedAdmitOwnerAccountActor
                    )
                })
                .count()
                != expected.len()
        {
            return Err("missing exact paid encoded admission routes".into());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        crate::native_cleanup_v2::bytes("oasis7.custody.native-model-physical-graph.v2", self)
            .map(|b| crate::approval::bytes_digest(&b))
    }
    pub fn validate_budget(&self, budget: &ModelInferenceBudgetV1) -> Result<(), String> {
        if self.digest()? != budget.physical_graph_digest
            || u64::try_from(self.first_probe.len()).map_err(|_| "invalid graph")?
                != budget.probe_physical_http[0].0
            || u64::try_from(self.second_probe.len()).map_err(|_| "invalid graph")?
                != budget.probe_physical_http[1].0
            || u64::try_from(self.inference.len()).map_err(|_| "invalid graph")?
                != budget.inference_physical_http.0
        {
            return Err("Native model graph differs from approved graph".into());
        }
        Ok(())
    }
}
