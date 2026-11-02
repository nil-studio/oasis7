//! Same Account invocation only. One prepaid fixed private Actor fact read;
//! no Current/key/Vault/Native/World callback, no retry, no portable certificate.
use super::*;
use crate::role_key_store::{
    TermGuardedSignerConfigurationV2, TermOwnerSlotQualificationReplyV2,
    TermOwnerSlotQualificationRequestV2,
};
use oasis7_custody_core::{
    native_cleanup_v2::AccountMachinePurposeV2,
    provisioning::{PreparedTermOwnerSlotReadV2, TermOwnedInvocationV2},
    time_units::UnixMillis,
};
use worker::Env;
/// Only this private actual-RPC consumer constructs the local eligibility.
/// Stored history, reply metadata, Env and another invocation cannot mint it.
pub(crate) struct InstalledTermOwnerSlotV2 {
    invocation_id: String,
    session_id: String,
    owner_scope_digest: String,
    account_draft_digest: String,
    until: u64,
}
impl InstalledTermOwnerSlotV2 {
    pub(crate) fn require_for(&self, owned: &TermOwnedInvocationV2, now: u64) -> Result<()> {
        if self.invocation_id != owned.invocation_id()
            || self.session_id.is_empty()
            || self.owner_scope_digest.len() != 64
            || self.account_draft_digest.len() != 64
            || now >= self.until
        {
            return Err(rejected());
        }
        Ok(())
    }
}
impl OwnerProvisionApprovalStore {
    // This typed port differs from the generic local_reservation: only the real
    // owned first Control holder can select a paid fact-read dispatch token.
    fn claim_term_slot_read_local_v2(
        &self,
        owned: &TermOwnedInvocationV2,
        attempt: &str,
    ) -> Result<PreparedTermOwnerSlotReadV2> {
        let initial = read(&self.state, KEY)?;
        let mut returned = None;
        let mut callback = |old: String, inventory: String| -> Result<Array> {
            let (mut next, _previous) = self.reader.transition(&old)?;
            validate_inventory(&inventory, next.state.draft(), true)?;
            let result = next
                .state
                .claim_test_session_term_owner_slot_read_v2(owned, attempt, UnixMillis::new(now()?))
                .map_err(|_| rejected())?;
            next.audit_sequence =
                DecimalU64(next.audit_sequence.0.checked_add(1).ok_or_else(rejected)?);
            let audit = serde_json::to_string(&(
                next.audit_sequence,
                "term_owner_slot_read_paid",
                result.session_id(),
                attempt,
            ))
            .map_err(|_| rejected())?;
            let array = Array::new();
            array.push(&encode(&next)?.into());
            array.push(&next.audit_sequence.0.to_string().into());
            array.push(&audit.into());
            returned = Some(result);
            Ok(array)
        };
        let closure = Closure::borrow_mut(&mut callback);
        update(&self.state, KEY, &initial, closure.as_ref().unchecked_ref())?;
        drop(closure);
        returned.ok_or_else(rejected)
    }
    pub(crate) async fn qualify_term_owner_slot_v2(
        &self,
        env: &Env,
        owned: &TermOwnedInvocationV2,
        attempt: &str,
    ) -> Result<InstalledTermOwnerSlotV2> {
        if !env
            .var("CUSTODY_TERM_GUARDED_DUAL_SIGNER_V2_ENABLED")
            .ok()
            .is_some_and(|v| v.to_string() == "true")
        {
            return Err(rejected());
        }
        if !env
            .var("CUSTODY_TERM_GUARDED_SIGNER_V2_ENABLED")
            .ok()
            .is_some_and(|v| v.to_string() == "true")
        {
            return Err(rejected());
        }
        let (stop_pin, stop_floor) = crate::execution_context_issuer_pin_v2::pin(
            env,
            AccountMachinePurposeV2::TermGuardedStopSignerV2,
        )
        .map_err(|_| rejected())?;
        let (close_pin, close_floor) = crate::execution_context_issuer_pin_v2::pin(
            env,
            AccountMachinePurposeV2::TermGuardedCloseSignerV2,
        )
        .map_err(|_| rejected())?;
        // Configuration validates fixed tuple/purposes only; never eligibility.
        let _checked = TermGuardedSignerConfigurationV2::new(stop_pin.clone(), stop_floor.clone())?
            .with_close(close_pin.clone(), close_floor.clone())?;
        let expected_pin_digest = oasis7_wasm_abi::canonical_hash(&(
            "oasis7.custody.term-owner-slot-dual-installation.v2",
            &stop_pin,
            &stop_floor,
            &close_pin,
            &close_floor,
        ))
        .map_err(|_| rejected())?;
        let first = self.claim_term_slot_read_local_v2(owned, attempt)?;
        let q = TermOwnerSlotQualificationRequestV2 {
            api_version: 2,
            paid_attempt_id: first.attempt_id().into(),
            owner_scope: first.scope().clone(),
            owner_key: first.key().clone(),
            issued_at: DecimalU64(first.issued_at().get()),
            until: DecimalU64(first.until().get()),
        };
        let request_digest = oasis7_wasm_abi::canonical_hash(&(
            "oasis7.custody.term-owner-slot-qualification-request.v2",
            &q,
        ))
        .map_err(|_| rejected())?;
        if request_digest != first.request_digest() || now()? >= first.until().get() {
            return Err(rejected());
        }
        let body = serde_json::to_string(&q).map_err(|_| rejected())?;
        if body.len() > 32768 {
            return Err(rejected());
        }
        let name = first.scope().task_digest().map_err(|_| rejected())?;
        // A closed helper fixes namespace/host/path/method and races both fetch
        // and bounded body against the original absolute deadline. No retry.
        let raw = fetch_qualification(env.as_ref(), &name, &body, first.until().get() as f64)
            .await
            .map_err(|_| rejected())?
            .as_string()
            .ok_or_else(rejected)?;
        let reply: TermOwnerSlotQualificationReplyV2 =
            serde_json::from_str(&raw).map_err(|_| rejected())?;
        let value: serde_json::Value = serde_json::from_str(&raw).map_err(|_| rejected())?;
        let at = now()?;
        if serde_json::to_value(&reply).map_err(|_| rejected())? != value
            || reply.api_version != 2
            || reply.installed_schema != 2
            || reply.paid_attempt_id != first.attempt_id()
            || reply.request_digest != request_digest
            || reply.owner_scope != *first.scope()
            || reply.owner_key != *first.key()
            || reply.pin_floor_digest != expected_pin_digest
            || reply.until.0 != first.until().get()
            || reply.observed_at.0 < first.issued_at().get()
            || reply.observed_at.0 > at
            || at >= reply.until.0
        {
            return Err(rejected());
        }
        first
            .lineage()
            .validate_for(first.scope())
            .map_err(|_| rejected())?;
        // Account factory derived the immutable lineage from its real draft and
        // retained authority inventory. The Actor response did not prove it.
        let installed = InstalledTermOwnerSlotV2 {
            invocation_id: owned.invocation_id().into(),
            session_id: first.session_id().into(),
            owner_scope_digest: name,
            account_draft_digest: first.lineage().account_draft_digest.clone(),
            until: first.until().get(),
        };
        installed.require_for(owned, at)?;
        Ok(installed)
    }
}
#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/term_owner_slot_qualification_v2.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch,js_name=fetchTermOwnerSlotQualification)]
    async fn fetch_qualification(
        env: &JsValue,
        name: &str,
        body: &str,
        until: f64,
    ) -> std::result::Result<JsValue, JsValue>;
}
