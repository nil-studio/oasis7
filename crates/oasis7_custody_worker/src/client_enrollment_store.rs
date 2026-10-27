//! Conditional enrollment SQL; public proofs never touch Account/TestSession budgets.
use crate::client_enrollment_authority::*;
use js_sys::Array;
use oasis7_custody_core::{
    DecimalU64, approval_admission::ApprovalSignatureVerifier, client_enrollment::*, http_pop::*,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, prelude::*};
use worker::Env;
const KEY: &str = "custody-client-enrollment-v1";
const ABUSE: &str = "custody-enrollment-abuse-v1";
const MAX: usize = 512 * 1024;
#[wasm_bindgen(module = "/platform/client_enrollment_sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=enrollmentTransition)]
    fn update(state: &JsValue, key: &str, callback: &js_sys::Function) -> Result<String, JsValue>;
    #[wasm_bindgen(catch,js_name=enrollmentAbuseTransition)]
    fn abuse_update(
        state: &JsValue,
        key: &str,
        callback: &js_sys::Function,
    ) -> Result<String, JsValue>;
    #[wasm_bindgen(catch,js_name=readEnrollment)]
    fn read(state: &JsValue, key: &str) -> Result<JsValue, JsValue>;
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    records: Vec<Row>,
    table_names: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: String,
    payload: String,
}
fn inventory(raw: &str, key: &str, old: Option<&str>) -> Result<(), JsValue> {
    if raw.len() > 6 * MAX {
        return Err(denied());
    }
    let v: Inventory = serde_json::from_str(raw).map_err(|_| denied())?;
    if v.records.len() != usize::from(old.is_some())
        || v.records
            .iter()
            .any(|r| r.id != key || r.payload.len() > MAX || Some(r.payload.as_str()) != old)
        || (key == ABUSE && v.table_names.iter().any(|n| n == "table:custody_audit"))
        || v.table_names.len() >= 16
        || v.table_names.iter().any(|n| {
            !matches!(
                n.as_str(),
                "table:custody_records"
                    | "table:custody_audit"
                    | "table:sqlite_sequence"
                    | "table:_cf_METADATA"
            )
        })
    {
        return Err(denied());
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    schema_version: u8,
    audit_sequence: DecimalU64,
    state: ClientEnrollmentStateV1,
}
fn decode(raw: &str) -> Result<Stored, JsValue> {
    if raw.is_empty() || raw.len() > MAX {
        return Err(denied());
    }
    let s: Stored = serde_json::from_str(raw).map_err(|_| denied())?;
    if s.schema_version != 1 || s.audit_sequence.0 == 0 {
        return Err(denied());
    }
    Ok(s)
}
fn encode<T: Serialize>(s: &T) -> Result<String, JsValue> {
    let raw = serde_json::to_string(s).map_err(|_| denied())?;
    if raw.len() > MAX {
        return Err(denied());
    }
    Ok(raw)
}
fn optional(v: JsValue) -> Result<Option<String>, JsValue> {
    if v.is_null() {
        Ok(None)
    } else {
        v.as_string().map(Some).ok_or_else(denied)
    }
}
fn result(raw: String, sequence: u64, event: &str, id: &str) -> Result<Array, JsValue> {
    let a = Array::new();
    a.push(&raw.into());
    a.push(&sequence.to_string().into());
    a.push(
        &serde_json::to_string(&(sequence, event, id))
            .map_err(|_| denied())?
            .into(),
    );
    Ok(a)
}
fn physical(s: &ClientEnrollmentStateV1, p: &Physical, id: &str) -> Result<(), JsValue> {
    p.validate()?;
    let r = s.registration();
    if r.enrollment_id != id
        || r.physical_actor_id != p.actual_actor_id
        || r.realm != p.actual_realm
        || r.public_origin != p.origin
    {
        return Err(denied());
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HeaderPair {
    pub name: String,
    pub value: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProveInput {
    pub enrollment_id: String,
    pub body: String,
    pub headers: Vec<HeaderPair>,
}
fn prepared(
    s: &ClientEnrollmentStateV1,
    input: &ProveInput,
    time: oasis7_custody_core::time_units::UnixMillis,
) -> Result<PreparedHttpPop, JsValue> {
    let r = s.registration();
    let body: ClientEnrollmentProofRequestV1 =
        serde_json::from_str(&input.body).map_err(|_| denied())?;
    if input.body.len() > 32768 || body.enrollment_id != input.enrollment_id {
        return Err(denied());
    }
    let headers: Vec<_> = input
        .headers
        .iter()
        .map(|h| HttpHeader {
            name: &h.name,
            value: &h.value,
        })
        .collect();
    let target = CanonicalHttpTarget::new(
        &r.public_origin,
        HttpPopRoute::EnrollmentProve(r.enrollment_id.clone()),
    )
    .map_err(|_| denied())?;
    let empty: [u8; 32] = hex::decode(oasis7_custody_core::approval::bytes_digest(&[]))
        .map_err(|_| denied())?
        .try_into()
        .map_err(|_| denied())?;
    prepare_http_pop(
        &HttpPopMessage {
            method: "POST",
            target_uri: target.uri(),
            headers: &headers,
            representation: HttpHeaderRepresentation::WorkerMergedSingletons,
            body: input.body.as_bytes(),
        },
        &HttpPopBinding {
            profile: HttpPopProfileV1::EnrollmentProof,
            target: &target,
            key_id: &r.key_id,
            public_key_hex: &r.request.descriptor.client_public_key,
            binding_id: &r.enrollment_id,
            request_id: &body.request_id,
            nonce: Some(&r.server_nonce),
            credential_digest: &empty,
            valid_until: r.challenge_expires_at,
        },
        time,
    )
    .map_err(|error| {
        #[cfg(feature = "native-runtime-harness")]
        worker::console_warn!(
            "enrollment.prove.http_pop.{}",
            match error {
                HttpPopError::Bounds => "Bounds",
                HttpPopError::Target => "Target",
                HttpPopError::Header => "Header",
                HttpPopError::Binding => "Binding",
                HttpPopError::Digest => "Digest",
                HttpPopError::Time => "Time",
                HttpPopError::Signature => "Signature",
                HttpPopError::Crypto => "Crypto",
            }
        );
        #[cfg(not(feature = "native-runtime-harness"))]
        let _ = error;
        denied()
    })
}
pub(crate) fn create(
    state: &JsValue,
    env: &Env,
    actor: &str,
    realm: &str,
    raw: &str,
) -> Result<ClientEnrollmentStateV1, JsValue> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let request: ClientEnrollmentCreateRequestV1 =
        serde_json::from_str(raw).map_err(|_| denied())?;
    request.validate().map_err(|_| denied())?;
    let c = Config::enabled(env)?;
    let _pin = c
        .deployments
        .configured_metadata(&request.descriptor)
        .map_err(|_| denied())?;
    let id = c.enrollment_id(&request)?;
    let key = format!(
        "client-{}",
        hash(
            "oasis7.custody.enrollment-key-id.v1",
            &request.descriptor.client_public_key
        )?
    );
    let nonce = nonce()?;
    let mut returned = None;
    let mut callback = |old: JsValue, inv: String| -> Result<Array, JsValue> {
        let old = optional(old)?;
        inventory(&inv, KEY, old.as_deref())?;
        let p = Physical::for_id(env, &id, actor, realm)?;
        p.validate()?;
        let s = if let Some(stored) = &old {
            let s = decode(stored)?;
            physical(&s.state, &p, &id)?;
            s
        } else {
            Stored {
                schema_version: 1,
                audit_sequence: DecimalU64(1),
                state: ClientEnrollmentStateV1::new(
                    raw.as_bytes(),
                    EnrollmentServerIds {
                        enrollment_id: &id,
                        key_id: &key,
                        nonce: &nonce,
                    },
                    &p.core(),
                    now()?,
                )
                .map_err(|_| denied())?,
            }
        };
        // The original request bytes, not a parsed/re-encoded JSON equivalent, bind retries.
        s.state
            .check_create_retry(raw.as_bytes())
            .map_err(|_| denied())?;
        let encoded = encode(&s)?;
        returned = Some(s.state.clone());
        result(encoded, s.audit_sequence.0, "enrollment_created", &id)
    };
    let closure = Closure::borrow_mut(&mut callback);
    update(state, KEY, closure.as_ref().unchecked_ref())?;
    drop(closure);
    returned.ok_or_else(denied)
}
pub(crate) async fn prove(
    state: &JsValue,
    env: &Env,
    actor: &str,
    realm: &str,
    input: ProveInput,
) -> Result<ProvedEnrollmentFactV1, JsValue> {
    Config::enabled(env)?;
    let initial =
        optional(read(state, KEY).map_err(|e| diagnostic("read", e))?)?.ok_or_else(denied)?;
    let saved = decode(&initial).map_err(|e| diagnostic("decode", e))?;
    physical(
        &saved.state,
        &Physical::for_id(env, &input.enrollment_id, actor, realm)?,
        &input.enrollment_id,
    )
    .map_err(|e| diagnostic("physical", e))?;
    let proof = prepared(&saved.state, &input, now()?).map_err(|e| diagnostic("prepare", e))?;
    let mut cache = verify(proof.signature_checks().map_err(|_| denied())?)
        .await
        .map_err(|e| diagnostic("precrypto", e))?;
    let mut returned = None;
    let mut callback = |old: JsValue, inv: String| -> Result<Array, JsValue> {
        let old = optional(old)?.ok_or_else(denied)?;
        inventory(&inv, KEY, Some(&old)).map_err(|e| diagnostic("finalinventory", e))?;
        let mut s = decode(&old)?;
        let before = s.state.clone();
        let p = Physical::for_id(env, &input.enrollment_id, actor, realm)?;
        physical(&s.state, &p, &input.enrollment_id)?;
        Config::enabled(env)?;
        let time = now()?;
        let proof = prepared(&s.state, &input, time).map_err(|e| diagnostic("finalprepare", e))?;
        let receipt = verify_http_possession(proof, time, &mut cache)
            .map_err(|_| diagnostic("verifyReceipt", denied()))?;
        let fact = s
            .state
            .prove(input.body.as_bytes(), receipt, &p.core(), time)
            .map_err(|_| diagnostic("stateProve", denied()))?;
        if s.state != before {
            s.audit_sequence = DecimalU64(s.audit_sequence.0.checked_add(1).ok_or_else(denied)?);
        }
        let raw = if s.state == before { old } else { encode(&s)? };
        returned = Some(fact);
        result(
            raw,
            s.audit_sequence.0,
            "enrollment_proved",
            &input.enrollment_id,
        )
    };
    let closure = Closure::borrow_mut(&mut callback);
    update(state, KEY, closure.as_ref().unchecked_ref()).map_err(|e| diagnostic("update", e))?;
    drop(closure);
    returned.ok_or_else(denied)
}
pub(crate) async fn fact(
    state: &JsValue,
    env: &Env,
    actor: &str,
    realm: &str,
    id: &str,
) -> Result<ProvedEnrollmentFactV1, JsValue> {
    let s = decode(&optional(read(state, KEY)?)?.ok_or_else(denied)?)?;
    physical(&s.state, &Physical::for_id(env, id, actor, realm)?, id)?;
    let fact = s.state.proved().cloned().ok_or_else(denied)?;
    let mut cache = verify(fact.checks().map_err(|_| denied())?).await?;
    let mut returned = None;
    let mut callback = |old: JsValue, inv: String| -> Result<Array, JsValue> {
        let raw = optional(old)?.ok_or_else(denied)?;
        inventory(&inv, KEY, Some(&raw))?;
        let s = decode(&raw)?;
        physical(&s.state, &Physical::for_id(env, id, actor, realm)?, id)?;
        let current = s.state.proved().ok_or_else(denied)?;
        if current != &fact {
            return Err(denied());
        }
        cache
            .verify(&current.checks().map_err(|_| denied())?)
            .map_err(|_| denied())?;
        returned = Some(current.clone());
        result(raw, s.audit_sequence.0, "enrollment_fact_read", id)
    };
    let closure = Closure::borrow_mut(&mut callback);
    update(state, KEY, closure.as_ref().unchecked_ref())?;
    drop(closure);
    returned.ok_or_else(denied)
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AbuseKind {
    Create,
    Prove,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AbuseScope {
    Realm,
    Provider,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AbuseRequest {
    pub bucket: String,
    pub kind: AbuseKind,
    pub scope: AbuseScope,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AbuseState {
    schema_version: u8,
    realm: String,
    bucket: String,
    scope: AbuseScope,
    window: u64,
    last_observed: u64,
    creates: u32,
    proves: u32,
}
pub(crate) fn charge_abuse(
    state: &JsValue,
    env: &Env,
    actor: &str,
    realm: &str,
    r: &AbuseRequest,
) -> Result<(), JsValue> {
    let c = Config::enabled(env)?;
    let expected = env
        .durable_object(BINDING)
        .map_err(|_| denied())?
        .id_from_name(&c.bucket_name(&r.bucket)?)
        .map_err(|_| denied())?
        .to_string();
    if actor != expected || realm != c.realm {
        return Err(denied());
    }
    let mut callback = |old: JsValue, inv: String| -> Result<String, JsValue> {
        let old = optional(old)?;
        inventory(&inv, ABUSE, old.as_deref())?;
        let c = Config::enabled(env)?;
        if c.realm != realm {
            return Err(denied());
        }
        let time = now()?.get();
        let window = time / 60_000;
        let mut s = if let Some(raw) = old {
            let s: AbuseState = serde_json::from_str(&raw).map_err(|_| denied())?;
            if s.schema_version != 1
                || s.realm != realm
                || s.bucket != r.bucket
                || s.scope != r.scope
                || time < s.last_observed
                || s.window != s.last_observed / 60_000
            {
                return Err(denied());
            }
            s
        } else {
            AbuseState {
                schema_version: 1,
                realm: realm.into(),
                bucket: r.bucket.clone(),
                scope: r.scope,
                window,
                last_observed: time,
                creates: 0,
                proves: 0,
            }
        };
        if s.window != window {
            s.window = window;
            s.creates = 0;
            s.proves = 0;
        }
        let (used, max) = match (r.kind, r.scope) {
            (AbuseKind::Create, AbuseScope::Provider) => (&mut s.creates, 8),
            (AbuseKind::Prove, AbuseScope::Provider) => (&mut s.proves, 32),
            (AbuseKind::Create, AbuseScope::Realm) => (&mut s.creates, 256),
            (AbuseKind::Prove, AbuseScope::Realm) => (&mut s.proves, 1024),
        };
        if *used >= max {
            return Err(denied());
        }
        *used += 1;
        s.last_observed = time;
        encode(&s)
    };
    let closure = Closure::borrow_mut(&mut callback);
    abuse_update(state, ABUSE, closure.as_ref().unchecked_ref())?;
    Ok(())
}

fn diagnostic(stage: &str, error: JsValue) -> JsValue {
    #[cfg(feature = "native-runtime-harness")]
    worker::console_warn!("enrollment.prove.{}", stage);
    #[cfg(not(feature = "native-runtime-harness"))]
    let _ = stage;
    error
}
