//! Closed private service messages. Selectors carry no caller-supplied authority or secrets.
use oasis7_custody_core::{
    DecimalU64,
    native_binding::SignedNativeBindingChallengeV1,
    provisioning::{NativeStageRequestV1, NativeStageResultV1},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum NativeRuntimeCall {
    Provision {
        request: NativeStageRequestV1,
    },
    ManagedLogin {
        custody_account_id: String,
        provision_id: String,
        operation_id: String,
    },
}
impl NativeRuntimeCall {
    pub fn account_id(&self) -> &str {
        match self {
            Self::Provision { request } => &request.custody_account_id,
            Self::ManagedLogin {
                custody_account_id, ..
            } => custody_account_id,
        }
    }
    pub fn provision_id(&self) -> &str {
        match self {
            Self::Provision { request } => &request.provision_id,
            Self::ManagedLogin { provision_id, .. } => provision_id,
        }
    }
    pub fn operation_id(&self) -> &str {
        match self {
            Self::Provision { request } => &request.operation_id,
            Self::ManagedLogin { operation_id, .. } => operation_id,
        }
    }
    pub fn validate(&self) -> bool {
        let id = |v: &str| {
            !v.is_empty()
                && v.len() <= 256
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
        };
        match self {
            Self::Provision { request } => request.validate().is_ok(),
            Self::ManagedLogin { .. } => {
                [self.account_id(), self.provision_id(), self.operation_id()]
                    .iter()
                    .all(|v| id(v))
            }
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeSessionAttestation {
    pub status_digest: String,
    pub session_ref: String,
    pub private_sid_generation: DecimalU64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum NativeRuntimeAction {
    GetPermit,
    OwnerStatement {
        challenge: SignedNativeBindingChallengeV1,
    },
    BindingStatement {
        binding_operation_id: String,
    },
    BindingReceipt,
    RecordResult {
        result: NativeStageResultV1,
        session: Option<NativeSessionAttestation>,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeRuntimeRpc {
    pub call: NativeRuntimeCall,
    pub action: NativeRuntimeAction,
}

/// A separate capability: readonly status recovery cannot be used as a login permit.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeRecoveryRpc {
    pub request: oasis7_custody_core::provisioning::SessionStatusRecoveryRequestV1,
    pub action: NativeRecoveryAction,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum NativeRecoveryAction {
    GetPermit,
    RecordResult {
        status: Box<oasis7_custody_core::native_binding::SignedManagedNativeSessionStatusV1>,
        session: oasis7_custody_core::native_history::NativeSessionStorageAttestationV1,
    },
}
