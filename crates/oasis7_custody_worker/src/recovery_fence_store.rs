//! Independent ledger, with real verification before final synchronous SQL admission.
use oasis7_custody_core::{DecimalU64, recovery::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, prelude::*};
use worker::{Env, Result};
const LEDGER: &str = "recovery-fence-ledger-v1";
const FENCE: &str = "recovery-fence-v1";
#[wasm_bindgen(module = "/platform/fence_sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readFenceInventory)]
    fn inventory(state: &JsValue) -> core::result::Result<String, JsValue>;
    #[wasm_bindgen(catch, js_name = fenceTransition)]
    fn transition(
        state: &JsValue,
        callback: &js_sys::Function,
    ) -> core::result::Result<String, JsValue>;
}
fn denied() -> JsValue {
    JsValue::from_str("independent fence authority unavailable")
}
fn now() -> core::result::Result<u64, JsValue> {
    let value = js_sys::Date::now();
    if !value.is_finite() || !(0.0..=9_007_199_254_740_991.0).contains(&value) {
        return Err(denied());
    }
    Ok(value as u64)
}
fn policy(env: &Env) -> core::result::Result<RecoveryAuthorityPolicyV1, JsValue> {
    let text = env
        .var("CUSTODY_RECOVERY_AUTHORITY_POLICY")
        .map_err(|_| denied())?
        .to_string();
    if text.len() > 8192 {
        return Err(denied());
    }
    let value: RecoveryAuthorityPolicyV1 = serde_json::from_str(&text).map_err(|_| denied())?;
    value.validate().map_err(|_| denied())?;
    Ok(value)
}
pub(crate) fn control_enabled(env: &Env) -> bool {
    env.var("CUSTODY_RECOVERY_CONTROL_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        == Some("true")
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    payload: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    records: Vec<Record>,
    table_names: Vec<String>,
    audit_count: DecimalU64,
    audit_rows: Vec<AuditRow>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditRow {
    record_id: String,
    sequence: String,
    payload: String,
}
fn parse(
    raw: &str,
    policy: &RecoveryAuthorityPolicyV1,
    object: &str,
) -> core::result::Result<RecoveryFenceLedgerV1, JsValue> {
    if raw.len() > 1_100_000 {
        return Err(denied());
    }
    let inventory: Inventory = serde_json::from_str(raw).map_err(|_| denied())?;
    if inventory.records.is_empty()
        || inventory.records.len() > 2
        || inventory.table_names.iter().any(|v| {
            !matches!(
                v.as_str(),
                "table:custody_records"
                    | "table:custody_audit"
                    | "table:sqlite_sequence"
                    | "table:_cf_METADATA"
            )
        })
    {
        return Err(denied());
    }
    let old = inventory
        .records
        .iter()
        .find(|v| v.id == FENCE)
        .ok_or_else(denied)?;
    let fence: RecoveryFence = serde_json::from_str(&old.payload).map_err(|_| denied())?;
    if old.payload != serde_json::to_string(&fence).map_err(|_| denied())? {
        return Err(denied());
    }
    if let Some(row) = inventory.records.iter().find(|v| v.id == LEDGER) {
        if inventory.records.len() != 2 || row.payload.len() > 524288 {
            return Err(denied());
        }
        let ledger: RecoveryFenceLedgerV1 =
            serde_json::from_str(&row.payload).map_err(|_| denied())?;
        if ledger.policy() != policy
            || ledger.object_id() != object
            || ledger.fence() != fence
            || ledger.revision() != inventory.audit_count
            || serde_json::to_string(&ledger).map_err(|_| denied())? != row.payload
        {
            return Err(denied());
        }
        let expected = ledger.audit_history().map_err(|_| denied())?;
        if inventory.audit_rows.len() != expected.len() {
            return Err(denied());
        }
        let mut seen = std::collections::BTreeSet::new();
        for row in &inventory.audit_rows {
            let sequence: DecimalU64 =
                serde_json::from_value(row.sequence.clone().into()).map_err(|_| denied())?;
            if row.record_id != LEDGER
                || row.payload.len() > 2048
                || sequence.0 == 0
                || !seen.insert(sequence.0)
            {
                return Err(denied());
            }
            let index = usize::try_from(sequence.0 - 1).map_err(|_| denied())?;
            let payload: RecoveryFenceAuditV1 =
                serde_json::from_str(&row.payload).map_err(|_| denied())?;
            if expected.get(index) != Some(&payload) {
                return Err(denied());
            }
        }
        Ok(ledger)
    } else {
        if inventory.records.len() != 1
            || inventory.audit_count.0 != 0
            || !inventory.audit_rows.is_empty()
        {
            return Err(denied());
        }
        RecoveryFenceLedgerV1::new(policy.clone(), object.into(), fence).map_err(|_| denied())
    }
}
struct Verified(Vec<RecoveryActivationSignatureCheck>);
impl RecoveryActivationVerifier for Verified {
    fn verify(
        &mut self,
        public_key: &str,
        signature: &str,
        bytes: &[u8],
    ) -> core::result::Result<(), InvalidRecoveryActivation> {
        if self
            .0
            .iter()
            .any(|v| v.public_key == public_key && v.signature == signature && v.bytes == bytes)
        {
            Ok(())
        } else {
            Err(InvalidRecoveryActivation)
        }
    }
}
pub(crate) struct FenceStore<'a> {
    state: &'a JsValue,
    env: &'a Env,
    object: &'a str,
    initial: &'a str,
}
#[derive(Serialize)]
pub(crate) struct FenceDescription {
    pub ledger: RecoveryFenceLedgerV1,
    pub state_digest: String,
    pub policy_digest: String,
}
impl<'a> FenceStore<'a> {
    pub fn new(state: &'a JsValue, env: &'a Env, object: &'a str, initial: &'a str) -> Self {
        Self {
            state,
            env,
            object,
            initial,
        }
    }
    fn snapshot(
        &self,
        policy: &RecoveryAuthorityPolicyV1,
    ) -> core::result::Result<RecoveryFenceLedgerV1, JsValue> {
        crate::sql_store::read_frozen_fence(self.state, self.initial).map_err(|_| denied())?;
        parse(&inventory(self.state)?, policy, self.object)
    }
    pub fn read(&self) -> Result<RecoveryFence> {
        let initial = crate::sql_store::read_frozen_fence(self.state, self.initial)?;
        let raw = inventory(self.state)?;
        let inv: Inventory = serde_json::from_str(&raw).map_err(|_| denied())?;
        if inv.records.iter().all(|v| v.id != LEDGER) {
            // An active legacy row without history is never trusted.
            if !initial.disabled()
                || inv.records.len() != 1
                || inv.records[0].id != FENCE
                || inv.audit_count.0 != 0
            {
                return Err(denied().into());
            }
            return Ok(initial);
        }
        Ok(parse(&raw, &policy(self.env)?, self.object)?.fence())
    }
    pub fn describe(&self) -> core::result::Result<FenceDescription, JsValue> {
        let ledger = self.snapshot(&policy(self.env)?)?;
        Ok(FenceDescription {
            state_digest: ledger.digest().map_err(|_| denied())?,
            policy_digest: ledger.policy().digest().map_err(|_| denied())?,
            ledger,
        })
    }
    fn update(
        &self,
        mut step: impl FnMut(&mut RecoveryFenceLedgerV1) -> core::result::Result<(), JsValue>,
    ) -> core::result::Result<RecoveryFenceLedgerV1, JsValue> {
        let configured = policy(self.env)?;
        let mut returned = None;
        let mut callback = |raw: String| -> core::result::Result<js_sys::Array, JsValue> {
            // Re-read actual deployment scope/policy and complete authority inventory inside SQL.
            if policy(self.env)? != configured {
                return Err(denied());
            }
            let mut ledger = parse(&raw, &configured, self.object)?;
            let before = ledger.clone();
            step(&mut ledger)?;
            let result = js_sys::Array::new();
            if ledger == before {
                for _ in 0..4 {
                    result.push(&"".into());
                }
            } else {
                let payload = serde_json::to_string(&ledger).map_err(|_| denied())?;
                if payload.len() > 524288 {
                    return Err(denied());
                }
                let audit=serde_json::to_string(&serde_json::json!({"revision":ledger.revision(),"state_digest":ledger.digest().map_err(|_|denied())?,"epoch":ledger.fence().epoch().to_string(),"disabled":ledger.fence().disabled()})).map_err(|_|denied())?;
                for value in [
                    payload,
                    serde_json::to_string(&ledger.fence()).map_err(|_| denied())?,
                    ledger.revision().0.to_string(),
                    audit,
                ] {
                    result.push(&value.into());
                }
            }
            returned = Some(ledger);
            Ok(result)
        };
        let closure = Closure::borrow_mut(&mut callback);
        transition(self.state, closure.as_ref().unchecked_ref())?;
        drop(closure);
        returned.ok_or_else(denied)
    }
    pub async fn activate(
        &self,
        signed: SignedRecoveryActivationV1,
    ) -> core::result::Result<RecoveryFenceLedgerV1, JsValue> {
        let configured = policy(self.env)?;
        let ledger = self.snapshot(&configured)?;
        let mut verified = Verified(Vec::new());
        if let Some(old) = ledger.original(&signed.manifest.operation_id) {
            if old.signed != signed {
                return Err(denied());
            }
        } else {
            let digest = ledger.digest().map_err(|_| denied())?;
            let checks = signed
                .signature_checks(&configured, &ledger.current(&digest), now()?)
                .map_err(|_| denied())?;
            for check in checks {
                crate::approval_crypto::verify_ed25519(
                    &check.public_key,
                    &check.signature,
                    &check.bytes,
                )
                .await
                .map_err(|_| denied())?;
                verified.0.push(check);
            }
        }
        self.update(|ledger| {
            ledger
                .activate(signed.clone(), now()?, &mut verified)
                .map_err(|_| denied())?;
            Ok(())
        })
    }
    pub fn disable(&self) -> core::result::Result<RecoveryFenceLedgerV1, JsValue> {
        self.snapshot(&policy(self.env)?)?;
        self.update(|ledger| ledger.disable().map_err(|_| denied()))
    }
}
