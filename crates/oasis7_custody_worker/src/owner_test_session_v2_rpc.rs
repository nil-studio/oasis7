//! Closed V2 management envelopes; no caller supplies Current or actor roles.
use super::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionManagementRpcV2<T> {
    pub manager_session: oasis7_custody_core::approval::CustodySessionStatusV1,
    pub provision_id: String,
    pub request: T,
}
pub(crate) type TestSessionBasisRpcV2 =
    TestSessionManagementRpcV2<oasis7_custody_core::provisioning::TestSessionBasisRequestV2>;
pub(crate) type TestSessionPrepareRpcV2 =
    TestSessionManagementRpcV2<oasis7_custody_core::provisioning::TestSessionPrepareRequestV2>;
pub(crate) type TestSessionApproveRpcV2 =
    TestSessionManagementRpcV2<oasis7_custody_core::provisioning::TestSessionApproveRequestV2>;
pub(crate) type TestSessionContinueRpcV2 =
    TestSessionManagementRpcV2<oasis7_custody_core::provisioning::TestSessionContinueRequestV2>;
/// Select only the explicit numeric wire version; unknown versions never fall
/// back through a permissive legacy DTO or infer a version from field presence.
pub(crate) fn session_management_version(raw: &str) -> std::result::Result<u64, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(raw)?;
    Ok(value
        .get("request")
        .and_then(|v| v.get("api_version"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionRuntimeRpcV2 {
    pub request: TestSessionSelectorV1,
    pub action: TestSessionRuntimeActionV2,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "owner")]
pub(crate) struct TestSessionSignerPermitV2 {
    pub permit: oasis7_custody_core::provisioning::TestSessionPreflightPermitV2,
    pub signing: oasis7_custody_core::provisioning::TestSessionPreflightSigningBundleV2,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "owner")]
pub(crate) struct TestSessionPublishedReadV2 {
    pub permit: oasis7_custody_core::provisioning::TestSessionPreflightPermitV2,
    pub original: ParticipantOwnerViewRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum TestSessionRuntimeActionV2 {
    DescribePreflight,
    ClaimApprovalVerification,
    ConsumeApproval {
        original: Box<oasis7_custody_core::provisioning::CommitTestSessionV2>,
    },
    ReserveSource {
        operation_id: String,
        funding: SessionSourceFundingV1,
    },
    ClaimSource {
        operation_id: String,
    },
    RecordSource {
        operation_id: String,
        original: Box<oasis7_custody_core::approval::SignedApprovalVerificationV1>,
    },
    ClaimPreflight,
    GetPermit {
        access: Option<Box<OwnerSetupAccessEvidenceV1>>,
    },
    ClaimStep {
        step: SessionPreflightStepV1,
    },
    RecordLifetime {
        original: Box<SignedManagedNativeSessionLifetimeV1>,
    },
    PublishOwnerRead {
        original: Box<ParticipantOwnerViewRequestV1>,
    },
    Activate {
        world: Box<OwnerSetupWorldEvidenceV1>,
        private_sid_generation: DecimalU64,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionSignRpcV2 {
    pub request: TestSessionSelectorV1,
    pub access: OwnerSetupAccessEvidenceV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionPublishedObservationV2 {
    pub descriptor: oasis7_custody_core::provisioning::TestSessionPreflightDescriptorV2,
    pub authority_revision: String,
    pub service_epoch: DecimalU64,
    pub target_security_epoch: DecimalU64,
    pub observed_at: oasis7_custody_core::time_units::UnixMillis,
    pub expires_at: oasis7_custody_core::time_units::UnixMillis,
    pub original: ParticipantOwnerViewRequestV1,
}
