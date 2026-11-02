//! Actual software-UV Native consent/current-owner test; no provider capability.
use super::*;
use crate::hosted_identity::custody::model_proxy_basis::ModelProxyBasisAuthority;
use oasis7_custody_core::approval::NativeModelProxyBasisV2;
struct ConsentBasis;
impl ModelProxyBasisAuthority for ConsentBasis {
    fn public_metadata(&self) -> Value {
        json!({"api_version":2,"available":false})
    }
    fn prepare(
        &self,
        request: &ApprovalOptionsRequestV1,
        account: &str,
        _session: &str,
        now: u64,
    ) -> Result<NativeModelProxyBasisV2, String> {
        NativeModelProxyBasisV2::for_request(
            request,
            &oasis7_custody_core::approval::NativeModelProxyConnectionBindingV2 {
                native_account_id: account.into(),
                native_instance_public_key: hex::encode(
                    SigningKey::from_bytes(&[91; 32]).verifying_key().to_bytes(),
                ),
                connection_ref_digest: "61".repeat(32),
                connection_generation: DecimalU64(1),
                endpoint_policy_digest: "62".repeat(32),
            },
            now,
            now + 20_000,
        )
    }
    fn validate_saved(
        &self,
        basis: &NativeModelProxyBasisV2,
        request: &ApprovalOptionsRequestV1,
        account: &str,
        _session: &str,
        now: u64,
    ) -> Result<(), String> {
        basis.validate_request(request, account, now)
    }
}
#[test]
fn native_model_proxy_completed_owner_requires_current_actual_session() {
    if std::env::var_os("OASIS7_MODEL_OWNER_TEST_CHILD").is_none() {
        let directory = std::env::temp_dir().join(format!("model-owner-{}", random_token()));
        std::fs::create_dir(&directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let now = now_ms();
        let pin = |purpose: &str| json!({"namespace":"account","service_id":"account-service","environment":"test","key_id":"account-key","public_key":hex::encode(SigningKey::from_bytes(&[81;32]).verifying_key().to_bytes()),"issuer_epoch":"1","config_generation":"1","not_before":now.saturating_sub(1000).to_string(),"not_after":(now+600000).to_string(),"native_audience_origin":"https://localhost","allowed_purpose":purpose});
        let config = json!({"api_version":2,"pending_pin":pin("consumed_pending"),"active_pin":pin("active_three_roots"),"current_pin":pin("native_current_attempt"),"term_current_pin":pin("native_term_current_attempt"),"issuer_floor":{"issuer_epoch":"1","config_generation":"1"},"native_origin":"https://localhost"});
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("native_model_proxy_completed_owner_requires_current_actual_session")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env("OASIS7_MODEL_OWNER_TEST_CHILD", "1")
            .env("OASIS7_CUSTODY_CLEANUP_MACHINE_V2_ENABLED", "true")
            .env(
                "OASIS7_CUSTODY_CLEANUP_ACCOUNT_ISSUER_CONFIG",
                config.to_string(),
            )
            .env(
                "OASIS7_CUSTODY_CLEANUP_SQLITE_RESTORE_GUARD_PATH",
                directory.join("lineage"),
            )
            .status()
            .unwrap();
        let _ = std::fs::remove_dir_all(directory);
        assert!(status.success(), "Native owner child failed");
        return;
    }
    let (mut f, key) = fixture();
    let now = now_ms();
    f.service
        .custody
        .as_mut()
        .unwrap()
        .full_session_v2_summary_contract_enabled = true;
    f.service
        .store
        .attach_machine_restore_guard_from_env(&f.service.config.sqlite_path)
        .unwrap();
    let trust=serde_json::from_value(json!({"endpoint":"http://127.0.0.1:9","service_public_key":hex::encode(SigningKey::from_bytes(&[82;32]).verifying_key().to_bytes()),"world":{"world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":0}})).unwrap();
    f.service.participant_entry = Some(
        super::super::super::super::participant_entry::ParticipantEntry::new(
            trust,
            zeroize::Zeroizing::new(hex::encode([83; 32])),
        )
        .unwrap(),
    );
    f.service
        .attach_model_proxy_basis(std::sync::Arc::new(ConsentBasis))
        .unwrap();
    let (account, _) = f.service.session(Some(&f.token), now).unwrap();
    let mut request = session_options(now);
    request["approval_request_id"] = json!("proxy-owner");
    let price_policy = json!({"currency":"USD","input_micro_units_per_million":"1000000",
        "output_micro_units_per_million":"2000000","max_total_micro_units":"600","quote_digest":"88".repeat(32),"valid_until":(now+100_000).to_string()});
    request["intent"]["plan"]["model_budget"] = json!({
        "api_version":1,"approval_contract":"native_custody_model_proxy_v2",
        "native_account_id":account.account_id,"native_public_key":hex::encode(SigningKey::from_bytes(&[51;32]).verifying_key().to_bytes()),
        "native_instance_public_key":hex::encode(SigningKey::from_bytes(&[91;32]).verifying_key().to_bytes()),
        "ledger_id":"owner-ledger","preset_id":"fixed-preset","preset_revision":"1","model":"fixed-model",
        "runtime_digest":"33".repeat(32),"skill_digest":"44".repeat(32),"tools_digest":"55".repeat(32),
        "context_policy_digest":"66".repeat(32),"physical_graph_digest":"77".repeat(32),
        "probe_physical_http":["10","10"],"inference_physical_http":"10","max_calls":"3",
        "max_input_tokens":"100","max_output_tokens":"50","max_total_tokens":"450","max_concurrent":"1",
        "deadline":(now+90_000).to_string(),"price_policy":price_policy});
    let start = f
        .service
        .custody_options(Some(&f.token), &request, "source", now)
        .unwrap();
    let completed = f
        .service
        .custody_complete(
            Some(&f.token),
            &complete_payload(&start, &key, 0x05),
            "source",
            now + 1,
        )
        .unwrap();
    let input = oasis7_custody_core::approval_admission::ApprovalAdmissionInput {
        operation_id: "session".into(),
        request: serde_json::from_value(request).unwrap(),
        approval: serde_json::from_value(completed["approval"].clone()).unwrap(),
    };
    let owner = f
        .service
        .model_proxy_owner_binding(&input, now + 2)
        .unwrap();
    assert_eq!(owner.account_id(), input.approval.proof.manager_account_id);
    assert_eq!(
        owner.session_ref(),
        input.approval.proof.manager_session_ref
    );
    assert_eq!(
        owner.approved_input_digest(),
        input.approved_test_session_v2_digest().unwrap()
    );
    // Real completed UV/current-owner is necessary but cannot replace the
    // absent actual qualified graph/price/Captured query producer.
    let query_key = format!(
        "custody-model-first-query-v2:{}",
        owner.approved_input_digest()
    );
    assert!(
        f.service
            .model_proxy_first_reservation_query(&input, now + 2)
            .is_err()
    );
    assert!(f.service.store.transaction(|tx| tx.get::<oasis7_custody_core::native_model_proxy_query_v2::SignedNativeModelReservationQueryV2>(&query_key)).unwrap().is_none());
    assert!(
        f.service
            .model_proxy_first_reservation_query(&input, now + 2)
            .is_err()
    );
    let mut changed = input.clone();
    changed.approval.signature.push('x');
    assert!(
        f.service
            .model_proxy_owner_binding(&changed, now + 3)
            .is_err()
    );
    // This actual logout mutates current Native state; a historical strong proof
    // and its guard provenance must not authorize a future provider key loan.
    f.service.logout(Some(&f.token), now + 4).unwrap();
    assert!(
        f.service
            .model_proxy_owner_binding(&input, now + 5)
            .is_err()
    );
}
