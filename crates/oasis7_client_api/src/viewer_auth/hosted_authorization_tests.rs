use super::*;

pub fn context() -> HostedAuthContextV2 {
    HostedAuthContextV2 {
        version: 2,
        identity_service_id: "identity".into(),
        environment_id: "test".into(),
        identity_instance_epoch: 1,
        account_id: "account".into(),
        account_session_id: "account-session".into(),
        credential_record_id: "credential".into(),
        credential_version: 1,
        account_security_epoch: 1,
        auth_method: "webauthn".into(),
        auth_time: 900,
        recovery_context: None,
        world_identity_scope: "test-world-scope".into(),
        world_id: "world".into(),
        player_id: "hosted-player-a".into(),
        device_session_id: "world-session".into(),
        session_public_key: hex::encode(
            SigningKey::from_bytes(&[3; 32]).verifying_key().to_bytes(),
        ),
        world_session_epoch: 1,
        capabilities: vec!["player_session".into()],
        checked_at: 1_000,
        issued_at: 1_100,
        expires_at: 31_000,
        nonce: "nonce".into(),
        issuer_key_id: "issuer".into(),
    }
}

#[test]
fn authorization_expires_at_original_authoritative_read_deadline() {
    let context = context();
    assert!(context.validate_at(1_100).is_ok());
    assert!(context.validate_at(30_999).is_ok());
    assert!(context.validate_at(31_000).is_err());
    assert!(context.validate_at(1_099).is_err());
    let mut late_signer = context;
    late_signer.issued_at = 30_000;
    late_signer.expires_at = 60_000;
    assert!(late_signer.validate_at(30_000).is_err());
}

#[test]
fn authorization_rejects_tamper_wrong_issuer_and_legacy_format() {
    let private = hex::encode([7; 32]);
    let public = derive_hosted_registration_issuer_public_key(&private).unwrap();
    let grant = issue_hosted_authorization_grant(&context(), &private).unwrap();
    assert_eq!(
        verify_grant_signature(&grant, &public, 2_000).unwrap(),
        context()
    );
    let wrong_public = derive_hosted_registration_issuer_public_key(&hex::encode([8; 32])).unwrap();
    assert!(verify_grant_signature(&grant, &wrong_public, 2_000).is_err());
    let mut parts: Vec<String> = grant.split('.').map(str::to_owned).collect();
    let mut tampered = context();
    tampered.world_id = "another-world".into();
    parts[1] = hex::encode(serde_json::to_vec(&tampered).unwrap());
    assert!(verify_grant_signature(&parts.join("."), &public, 2_000).is_err());
    assert!(verify_grant_signature(&grant.replacen(PREFIX, "v1", 1), &public, 2_000).is_err());
}

#[test]
fn authorization_cannot_request_operator_or_asset_capabilities() {
    for capability in [
        "strong_auth",
        "operator",
        "asset_transfer",
        "player_session,operator",
    ] {
        let mut context = context();
        context.capabilities = vec![capability.into()];
        assert!(issue_hosted_authorization_grant(&context, &hex::encode([7; 32])).is_err());
    }
}

#[test]
fn nonce_scope_includes_namespace_and_disaster_recovery_epoch() {
    let original = context();
    let mut changed = original.clone();
    changed.identity_instance_epoch += 1;
    assert_ne!(
        authorization_nonce(&original),
        authorization_nonce(&changed)
    );
    changed = original.clone();
    changed.environment_id = "production".into();
    assert_ne!(
        authorization_nonce(&original),
        authorization_nonce(&changed)
    );
}

#[test]
fn v2_registration_boundary_child() {
    if std::env::var("OASIS7_TEST_V2_REGISTRATION_CHILD").is_err() {
        return;
    }
    let private = hex::encode([3; 32]);
    let issuer = hex::encode([7; 32]);
    let mut context = context();
    let now = now_unix_ms();
    context.checked_at = now;
    context.issued_at = now;
    context.expires_at = now + 30_000;
    let mut request = AuthoritativeSessionRegisterRequest {
        player_id: context.player_id.clone(),
        public_key: Some(context.session_public_key.clone()),
        registration_grant: Some(issue_hosted_authorization_grant(&context, &issuer).unwrap()),
        auth: None,
        requested_agent_id: None,
        force_rebind: false,
    };
    let proof =
        sign_session_register_auth_proof(&request, 1, &context.session_public_key, &private)
            .unwrap();
    assert!(verify_session_register_auth_proof_signature(&request, &proof).is_ok());
    assert!(verify_hosted_release_grant(request.registration_grant.as_deref().unwrap()).is_err());
    let mut release_context = context.clone();
    release_context.capabilities = vec!["player_session_release".into()];
    request.registration_grant =
        Some(issue_hosted_authorization_grant(&release_context, &issuer).unwrap());
    assert!(verify_hosted_release_grant(request.registration_grant.as_deref().unwrap()).is_ok());
    assert!(verify_session_register_auth_proof_signature(&request, &proof).is_err());
    request.registration_grant = Some(
        issue_hosted_registration_grant(
            &context.player_id,
            &context.session_public_key,
            &context.device_session_id,
            "legacy-nonce",
            now,
            &issuer,
        )
        .unwrap(),
    );
    assert!(verify_session_register_auth_proof_signature(&request, &proof).is_err());
    request.registration_grant = None;
    request.player_id = "non-hosted-prefix-bypass".into();
    let proof =
        sign_session_register_auth_proof(&request, 2, &context.session_public_key, &private)
            .unwrap();
    assert!(verify_session_register_auth_proof_signature(&request, &proof).is_err());
    request.player_id = context.player_id.clone();
    for field in ["world", "scope", "environment", "epoch", "player", "key"] {
        let mut wrong = context.clone();
        match field {
            "world" => wrong.world_id = "wrong-world".into(),
            "scope" => wrong.world_identity_scope = "wrong-scope".into(),
            "environment" => wrong.environment_id = "production".into(),
            "epoch" => wrong.identity_instance_epoch += 1,
            "player" => wrong.player_id = "another-player".into(),
            _ => {
                wrong.session_public_key =
                    hex::encode(SigningKey::from_bytes(&[4; 32]).verifying_key().to_bytes())
            }
        }
        request.registration_grant =
            Some(issue_hosted_authorization_grant(&wrong, &issuer).unwrap());
        let proof =
            sign_session_register_auth_proof(&request, 3, &context.session_public_key, &private)
                .unwrap();
        assert!(
            verify_session_register_auth_proof_signature(&request, &proof).is_err(),
            "{field}"
        );
    }
}

#[test]
fn v2_registration_rejects_legacy_and_cross_scope_without_changing_parent_environment() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "viewer::auth::hosted_authorization::tests::v2_registration_boundary_child",
            "--nocapture",
        ])
        .env("OASIS7_TEST_V2_REGISTRATION_CHILD", "1")
        .env(HOSTED_IDENTITY_V2_ENABLED_ENV, "1")
        .env(
            HOSTED_AUTHORIZATION_ISSUER_PUBLIC_KEY_ENV,
            derive_hosted_registration_issuer_public_key(&hex::encode([7; 32])).unwrap(),
        )
        .env("OASIS7_HOSTED_IDENTITY_NAMESPACE", "8:identity:test")
        .env("OASIS7_HOSTED_WORLD_ID", "world")
        .env("OASIS7_HOSTED_WORLD_IDENTITY_SCOPE", "test-world-scope")
        .env("OASIS7_HOSTED_IDENTITY_INSTANCE_EPOCH", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn native_key_authorization_is_limited_to_player_session_capabilities() {
    let mut context = context();
    context.auth_method = "native_key".into();
    let private = hex::encode([7; 32]);
    let public = derive_hosted_registration_issuer_public_key(&private).unwrap();
    let grant = issue_hosted_authorization_grant(&context, &private).unwrap();
    assert_eq!(
        verify_grant_signature(&grant, &public, 2000)
            .unwrap()
            .auth_method,
        "native_key"
    );
    context.capabilities = vec!["player_session_release".into()];
    assert!(issue_hosted_authorization_grant(&context, &private).is_ok());
    for capability in ["operator", "asset_transfer", "strong_auth"] {
        context.capabilities = vec![capability.into()];
        assert!(issue_hosted_authorization_grant(&context, &private).is_err());
    }
}
