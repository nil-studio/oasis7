//! Closed private reconciliation ports. Role authority is supplied by the entrypoint.
use oasis7_custody_core::{
    approval::CustodySessionStatusV1,
    provisioning::{
        OwnerSetupAccessEvidenceV1, OwnerSetupAdmissionEvidenceV1, OwnerSetupCanonicalFactV1,
        OwnerSetupReadPurposeV1, OwnerSetupReconcileApproveRequestV1,
        OwnerSetupReconcileAttemptRequestV1, OwnerSetupReconcileBasisRequestV1,
        OwnerSetupReconcileCompletionV1, OwnerSetupReconcilePermitV1,
        OwnerSetupReconcilePrepareRequestV1, OwnerSetupReconcileReadCandidateV1,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReconcileBasisRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: OwnerSetupReconcileBasisRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReconcilePrepareRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: OwnerSetupReconcilePrepareRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReconcileApproveRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: OwnerSetupReconcileApproveRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReconcileContinueRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: OwnerSetupReconcileAttemptRequestV1,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReconcileSelector {
    pub custody_account_id: String,
    pub provision_id: String,
    pub request: OwnerSetupReconcileAttemptRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReconcileRuntimeRpc {
    pub selector: ReconcileSelector,
    pub action: ReconcileAction,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ReconcileAction {
    GetPermit {
        purpose: OwnerSetupReadPurposeV1,
        access: Option<Box<OwnerSetupAccessEvidenceV1>>,
    },
    PublishReads {
        candidate: Box<OwnerSetupReconcileReadCandidateV1>,
    },
    Admit {
        evidence: Box<OwnerSetupAdmissionEvidenceV1>,
    },
    ClaimLookup,
    ClaimResult,
    ArchiveFact {
        fact: Box<OwnerSetupCanonicalFactV1>,
    },
    RecordResult {
        completion: Box<ReconcileCompletion>,
    },
}
// The Core transition context is Rust-only; only its closed evidence crosses a port.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReconcileCompletion {
    pub result: oasis7_custody_core::provisioning::OwnerSetupResultV1,
    pub session: oasis7_custody_core::provisioning::ParticipantSessionEvidenceV1,
    pub world: oasis7_custody_core::provisioning::OwnerSetupWorldEvidenceV1,
    pub lookup_session: Option<oasis7_custody_core::provisioning::ParticipantSessionEvidenceV1>,
}
impl From<ReconcileCompletion> for OwnerSetupReconcileCompletionV1 {
    fn from(v: ReconcileCompletion) -> Self {
        Self {
            result: v.result,
            session: v.session,
            world: v.world,
            lookup_session: v.lookup_session,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReconcileSignRpc {
    pub selector: ReconcileSelector,
    pub access: OwnerSetupAccessEvidenceV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReconcilePublished {
    pub permit: OwnerSetupReconcilePermitV1,
    pub candidate: OwnerSetupReconcileReadCandidateV1,
}
