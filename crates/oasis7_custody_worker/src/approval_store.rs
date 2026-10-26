//! Internal component only. No production route or JavaScript export enables approval authority.
use crate::approval_crypto::{verification_nonce, verify_ed25519};
use js_sys::Array;
use oasis7_custody_core::{
    DecimalU64,
    approval::SignedApprovalVerificationV1,
    approval_admission::{
        ApprovalAdmissionBook, ApprovalAdmissionContext, ApprovalAdmissionError,
        ApprovalAdmissionInput, ApprovalAdmissionRecord, ApprovalSignatureVerifier,
        ExactSignatureChecks,
    },
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, prelude::*};

const KEY: &str = "owner-approval-admission-v1";
pub const MAX_APPROVAL_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
type Result<T> = std::result::Result<T, JsValue>;

#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = initializeTextRecord)]
    fn initialize(state: &JsValue, key: &str, initial: &str) -> Result<String>;
    #[wasm_bindgen(catch, js_name = readTextRecord)]
    fn read(state: &JsValue, key: &str) -> Result<String>;
    #[wasm_bindgen(catch, js_name = updateAuditedTextRecord)]
    fn update(state: &JsValue, key: &str, transition: &js_sys::Function) -> Result<String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalSnapshot {
    pub context: ApprovalAdmissionContext,
    pub book: ApprovalAdmissionBook,
    pub audit_sequence: DecimalU64,
}
fn error(value: impl std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("approval storage: {value:?}"))
}
fn decode(value: &str) -> Result<ApprovalSnapshot> {
    if value.len() > MAX_APPROVAL_SNAPSHOT_BYTES {
        return Err(error("snapshot capacity"));
    }
    serde_json::from_str(value).map_err(error)
}
fn encode(value: &ApprovalSnapshot) -> Result<String> {
    let value = serde_json::to_string(value).map_err(error)?;
    if value.len() > MAX_APPROVAL_SNAPSHOT_BYTES {
        return Err(error("snapshot capacity"));
    }
    Ok(value)
}
pub(crate) fn now() -> Result<u64> {
    let value = js_sys::Date::now();
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > 9_007_199_254_740_991.0
    {
        return Err(error("invalid trusted clock"));
    }
    Ok(value as u64)
}
pub(crate) struct ExactCache(ExactSignatureChecks);
impl ExactCache {
    pub(crate) fn verify_nonce(&self) -> &str {
        self.0.verify_nonce()
    }
}
impl ApprovalSignatureVerifier for ExactCache {
    fn verify(
        &mut self,
        checks: &ExactSignatureChecks,
    ) -> std::result::Result<(), ApprovalAdmissionError> {
        if checks == &self.0 {
            Ok(())
        } else {
            Err(ApprovalAdmissionError::SignatureRejected)
        }
    }
}
pub(crate) async fn verify(checks: ExactSignatureChecks) -> Result<ExactCache> {
    for check in checks.checks() {
        verify_ed25519(
            check.public_key_hex(),
            check.signature_base64url(),
            check.canonical_bytes(),
        )
        .await
        .map_err(error)?;
    }
    Ok(ExactCache(checks))
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum AuditStage {
    Pending,
    Consumed,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ApprovalAudit<'a> {
    sequence: DecimalU64,
    operation_id: &'a str,
    approval_id: &'a str,
    stage: AuditStage,
    service_epoch: DecimalU64,
}

pub struct OwnerApprovalStore {
    state: JsValue,
}
impl OwnerApprovalStore {
    /// Context must come from trusted configuration/current source state, never request JSON.
    pub fn initialize(state: JsValue, context: ApprovalAdmissionContext) -> Result<Self> {
        let initial = encode(&ApprovalSnapshot {
            context,
            book: ApprovalAdmissionBook::default(),
            audit_sequence: DecimalU64(0),
        })?;
        decode(&initialize(&state, KEY, &initial)?)?;
        Ok(Self { state })
    }
    pub fn snapshot(&self) -> Result<ApprovalSnapshot> {
        decode(&read(&self.state, KEY)?)
    }

    fn transaction<F, C, T>(
        &self,
        context: &F,
        clock: &C,
        mut transition: T,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<ApprovalAdmissionContext>,
        C: Fn() -> Result<u64>,
        T: FnMut(
            &mut ApprovalAdmissionBook,
            &ApprovalAdmissionContext,
            u64,
        ) -> Result<ApprovalAdmissionRecord>,
    {
        let mut returned = None;
        let mut callback = |old: String| -> Result<Array> {
            let mut snapshot = decode(&old)?;
            let current = context()?;
            let record = transition(&mut snapshot.book, &current, clock()?)?;
            // Comparing book first ensures retries do not mutate context, sequence, or audit.
            let previous = decode(&old)?;
            let result = Array::new();
            if snapshot.book == previous.book {
                result.push(&old.into());
                result.push(&"".into());
                result.push(&"".into());
            } else {
                snapshot.context = current;
                snapshot.audit_sequence = DecimalU64(
                    snapshot
                        .audit_sequence
                        .0
                        .checked_add(1)
                        .ok_or_else(|| error("audit sequence overflow"))?,
                );
                let next = encode(&snapshot)?;
                let audit = serde_json::to_string(&ApprovalAudit {
                    sequence: snapshot.audit_sequence,
                    operation_id: &record.input().operation_id,
                    approval_id: &record.input().approval.proof.approval_id,
                    stage: match record.stage() {
 oasis7_custody_core::approval_admission::ApprovalAdmissionStage::Pending => AuditStage::Pending,
 oasis7_custody_core::approval_admission::ApprovalAdmissionStage::Consumed => AuditStage::Consumed,
},
                    service_epoch: snapshot.context.authority.service_epoch,
                })
                .map_err(error)?;
                result.push(&next.into());
                result.push(&snapshot.audit_sequence.0.to_string().into());
                result.push(&audit.into());
            }
            returned = Some(record);
            Ok(result)
        };
        let closure = Closure::borrow_mut(&mut callback);
        // The SQL API is strictly synchronous; this scoped closure cannot escape the call.
        let function = closure.as_ref().unchecked_ref();
        update(&self.state, KEY, function)?;
        drop(closure);
        returned.ok_or_else(|| error("missing transition result"))
    }

    pub async fn begin<F>(
        &self,
        input: ApprovalAdmissionInput,
        context: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<ApprovalAdmissionContext>,
    {
        self.begin_with_clock(input, context, now).await
    }
    /// Trusted clock injection for component verification; never deserialize a clock from a request.
    pub async fn begin_with_clock<F, C>(
        &self,
        input: ApprovalAdmissionInput,
        context: F,
        clock: C,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<ApprovalAdmissionContext>,
        C: Fn() -> Result<u64>,
    {
        let nonce = verification_nonce().map_err(error)?;
        // One bounded retry handles a concurrent same-body begin storing its original nonce.
        for attempt in 0..2 {
            let snapshot = self.snapshot()?;
            let current = context()?;
            let checks = snapshot
                .book
                .begin_checks(&input, &nonce, &current, clock()?)
                .map_err(error)?;
            let mut cache = verify(checks).await?;
            let result = self.transaction(&context, &clock, |book, current, time| {
                book.begin(input.clone(), nonce.clone(), current, time, &mut cache)
                    .map_err(error)
            });
            if result.is_ok() || attempt == 1 {
                return result;
            }
            let latest = self.snapshot()?;
            let concurrent_original =
                latest
                    .book
                    .original(&input.operation_id)
                    .is_some_and(|record| {
                        record.input() == &input && record.verify_nonce() != cache.0.verify_nonce()
                    });
            if !concurrent_original {
                return result;
            }
        }
        unreachable!()
    }
    pub async fn commit<F>(
        &self,
        id: &str,
        verification: SignedApprovalVerificationV1,
        context: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<ApprovalAdmissionContext>,
    {
        self.commit_with_clock(id, verification, context, now).await
    }
    pub async fn commit_with_clock<F, C>(
        &self,
        id: &str,
        verification: SignedApprovalVerificationV1,
        context: F,
        clock: C,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<ApprovalAdmissionContext>,
        C: Fn() -> Result<u64>,
    {
        let snapshot = self.snapshot()?;
        let current = context()?;
        let checks = snapshot
            .book
            .commit_checks(id, &verification, &current, clock()?)
            .map_err(error)?;
        let mut cache = verify(checks).await?;
        self.transaction(&context, &clock, |book, current, time| {
            book.commit(id, verification.clone(), current, time, &mut cache)
                .map_err(error)
        })
    }
}
