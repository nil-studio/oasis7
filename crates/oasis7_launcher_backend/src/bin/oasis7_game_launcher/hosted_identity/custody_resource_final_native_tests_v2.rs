//! Actual static dispatcher, real cookie/CSRF/UV/Book and two TCP World sends.
//! Account signer and World snapshots are fixtures, not actual CF payment/finality.
use super::*;
use oasis7_client_api::world_service::resource_basis as resource;
use oasis7_custody_core::provisioning::OwnerSetupResourceFinalHttpV2;
#[path = "custody_resource_final_native_frame_fixture_v2.rs"]
mod frame_fixture;
#[path = "custody_resource_final_native_tcp_fixture_v2.rs"]
mod tcp_fixture;
use identity::participant_entry::resource_fixture as world_fixture;
fn child(scenario: &str) {
    let directory = std::env::temp_dir().join(format!("resource-native-{}", random_token()));
    std::fs::create_dir(&directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let now = now_ms();
    let pin = json!({"namespace":"account","service_id":"account-service","environment":"test","key_id":"account-key","public_key":hex::encode(SigningKey::from_bytes(&[81;32]).verifying_key().to_bytes()),"issuer_epoch":"1","config_generation":"1","not_before":now.saturating_sub(1000).to_string(),"not_after":(now+600000).to_string(),"native_audience_origin":"https://localhost","allowed_purpose":"owner_setup_resource_final_v2"});
    let cfg = json!({"issuer_pin":pin,"issuer_floor":{"issuer_epoch":"1","config_generation":"1"},"resource_feature_generation":"1"});
    let status=std::process::Command::new(std::env::current_exe().unwrap()).arg("resource_final_actual_static_dispatch_spends_two_world_reads_once").arg("--nocapture").arg("--test-threads=1")
        .env("OASIS7_RESOURCE_NATIVE_SCENARIO",scenario).env("OASIS7_CUSTODY_OWNER_RESOURCE_FINAL_V2_ENABLED","true").env("OASIS7_CUSTODY_OWNER_RESOURCE_FINAL_V2_CONFIG_JSON",cfg.to_string())
        .env("OASIS7_CUSTODY_MANAGED_BINDINGS_ENABLED","true").env("OASIS7_CUSTODY_MANAGED_BINDING_OWNER_PIN",json!({"key_id":"owner-binding","public_key":hex::encode(SigningKey::from_bytes(&[72;32]).verifying_key().to_bytes()),"environment":"test","service_identity":"owner-service","service_epoch":"1"}).to_string())
        .env("OASIS7_CUSTODY_CLEANUP_SQLITE_RESTORE_GUARD_PATH",directory.join("lineage")).status().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(status.success(), "Resource Native component child failed");
}
#[test]
fn resource_final_actual_static_dispatch_spends_two_world_reads_once() {
    let Ok(scenario) = std::env::var("OASIS7_RESOURCE_NATIVE_SCENARIO") else {
        for s in [
            "normal",
            "bad-header",
            "wrong-session",
            "current-config",
            "weak-node",
            "protocol-failure",
            "missing-provenance",
        ] {
            child(s);
        }
        return;
    };
    let mut f = native_fixture::managed_fixture();
    let mut data = world_fixture::Data::new();
    data.issuer = hex::encode([83; 32]);
    let server = tcp_fixture::Server::start(
        data,
        scenario == "weak-node",
        scenario == "protocol-failure",
    );
    let node_public = if scenario == "weak-node" {
        format!("01{}", "00".repeat(31))
    } else {
        ws::sign_read_request("key", (), &hex::encode([60; 32]))
            .unwrap()
            .subject_public_key
    };
    let trust=serde_json::from_value(json!({"endpoint":server.endpoint,"service_public_key":node_public,"world":{"world_id":"world","genesis_digest":"genesis"}})).unwrap();
    f.f.service.participant_entry = Some(
        identity::participant_entry::ParticipantEntry::new(
            trust,
            zeroize::Zeroizing::new(hex::encode([83; 32])),
        )
        .unwrap(),
    );
    f.f.service
        .participant_entry
        .as_mut()
        .unwrap()
        .owner_control_enabled = true;
    f.f.service
        .store
        .attach_machine_restore_guard_from_env(&f.f.service.config.sqlite_path)
        .unwrap();
    let mut data = world_fixture::Data::new();
    data.issuer = hex::encode([83; 32]);
    let frame = frame_fixture::frame(&mut f, &data);
    if scenario == "missing-provenance" {
        let config = f.f.service.custody.as_ref().unwrap();
        let approval_id = &frame.input().approval.proof.approval_id;
        f.f.service.store.transaction(|tx| {
            let saved=tx.get::<ApprovalRecord>(&approval_key(approval_id))?.unwrap();
            identity::custody::guarded_consent::verify_existing_guarded_consent_v2(tx,config,&f.f.service.config,&saved.value)?;
            assert!(saved.value.guarded_consent_provenance.is_some());
            let before=serde_json::to_value(&saved.value).unwrap();
            let mut legacy=saved.value.clone();legacy.guarded_consent_provenance=None;
            assert!(identity::custody::guarded_consent::verify_existing_guarded_consent_v2(tx,config,&f.f.service.config,&legacy).is_err(),"Resource legacy no-tag consent must not upgrade into protected Resource authorization");
            assert!(serde_json::to_value(&tx.get::<ApprovalRecord>(&approval_key(approval_id))?.unwrap().value).unwrap()==before,"original protected Resource record must stay unchanged");
            Ok(())
        }).unwrap();
        assert_eq!(server.finish(), 0);
        return;
    }

    if scenario == "current-config" {
        use identity::custody::{
            resource_final_config::ResourceFinalConfigV2, resource_final_registry as registry,
        };
        let current_a = ResourceFinalConfigV2::from_env().unwrap();
        let mut raw = serde_json::to_value(&current_a).unwrap();
        let until = raw["issuer_pin"]["not_after"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        raw["issuer_pin"]["not_after"] = json!((until - 1).to_string());
        let current_b: ResourceFinalConfigV2 = serde_json::from_value(raw).unwrap();
        let config = f.f.service.custody.as_ref().unwrap();
        let gateway =
            f.f.service
                .participant_entry
                .as_ref()
                .unwrap()
                .owned_owner_read_gateway();
        let actual = current_b
            .fingerprint(&gateway.fingerprint().unwrap())
            .unwrap();
        assert_ne!(
            current_a
                .fingerprint(&gateway.fingerprint().unwrap())
                .unwrap(),
            actual
        );
        let scope = registry::ResourceFinalScopeV2 {
            custody: config,
            identity: &f.f.service.config,
            resource: &current_b,
            configuration_digest: &actual,
        };
        let key = session_key(&f.f.service.config.session_key, &f.sid);
        let claimed =
            f.f.service
                .store
                .transaction(|tx| {
                    registry::claim_native(tx, &scope, frame.clone(), key.clone(), now_ms())
                })
                .unwrap();
        assert!(
            f.f.service
                .store
                .transaction(|tx| registry::claim_preflight(tx, &scope, claimed, now_ms()))
                .is_err()
        );
        assert!(
            f.f.service
                .store
                .transaction(|tx| registry::claim_native(tx, &scope, frame.clone(), key, now_ms()))
                .is_err(),
            "failed Current cannot refund independently committed Native once"
        );
        assert_eq!(server.finish(), 0);
        return;
    }
    let body = frame.body_bytes().unwrap();
    let mut proof = frame.proof_header().unwrap();
    if scenario == "bad-header" {
        proof.push('=');
    }
    let sid = if scenario == "wrong-session" {
        f.f.token.clone()
    } else {
        f.sid.clone()
    };
    let csrf = digest(&f.f.service.config.session_key, "csrf", &sid);
    let headers = format!(
        "POST / HTTP/1.1\r\nOrigin: https://localhost\r\nSec-Fetch-Site: same-origin\r\nContent-Type: application/json\r\nX-Oasis7-CSRF: {csrf}\r\nCookie: __Host-oasis7_sid={sid}\r\nOasis7-Custody-Resource-Read: {proof}\r\n\r\n"
    );
    let service = Arc::new(std::sync::Mutex::new(Some(f.f.service)));
    let probe = service.clone();
    *server.probe.lock().unwrap() = Some(Box::new(move || probe.try_lock().is_ok()));
    let reply = crate::static_http::cleanup_dispatch_for_test(
        service.clone(),
        "/api/public/hosted-account/v2/entry-owner/control",
        &headers,
        &body,
    );
    if scenario == "protocol-failure" {
        assert_ne!(reply.status, 200);
        assert_eq!(
            server.count.load(Ordering::SeqCst),
            1,
            "real protocol failure may not dispatch twice"
        );
        let replay = crate::static_http::cleanup_dispatch_for_test(
            service,
            "/api/public/hosted-account/v2/entry-owner/control",
            &headers,
            &body,
        );
        assert_ne!(replay.status, 200);
        assert_eq!(
            server.finish(),
            1,
            "unknown first read keeps Native once spent; no second World or same-proof redispatch"
        );
        return;
    }
    if scenario == "weak-node" {
        assert!(
            server.legacy_weak_preflight.load(Ordering::SeqCst),
            "exact preflight signature must actually pass old SDK verifier before target"
        );
        assert_ne!(reply.status, 200);
        assert_eq!(
            server.finish(),
            1,
            "Resource-only strict Node verification must reject weak preflight before second World send"
        );
        return;
    }
    if scenario != "normal" {
        assert_ne!(reply.status, 200);
        assert_eq!(server.finish(), 0);
        return;
    }
    assert_eq!(reply.status, 200);
    let envelope: resource::NativeResourceEnvelopeV1<
        resource::OwnerSetupFinalResourceObservationV2,
    > = serde_json::from_value(reply.body).unwrap();
    resource::verify_resource_goal_response(
        match &frame.request().operation {
            ws::entry_control::EntryOwnerOperationV1::ReadResourceGoal(q) => q,
            _ => unreachable!(),
        },
        &envelope.world_response,
        &ws::sign_read_request("key", (), &hex::encode([60; 32]))
            .unwrap()
            .subject_public_key,
    )
    .unwrap();
    assert_eq!(server.count.load(Ordering::SeqCst), 2);
    let replay = crate::static_http::cleanup_dispatch_for_test(
        service,
        "/api/public/hosted-account/v2/entry-owner/control",
        &headers,
        &body,
    );
    assert_ne!(replay.status, 200);
    assert_eq!(server.finish(), 2);
}
