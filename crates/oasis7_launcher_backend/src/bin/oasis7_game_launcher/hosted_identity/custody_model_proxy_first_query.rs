//! First reserve query is durable before H/Account I/O; retries keep original.
use super::*;
use oasis7_custody_core::{
    approval_admission::ApprovalAdmissionInput,
    native_model_proxy_query_v2::SignedNativeModelReservationQueryV2,
};
impl IdentityService {
    pub(crate) fn model_proxy_first_reservation_query(
        &mut self,
        input: &ApprovalAdmissionInput,
        now: u64,
    ) -> Result<SignedNativeModelReservationQueryV2, StoreError> {
        let owner = self.model_proxy_owner_binding(input, now)?;
        let storage_key = format!(
            "custody-model-first-query-v2:{}",
            owner.approved_input_digest()
        );
        if let Some(old) = self
            .store
            .transaction(|tx| tx.get::<SignedNativeModelReservationQueryV2>(&storage_key))?
        {
            // Historical exact original only: no new nonce, timestamp or floor.
            old.value
                .verify_input_binding(input, old.value.query.issued_at.0)
                .map_err(|_| StoreError::Corrupt)?;
            return Ok(old.value);
        }
        let query = self
            .model_proxy_basis
            .as_ref()
            .ok_or_else(|| reject("model_proxy_unsupported"))?
            .first_reservation_query(input, &owner, now)
            .map_err(|_| reject("model_proxy_unsupported"))?;
        query
            .verify_input_binding(input, now)
            .map_err(|_| reject("model_proxy_query_mismatch"))?;
        // No await/HTTP/key loan between strong current owner binding and this
        // ordinary guarded SQL. Store commit must finish before caller gets query.
        self.store.transaction(|tx| {
            let saved = tx
                .get::<ApprovalRecord>(&approval_key(&input.approval.proof.approval_id))?
                .ok_or_else(|| reject("approval_not_found"))?
                .value;
            let (account, session) = management_session(
                tx,
                &saved.session_storage_key,
                self.config.rp.identity_instance_epoch,
                now,
            )?;
            let config = self
                .custody
                .as_ref()
                .ok_or_else(|| reject("custody_unavailable"))?;
            super::guarded_consent::require_guarded_consent_v2(
                tx,
                config,
                &self.config,
                &saved,
                super::guarded_consent::GuardedConsentUseV2::NativeModelProxy,
                None,
            )?;
            let exact_proof: SignedCustodyApprovalV1 = serde_json::from_slice(
                saved
                    .proof_bytes
                    .as_ref()
                    .ok_or_else(|| reject("model_proxy_consent_missing"))?,
            )
            .map_err(|_| StoreError::Corrupt)?;
            if exact_proof != input.approval || saved.complete_digest.is_none() {
                return Err(reject("model_proxy_consent_mismatch"));
            }
            super::complete::check_record(
                &saved,
                &account,
                &session,
                &saved.session_storage_key,
                &saved.intent_digest,
            )?;
            if saved.request != input.request
                || account.account_id != owner.account_id()
                || session.session_id != owner.session_ref()
            {
                return Err(reject("model_proxy_owner_changed"));
            }
            if let Some(old) = tx.get::<SignedNativeModelReservationQueryV2>(&storage_key)? {
                old.value
                    .verify_input_binding(input, old.value.query.issued_at.0)
                    .map_err(|_| StoreError::Corrupt)?;
                return Ok(old.value);
            }
            tx.create(
                &storage_key,
                "custody_model_first_query_v2",
                Some(&account.account_id),
                None,
                &query,
            )?;
            Ok(query.clone())
        })
    }
}
