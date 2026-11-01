//! Identity-local versioned once rows, separate from legacy manifest schema1.
//! A SQL row or snapshot never reconstructs an outbound/observed capability.
use oasis7_custody_core::{
    approval::bytes_digest, execution_private_current_v2::*, native_execution_prepare_v2::*,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
fn rejected() -> JsValue {
    JsValue::from_str("execution context stage rejected")
}
type Result<T> = std::result::Result<T, JsValue>;
#[wasm_bindgen(module = "/platform/execution_context_stage_sql_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readExecutionContextStagesV2)]
    fn read(state: &JsValue) -> Result<String>;
    #[wasm_bindgen(catch,js_name=executionContextActorMatchesV2)]
    fn physical(state: &JsValue, env: &worker::Env, name: &str) -> Result<bool>;
    #[wasm_bindgen(catch,js_name=readExecutionContextStageRowV2)]
    fn read_row(state: &JsValue, operation: &str) -> Result<String>;
    #[wasm_bindgen(catch,js_name=claimExecutionContextStageV2)]
    fn claim(state: &JsValue, raw: &str) -> Result<String>;
    #[wasm_bindgen(catch,js_name=completeExecutionPrivateCurrentV2)]
    fn complete_private(state: &JsValue, op: &str, proof: &str, result: &str) -> Result<String>;
    #[wasm_bindgen(catch,js_name=claimExecutionContextEdgeV2)]
    fn claim_edge(
        state: &JsValue,
        op: &str,
        proof: &str,
        edge: u8,
        attempt: &str,
        receipt: &str,
    ) -> Result<()>;
    #[wasm_bindgen(catch,js_name=recordExecutionNativeCompletionV2)]
    fn native_complete(state: &JsValue, op: &str, proof: &str, receipts: &JsValue) -> Result<()>;
    #[wasm_bindgen(catch,js_name=readExecutionDispatchReceiptsV2)]
    fn read_receipts(state: &JsValue, op: &str) -> Result<String>;
    #[wasm_bindgen(catch,js_name=persistExecutionContextPutV2)]
    fn persist_put(
        state: &JsValue,
        op: &str,
        proof: &str,
        attempt: &str,
        sealed: &str,
        receipt: &str,
    ) -> Result<()>;
    #[wasm_bindgen(catch,js_name=completeExecutionContextPutV2)]
    fn complete_put(state: &JsValue, op: &str, proof: &str, result: &str) -> Result<String>;
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mode {
    api_version: u8,
    actor_id: String,
    actor_tag: String,
    issuer_namespace: String,
    issuer_service: String,
    environment: String,
    issuer_epoch: String,
    config_generation: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    operation_id: String,
    profile: String,
    proof_digest: String,
    scope_digest: String,
    payload: String,
    next_edge: u8,
    sealed: Option<String>,
    completed: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edge {
    attempt_id: String,
    operation_id: String,
    edge: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    mode: Option<Mode>,
    records: Vec<Row>,
    edges: Vec<Edge>,
}
fn hash<T: Serialize>(v: &T) -> Result<String> {
    Ok(bytes_digest(
        &oasis7_wasm_abi::encode_canonical_cbor(v).map_err(|_| rejected())?,
    ))
}
fn operation(profile: &str, session: &str, input: &str, stage: &str) -> Result<String> {
    Ok(format!(
        "execution-stage-v2:{}",
        hash(&(profile, session, input, stage))?
    ))
}
pub(crate) fn validate_snapshot(state: &JsValue) -> Result<()> {
    let s: Snapshot = serde_json::from_str(&read(state)?).map_err(|_| rejected())?;
    if s.records.len() > 512
        || s.edges.len() > 4096
        || s.mode.is_none() && (!s.records.is_empty() || !s.edges.is_empty())
    {
        return Err(rejected());
    }
    if let Some(m) = &s.mode {
        if m.api_version != 2
            || [
                &m.actor_id,
                &m.actor_tag,
                &m.issuer_namespace,
                &m.issuer_service,
                &m.environment,
            ]
            .iter()
            .any(|v| v.is_empty())
            || m.issuer_epoch.parse::<u64>().ok().is_none_or(|v| v == 0)
            || m.config_generation
                .parse::<u64>()
                .ok()
                .is_none_or(|v| v == 0)
        {
            return Err(rejected());
        }
    }
    for header in &s.records {
        let r: Row = serde_json::from_str(&read_row(state, &header.operation_id)?)
            .map_err(|_| rejected())?;
        if r.operation_id != header.operation_id
            || r.profile != header.profile
            || r.proof_digest != header.proof_digest
            || r.scope_digest != header.scope_digest
            || r.next_edge != header.next_edge
            || r.sealed.is_some() != header.sealed.is_some()
            || r.completed.is_some() != header.completed.is_some()
        {
            return Err(rejected());
        }
        let (expected, scope) = match r.profile.as_str() {
            "private_current_v2" => {
                let p: SignedAccountExecutionPrivateCurrentV2 =
                    serde_json::from_str(&r.payload).map_err(|_| rejected())?;
                p.wire_bytes().map_err(|_| rejected())?;
                mode_matches(
                    s.mode.as_ref(),
                    &p.certificate.issuer,
                    &p.certificate.session,
                )?;
                if p.digest().map_err(|_| rejected())? != r.proof_digest
                    || r.next_edge != 2
                    || r.sealed.is_some()
                {
                    return Err(rejected());
                }
                let actual: Vec<_> = s
                    .edges
                    .iter()
                    .filter(|e| e.operation_id == r.operation_id)
                    .collect();
                if actual.len() != 2
                    || (0..2).any(|i| {
                        !actual.iter().any(|e| {
                            usize::from(e.edge) == i
                                && e.attempt_id == p.certificate.ports[i].physical_id
                        })
                    })
                {
                    return Err(rejected());
                }
                if let Some(raw) = &r.completed {
                    let reply: ExecutionPrivateCurrentReplyV2 =
                        serde_json::from_str(raw).map_err(|_| rejected())?;
                    if reply.api_version != 2
                        || reply.proof_digest != r.proof_digest
                        || reply.metadata.session_ref != p.certificate.session.session_ref
                        || reply.metadata.private_sid_generation
                            != p.certificate.session.private_sid_generation
                        || reply.metadata.lifetime_digest
                            != p.certificate
                                .lifetime
                                .lifetime
                                .digest()
                                .map_err(|_| rejected())?
                    {
                        return Err(rejected());
                    }
                }
                (
                    operation(
                        &r.profile,
                        &p.certificate.session_id,
                        &p.certificate.approved_input_digest,
                        &p.certificate.stage_operation_id,
                    )?,
                    hash(&p.certificate.session.scope)?,
                )
            }
            "prepare_context_v2" | "prepare_context_gated_v2" => {
                let q: NativeExecutionPrepareRequestV2 =
                    serde_json::from_str(&r.payload).map_err(|_| rejected())?;
                q.wire_bytes().map_err(|_| rejected())?;
                mode_matches(
                    s.mode.as_ref(),
                    &q.proof.certificate.issuer,
                    &q.proof.certificate.session,
                )?;
                if q.proof.digest().map_err(|_| rejected())? != r.proof_digest {
                    return Err(rejected());
                }
                if r.profile == "prepare_context_gated_v2" {
                    gated::validate_receipts(state, &q, &r)?;
                }
                let expected_indices: Vec<usize> = match r.next_edge {
                    2..=7 => (0..usize::from(r.next_edge)).collect(),
                    9 => (0..7).collect(),
                    10 => vec![0, 1, 2, 3, 4, 5, 6, 9],
                    _ => return Err(rejected()),
                };
                let actual: Vec<_> = s
                    .edges
                    .iter()
                    .filter(|e| e.operation_id == r.operation_id)
                    .collect();
                if actual.len() != expected_indices.len()
                    || expected_indices.iter().any(|i| {
                        !actual.iter().any(|e| {
                            usize::from(e.edge) == *i
                                && e.attempt_id == q.proof.certificate.ports[*i].attempt_id
                        })
                    })
                {
                    return Err(rejected());
                }
                if let Some(raw) = &r.sealed {
                    let (record, reference, receipt): (
                        oasis7_custody_core::context_body_v2::ContextBodyRecordV2,
                        oasis7_custody_core::context_body_v2::AttestedSlotRefV2,
                        SignedNativeExecutionPrepareReceiptV2,
                    ) = serde_json::from_str(raw).map_err(|_| rejected())?;
                    let expected = ExecutionPreparedPublicResultV2 {
                        api_version: 2,
                        proof_digest: r.proof_digest.clone(),
                        reference,
                        native_receipt: receipt,
                        storage: record.storage_fact().map_err(|_| rejected())?,
                    };
                    expected.validate_public(&q).map_err(|_| rejected())?;
                    if let Some(raw) = &r.completed {
                        let value: ExecutionPreparedPublicResultV2 =
                            serde_json::from_str(raw).map_err(|_| rejected())?;
                        if oasis7_wasm_abi::encode_canonical_cbor(&value).map_err(|_| rejected())?
                            != oasis7_wasm_abi::encode_canonical_cbor(&expected)
                                .map_err(|_| rejected())?
                        {
                            return Err(rejected());
                        }
                    }
                } else if r.completed.is_some() {
                    return Err(rejected());
                }
                (
                    operation(
                        &r.profile,
                        &q.proof.certificate.session_id,
                        &q.proof.certificate.approved_input_digest,
                        &q.proof.certificate.stage_operation_id,
                    )?,
                    hash(&q.proof.certificate.session.scope)?,
                )
            }
            _ => return Err(rejected()),
        };
        if r.operation_id != expected || r.scope_digest != scope {
            return Err(rejected());
        }
    }
    for e in s.edges {
        if e.attempt_id.is_empty()
            || e.edge > 9
            || !s.records.iter().any(|r| r.operation_id == e.operation_id)
        {
            return Err(rejected());
        }
    }
    Ok(())
}
#[derive(Serialize)]
struct Claim<'a, T: Serialize> {
    operation_id: &'a str,
    profile: &'a str,
    proof_digest: &'a str,
    scope_digest: String,
    payload: String,
    actor_tag: String,
    issuer_namespace: &'a str,
    issuer_service: &'a str,
    environment: &'a str,
    issuer_epoch: String,
    config_generation: String,
    first_ports: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    dispatch_receipt: Option<String>,
}
fn actor_tag(
    session: &oasis7_custody_core::provisioning::ParticipantSessionRefV1,
) -> Result<String> {
    let r = &session.scope.role_scope;
    hash(&(
        "execution-manifest-actor-v2",
        &r.context.identity.environment,
        &r.context.identity.account_id,
        &r.provision_id,
    ))
}
/// Only the freshly verified handler can construct this non-Serde token.
pub(crate) struct PrivateReadClaim {
    operation: String,
    proof: String,
}
pub(crate) fn claim_private_read(
    state: &JsValue,
    p: &SignedAccountExecutionPrivateCurrentV2,
) -> Result<PrivateReadClaim> {
    validate_snapshot(state)?;
    let c = &p.certificate;
    let op = operation(
        "private_current_v2",
        &c.session_id,
        &c.approved_input_digest,
        &c.stage_operation_id,
    )?;
    let proof = p.digest().map_err(|_| rejected())?;
    let raw = serde_json::to_string(&Claim {
        operation_id: &op,
        profile: "private_current_v2",
        proof_digest: &proof,
        scope_digest: hash(&c.session.scope)?,
        payload: serde_json::to_string(p).map_err(|_| rejected())?,
        actor_tag: actor_tag(&c.session)?,
        issuer_namespace: &c.issuer.namespace,
        issuer_service: &c.issuer.service_id,
        environment: &c.issuer.environment,
        issuer_epoch: c.issuer.issuer_epoch.0.to_string(),
        config_generation: c.issuer.config_generation.0.to_string(),
        first_ports: [&c.ports[0].physical_id, &c.ports[1].physical_id],
        dispatch_receipt: None,
    })
    .map_err(|_| rejected())?;
    if claim(state, &raw)? != op {
        return Err(rejected());
    }
    Ok(PrivateReadClaim {
        operation: op,
        proof,
    })
}
pub(crate) fn finish_private_read(
    state: &JsValue,
    t: PrivateReadClaim,
    reply: &ExecutionPrivateCurrentReplyV2,
) -> Result<()> {
    if reply.proof_digest != t.proof {
        return Err(rejected());
    }
    let raw = serde_json::to_string(reply).map_err(|_| rejected())?;
    if complete_private(state, &t.operation, &t.proof, &raw)? != raw {
        return Err(rejected());
    }
    validate_snapshot(state)
}

#[path = "execution_context_gated_stage_store_v2.rs"]
mod gated;
pub(crate) use gated::claim_native_prepare_gated;

fn mode_matches(
    mode: Option<&Mode>,
    issuer: &oasis7_custody_core::native_cleanup_v2::AccountMachineIssuerV2,
    session: &oasis7_custody_core::provisioning::ParticipantSessionRefV1,
) -> Result<()> {
    let m = mode.ok_or_else(rejected)?;
    if m.actor_tag != actor_tag(session)?
        || m.issuer_namespace != issuer.namespace
        || m.issuer_service != issuer.service_id
        || m.environment != issuer.environment
        || m.issuer_epoch.parse::<u64>().map_err(|_| rejected())? < issuer.issuer_epoch.0
        || m.config_generation.parse::<u64>().map_err(|_| rejected())? < issuer.config_generation.0
    {
        return Err(rejected());
    }
    Ok(())
}

pub(crate) fn require_manifest_recipient(
    state: &JsValue,
    env: &worker::Env,
    name: &str,
) -> Result<()> {
    if physical(state, env, name)? {
        Ok(())
    } else {
        Err(rejected())
    }
}
