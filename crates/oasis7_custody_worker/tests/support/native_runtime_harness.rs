//! Explicit synthetic authority/root keys for SQLite lifecycle regression only.
//! This does not exercise or substitute for real Account admission or native I/O.
use crate::native_private_store::manifest::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::{
    DecimalU64, http::Role, native_binding::*, provisioning::*, role_key_slot::*,
};
use serde_json::{Value, json};
use wasm_bindgen::JsValue;
use worker::{Error, Response, Result};
#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/identity_native_rpc.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = readNativeManifest)]
    fn inventory(state: &JsValue) -> std::result::Result<String, JsValue>;
}
fn denied() -> Error {
    Error::RustError("synthetic native lifecycle fixture rejected".into())
}
fn now() -> u64 {
    js_sys::Date::now() as u64
}
fn scope() -> Result<ManifestScope> {
    let role_scope = RoleKeySlotScopeV1::from_task(&ProvisionRoleKeyTaskV1 {
        operation_id: "fixture-identity-key-task".into(),
        provision_id: "fixture-provision".into(),
        environment: "fixture-only".into(),
        custody_account_id: "fixture-account".into(),
        role: Role::Identity,
        key_id: "fixture-login-key".into(),
        key_version: DecimalU64(1),
        algorithm: ProvisionRoleKeyAlgorithmV1::Ed25519,
        state: ProvisionRoleKeyTaskStateV1::Pending,
    })
    .map_err(|_| denied())?;
    Ok(ManifestScope {
        role_scope,
        identity_namespace: "fixture-native".into(),
        identity_instance_epoch: DecimalU64(1),
        registration_operation_id: "fixture-native-register".into(),
        initial_login_operation_id: "fixture-native-initial-login".into(),
    })
}
fn current(phase: NativePhase, operation: &str) -> Result<ManifestCurrent> {
    let scope = scope()?;
    let request_digest =
        oasis7_custody_core::native_history::stable_native_intent_digest(&scope, phase, operation)
            .map_err(|_| denied())?;
    Ok(ManifestCurrent {
        scope,
        phase,
        operation_id: operation.into(),
        permit_id: format!("fixture-account-{operation}"),
        request_digest,
        authority_revision: "a2".repeat(32),
        expires_at: DecimalU64(now() + 30_000),
    })
}
fn registration_current() -> Result<ManifestCurrent> {
    current(NativePhase::Register, "fixture-native-register")
}
fn public() -> String {
    hex::encode(SigningKey::from_bytes(&[5; 32]).verifying_key().to_bytes())
}
fn challenge() -> Result<SignedNativeBindingChallengeV1> {
    let issued = now();
    let challenge = NativeBindingChallengeV1 {
        api_version: 1,
        challenge_id: "fixture-B1".into(),
        nonce: "11".repeat(32),
        identity_namespace: "fixture-native".into(),
        identity_instance_epoch: DecimalU64(1),
        registration_operation_id: "fixture-native-register".into(),
        registration_request_digest: "22".repeat(32),
        registration_receipt_digest: "33".repeat(32),
        native_account_id: "fixture-native-account".into(),
        native_credential_ref: "fixture-native-credential".into(),
        login_public_key: public(),
        owner_context_ref: "44".repeat(32),
        issued_at: DecimalU64(issued),
        expires_at: DecimalU64(issued + 30_000),
    };
    let signature = URL_SAFE_NO_PAD.encode(
        SigningKey::from_bytes(&[6; 32])
            .sign(&challenge.signing_bytes().map_err(|_| denied())?)
            .to_bytes(),
    );
    Ok(SignedNativeBindingChallengeV1 {
        challenge,
        key_id: "fixture-root".into(),
        signature,
    })
}
fn receipt() -> Result<SignedNativeBindingReceiptV1> {
    let c = challenge()?.challenge;
    let statement = OwnerNativeBindingStatementV1 {
        challenge: c.clone(),
        environment: "fixture-only".into(),
        custody_account_id: "fixture-account".into(),
        provision_id: "fixture-provision".into(),
        approved_plan_digest: "55".repeat(32),
        login_key_ref: "fixture-login-key".into(),
        binding_operation_id: "fixture-B1".into(),
        permit_id: "fixture-owner-permit".into(),
        source_revision: "66".repeat(32),
        account_authority_revision: "77".repeat(32),
        service_epoch: DecimalU64(1),
        service_identity: "fixture-owner".into(),
        signer_key_id: "fixture-owner-service".into(),
        authority_observed_at: c.issued_at,
        expires_at: DecimalU64(c.issued_at.0 + 5_000),
    };
    let receipt = NativeBindingReceiptV1 {
        api_version: 1,
        binding_operation_id: "fixture-B1".into(),
        statement_digest: statement.digest().map_err(|_| denied())?,
        environment: statement.environment.clone(),
        custody_account_id: statement.custody_account_id.clone(),
        provision_id: statement.provision_id.clone(),
        identity_namespace: c.identity_namespace,
        identity_instance_epoch: c.identity_instance_epoch,
        native_account_id: c.native_account_id,
        native_credential_ref: c.native_credential_ref,
        login_public_key: c.login_public_key,
        login_key_ref: statement.login_key_ref,
        registration_operation_id: c.registration_operation_id,
        registration_receipt_digest: c.registration_receipt_digest,
        native_account_security_epoch: DecimalU64(1),
        native_credential_version: DecimalU64(1),
        committed_at: c.issued_at,
    };
    let signature = URL_SAFE_NO_PAD.encode(
        SigningKey::from_bytes(&[6; 32])
            .sign(&receipt.signing_bytes().map_err(|_| denied())?)
            .to_bytes(),
    );
    Ok(SignedNativeBindingReceiptV1 {
        receipt,
        key_id: "fixture-root".into(),
        signature,
    })
}
fn prepare_registration(store: &NativeManifestStore) -> Result<NativeStageResultV1> {
    let authority = registration_current()?;
    store
        .claim_phase(|| Ok(authority.clone()))
        .map_err(|_| denied())?;
    let result = NativeStageResultV1::RegistrationChallenge(challenge()?);
    store
        .persist_result(|| Ok(authority.clone()), &result, None)
        .map_err(|_| denied())?;
    Ok(result)
}
async fn run(state: JsValue, mode: &str) -> Result<Value> {
    let store = NativeManifestStore::new(state.clone());
    match mode {
        "read-only-registration-history" | "read-only-unknown-history" => {
            let before = inventory(&state).map_err(|_| denied())?;
            let authority = registration_current()?;
            let history = store
                .read_completed_history(&NativeHistoryLookup {
                    environment: "fixture-only",
                    account_id: "fixture-account",
                    provision_id: "fixture-provision",
                    result_permit_id: &authority.permit_id,
                    phase: NativePhase::Register,
                    native_operation_id: None,
                })
                .map_err(|_| denied())?;
            let after = inventory(&state).map_err(|_| denied())?;
            Ok(
                json!({"fixture_only":true,"unchanged":before==after,"history_result":history.map(|h|h.result),
                "inventory":serde_json::from_str::<Value>(&after).map_err(|_|denied())?}),
            )
        }
        "complete-registration-before-account-submit" => {
            let original = prepare_registration(&store)?;
            Ok(json!({"fixture_only":true,"original":original}))
        }
        "recover-completed-registration" => {
            let authority = registration_current()?;
            let phase = store
                .claim_phase(|| Ok(authority.clone()))
                .map_err(|_| denied())?;
            let (original, generation) = store
                .completed_result(|| Ok(authority.clone()))
                .map_err(|_| denied())?
                .ok_or_else(denied)?;
            Ok(
                json!({"fixture_only":true,"phase":phase,"reconcile":original,"private_sid_generation":generation}),
            )
        }
        "b2-recovers-known-b1" => {
            prepare_registration(&store)?;
            let b1 = current(NativePhase::Binding, "fixture-B1")?;
            store.claim_phase(|| Ok(b1.clone())).map_err(|_| denied())?;
            store
                .set_phase_state(|| Ok(b1.clone()), PhaseState::Dispatched)
                .map_err(|_| denied())?;
            store
                .recover_phase(|| Ok(b1.clone()))
                .map_err(|_| denied())?;
            let b2 = current(NativePhase::Binding, "fixture-B2")?;
            store.claim_phase(|| Ok(b2.clone())).map_err(|_| denied())?;
            let original = NativeStageResultV1::BindingReceipt(receipt()?);
            store
                .persist_result(|| Ok(b2.clone()), &original, None)
                .map_err(|_| denied())?;
            let (recovered, _) = store
                .completed_result(|| Ok(b2.clone()))
                .map_err(|_| denied())?
                .ok_or_else(denied)?;
            Ok(
                json!({"fixture_only":true,"current_binding_operation":"fixture-B2","original":original,"recovered":recovered}),
            )
        }
        "prepare-initial-login" => {
            prepare_registration(&store)?;
            let binding = current(NativePhase::Binding, "fixture-B1")?;
            store
                .claim_phase(|| Ok(binding.clone()))
                .map_err(|_| denied())?;
            store
                .persist_result(
                    || Ok(binding.clone()),
                    &NativeStageResultV1::BindingReceipt(receipt()?),
                    None,
                )
                .map_err(|_| denied())?;
            let login = current(NativePhase::InitialLogin, "fixture-native-initial-login")?;
            let phase = store
                .claim_phase(|| Ok(login.clone()))
                .map_err(|_| denied())?;
            Ok(json!({"fixture_only":true,"phase":phase}))
        }
        "dispatch-initial-login" => {
            let login = current(NativePhase::InitialLogin, "fixture-native-initial-login")?;
            store
                .set_phase_state(|| Ok(login.clone()), PhaseState::Dispatched)
                .map_err(|_| denied())?;
            Ok(json!({"fixture_only":true,"send_right_consumed":true}))
        }
        "recover-initial-login-without-sid" => {
            let login = current(NativePhase::InitialLogin, "fixture-native-initial-login")?;
            let phase = store
                .recover_phase(|| Ok(login.clone()))
                .map_err(|_| denied())?;
            let retry_rejected = store
                .set_phase_state(|| Ok(login.clone()), PhaseState::Dispatched)
                .is_err();
            Ok(json!({"fixture_only":true,"phase":phase,"retry_rejected":retry_rejected}))
        }
        _ => Err(denied()),
    }
}
pub async fn handle(state: JsValue, raw: &str) -> Result<Response> {
    let value: Value = serde_json::from_str(raw).map_err(|_| denied())?;
    let mode = value.as_str().ok_or_else(denied)?;
    match run(state, mode).await {
        Ok(value) => Response::from_json(&value),
        Err(_) => Ok(
            Response::from_json(&json!({"fixture_only":true,"rejected":true}))?.with_status(409),
        ),
    }
}
