//! Synthetic Account grants/current, real normal UTC/Owner key/SQLite only.
use super::*;
use crate::role_key_store::TermOwnerSlotQualificationRequestV2;
use oasis7_agent_api::game::{GameTaskCloseConditionsV2, GameTaskRevokeAndCloseV2};
use oasis7_custody_core::{term_guarded_close_signer_v2::*, term_owner_slot_lineage_v2::*};
fn lineage(scope: &RoleKeySlotScopeV1) -> Result<TermOwnerSlotLineageV2> {
    // Explicit synthetic Account implementation contract, not a production proof.
    Ok(TermOwnerSlotLineageV2 {
        contract: TermAccountLineageContractV2::RetainedAuthorities256V2,
        custody_account_id: scope.context.identity.account_id.clone(),
        provision_id: scope.provision_id.clone(),
        account_draft_digest: "19".repeat(32),
        owner_slot_scope_digest: scope.task_digest().map_err(failure)?,
    })
}
fn close_permit(
    key: &SigningKey,
    stop: &SignedTermGuardedStopSignerPermitV2,
    attempt: &str,
) -> Result<SignedTermGuardedCloseSignerPermitV2> {
    let p = &stop.payload;
    let payload = TermGuardedCloseSignerPermitV2 {
        account_lineage: p.account_lineage.clone(),
        api_version: 2,
        issuer: p.issuer.clone(),
        recipient_role: Role::Owner,
        recipient_path: TERM_GUARDED_CLOSE_SIGNER_PATH_V2.into(),
        session_id: p.session_id.clone(),
        approval_input_digest: p.approval_input_digest.clone(),
        source_operation_id: p.source_operation_id.clone(),
        source_record_digest: p.source_record_digest.clone(),
        paid_attempt_id: attempt.into(),
        paid_record_digest: "28".repeat(32),
        owner_scope: p.owner_scope.clone(),
        owner_key: p.owner_key.clone(),
        issued_at: p.issued_at,
        expires_at: p.expires_at,
        original: GameTaskRevokeAndCloseV2 {
            api_revision: 2,
            client_operation_id: "synthetic-independent-close".into(),
            custody_account_id: p.original.custody_account_id.clone(),
            termination_plan_digest: p.original.termination_plan_digest.clone(),
            permitted_branches: GameTaskCloseConditionsV2 {
                exact_initial_active: p.original.initial_fence.clone(),
                own_stop_intent: None,
            },
            signer_public_key: "33".repeat(32),
            signer_generation: DecimalU64(1),
            signer_nonce: DecimalU64(1),
            revoke_generation: DecimalU64(2),
            revoke_nonce: DecimalU64(2),
            cleanup_until_ms: p.expires_at,
        },
    };
    let signature_hex = hex::encode(
        key.sign(&payload.signing_bytes().map_err(failure)?)
            .to_bytes(),
    );
    Ok(SignedTermGuardedCloseSignerPermitV2 {
        payload,
        signature_hex,
    })
}
pub(super) async fn run(state: JsValue, path: &str) -> Result<Value> {
    let (key, issuer, old_config) = configuration()?;
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
        allowed_purpose: AccountMachinePurposeV2::TermGuardedCloseSignerV2,
    };
    let config = old_config
        .clone()
        .with_close(
            pin,
            AccountMachineIssuerFloorV2 {
                issuer_epoch: DecimalU64(1),
                config_generation: DecimalU64(1),
            },
        )
        .map_err(failure)?;
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
    let public = store
        .ensure_with_refresh_and_term_profile(
            expected.clone(),
            (DecimalU64(1), &kek),
            authority,
            || async { Ok(()) },
            if old { old_config } else { config.clone() },
        )
        .await
        .map_err(failure)?;
    let now = js_sys::Date::now() as u64;
    let q = TermOwnerSlotQualificationRequestV2 {
        api_version: 2,
        paid_attempt_id: "synthetic-paid-qualification".into(),
        owner_scope: expected.clone(),
        owner_key: public.clone(),
        issued_at: DecimalU64(now),
        until: DecimalU64(now + 4900),
    };
    let observed = store.qualify_term_owner_slot_v2(q.clone(), &config);
    let mut stop = permit(
        &key,
        issuer,
        expected.clone(),
        public.clone(),
        "synthetic-paid-stop",
    )?;
    stop.payload.account_lineage = Some(lineage(&expected)?);
    stop.signature_hex = hex::encode(
        key.sign(&stop.payload.signing_bytes().map_err(failure)?)
            .to_bytes(),
    );
    let mut close = close_permit(&key, &stop, "synthetic-paid-close")?;
    let loans = std::cell::Cell::new(0u8);
    if old {
        let result = store
            .test_sign_term_guarded_close_with_loan_v2(close, &config, || {
                loans.set(loans.get() + 1);
                Ok((DecimalU64(1), kek))
            })
            .await;
        if observed.is_ok() || result.is_ok() || loans.get() != 0 {
            return Err(failure("old slot gained dual mode"));
        }
        return Ok(json!({"old_slot_rejected":true,"key_loans":0}));
    }
    if path.ends_with("cold-replay") {
        let a = store
            .test_sign_term_guarded_close_with_loan_v2(close, &config, || {
                loans.set(loans.get() + 1);
                Ok((DecimalU64(1), kek))
            })
            .await;
        if observed.is_ok() || a.is_ok() || loans.get() != 0 {
            return Err(failure("cold recreated handle"));
        }
        return Ok(json!({"cold_replay_rejected":true,"key_loans":0}));
    }
    let reply = observed.map_err(failure)?;
    if reply.paid_attempt_id != q.paid_attempt_id
        || reply.owner_scope != expected
        || reply.owner_key != public
        || reply.installed_schema != 2
        || reply.until != q.until
    {
        return Err(failure("qualification echo"));
    }
    if store.qualify_term_owner_slot_v2(q.clone(), &config).is_ok() {
        return Err(failure("qualification replay"));
    }
    // Qualifier cannot mint a signing attempt with the same paid physical ID.
    let mut alias = close_permit(&key, &stop, &q.paid_attempt_id)?;
    if store
        .test_sign_term_guarded_close_with_loan_v2(alias.clone(), &config, || {
            loans.set(loans.get() + 1);
            Ok((DecimalU64(1), kek))
        })
        .await
        .is_ok()
        || loans.get() != 0
    {
        return Err(failure("qualification/sign alias"));
    }
    alias.payload.paid_attempt_id = "synthetic-paid-stop".into();
    let signed_stop = store
        .test_sign_term_guarded_with_loan_v2(stop.clone(), &config, || {
            loans.set(loans.get() + 1);
            Ok((DecimalU64(1), kek))
        })
        .await
        .map_err(failure)?;
    oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
        oasis7_agent_api::game::GAME_TASK_STOP_DOMAIN_V2,
        &signed_stop.request,
        &public.public_key,
        &signed_stop.signature_hex,
    )
    .map_err(failure)?;
    alias.signature_hex = hex::encode(
        key.sign(&alias.payload.signing_bytes().map_err(failure)?)
            .to_bytes(),
    );
    if store
        .test_sign_term_guarded_close_with_loan_v2(alias, &config, || {
            loans.set(loans.get() + 1);
            Ok((DecimalU64(1), kek))
        })
        .await
        .is_ok()
        || loans.get() != 1
    {
        return Err(failure("crosspurpose attempt"));
    }
    if path.ends_with("bad-signature") {
        close.signature_hex = "00".repeat(64);
    }
    let unknown = path.ends_with("unknown");
    let result = store
        .test_sign_term_guarded_close_with_loan_v2(close.clone(), &config, || {
            loans.set(loans.get() + 1);
            if unknown {
                Err(JsValue::from_str("synthetic loan failed"))
            } else {
                Ok((DecimalU64(1), kek))
            }
        })
        .await;
    if path.ends_with("bad-signature") {
        if result.is_ok() || loans.get() != 1 {
            return Err(failure("bad Close accessed key"));
        }
        return Ok(json!({"invalid_close_rejected":true,"key_loans":1}));
    }
    if unknown != result.is_err() || loans.get() != 2 {
        return Err(failure("Close outcome"));
    }
    if let Ok(signed) = result {
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            oasis7_agent_api::game::GAME_TASK_CLEANUP_DOMAIN_V2,
            &signed.request,
            &public.public_key,
            &signed.signature_hex,
        )
        .map_err(failure)?;
        if path.ends_with("foreign-history") {
            // The exact newly signed Close and full inventory must cold-check first.
            let mut before = q.clone();
            before.paid_attempt_id = "synthetic-cold-positive".into();
            RoleKeyStore::new(state.clone())
                .qualify_term_owner_slot_v2(before, &config)
                .map_err(failure)?;
            let foreign = SigningKey::from_bytes(&[99; 32]);
            let bytes = oasis7_client_api::world_service::signing_bytes(
                oasis7_agent_api::game::GAME_TASK_CLEANUP_DOMAIN_V2,
                &signed.request,
            )
            .map_err(failure)?;
            let foreign_subject = hex::encode(foreign.verifying_key().as_bytes());
            let foreign_sig = hex::encode(foreign.sign(&bytes).to_bytes());
            oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
                oasis7_agent_api::game::GAME_TASK_CLEANUP_DOMAIN_V2,
                &signed.request,
                &foreign_subject,
                &foreign_sig,
            )
            .map_err(failure)?;
            if foreign_subject == public.public_key {
                return Err(failure("foreign key must differ"));
            }
            let rows = super::super::super::rows(
                &state,
                "SELECT id,payload FROM custody_term_guarded_signer_once_v2",
            )?;
            let row = rows
                .as_array()
                .ok_or_else(|| failure("rows"))?
                .iter()
                .find(|row| {
                    row["payload"]
                        .as_str()
                        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                        .is_some_and(|v| {
                            v["permit"]["payload"]["recipient_path"]
                                == TERM_GUARDED_CLOSE_SIGNER_PATH_V2
                        })
                })
                .ok_or_else(|| failure("Close row"))?;
            let old: Value =
                serde_json::from_str(row["payload"].as_str().ok_or_else(|| failure("payload"))?)
                    .map_err(failure)?;
            let mut changed = old.clone();
            changed["original"]["subject_public_key"] = foreign_subject.into();
            changed["original"]["signature_hex"] = foreign_sig.into();
            if changed["permit"] != old["permit"]
                || changed["original"]["request"] != old["original"]["request"]
                || changed["claimed_at"] != old["claimed_at"]
                || changed["signed_at"] != old["signed_at"]
            {
                return Err(failure("balanced history"));
            }
            let raw = serde_json::to_string(&changed)
                .map_err(failure)?
                .replace('\'', "''");
            let row_id = row["id"].as_str().ok_or_else(|| failure("row id"))?;
            if row_id.len() != 64 || !row_id.bytes().all(|v| v.is_ascii_hexdigit()) {
                return Err(failure("row id shape"));
            }
            super::super::super::sql(
                &state,
                &format!(
                    "UPDATE custody_term_guarded_signer_once_v2 SET payload='{raw}' WHERE id='{row_id}'"
                ),
            )?;
            let mut after = q.clone();
            after.paid_attempt_id = "synthetic-cold-foreign-target".into();
            let result =
                RoleKeyStore::new(state.clone()).qualify_term_owner_slot_v2(after, &config);
            return Ok(
                json!({"foreign_history_rejected":result.is_err(),"strong_foreign_signature":true,"key_loans":loans.get()}),
            );
        }
        if oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            oasis7_agent_api::game::GAME_TASK_STOP_DOMAIN_V2,
            &signed.request,
            &public.public_key,
            &signed.signature_hex,
        )
        .is_ok()
        {
            return Err(failure("Close wrong domain"));
        }
    }
    if store
        .test_sign_term_guarded_close_with_loan_v2(close, &config, || {
            loans.set(loans.get() + 1);
            Ok((DecimalU64(1), kek))
        })
        .await
        .is_ok()
        || loans.get() != 2
    {
        return Err(failure("Close replay"));
    }
    Ok(
        json!({"stop_signed":true,"close_signed":!unknown,"close_unknown":unknown,"qualification_once":true,"crosspurpose_rejected":true,"key_loans":2}),
    )
}
