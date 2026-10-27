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
    pub request: OwnerSetupPrepareRequestV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerSetupApproveRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: OwnerSetupApproveRequestV1,
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
