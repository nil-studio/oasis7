//! Synthetic fixed authority only. No deployment artifact enables this module.
use crate::provision_approval_store::{OwnerProvisionApprovalStore, ProvisionApprovalCurrent};
use oasis7_custody_core::{DecimalU64, approval::*, approval_admission::*, provisioning::*};
use serde_json::{Value, json};
use wasm_bindgen::JsValue;
use worker::*;

fn failure(_: impl std::fmt::Debug) -> Error {
    Error::RustError("provision fixture rejected".into())
}
fn fixture() -> Result<(
    AccountDraftV1,
    ApprovalAdmissionInput,
    ApprovalAdmissionContext,
)> {
    let value: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/artifacts/approval-fixture.json"
    )))?;
    Ok((
        serde_json::from_value(value["provision"]["draft"].clone())?,
        serde_json::from_value(value["provision"]["input"].clone())?,
        serde_json::from_value(value["provision"]["context"].clone())?,
    ))
}
fn current(draft: &AccountDraftV1) -> Result<ProvisionApprovalCurrent> {
    let (_, _, mut approval) = fixture()?;
    // Fixed session/configuration truth for this synthetic account, stable after restart.
    approval.source.session_expires_at = DecimalU64(draft.plan.expires_at.0 + 20_000);
    approval.source.session_idle_expires_at = DecimalU64(draft.plan.expires_at.0 + 10_000);
    approval.handoff_until = DecimalU64(draft.plan.expires_at.0 - 80_000);
    Ok(ProvisionApprovalCurrent {
        approval,
        routes: ProvisionRoutes {
            environment: draft.plan.environment.clone(),
            identity_route: draft.plan.identity_route.clone(),
            world_route: draft.plan.world_route.clone(),
            world_identity: draft.plan.world_identity.clone(),
        },
        managers: vec![BootstrapManager {
            principal: draft.entry.manager.clone(),
            environments: vec![draft.plan.environment.clone()],
        }],
        policy_version: draft.entry.bootstrap_config_version,
        world_identity: draft.world_identity.clone(),
        native_identity_namespace: draft.entry.manager.identity_namespace.clone(),
    })
}
pub async fn handle(state: JsValue, mut request: Request) -> Result<Response> {
    let result = run(state, &mut request).await;
    match result {
        Ok(value) => Response::from_json(&value),
        Err(_) => Ok(Response::from_json(&json!({"rejected": true}))?.with_status(409)),
    }
}
async fn run(state: JsValue, request: &mut Request) -> Result<Value> {
    let path = request.path();
    if path.starts_with("/provision/prepare") || path == "/provision/missing-draft" {
        let (mut draft, mut input, _) = fixture()?;
        let now = js_sys::Date::now() as u64;
        draft.plan.expires_at = DecimalU64(now + 120_000);
        draft.entry.plan_digest = draft.plan.digest().map_err(failure)?;
        if let CustodyIntentV1::Provision {
            deadline,
            plan_digest,
            ..
        } = &mut input.request.intent
        {
            *deadline = draft.plan.expires_at;
            *plan_digest = draft.entry.plan_digest.clone();
        }
        if path == "/provision/prepare-purpose" {
            if let CustodyIntentV1::Provision { purpose, .. } = &mut input.request.intent {
                *purpose = "different-purpose".into();
            }
        }
        input.approval.proof.intent_digest = input.request.intent_digest().map_err(failure)?;
        input.approval.proof.issued_at = DecimalU64(now);
        input.approval.proof.expires_at = DecimalU64(now + 30_000);
        if path != "/provision/missing-draft" {
            let response = crate::owner_account::initialize_fixture(state.clone(), draft.clone())?;
            if response.status_code() != 200 {
                return Err(failure("draft init"));
            }
        }
        OwnerProvisionApprovalStore::initialize(state, draft.clone(), current(&draft)?)
            .map_err(failure)?;
        return Ok(
            json!({"input":input,"canonical_bytes":input.approval.proof.signing_bytes().map_err(failure)?}),
        );
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(failure)?;
    let draft = store.snapshot().map_err(failure)?.state.draft().clone();
    if path == "/provision/fault" {
        audit_constraint(&state, true)?;
        return Ok(json!({"ok":true}));
    }
    if path == "/provision/clear-fault" {
        audit_constraint(&state, false)?;
        return Ok(json!({"ok":true}));
    }
    if path == "/provision/audit" {
        return super::rows(
            &state,
            "SELECT sequence,payload FROM custody_audit ORDER BY CAST(sequence AS INTEGER)",
        );
    }
    if path == "/provision/snapshot" {
        return serde_json::to_value(store.snapshot().map_err(failure)?).map_err(failure);
    }
    if path == "/provision/freeze" {
        let target = AccountFreezeTarget {
            custody_account_id: draft.entry.custody_account_id.clone(),
            manager: draft.entry.manager.clone(),
            environment: draft.entry.environment.clone(),
            revocation_digest: "ab".repeat(32),
            minimum_security_epoch: DecimalU64(1),
        };
        let response = crate::owner_account::freeze_fixture(state, target)?;
        return Ok(json!({"status":response.status_code()}));
    }
    if path == "/provision/cancel" {
        return serde_json::to_value(store.cancel("cancel-one", 1).map_err(failure)?)
            .map_err(failure);
    }
    let reads = std::cell::Cell::new(0_u8);
    let authority = || {
        reads.set(reads.get() + 1);
        let mut value = current(&draft).map_err(|_| JsValue::from_str("fixture authority"))?;
        if reads.get() > 1 {
            if path.ends_with("-expired") {
                value.approval.source.session_idle_expires_at = DecimalU64(1);
            }
            if path.ends_with("-frozen") {
                value.approval.frozen = true;
            }
            if path.ends_with("-policy") {
                value.policy_version = DecimalU64(value.policy_version.0 + 1);
            }
            if path.ends_with("-allowlist") {
                value.managers.clear();
            }
        }
        Ok(value)
    };
    let clock_reads = std::cell::Cell::new(0_u8);
    let clock = || {
        clock_reads.set(clock_reads.get() + 1);
        let now = js_sys::Date::now() as u64;
        Ok(if path.ends_with("-clock") && clock_reads.get() > 1 {
            now + 40_000
        } else {
            now
        })
    };
    if path.starts_with("/provision/begin") {
        let input: ApprovalAdmissionInput = request.json().await?;
        return serde_json::to_value(
            store
                .begin_with_clock(input, authority, clock)
                .await
                .map_err(failure)?,
        )
        .map_err(failure);
    }
    if path == "/provision/verification" {
        let snapshot = store.snapshot().map_err(failure)?;
        let record = snapshot
            .state
            .original("operation-one")
            .ok_or_else(|| failure("missing original"))?;
        let req = record.native_verify_request().map_err(failure)?;
        let proof = &req.approval.proof;
        let context = current(&draft)?.approval;
        let source = context.source;
        let verification = ApprovalVerificationV1 {
            version: 1,
            key_id: "native-one".into(),
            identity_namespace: context.identity_namespace,
            purpose: req.purpose,
            proof_digest: req.proof_digest,
            approval_request_id: proof.approval_request_id.clone(),
            audience: req.expected_audience,
            intent_digest: req.expected_intent_digest,
            verify_nonce: req.verify_nonce,
            manager_security_version: source.manager_security_version,
            manager_policy_version: source.manager_policy_version,
            origin_credential_version: source.origin_credential_version,
            identity_instance_epoch: context.identity_instance_epoch,
            credential_version: source.passkey_credential_version,
            observed_at: DecimalU64(js_sys::Date::now() as u64),
            proof_expires_at: proof.expires_at,
            source_session_expires_at: source.session_expires_at,
            source_session_idle_expires_at: source.session_idle_expires_at,
            proof_state: ProofStateV1::Active,
            origin_state: OriginStateV1::Active,
        };
        return Ok(
            json!({"verification":verification,"canonical_bytes":verification.signing_bytes().map_err(failure)?}),
        );
    }
    if path.starts_with("/provision/commit") {
        let signed: SignedApprovalVerificationV1 = request.json().await?;
        return serde_json::to_value(
            store
                .commit_with_clock("operation-one", signed, authority, clock)
                .await
                .map_err(failure)?,
        )
        .map_err(failure);
    }
    Err(failure("unknown fixture path"))
}

fn audit_constraint(state: &JsValue, fail: bool) -> Result<()> {
    // Keep metadata recognized, preserve all prior audit rows, and cause the real
    // transaction's next approved audit INSERT to fail after the pure transition.
    super::sql(
        state,
        "ALTER TABLE custody_audit RENAME TO fixture_saved_audit",
    )?;
    super::sql(
        state,
        if fail {
            "CREATE TABLE custody_audit (record_id TEXT NOT NULL, sequence TEXT NOT NULL, payload TEXT NOT NULL CHECK(payload NOT LIKE '%\"approved\"%'), PRIMARY KEY(record_id,sequence))"
        } else {
            "CREATE TABLE custody_audit (record_id TEXT NOT NULL, sequence TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(record_id,sequence))"
        },
    )?;
    super::sql(
        state,
        "INSERT INTO custody_audit SELECT * FROM fixture_saved_audit",
    )?;
    super::sql(state, "DROP TABLE fixture_saved_audit")?;
    Ok(())
}
