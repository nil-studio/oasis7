//! Public test seed and synthetic authority only; excluded from every production role artifact.
use crate::approval_store::OwnerApprovalStore;
use oasis7_custody_core::{DecimalU64, approval::*, approval_admission::*};
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue};
use worker::*;

fn failure(value: impl std::fmt::Debug) -> Error {
    Error::RustError(format!("fixture: {value:?}"))
}
fn sql(state: &JsValue, query: &str) -> Result<JsValue> {
    let storage = js_sys::Reflect::get(state, &"storage".into()).map_err(failure)?;
    let sql = js_sys::Reflect::get(&storage, &"sql".into()).map_err(failure)?;
    let exec: js_sys::Function = js_sys::Reflect::get(&sql, &"exec".into())
        .map_err(failure)?
        .unchecked_into();
    exec.call1(&sql, &query.into()).map_err(failure)
}
fn rows(state: &JsValue, query: &str) -> Result<Value> {
    let cursor = sql(state, query)?;
    let function: js_sys::Function = js_sys::Reflect::get(&cursor, &"toArray".into())
        .map_err(failure)?
        .unchecked_into();
    let rows = function.call0(&cursor).map_err(failure)?;
    let text = js_sys::JSON::stringify(&rows)
        .map_err(failure)?
        .as_string()
        .ok_or_else(|| failure("rows"))?;
    Ok(serde_json::from_str(&text)?)
}
fn prepared() -> Result<(ApprovalAdmissionInput, ApprovalAdmissionContext)> {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/artifacts/approval-fixture.json"
    )))?;
    let mut input: ApprovalAdmissionInput = serde_json::from_value(fixture["input"].clone())?;
    let mut context: ApprovalAdmissionContext = serde_json::from_value(fixture["context"].clone())?;
    let now = js_sys::Date::now() as u64;
    context.source.session_expires_at = DecimalU64(now + 120_000);
    context.source.session_idle_expires_at = DecimalU64(now + 100_000);
    context.handoff_until = DecimalU64(now + 40_000);
    if let CustodyIntentV1::ManagedLogin { deadline, .. } = &mut input.request.intent {
        *deadline = DecimalU64(now + 100_000);
    }
    input.approval.proof.intent_digest = input.request.intent_digest().map_err(failure)?;
    input.approval.proof.issued_at = DecimalU64(now);
    input.approval.proof.expires_at = DecimalU64(now + 30_000);
    Ok((input, context))
}

#[durable_object]
pub struct TestApprovalAuthority {
    state: JsValue,
}
impl DurableObject for TestApprovalAuthority {
    fn new(state: State, _env: Env) -> Self {
        Self {
            state: state._inner().into(),
        }
    }
    async fn fetch(&self, mut request: Request) -> Result<Response> {
        let path = request.path();
        if path == "/crypto" {
            let body: Value = request.json().await?;
            let bytes: Vec<u8> = serde_json::from_value(body["bytes"].clone())?;
            let valid = crate::approval_crypto::verify_ed25519(
                body["key"].as_str().unwrap_or(""),
                body["signature"].as_str().unwrap_or(""),
                &bytes,
            )
            .await
            .is_ok();
            return Response::from_json(&json!({"valid": valid}));
        }
        if path == "/prepare" {
            let (input, context) = prepared()?;
            OwnerApprovalStore::initialize(self.state.clone(), context).map_err(failure)?;
            return Response::from_json(
                &json!({ "input": input, "canonical_bytes": input.approval.proof.signing_bytes().map_err(failure)? }),
            );
        }
        // The context is a trusted fixture authority; request DTOs cannot replace it.
        let (_, initial) = prepared()?;
        let store = OwnerApprovalStore::initialize(self.state.clone(), initial).map_err(failure)?;
        if path == "/fault" {
            // Abort after audit INSERT, proving the real SQLite transaction rolls both writes back.
            sql(
                &self.state,
                "CREATE TRIGGER fixture_abort_update BEFORE UPDATE ON custody_records BEGIN SELECT RAISE(ABORT, 'fixture rollback'); END",
            )?;
            return Response::from_json(&json!({"ok": true}));
        }
        if path == "/clear-fault" {
            sql(&self.state, "DROP TRIGGER fixture_abort_update")?;
            return Response::from_json(&json!({"ok": true}));
        }
        if path == "/audit" {
            let tables = rows(
                &self.state,
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'custody_audit'",
            )?;
            if tables.as_array().is_some_and(|v| v.is_empty()) {
                return Response::from_json(&json!([]));
            }
            return Response::from_json(&rows(
                &self.state,
                "SELECT sequence, payload FROM custody_audit ORDER BY CAST(sequence AS INTEGER)",
            )?);
        }
        let result: std::result::Result<Value, JsValue> = async {
            let reads = std::cell::Cell::new(0_u8);
            let current = || {
                reads.set(reads.get() + 1);
                let mut context = store.snapshot()?.context;
                if reads.get() > 1 {
                    if path.ends_with("-expired") { context.source.session_idle_expires_at = DecimalU64(1); }
                    if path.ends_with("-frozen") { context.frozen = true; }
                    if path.ends_with("-pin") { context.native_key_pins.insert("native-one".into(), "02".repeat(32)); }
                }
                Ok(context)
            };
            let clock_reads = std::cell::Cell::new(0_u8);
            let clock = || {
                clock_reads.set(clock_reads.get() + 1);
                let time = js_sys::Date::now() as u64;
                Ok(if path.ends_with("-clock") && clock_reads.get() > 1 { time + 40_000 } else { time })
            };
            match path.as_str() {
                "/begin" | "/begin-expired" | "/begin-frozen" | "/begin-pin" | "/begin-clock" => {
                    let input: ApprovalAdmissionInput = request.json().await.map_err(|e| JsValue::from_str(&e.to_string()))?;
                    serde_json::to_value(store.begin_with_clock(input, current, clock).await?).map_err(|e| JsValue::from_str(&e.to_string()))
                }
                "/verification" => {
                    let snapshot = store.snapshot()?;
                    let record = snapshot.book.original("operation-one").ok_or_else(|| JsValue::from_str("missing original"))?;
                    let req = record.native_verify_request().map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
                    let proof = &req.approval.proof;
                    let context = snapshot.context;
                    let source = context.source;
                    let verification = ApprovalVerificationV1 {
                        version: 1, key_id: "native-one".into(), identity_namespace: context.identity_namespace,
                        purpose: req.purpose, proof_digest: req.proof_digest,
                        approval_request_id: proof.approval_request_id.clone(), audience: req.expected_audience,
                        intent_digest: req.expected_intent_digest, verify_nonce: req.verify_nonce,
                        manager_security_version: source.manager_security_version,
                        manager_policy_version: source.manager_policy_version,
                        origin_credential_version: source.origin_credential_version,
                        identity_instance_epoch: context.identity_instance_epoch,
                        credential_version: source.passkey_credential_version,
                        observed_at: DecimalU64(js_sys::Date::now() as u64), proof_expires_at: proof.expires_at,
                        source_session_expires_at: source.session_expires_at,
                        source_session_idle_expires_at: source.session_idle_expires_at,
                        proof_state: ProofStateV1::Active, origin_state: OriginStateV1::Active,
                    };
                    Ok(json!({ "canonical_bytes": verification.signing_bytes().map_err(|e| JsValue::from_str(&format!("{e:?}")))?, "verification": verification }))
                }
                "/commit" | "/commit-expired" | "/commit-frozen" | "/commit-pin" | "/commit-clock" => {
                    let verification: SignedApprovalVerificationV1 = request.json().await.map_err(|e| JsValue::from_str(&e.to_string()))?;
                    serde_json::to_value(store.commit_with_clock("operation-one", verification, current, clock).await?).map_err(|e| JsValue::from_str(&e.to_string()))
                }
                "/snapshot" => serde_json::to_value(store.snapshot()?).map_err(|e| JsValue::from_str(&e.to_string())),
                _ => Err(JsValue::from_str("unknown fixture operation")),
            }
        }.await;
        match result {
            Ok(value) => Response::from_json(&value),
            Err(_) => Ok(Response::from_json(&json!({"rejected": true}))?.with_status(409)),
        }
    }
}
