//! A fixed Account implementation contract, not a caller-configured row cap.
//! The issuer is trusted only with its exact configured version; this cannot
//! attest whole Account/Owner-DO rollback or upgrade a different provision.
use crate::{provisioning::*, role_key_slot::RoleKeySlotScopeV1};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermAccountLineageContractV2 {
    RetainedAuthorities256V2,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermOwnerSlotLineageV2 {
    pub contract: TermAccountLineageContractV2,
    pub custody_account_id: String,
    pub provision_id: String,
    pub account_draft_digest: String,
    pub owner_slot_scope_digest: String,
}
impl TermOwnerSlotLineageV2 {
    pub(crate) fn from_actual_account(
        state: &AccountProvisionApprovalState,
        scope: &RoleKeySlotScopeV1,
    ) -> Result<Self, ProvisionApprovalError> {
        let draft = state.draft();
        if state.retained_test_session_authority_count_v2() > MAX_TEST_SESSION_AUTHORITIES_V2
            || scope.context.identity.account_id != draft.entry.custody_account_id
            || scope.provision_id != draft.entry.provision_id
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let v = Self {
            contract: TermAccountLineageContractV2::RetainedAuthorities256V2,
            custody_account_id: draft.entry.custody_account_id.clone(),
            provision_id: draft.entry.provision_id.clone(),
            account_draft_digest: oasis7_wasm_abi::canonical_hash(&(
                "oasis7.custody.term-account-retained-lineage.v2",
                draft,
            ))
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?,
            owner_slot_scope_digest: scope
                .task_digest()
                .map_err(|_| ProvisionApprovalError::BindingMismatch)?,
        };
        v.validate_for(scope)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        Ok(v)
    }
    pub fn validate_for(&self, scope: &RoleKeySlotScopeV1) -> Result<(), String> {
        scope
            .validate()
            .map_err(|_| "Term owner lineage unavailable".to_owned())?;
        if self.custody_account_id != scope.context.identity.account_id
            || self.provision_id != scope.provision_id
            || !crate::native_cleanup_v2::hex64(&self.account_draft_digest)
            || self.owner_slot_scope_digest
                != scope
                    .task_digest()
                    .map_err(|_| "Term owner lineage unavailable")?
        {
            return Err("Term owner lineage unavailable".into());
        }
        Ok(())
    }
    pub fn maximum_sessions() -> usize {
        MAX_TEST_SESSION_AUTHORITIES_V2
    }
    pub fn maximum_signature_claims() -> Result<usize, String> {
        Self::maximum_sessions()
            .checked_mul(2)
            .ok_or_else(|| "Term inventory overflow".into())
    }
}
