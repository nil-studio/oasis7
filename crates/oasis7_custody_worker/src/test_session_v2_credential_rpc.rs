//! Private fixed V2 credential protocol, not public JSON authority.
use oasis7_custody_core::{
    DecimalU64, provisioning::TestSessionCredentialLiveDescriptorV2, time_units::UnixMillis,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionCredentialStageRpcV2 {
    pub api_version: u8,
    pub approved_input_digest: String,
    pub admission: crate::test_session_client_rpc::TestSessionClientAdmissionRpc,
    pub action: TestSessionCredentialActionV2,
}

pub(crate) const CREDENTIAL_ROUTING_DOMAIN_V2: &str = "oasis7.custody.client-credential-routing.v2";
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CredentialRoutingOperationV2 {
    Challenge,
    Claim,
}
impl CredentialRoutingOperationV2 {
    pub(crate) fn from_operation(
        op: oasis7_custody_core::provisioning::SessionClientOperation,
    ) -> Result<Self, ()> {
        use oasis7_custody_core::provisioning::SessionClientOperation;
        match op {
            SessionClientOperation::Challenge => Ok(Self::Challenge),
            SessionClientOperation::CredentialClaim => Ok(Self::Claim),
            _ => Err(()),
        }
    }
}
/// Private routing metadata only. No Current, verifier result or dispatch grant.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CredentialAdmissionRoutingV2 {
    pub domain: String,
    pub api_version: u8,
    pub session_id: String,
    pub ingress_attempt_id: String,
    pub operation: CredentialRoutingOperationV2,
    pub raw_body_digest: String,
    pub approved_input_digest: String,
    #[serde(deserialize_with = "required_option")]
    pub historical_reply: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CredentialBoundAdmissionV2 {
    pub approved_input_digest: String,
    pub original: crate::test_session_client_rpc::TestSessionClientAdmissionRpc,
}
impl core::ops::Deref for CredentialBoundAdmissionV2 {
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
pub(crate) enum TestSessionCredentialActionV2 {
    Admit,
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
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TestSessionCredentialObservationV2 {
    PreDecrypt,
    PostDecrypt,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionCredentialObservedV2 {
    pub descriptor: TestSessionCredentialLiveDescriptorV2,
    pub source_ticket: u8,
    pub authority_revision: String,
    pub service_epoch: DecimalU64,
    pub target_security_epoch: DecimalU64,
    pub observed_at: UnixMillis,
    pub expires_at: UnixMillis,
}
