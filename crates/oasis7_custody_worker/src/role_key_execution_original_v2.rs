//! Public-only immutable signed original; no replay creates signing permission.
use super::*;
use oasis7_custody_core::provisioning::{
    ExecutionPrepareSignedOriginalV2, ExecutionSigningBundleV2,
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishedV2 {
    api_version: u8,
    operation_digest: String,
    permit_digest: String,
    bundle: ExecutionSigningBundleV2,
    original: ExecutionPrepareSignedOriginalV2,
}
#[wasm_bindgen(module = "/platform/execution_signer_sql_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = publishExecutionOriginalV2)]
    fn publish_sql(
        state: &JsValue,
        key: &str,
        slot: &str,
        operation: &str,
        record: &str,
        validate: &js_sys::Function,
    ) -> Result<String>;
    #[wasm_bindgen(catch, js_name = readExecutionOriginalV2)]
    fn read_original_sql(
        state: &JsValue,
        key: &str,
        slot: &str,
        operation: &str,
        validate: &js_sys::Function,
    ) -> Result<Option<String>>;
}
impl RoleKeyStore {
    pub(in crate::role_key_store) fn publish_execution_prepare_v2(
        &self,
        claimed: &ClaimedExecutionPrepareV2,
        original: &ExecutionPrepareSignedOriginalV2,
    ) -> Result<()> {
        claimed
            .verified
            .bundle()
            .verify_prepare_original(original)
            .map_err(|_| rejected())?;
        let (slot, expected) = self.execution_v2_slot(&claimed.verified)?;
        if slot != claimed.immutable_slot {
            return Err(rejected());
        }
        let operation = claimed
            .verified
            .once_operation_digest()
            .map_err(|_| rejected())?;
        let permit = claimed.verified.proof_digest().map_err(|_| rejected())?;
        let record = PublishedV2 {
            api_version: 2,
            operation_digest: operation.clone(),
            permit_digest: permit.clone(),
            bundle: claimed.verified.bundle().clone(),
            original: original.clone(),
        };
        let text = serde_json::to_string(&record).map_err(|_| rejected())?;
        if text.len() > 65536 {
            return Err(rejected());
        }
        let mut callback = |old: String, rows: String| -> Result<()> {
            validate(&old, &rows, &expected)?;
            let rows = serde_json::from_str::<InventoryV2>(&rows)
                .map_err(|_| rejected())?
                .sign;
            let row = rows
                .iter()
                .find(|r| r.operation_id == operation)
                .ok_or_else(rejected)?;
            let claim: ClaimV2 = serde_json::from_str(&row.payload).map_err(|_| rejected())?;
            if claim.permit_digest != permit
                || claim.bundle_digest != record.bundle.digest().map_err(|_| rejected())?
            {
                return Err(rejected());
            }
            Ok(())
        };
        let closure = Closure::borrow_mut(&mut callback);
        publish_sql(
            &self.state,
            KEY,
            &slot,
            &operation,
            &text,
            closure.as_ref().unchecked_ref(),
        )?;
        drop(closure);
        Ok(())
    }
    /// Fact-only lookup; no verified token, key access, fresh Current or send
    /// permission is recreated from either a known original or an unknown row.
    pub(crate) fn execution_prepare_original_v2(
        &self,
        scope: &RoleKeySlotScopeV1,
        operation: &str,
    ) -> Result<Option<ExecutionPrepareSignedOriginalV2>> {
        if COMPILED_ROLE != Role::Executor || !canonical_hex(operation) {
            return Err(rejected());
        }
        let slot = read(&self.state, KEY)?;
        let public = decode(&slot)?.slot.ok_or_else(rejected)?;
        public.validate_for(scope).map_err(|_| rejected())?;
        let expected = initial(scope, &slot, 0)?;
        let mut stored_claim = None;
        let mut callback = |old: String, rows: String| -> Result<()> {
            validate(&old, &rows, &expected)?;
            let rows = serde_json::from_str::<InventoryV2>(&rows)
                .map_err(|_| rejected())?
                .sign;
            let row = rows
                .into_iter()
                .find(|r| r.operation_id == operation)
                .ok_or_else(rejected)?;
            stored_claim =
                Some(serde_json::from_str::<ClaimV2>(&row.payload).map_err(|_| rejected())?);
            Ok(())
        };
        let closure = Closure::borrow_mut(&mut callback);
        let raw = read_original_sql(
            &self.state,
            KEY,
            &slot,
            operation,
            closure.as_ref().unchecked_ref(),
        )?;
        drop(closure);
        let Some(raw) = raw else {
            return Ok(None);
        };
        if raw.len() > 65536 {
            return Err(rejected());
        }
        let record: PublishedV2 = serde_json::from_str(&raw).map_err(|_| rejected())?;
        let claim = stored_claim.ok_or_else(rejected)?;
        let actual_operation = hash(&(
            "oasis7.custody.execution-signer.once-operation.v2",
            scope,
            &record.bundle.session_id,
            &record.bundle.action_request_id,
            &record.bundle.stage_operation_id,
        ))?;
        if record.api_version != 2
            || record.operation_digest != operation
            || actual_operation != operation
            || record.permit_digest != claim.permit_digest
            || record.bundle.digest().map_err(|_| rejected())? != claim.bundle_digest
            || record
                .bundle
                .fence
                .executor_binding
                .as_ref()
                .is_none_or(|v| v.executor_public_key != public.public_ref.public_key)
        {
            return Err(rejected());
        }
        record
            .bundle
            .verify_prepare_original(&record.original)
            .map_err(|_| rejected())?;
        Ok(Some(record.original))
    }
}

#[cfg(feature = "execution-signer-harness")]
impl RoleKeyStore {
    pub(crate) fn initialize_execution_historical_harness_v2(
        &self,
        verified: VerifiedExecutionPreparePermitV2,
        at: u64,
    ) -> Result<()> {
        self.initialize_execution_signer_at_v2(verified, at)
    }
}
