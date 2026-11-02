//! A new genuine credential cannot turn an old unconsumed nonce into a new action.
use super::*;
#[test]
fn session_v2_rotated_credential_does_not_reauthorize_old_unconsumed_action_nonce() {
    let mut f = claimed_execution();
    let (body, old) = action_pop(&mut f, "action-before-rotation", 13_000, false);
    let nonce = old.nonce().unwrap().to_owned();
    let mut q = request(&f, "execution-credential-rotation", Some(1));
    q.purposes = vec![TestSessionPurposeV1::Execute];
    assert!(
        claim(&mut f, &q, &[allocation(33)], 99, 14_000)
            .unwrap()
            .newly_claimed
    );
    let saved = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    let root = &saved.roots()[&SessionPurposeV1::Execution];
    assert_eq!(root.credential.as_ref().unwrap().generation.0, 2);
    let token = URL_SAFE_NO_PAD.encode([33; 32]);
    let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    assert_eq!(
        root.credential.as_ref().unwrap().digest,
        hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    let CustodyIntentV1::TestSessionV2 { scope, .. } = &saved.input().request.intent else {
        unreachable!()
    };
    let target = CanonicalHttpTarget::new(
        &f.fact.registration.public_origin,
        HttpPopRoute::ScopedClient {
            custody_account_id: f.state.draft().entry.custody_account_id.clone(),
            provision_id: f.state.draft().entry.provision_id.clone(),
            route: Box::new(HttpPopRoute::Actions),
        },
    )
    .unwrap();
    let action: SessionActionRequestV2 = serde_json::from_slice(&body).unwrap();
    let host = SigningKey::from_bytes(&[8; 32]);
    assert_eq!(
        scope.client_pairing.client_public_key,
        host.verifying_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile: HttpPopProfileV1::Custody,
        target: &target,
        key_id: &scope.client_pairing.client_key_id,
        binding_id: &f.session_id,
        request_id: &action.request_id,
        nonce: Some(&nonce),
        token: Some(&token),
        created: UnixSeconds::new(15),
        expires: UnixSeconds::new((root.deadline.get() / 1000).min(20)),
        body: &body,
    })
    .unwrap();
    let signature = host.sign(material.signature_base());
    host.verifying_key()
        .verify_strict(material.signature_base(), &signature)
        .unwrap();
    let http = SessionClientHttpRequestV1 {
        session_id: f.session_id.clone(),
        method: target.method().into(),
        target_uri: target.uri().into(),
        headers: material
            .signed_headers(&signature.to_bytes())
            .into_iter()
            .map(|h| SessionClientHeaderV1 {
                name: h.name,
                value: h.value,
            })
            .collect(),
        body: std::str::from_utf8(&body).unwrap().into(),
        nonce_hint: Some(nonce),
    };
    let before = f.state.clone();
    assert!(
        f.state
            .prepare_test_session_v2_action_http(
                &http,
                HttpHeaderRepresentation::RawInstances,
                UnixMillis::new(15_001)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
}

#[test]
fn session_v2_metadata_nonce_cannot_precede_its_actual_paid_ingress_event() {
    let mut f = claimed_execution();
    let _ = action_pop(&mut f, "journal-bound-nonce", 13_000, false);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(
            serde_json::to_value(&f.state).unwrap()
        )
        .unwrap(),
        f.state
    );
    let mut json = serde_json::to_value(&f.state).unwrap();
    let events = json["test_sessions_v2"][&f.session_id]["events"]
        .as_array_mut()
        .unwrap();
    let paid = events
        .iter()
        .position(|v| {
            v["event"]["kind"] == "purpose_http_charged"
                && v["event"]["ingress_attempt_id"] == "metadata-ingress-journal-bound-nonce"
        })
        .unwrap();
    let issued = events
        .iter()
        .position(|v| {
            v["event"]["kind"] == "tool_metadata_nonce_issued"
                && v["event"]["request_id"] == "metadata-journal-bound-nonce"
        })
        .unwrap();
    assert!(paid < issued);
    events.swap(paid, issued);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(json).is_err());
}
