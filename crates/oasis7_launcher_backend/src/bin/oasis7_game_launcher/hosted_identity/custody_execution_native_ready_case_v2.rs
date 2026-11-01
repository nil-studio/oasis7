//! Native receiver component only. Account signer/World remain explicit fixtures.
use super::*;
#[path = "custody_execution_native_ready_world_fixture_v2.rs"]
mod ready_world;
fn child() {
    let directory = std::env::temp_dir().join(format!("execution-ready-{}", random_token()));
    std::fs::create_dir(&directory).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let now = now_ms();
    let pin = |purpose: &str| {
        json!({"namespace":"account","service_id":"account-service","environment":"test","key_id":"account-key",
        "public_key":hex::encode(SigningKey::from_bytes(&[81;32]).verifying_key().to_bytes()),"issuer_epoch":"1","config_generation":"1",
        "not_before":now.saturating_sub(1000).to_string(),"not_after":(now+600000).to_string(),"native_audience_origin":"https://localhost","allowed_purpose":purpose})
    };
    let config = json!({"api_version":2,"pending_pin":pin("consumed_pending"),"active_pin":pin("active_three_roots"),
        "current_pin":pin("native_current_attempt"),"term_current_pin":pin("native_term_current_attempt"),
        "preflight_access_pin":pin("native_preflight_access_v2"),"execution_prepare_pin":pin("native_execution_prepare_v2"),
        "execution_dispatch_pin":pin("execution_dispatch_claim_v2"),"execution_identity_origin":"https://identity.fixture",
        "issuer_floor":{"issuer_epoch":"1","config_generation":"1"},"native_origin":"https://localhost"});
    let status=std::process::Command::new(std::env::current_exe().unwrap())
        .arg("execution_native_ready_uses_real_registration_and_activation_apis").arg("--nocapture").arg("--test-threads=1")
        .env("OASIS7_EXECUTION_READY_CHILD","true").env("OASIS7_CUSTODY_MANAGED_BINDINGS_ENABLED","true")
        .env("OASIS7_CUSTODY_MANAGED_BINDING_OWNER_PIN",json!({"key_id":"owner-binding","public_key":hex::encode(SigningKey::from_bytes(&[72;32]).verifying_key().to_bytes()),"environment":"test","service_identity":"owner-service","service_epoch":"1"}).to_string())
        .env("OASIS7_CUSTODY_CLEANUP_MACHINE_V2_ENABLED","true")
        .env("OASIS7_CUSTODY_EXECUTION_PREPARE_DISPATCH_V2_ENABLED","true")
        .env("OASIS7_CUSTODY_CLEANUP_ACCOUNT_ISSUER_CONFIG",config.to_string())
        .env("OASIS7_CUSTODY_CLEANUP_SQLITE_RESTORE_GUARD_PATH",directory.join("lineage")).status().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(status.success());
}
#[test]
fn execution_native_ready_uses_real_registration_and_activation_apis() {
    if std::env::var("OASIS7_EXECUTION_READY_CHILD")
        .ok()
        .as_deref()
        != Some("true")
    {
        child();
        return;
    }
    let mut f = native_fixture::managed_fixture();
    let mut data = identity::participant_entry::resource_fixture::Data::new();
    data.owner = hex::encode([11; 32]);
    data.issuer = hex::encode([83; 32]);
    data.world_key = hex::encode([82; 32]);
    let owner = ws::sign_read_request("key", (), &data.owner)
        .unwrap()
        .subject_public_key;
    data.state.owner.public_key = owner.clone();
    data.state.control.owner_public_key = owner;
    let namespace = json!({"service_id":f.f.service.config.namespace.identity_service_id,
        "environment_id":f.f.service.config.namespace.environment_id,"instance_epoch":f.f.service.config.rp.identity_instance_epoch.to_string()});
    let mut request = native_certificate::options_for(&f, now_ms());
    let CustodyIntentV1::TestSessionV2 { plan, scope } = &mut request.intent else {
        unreachable!()
    };
    plan.termination.initial_fence =
        oasis7_agent_api::game::GameTaskFenceV1::from_control(&data.state.control);
    let d = data.state.signer_delegation.as_ref().unwrap();
    plan.termination.original_delegation =
        oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1 {
            world_id: d.world_id.clone(),
            branch_id: d.branch_id.clone(),
            agent_id: d.agent_id.clone(),
            owner_binding: d.owner_binding.clone(),
            agent_identity_generation: d.agent_identity_generation.0,
            generation: d.generation.0,
            delegate_public_key: d.delegate_public_key.clone(),
            revoked: d.revoked,
            nonce: d.nonce.0,
        };
    plan.termination.stop_operation_id = plan.termination.expected_stop_operation_id().unwrap();
    plan.termination.close_operation_id = plan.termination.expected_close_operation_id().unwrap();
    plan.termination.world.branch_id = data.state.control.branch_id.clone();
    plan.termination.world.reorg_epoch = data.state.control.reorg_epoch;
    scope.world_identity = plan.termination.world.clone();
    scope.task_fence = plan.termination.initial_fence.clone();
    scope.termination_plan_digest = plan.termination.digest().unwrap();
    let server = ready_world::Server::start(data, namespace);
    let trust=serde_json::from_value(json!({"endpoint":server.endpoint,"service_public_key":hex::encode(SigningKey::from_bytes(&[82;32]).verifying_key().to_bytes()),"world":{"world_id":"world","genesis_digest":"genesis"}})).unwrap();
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
        .participant_entry
        .as_mut()
        .unwrap()
        .executor_control_enabled = true;
    f.f.service
        .store
        .attach_machine_restore_guard_from_env(&f.f.service.config.sqlite_path)
        .unwrap();
    let (input, consumed) = native_certificate::approve_and_consume(&mut f, request);
    let mut proof = native_certificate::certificate(&f, input, consumed);
    proof.certificate.descriptor.owner_read.scope_id = "agent:agent-a".into();
    proof.signature_hex = hex::encode(
        SigningKey::from_bytes(&[81; 32])
            .sign(&proof.certificate.signing_bytes().unwrap())
            .to_bytes(),
    );
    let csrf = digest(&f.f.service.config.session_key, "csrf", &f.sid);
    let headers = format!(
        "POST / HTTP/1.1\r\nOrigin: https://localhost\r\nSec-Fetch-Site: same-origin\r\nContent-Type: application/json\r\nX-Oasis7-CSRF: {csrf}\r\nCookie: __Host-oasis7_sid={}\r\n\r\n",
        f.sid
    );
    let service = Arc::new(std::sync::Mutex::new(Some(f.f.service)));
    let probe = service.clone();
    *server.probe.lock().unwrap() = Some(Box::new(move || probe.try_lock().is_ok()));
    exchange(
        &service,
        NATIVE_PREFLIGHT_POLICY_V2_PATH,
        &headers,
        &NativePreflightPolicyRequestV2 {
            proof: Box::new(proof.clone()),
            body: PreflightPolicyBodyV2 {
                observation_nonce: proof.certificate.descriptor.access_nonces.policy.clone(),
            },
        },
    )
    .unwrap();
    let admission: NativePreflightAuthorizeReplyV2 = serde_json::from_value(
        exchange(
            &service,
            NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH,
            &headers,
            &NativePreflightAuthorizeRequestV2 {
                proof: Box::new(proof.clone()),
                body: proof.certificate.descriptor.participant_owner.clone(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    let read = ws::sign_read_request(
        ws::VIEW_JSON_PATH,
        proof.certificate.descriptor.owner_read.clone(),
        &hex::encode([11; 32]),
    )
    .unwrap();
    let owner_read = ParticipantOwnerViewRequestV1 {
        owner: proof.certificate.descriptor.participant_owner.clone(),
        admission: admission.admission,
        read,
        response_profile: Some(
            ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1,
        ),
    };
    let ready = register_and_activate(service, proof, owner_read, &f.sid);
    assert_eq!(
        ready.active.certificate.opaque_registration_ref,
        ready.registration.ack.opaque_registration_ref
    );
    assert_eq!(
        ready.activation.ack.active_certificate_digest,
        ready.active.digest().unwrap()
    );
    assert_eq!(
        ready.activation.ack.pending_certificate_digest,
        ready.pending.digest().unwrap()
    );
    assert!(!ready.headers.is_empty());
    let q = NativeActivateRequestV2 {
        api_version: 2,
        opaque_registration_ref: ready.active.certificate.opaque_registration_ref.clone(),
        active: ready.active.clone(),
    };
    assert!(
        exchange(&ready.service, NATIVE_ACTIVATE_V2_PATH, &ready.headers, &q).is_err(),
        "actual Active cannot activate twice"
    );
    assert_eq!(
        server.finish(),
        3,
        "two actual preflight reads plus original registration read; activation adds no World HTTP"
    );
}
