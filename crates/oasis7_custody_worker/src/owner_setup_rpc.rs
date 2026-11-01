//! Fixed private setup service ports. Manager metadata comes only from Identity.
#[cfg(feature = "owner")]
use oasis7_custody_core::provisioning::OwnerSetupSigningBundleV1;
use oasis7_custody_core::{
    approval::CustodySessionStatusV1,
    provisioning::{
        OwnerSetupAccessEvidenceV1, OwnerSetupAdmissionEvidenceV1, OwnerSetupApproveRequestV1,
        OwnerSetupBasisRequestV1, OwnerSetupCandidateV1, OwnerSetupCanonicalFactV1,
        OwnerSetupDriveRequestV1, OwnerSetupPermitViewV1, OwnerSetupPrepareRequestV1,
        OwnerSetupReadPurposeV1, OwnerSetupResultV1, OwnerSetupStageV1,
        ParticipantSessionEvidenceV1,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerSetupBasisRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: OwnerSetupBasisRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerSetupPrepareRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: OwnerSetupPrepareInput,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerSetupApproveRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: OwnerSetupApproveInput,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerSetupContinueRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: OwnerSetupDriveRequestV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerSetupRuntimeRpc {
    pub request: OwnerSetupDriveRequestV1,
    pub stage: OwnerSetupStageV1,
    pub action: OwnerSetupRuntimeAction,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum OwnerSetupRuntimeAction {
    GetPermit {
        purpose: OwnerSetupReadPurposeV1,
        access: Option<Box<OwnerSetupAccessEvidenceV1>>,
    },
    PublishCandidate {
        candidate: Box<OwnerSetupCandidateV1>,
    },
    Admit {
        evidence: Box<OwnerSetupAdmissionEvidenceV1>,
    },
    ClaimLookup,
    ArchiveCanonical {
        fact: Box<OwnerSetupCanonicalFactV1>,
    },
    RecordResult {
        result: Box<OwnerSetupResultV1>,
        session: Box<ParticipantSessionEvidenceV1>,
    },
    RecordResourceResult {
        completed: Box<oasis7_custody_core::provisioning::CompleteOwnerSetupResourceFinalV2>,
    },
    MarkUnknown,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "owner")]
pub(crate) struct OwnerSetupSignerPermit {
    pub permit: OwnerSetupPermitViewV1,
    pub signing: OwnerSetupSigningBundleV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerSetupPublished {
    pub permit: OwnerSetupPermitViewV1,
    pub candidate: OwnerSetupCandidateV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerSetupStageRpc {
    pub request: OwnerSetupDriveRequestV1,
    pub stage: OwnerSetupStageV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerSetupSignRpc {
    pub selector: OwnerSetupStageRpc,
    pub access: OwnerSetupAccessEvidenceV1,
}

/// Same management route, explicitly versioned plans. Each alternative validates
/// its own api_version; a V1 plan cannot deserialize or upgrade into V2.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum OwnerSetupPrepareInput {
    V2(Box<oasis7_custody_core::provisioning::OwnerSetupPrepareRequestV2>),
    V1(Box<OwnerSetupPrepareRequestV1>),
}
impl OwnerSetupPrepareInput {
    #[cfg(feature = "owner")]
    pub(crate) fn approval_request_id(&self) -> &str {
        match self {
            Self::V1(v) => &v.approval_request_id,
            Self::V2(v) => &v.approval_request_id,
        }
    }
    #[cfg(feature = "owner")]
    pub(crate) fn expected_intent(
        &self,
    ) -> std::result::Result<oasis7_custody_core::approval::CustodyIntentV1, String> {
        match self {
            Self::V1(v) => v.plan.intent().map_err(|_| "invalid setup intent".into()),
            Self::V2(v) => v.plan.intent(),
        }
    }
    pub(crate) fn provision_id(&self) -> &str {
        match self {
            Self::V1(v) => &v.plan.provision_id,
            Self::V2(v) => &v.plan.provision_id,
        }
    }
    pub(crate) fn validate(&self) -> std::result::Result<(), String> {
        match self {
            Self::V1(v) => v.validate().map_err(|_| "invalid setup prepare".into()),
            Self::V2(v) => v.validate(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum OwnerSetupApproveInput {
    V2(Box<oasis7_custody_core::provisioning::OwnerSetupApproveRequestV2>),
    V1(Box<OwnerSetupApproveRequestV1>),
}
impl OwnerSetupApproveInput {
    #[cfg(feature = "owner")]
    pub(crate) fn api_version(&self) -> u8 {
        match self {
            Self::V1(v) => v.api_version,
            Self::V2(v) => v.api_version,
        }
    }
    #[cfg(feature = "owner")]
    pub(crate) fn setup_id(&self) -> &str {
        match self {
            Self::V1(v) => &v.plan.setup_id,
            Self::V2(v) => &v.plan.setup_id,
        }
    }
    #[cfg(feature = "owner")]
    pub(crate) fn input(&self) -> &oasis7_custody_core::approval_admission::ApprovalAdmissionInput {
        match self {
            Self::V1(v) => &v.input,
            Self::V2(v) => &v.input,
        }
    }
    pub(crate) fn provision_id(&self) -> &str {
        match self {
            Self::V1(v) => &v.plan.provision_id,
            Self::V2(v) => &v.plan.provision_id,
        }
    }
    pub(crate) fn validate(&self) -> std::result::Result<(), String> {
        match self {
            Self::V1(v) => v.validate().map_err(|_| "invalid setup approval".into()),
            Self::V2(v) => v.validate(),
        }
    }
}
#[cfg(feature = "owner")]
pub(crate) struct OwnerSetupApproveRpcV1 {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: OwnerSetupApproveRequestV1,
}
