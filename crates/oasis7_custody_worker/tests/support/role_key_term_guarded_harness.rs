//! Normal-clock Actor/SQLite component fixture only. Account/Current are synthetic;
//! no Term6, World transport or whole-ledger qualification is implied.
use super::*;
use crate::role_key_store::TermGuardedSignerConfigurationV2;
use oasis7_agent_api::game::{GameTaskStopV2, StopObservationGrantV2};
use oasis7_custody_core::{native_cleanup_v2::*, term_guarded_signer_v2::*};

fn configuration() -> Result<(
    SigningKey,
    AccountMachineIssuerV2,
    TermGuardedSignerConfigurationV2,
)> {
    let key = SigningKey::from_bytes(&[81; 32]);
    let issuer = AccountMachineIssuerV2 {
        namespace: "synthetic-account".into(),
        service_id: "synthetic-account-service".into(),
        environment: "staging".into(),
        key_id: "synthetic-term-issuer".into(),
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
        native_audience_origin: "https://native.example".into(),
    };
    let pin = AccountMachineIssuerPinV2 {
        namespace: issuer.namespace.clone(),
        service_id: issuer.service_id.clone(),
        environment: issuer.environment.clone(),
        key_id: issuer.key_id.clone(),
        public_key: hex::encode(key.verifying_key().as_bytes()),
        issuer_epoch: issuer.issuer_epoch,
        config_generation: issuer.config_generation,
        not_before: DecimalU64(1),
        not_after: DecimalU64(4_102_444_800_000),
        native_audience_origin: issuer.native_audience_origin.clone(),
        allowed_purpose: AccountMachinePurposeV2::TermGuardedStopSignerV2,
    };
    let floor = AccountMachineIssuerFloorV2 {
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
    };
    Ok((
        key,
        issuer,
        TermGuardedSignerConfigurationV2::new(pin, floor).map_err(failure)?,
    ))
}
fn original(public: &PublicRoleKey, until: u64) -> Result<GameTaskStopV2> {
    let mut p: GameTaskStopV2 = serde_json::from_value(json!({
        "api_revision":2,"client_operation_id":"synthetic-term-stop",
        "custody_account_id":"synthetic-account","termination_plan_digest":"44".repeat(32),
        "initial_fence":{
            "world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0",
            "agent_id":"agent","owner_binding":"owner","owner_generation":"1","owner_public_key":public.public_key,
            "task_id":"task","task_revision":"3","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1",
            "active_goal_reference":{"goal_revision":"1","goal_ref":"22".repeat(32),"prepare_operation_id":"prepare","publish_operation_id":"publish","activation_committed_height":"1"},
            "executor_epoch":"1","executor_binding":{"executor_id":"executor","executor_public_key":"33".repeat(32),"delegation_generation":"1","executor_epoch":"1"}
        },
        "target_executor_public_key":"33".repeat(32),"target_executor_generation":"1","target_executor_nonce":"1",
        "execute_until_ms":until.to_string()
    })).map_err(failure)?;
    p.observation_grant = Some(
        StopObservationGrantV2::known_control_for_exact_target(&p, until, until, until)
            .map_err(failure)?,
    );
    p.validate().map_err(failure)?;
    Ok(p)
}
fn permit(
    key: &SigningKey,
    issuer: AccountMachineIssuerV2,
    scope: RoleKeySlotScopeV1,
    public: PublicRoleKey,
    attempt: &str,
) -> Result<SignedTermGuardedStopSignerPermitV2> {
    let now = js_sys::Date::now() as u64;
    let until = now
        .checked_add(4900)
        .ok_or_else(|| failure("clock overflow"))?;
    let payload = TermGuardedStopSignerPermitV2 {
        api_version: 2,
        issuer,
        recipient_role: Role::Owner,
        recipient_path: TERM_GUARDED_STOP_SIGNER_PATH_V2.into(),
        session_id: "synthetic-term-session".into(),
        approval_input_digest: "55".repeat(32),
        source_operation_id: "synthetic-term-source".into(),
        source_record_digest: "66".repeat(32),
        paid_attempt_id: attempt.into(),
        paid_record_digest: "77".repeat(32),
        original: original(&public, until)?,
        owner_scope: scope,
        owner_key: public,
        issued_at: DecimalU64(now),
        expires_at: DecimalU64(until),
    };
    let signature_hex = hex::encode(
        key.sign(&payload.signing_bytes().map_err(failure)?)
            .to_bytes(),
    );
    Ok(SignedTermGuardedStopSignerPermitV2 {
        payload,
        signature_hex,
    })
}
pub(super) async fn run(state: JsValue, path: &str) -> Result<Value> {
    let (key, issuer, config) = configuration()?;
    let expected = scope()?;
    let authority = || {
        Ok(RoleKeyCurrent {
            scope: expected.clone(),
            approved: true,
            frozen: false,
            cancelled: false,
            expires_at: DecimalU64(4_102_444_800_000),
            authority_revision: "ab".repeat(32),
        })
    };
    let store = RoleKeyStore::new(state.clone());
    let kek = [73; 32];
    let old = path.ends_with("old-slot");
    let public = if old {
        store
            .ensure_with_clock(expected.clone(), DecimalU64(1), &kek, authority, || {
                Ok(js_sys::Date::now() as u64)
            })
            .await
    } else {
        store
            .ensure_with_refresh_and_term_profile(
                expected.clone(),
                (DecimalU64(1), &kek),
                authority,
                || async { Ok(()) },
                config.clone(),
            )
            .await
    }
    .map_err(failure)?;
    let mut signed = permit(
        &key,
        issuer,
        expected,
        public,
        "synthetic-paid-term-attempt",
    )?;
    let invalid = path.ends_with("bad-signature")
        || path.ends_with("expired")
        || path.ends_with("wrong-slot");
    if path.ends_with("expired") {
        let now = js_sys::Date::now() as u64;
        signed.payload.issued_at = DecimalU64(now - 10);
        signed.payload.expires_at = DecimalU64(now - 1);
    }
    if path.ends_with("wrong-slot") {
        signed.payload.owner_scope.operation_id = "another-owner-task".into();
    }
    if invalid {
        signed.signature_hex = hex::encode(
            key.sign(&signed.payload.signing_bytes().map_err(failure)?)
                .to_bytes(),
        );
        if path.ends_with("bad-signature") {
            signed.signature_hex = "00".repeat(64);
        }
    }
    let loans = std::cell::Cell::new(0_u8);
    let fail_loan = path.ends_with("unknown");
    let result = store
        .test_sign_term_guarded_with_loan_v2(signed.clone(), &config, || {
            loans.set(loans.get() + 1);
            if fail_loan {
                Err(JsValue::from_str("synthetic loan failed"))
            } else {
                Ok((DecimalU64(1), kek))
            }
        })
        .await;
    if path.ends_with("replay") {
        let rows = super::super::rows(
            &state,
            "SELECT COUNT(*) AS count FROM custody_term_guarded_signer_once_v2",
        )?;
        if result.is_ok() || loans.get() != 0 || rows[0]["count"] != 1 {
            return Err(failure("cold replay borrowed key"));
        }
        return Ok(json!({"cold_replay_rejected":true,"key_loans":0,"claims":1}));
    }
    if invalid {
        let rows = super::super::rows(
            &state,
            "SELECT COUNT(*) AS count FROM custody_term_guarded_signer_once_v2",
        )?;
        if result.is_ok() || loans.get() != 0 || rows[0]["count"] != 0 {
            return Err(failure("invalid permit mutated claim"));
        }
        return Ok(json!({"invalid_rejected":true,"key_loans":0,"claims":0}));
    }
    if old {
        if result.is_ok() || loans.get() != 0 {
            return Err(failure("old slot borrowed key"));
        }
        return Ok(json!({"old_slot_rejected":true,"key_loans":0}));
    }
    if fail_loan != result.is_err() || loans.get() != 1 {
        return Err(failure("first claim result"));
    }
    let rows = super::super::rows(
        &state,
        "SELECT payload FROM custody_term_guarded_signer_once_v2",
    )?;
    let first_unknown = rows[0]["payload"]
        .as_str()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .is_some_and(|v| v["original"].is_null());
    if first_unknown != fail_loan {
        return Err(failure("unknown persistence"));
    }
    let repeat = store
        .test_sign_term_guarded_with_loan_v2(signed, &config, || {
            loans.set(loans.get() + 1);
            Ok((DecimalU64(1), kek))
        })
        .await;
    if repeat.is_ok() || loans.get() != 1 {
        return Err(failure("duplicate borrowed key"));
    }
    Ok(
        json!({"signed":result.is_ok(),"unknown":first_unknown,"repeat_rejected":true,"key_loans":loans.get()}),
    )
}
