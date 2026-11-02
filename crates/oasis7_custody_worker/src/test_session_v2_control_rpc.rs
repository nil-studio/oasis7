//! Private fixed V2 Control protocol, not public JSON authority.
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionControlStageRpcV2 {
    pub api_version: u8,
    pub approved_input_digest: String,
    pub admission: crate::test_session_client_rpc::TestSessionClientAdmissionRpc,
    pub action: TestSessionControlActionV2,
}

pub(crate) const CONTROL_ROUTING_DOMAIN_V2: &str = "oasis7.custody.client-control-routing.v2";
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ControlRoutingOperationV2 {
    Challenge,
    Control(oasis7_custody_core::provisioning::SessionControlKindV1),
}
impl ControlRoutingOperationV2 {
    pub(crate) fn from_operation(
        op: oasis7_custody_core::provisioning::SessionClientOperation,
    ) -> Result<Self, ()> {
        use oasis7_custody_core::provisioning::SessionClientOperation;
        match op {
            SessionClientOperation::Challenge => Ok(Self::Challenge),
            SessionClientOperation::Control(k) => Ok(Self::Control(k)),
            _ => Err(()),
        }
    }
}
/// Private routing metadata only. No Current, verifier result or dispatch grant.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControlAdmissionRoutingV2 {
    pub domain: String,
    pub api_version: u8,
    pub session_id: String,
    pub ingress_attempt_id: String,
    pub operation: ControlRoutingOperationV2,
    pub raw_body_digest: String,
    pub approved_input_digest: String,
    #[serde(deserialize_with = "required_option")]
    pub immediate_reply: Option<oasis7_custody_core::provisioning::SessionControlAckV1>,
}
#[cfg(feature = "identity")]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControlBoundAdmissionV2 {
    pub approved_input_digest: String,
    pub original: crate::test_session_client_rpc::TestSessionClientAdmissionRpc,
}
#[cfg(feature = "identity")]
impl core::ops::Deref for ControlBoundAdmissionV2 {
    type Target = crate::test_session_client_rpc::TestSessionClientAdmissionRpc;
    fn deref(&self) -> &Self::Target {
        &self.original
    }
}
fn required_option<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(d)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum TestSessionControlActionV2 {
    ClaimSource,
    RecordSource {
        signed: Box<oasis7_custody_core::approval::SignedApprovalVerificationV1>,
    },
    Observe {
        index: TestSessionCredentialObservationV2,
    },
    ClaimLifetime,
    Complete {
        lifetime: Box<oasis7_custody_core::native_binding::SignedManagedNativeSessionLifetimeV1>,
    },
}
pub(crate) use crate::test_session_v2_credential_rpc::TestSessionCredentialObservationV2;
#[cfg(feature = "owner")]
pub(crate) use crate::test_session_v2_credential_rpc::TestSessionCredentialObservedV2;
