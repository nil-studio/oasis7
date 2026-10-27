use super::*;
use crate::native_history::NativeHistoryEntryV1;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStatusRecoveryRequestV1 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub operation_id: String,
    pub history_digest: String,
}
impl SessionStatusRecoveryRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1
            || [
                &self.custody_account_id,
                &self.provision_id,
                &self.operation_id,
            ]
            .iter()
            .any(|v| !super::super::super::types::valid_id(v))
            || !recovery_hex(&self.history_digest)
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        super::super::super::types::digest(&(
            "oasis7.custody.session-status-recovery-request.v1",
            self,
        ))
        .map_err(Into::into)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStatusRecoveryPermitViewV1 {
    pub request: SessionStatusRecoveryRequestV1,
    pub history: NativeHistoryEntryV1,
    pub source_permit_id: String,
    pub nonce: String,
    pub private_sid_generation: DecimalU64,
    pub native_binding_generation: DecimalU64,
    pub target_security_epoch: DecimalU64,
    pub service_epoch: DecimalU64,
    pub source_revision: String,
    pub authority_revision: String,
    pub observed_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub http_origin: String,
    pub binding: NativeBindingReceiptV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionStatusRecoveryFundingV1 {
    Provision,
    ManagedLogin { operation_id: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStatusRecoveryOriginalV1 {
    pub request: SessionStatusRecoveryRequestV1,
    pub funding: SessionStatusRecoveryFundingV1,
    pub source_permit_id: String,
    pub nonce: String,
    pub private_sid_generation: DecimalU64,
    pub native_binding_generation: DecimalU64,
    pub target_security_epoch: DecimalU64,
    pub http_origin: String,
    pub created_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub source_inspection: ProvisionOriginInspectionV1,
    pub claimed_at: Option<DecimalU64>,
    pub reads: u8,
    pub result: Option<SignedManagedNativeSessionStatusV1>,
    pub completed_at: Option<DecimalU64>,
    pub result_private_sid_generation: Option<DecimalU64>,
}
pub(super) fn recovery_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
