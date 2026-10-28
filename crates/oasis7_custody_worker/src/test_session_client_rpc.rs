//! Private client transport. Fixed entrypoints supply the operation and origin.
//! These messages never carry a verified receipt, public key, token digest or Current.
use crate::owner_test_session_rpc::TestSessionSelectorV1;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionClientHttpRpc {
    pub selector: TestSessionSelectorV1,
    pub original: oasis7_custody_core::provisioning::SessionClientHttpRequestV1,
}

impl core::fmt::Debug for TestSessionClientHttpRpc {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionClientHttpRpc(<private>)")
    }
}

/// Generated once by the trusted Identity ingress for each actual HTTP attempt.
/// A client header or request body must never supply this value.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionClientAdmissionRpc {
    pub ingress_attempt_id: String,
    pub http: TestSessionClientHttpRpc,
}

impl core::fmt::Debug for TestSessionClientAdmissionRpc {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionClientAdmissionRpc(<private>)")
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionClientStageRpc {
    pub admission: TestSessionClientAdmissionRpc,
    pub action: TestSessionClientAction,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TestSessionClientObservation {
    PreDecrypt,
    PostDecrypt,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum TestSessionClientAction {
    Admit,
    ClaimSource,
    RecordSource {
        signed: Box<oasis7_custody_core::approval::SignedApprovalVerificationV1>,
    },
    Observe {
        index: TestSessionClientObservation,
    },
    ClaimLifetime,
    Complete {
        /// None is valid only for the already authenticated Finish/Abort tightening lane.
        lifetime:
            Option<Box<oasis7_custody_core::native_binding::SignedManagedNativeSessionLifetimeV1>>,
    },
}

/// A paid Account observation for the fixed Identity private-reader port.
/// Identity must also validate its own deployment scope, independent Fence and clock.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionClientObserved {
    pub descriptor: oasis7_custody_core::provisioning::TestSessionLiveDescriptorV1,
    pub source_ticket: u8,
    pub authority_revision: String,
    pub service_epoch: oasis7_custody_core::DecimalU64,
    pub target_security_epoch: oasis7_custody_core::DecimalU64,
    pub observed_at: oasis7_custody_core::time_units::UnixMillis,
    pub expires_at: oasis7_custody_core::time_units::UnixMillis,
}
