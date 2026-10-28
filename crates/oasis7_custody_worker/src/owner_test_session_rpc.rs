//! Closed private ports. Static service entrypoints supply the actor role.
use oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewRequestV1;
use oasis7_custody_core::{
    DecimalU64,
    native_binding::SignedManagedNativeSessionLifetimeV1,
    provisioning::{
        CommitTestSessionV1, OwnerSetupAccessEvidenceV1, OwnerSetupWorldEvidenceV1,
        SessionPreflightStepV1, SessionSourceFundingV1,
    },
};
use serde::{Deserialize, Serialize};

macro_rules! management_rpc {
    ($name:ident, $request:ty) => {
        #[derive(Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name {
            pub manager_session: oasis7_custody_core::approval::CustodySessionStatusV1,
            pub provision_id: String,
            pub request: $request,
        }
    };
}
management_rpc!(
    TestSessionInviteRpc,
    oasis7_custody_core::provisioning::PairingInviteCreateRequestV1
);
management_rpc!(
    TestSessionBasisRpc,
    oasis7_custody_core::provisioning::TestSessionBasisRequestV1
);
management_rpc!(
    TestSessionPrepareRpc,
    oasis7_custody_core::provisioning::TestSessionPrepareRequestV1
);
management_rpc!(
    TestSessionApproveRpc,
    oasis7_custody_core::provisioning::TestSessionApproveRequestV1
);
management_rpc!(
    TestSessionContinueRpc,
    oasis7_custody_core::provisioning::TestSessionContinueRequestV1
);

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionSelectorV1 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub session_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionRuntimeRpc {
    pub request: TestSessionSelectorV1,
    pub action: TestSessionRuntimeAction,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionAttestationRequest {
    pub session: oasis7_custody_core::provisioning::ParticipantSessionRefV1,
    pub lifetime: SignedManagedNativeSessionLifetimeV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionPrivateAttestation {
    pub session_ref: String,
    pub private_sid_generation: DecimalU64,
    pub lifetime_digest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "owner")]
pub(crate) struct TestSessionSignerPermit {
    pub permit: oasis7_custody_core::provisioning::TestSessionPreflightPermitV1,
    pub signing: oasis7_custody_core::provisioning::TestSessionPreflightSigningBundleV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionSignRpc {
    pub request: TestSessionSelectorV1,
    pub access: OwnerSetupAccessEvidenceV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "owner")]
pub(crate) struct TestSessionPublishedRead {
    pub permit: oasis7_custody_core::provisioning::TestSessionPreflightPermitV1,
    pub original: ParticipantOwnerViewRequestV1,
}

/// Read-only post-publication metadata; never an Owner signing permit.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestSessionPublishedObservation {
    pub descriptor: oasis7_custody_core::provisioning::TestSessionPreflightDescriptorV1,
    pub authority_revision: String,
    pub service_epoch: DecimalU64,
    pub target_security_epoch: DecimalU64,
    pub observed_at: oasis7_custody_core::time_units::UnixMillis,
    pub expires_at: oasis7_custody_core::time_units::UnixMillis,
    pub original: ParticipantOwnerViewRequestV1,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum TestSessionRuntimeAction {
    DescribePreflight,
    ClaimApprovalVerification,
    ConsumeApproval {
        original: Box<CommitTestSessionV1>,
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
