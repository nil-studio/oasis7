//! Genuine Book/roots/credential/N+Host signatures. Graph/price/body are explicit
//! component fixtures; no provider qualification or model request is performed.
use super::*;
use oasis7_custody_core::{
    native_model_proxy_query_v2::*, native_model_proxy_v2::NativeModelCallKindV2,
};
fn graph() -> oasis7_custody_core::native_model_graph_v2::NativeModelPhysicalGraphV2 {
    use oasis7_custody_core::native_model_graph_v2::NativeModelGraphPortClassV2::*;
    let ports = vec![
        HostIdentityIngress,
        ProviderAbuseActor,
        RealmAbuseActor,
        IdentityOwnerService,
        OwnerAccountActor,
        AccountDirectory,
        AccountOwnerRecoveryFence,
        AccountIdentityRecoveryFence,
        IdentityNativeSource,
        IdentityNativeLifetime,
        IdentityPrivateOwnerActor,
        NativeProviderRequest,
    ];
    oasis7_custody_core::native_model_graph_v2::NativeModelPhysicalGraphV2 {
        api_version: 2,
        first_probe: ports.clone(),
        second_probe: ports.clone(),
        inference: ports,
    }
}
fn ready() -> SessionFixture {
    ready_with_graph(graph(), 400)
}
fn ready_with_graph(
    graph: oasis7_custody_core::native_model_graph_v2::NativeModelPhysicalGraphV2,
    transport: u64,
) -> SessionFixture {
    let mut budget = native_model_budget_tests::budget(true);
    budget.physical_graph_digest = graph.digest().unwrap();
    budget.probe_physical_http = [
        DecimalU64(graph.first_probe.len() as u64),
        DecimalU64(graph.second_probe.len() as u64),
    ];
    budget.inference_physical_http = DecimalU64(graph.inference.len() as u64);
    let mut f = activation_tests::activated_with_model_transport(9000, budget, transport);
    let mut q = request(&f, "model-initial-claim", None);
    q.purposes = vec![TestSessionPurposeV1::Execute];
    assert!(
        claim(&mut f, &q, &[allocation(31)], 41, 12000)
            .unwrap()
            .newly_claimed
    );
    f
}

fn query(f: &SessionFixture) -> SignedNativeModelReservationQueryV2 {
    query_with_graph(f, graph())
}
fn query_with_graph(
    f: &SessionFixture,
    graph: oasis7_custody_core::native_model_graph_v2::NativeModelPhysicalGraphV2,
) -> SignedNativeModelReservationQueryV2 {
    let snapshot = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    let input = snapshot.input();
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &input.request.intent else {
        panic!("V2");
    };
    let Some(
        oasis7_custody_core::approval::NativeCustodySummaryContractV1::FullTestSessionProxyV2(
            contract,
        ),
    ) = &input.approval.proof.model_budget_summary_contract
    else {
        panic!("new full proxy");
    };
    let q = NativeModelReservationQueryV2 {
        api_version: 2,
        request_id: "model-query-one".into(),
        reservation_id: "model-reserve-one".into(),
        provision_id: f.state.draft().entry.provision_id.clone(),
        custody_account_id: f.state.draft().entry.custody_account_id.clone(),
        session_id: f.session_id.clone(),
        approved_input_digest: f
            .state
            .test_session_v2_credential_input_digest(&f.session_id)
            .unwrap(),
        budget_digest: plan
            .model_budget
            .as_ref()
            .unwrap()
            .digest(plan.write_deadline.0)
            .unwrap(),
        basis_digest: contract.basis.digest().unwrap(),
        request_commitment: NativeModelRequestCommitmentV2::EncodedRequestV2 {
            provider_request_digest: "99".repeat(32),
        },
        physical_graph: graph,
        call_kind: NativeModelCallKindV2::ProbeFirst,
        ordinal: DecimalU64(1),
        expected_floor: DecimalU64(0),
        query_nonce: URL_SAFE_NO_PAD.encode([71; 32]),
        issued_at: DecimalU64(13000),
        admit_until: DecimalU64(18000),
    };
    let signature_hex = SigningKey::from_bytes(&[91; 32])
        .sign(&q.signing_bytes().unwrap())
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    SignedNativeModelReservationQueryV2 {
        query: q,
        signature_hex,
    }
}
fn raw_http(
    f: &SessionFixture,
    query: &SignedNativeModelReservationQueryV2,
) -> SessionClientHttpRequestV1 {
    let bytes = serde_json::to_vec(query).unwrap();
    let target = CanonicalHttpTarget::new(
        "https://custody.example",
        HttpPopRoute::ScopedClient {
            custody_account_id: f.state.draft().entry.custody_account_id.clone(),
            provision_id: f.state.draft().entry.provision_id.clone(),
            route: Box::new(HttpPopRoute::ModelReservations(f.session_id.clone())),
        },
    )
    .unwrap();
    let token = URL_SAFE_NO_PAD.encode([31; 32]);
    let material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile: HttpPopProfileV1::NativeModelCustodyV2,
        target: &target,
        key_id: &f.fact.registration.key_id,
        binding_id: &f.session_id,
        request_id: &query.query.request_id,
        nonce: Some(&query.query.query_nonce),
        token: Some(&token),
        created: UnixSeconds::new(13),
        expires: UnixSeconds::new(18),
        body: &bytes,
    })
    .unwrap();
    let headers = material.signed_headers(
        &SigningKey::from_bytes(&[8; 32])
            .sign(material.signature_base())
            .to_bytes(),
    );
    let http = SessionClientHttpRequestV1 {
        session_id: f.session_id.clone(),
        method: target.method().into(),
        target_uri: target.uri().into(),
        headers: headers
            .into_iter()
            .map(|h| SessionClientHeaderV1 {
                name: h.name,
                value: h.value,
            })
            .collect(),
        body: String::from_utf8(bytes).unwrap(),
        nonce_hint: Some(query.query.query_nonce.clone()),
    };
    http
}
fn pop(f: &SessionFixture, query: &SignedNativeModelReservationQueryV2) -> HttpPossessionReceipt {
    let http = raw_http(f, query);
    let context = NativeModelHttpContextV2 {
        public_origin: "https://custody.example",
        custody_account_id: &f.state.draft().entry.custody_account_id,
        provision_id: &f.state.draft().entry.provision_id,
        representation: HttpHeaderRepresentation::RawInstances,
    };
    let prepared = f
        .state
        .prepare_test_session_v2_native_model_reserve_http(&http, &context, UnixMillis::new(13000))
        .unwrap();
    verify_http_possession(prepared, UnixMillis::new(13000), &mut StrictCrypto).unwrap()
}
fn spent(f: &SessionFixture) -> u64 {
    let snapshot = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    serde_json::to_value(
        &snapshot.roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport,
    )
    .unwrap()["spent"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn session_v2_native_model_entry_fee_survives_pending_and_exact_history() {
    let mut f = ready();
    let q = query(&f);
    let proof = pop(&f, &q);
    let body = serde_json::to_vec(&q).unwrap();
    if let Some(path) = std::env::var_os("OASIS7_N_MODEL_ENTRY_FIXTURE_PATH") {
        let wire = serde_json::json!({"fixture":"genuine-core-history-routing-refusal-v2","component_time":"13000",
            "public_origin":"https://custody.example","account_id":f.state.draft().entry.custody_account_id,
            "http_request":raw_http(&f,&q)});
        let bytes = serde_json::to_vec_pretty(&wire).unwrap();
        assert!(
            !std::str::from_utf8(&bytes)
                .unwrap()
                .contains(&URL_SAFE_NO_PAD.encode([31; 32])),
            "never export a raw credential"
        );
        std::fs::write(path, bytes).unwrap();
    }
    let before = spent(&f);
    let original = SessionProofRequest {
        ingress_attempt_id: "model-ingress-a",
        original_bytes: &body,
        possession: &proof,
    };
    f.state
        .charge_test_session_v2_native_model_http_attempt(
            &f.session_id,
            original,
            UnixMillis::new(13000),
        )
        .unwrap();
    assert_eq!(spent(&f), before + 5);
    let original = SessionProofRequest {
        ingress_attempt_id: "model-ingress-a",
        original_bytes: &body,
        possession: &proof,
    };
    assert_eq!(
        f.state
            .reserve_test_session_v2_native_model_graph(
                &f.session_id,
                original,
                UnixMillis::new(13000)
            )
            .unwrap(),
        NativeModelReserveOutcomeV2::RecordedPending
    );
    assert_eq!(spent(&f), before + 12);
    let saved = f.state.clone();
    let original = SessionProofRequest {
        ingress_attempt_id: "model-ingress-a",
        original_bytes: &body,
        possession: &proof,
    };
    assert_eq!(
        f.state
            .reserve_test_session_v2_native_model_graph(
                &f.session_id,
                original,
                UnixMillis::new(13000)
            )
            .unwrap(),
        NativeModelReserveOutcomeV2::ExactHistory
    );
    assert_eq!(f.state, saved);
    let original = SessionProofRequest {
        ingress_attempt_id: "model-ingress-b",
        original_bytes: &body,
        possession: &proof,
    };
    f.state
        .charge_test_session_v2_native_model_http_attempt(
            &f.session_id,
            original,
            UnixMillis::new(13000),
        )
        .unwrap();
    assert_eq!(spent(&f), before + 17);
    let paid = f.state.clone();
    let original = SessionProofRequest {
        ingress_attempt_id: "model-ingress-b",
        original_bytes: &body,
        possession: &proof,
    };
    assert_eq!(
        f.state
            .reserve_test_session_v2_native_model_graph(
                &f.session_id,
                original,
                UnixMillis::new(13000)
            )
            .unwrap(),
        NativeModelReserveOutcomeV2::ExactHistory
    );
    assert_eq!(f.state, paid);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
    assert_eq!(cold, f.state);
}

#[test]
fn session_v2_native_model_entry_wrong_n_signature_with_genuine_host_proof_is_zero_mutation() {
    let mut f = ready();
    let mut q = query(&f);
    let sig = SigningKey::from_bytes(&[92; 32]).sign(&q.query.signing_bytes().unwrap());
    SigningKey::from_bytes(&[92; 32])
        .verifying_key()
        .verify_strict(&q.query.signing_bytes().unwrap(), &sig)
        .unwrap();
    assert!(
        SigningKey::from_bytes(&[91; 32])
            .verifying_key()
            .verify_strict(&q.query.signing_bytes().unwrap(), &sig)
            .is_err()
    );
    q.signature_hex = sig.to_bytes().iter().map(|b| format!("{b:02x}")).collect();
    let http = raw_http(&f, &q);
    let target = CanonicalHttpTarget::new(
        "https://custody.example",
        HttpPopRoute::ScopedClient {
            provision_id: f.state.draft().entry.provision_id.clone(),
            custody_account_id: f.state.draft().entry.custody_account_id.clone(),
            route: Box::new(HttpPopRoute::ModelReservations(f.session_id.clone())),
        },
    )
    .unwrap();
    let headers: Vec<_> = http
        .headers
        .iter()
        .map(|h| HttpHeader {
            name: &h.name,
            value: &h.value,
        })
        .collect();
    let token = URL_SAFE_NO_PAD.encode([31; 32]);
    let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    let prepared = prepare_http_pop(
        &HttpPopMessage {
            method: &http.method,
            target_uri: &http.target_uri,
            headers: &headers,
            representation: HttpHeaderRepresentation::RawInstances,
            body: http.body.as_bytes(),
        },
        &HttpPopBinding {
            profile: HttpPopProfileV1::NativeModelCustodyV2,
            target: &target,
            key_id: &f.fact.registration.key_id,
            public_key_hex: &f.fact.registration.request.descriptor.client_public_key,
            binding_id: &f.session_id,
            request_id: &q.query.request_id,
            nonce: Some(&q.query.query_nonce),
            credential_digest: &digest,
            valid_until: UnixMillis::new(18000),
        },
        UnixMillis::new(13000),
    )
    .unwrap();
    let proof =
        verify_http_possession(prepared, UnixMillis::new(13000), &mut StrictCrypto).unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .charge_test_session_v2_native_model_http_attempt(
                &f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: "wrong-n-ingress",
                    original_bytes: http.body.as_bytes(),
                    possession: &proof
                },
                UnixMillis::new(13000)
            )
            .is_err()
    );
    assert_eq!(f.state, before);
}
#[test]
fn session_v2_native_model_entry_reserved_ports_are_reverse_bound_without_counter_edits() {
    let mut f = ready();
    let q = query(&f);
    let proof = pop(&f, &q);
    let body = serde_json::to_vec(&q).unwrap();
    f.state
        .charge_test_session_v2_native_model_http_attempt(
            &f.session_id,
            SessionProofRequest {
                ingress_attempt_id: "cold-ports",
                original_bytes: &body,
                possession: &proof,
            },
            UnixMillis::new(13000),
        )
        .unwrap();
    f.state
        .reserve_test_session_v2_native_model_graph(
            &f.session_id,
            SessionProofRequest {
                ingress_attempt_id: "cold-ports",
                original_bytes: &body,
                possession: &proof,
            },
            UnixMillis::new(13000),
        )
        .unwrap();
    let raw = serde_json::to_value(&f.state).unwrap();
    let cold: AccountProvisionApprovalState = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(cold, f.state);
    let model = &raw["test_sessions_v2"][&f.session_id]["native_model_budget"];
    assert_eq!(model["entry_ports"].as_object().unwrap().len(), 4);
    assert_eq!(model["graph_ports"].as_object().unwrap().len(), 7);
    let key = model["graph_ports"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let mut changed = raw.clone();
    changed["test_sessions_v2"][&f.session_id]["native_model_budget"]["graph_ports"][&key]["class"] =
        serde_json::json!("native_world_view");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(changed).is_err());
    let mut missing = raw;
    missing["test_sessions_v2"][&f.session_id]["native_model_budget"]["graph_ports"]
        .as_object_mut()
        .unwrap()
        .remove(&key);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
}

fn eleven() -> oasis7_custody_core::native_model_graph_v2::NativeModelPhysicalGraphV2 {
    let mut g = graph();
    g.first_probe.remove(10);
    g.second_probe.remove(10);
    g.inference.remove(10);
    assert_eq!(g.first_probe.len(), 11);
    g
}
fn remaining(f: &SessionFixture) -> u64 {
    f.state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::Execution]
        .budgets
        .transport
        .remaining()
        .unwrap()
}
#[test]
fn session_v2_native_model_entry_remaining_graph_n_minus_one_keeps_committed_five_fees() {
    let g = eleven();
    let mut f = ready_with_graph(g.clone(), 35);
    let q = query_with_graph(&f, g);
    let proof = pop(&f, &q);
    let body = serde_json::to_vec(&q).unwrap();
    for i in 0..6 {
        f.state
            .charge_test_session_v2_native_model_http_attempt(
                &f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: &format!("n-minus-one-{i}"),
                    original_bytes: &body,
                    possession: &proof,
                },
                UnixMillis::new(13000),
            )
            .unwrap();
    }
    assert_eq!(remaining(&f), 5); // remaining graph is six real typed reservations
    let paid = f.state.clone();
    assert!(
        f.state
            .reserve_test_session_v2_native_model_graph(
                &f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: "n-minus-one-5",
                    original_bytes: &body,
                    possession: &proof
                },
                UnixMillis::new(13000)
            )
            .is_err()
    );
    assert_eq!(f.state, paid);
}
#[test]
fn session_v2_native_model_entry_fewer_than_five_is_atomic_without_new_graph_or_refund() {
    let g = eleven();
    let mut f = ready_with_graph(g.clone(), 35);
    let q = query_with_graph(&f, g);
    let proof = pop(&f, &q);
    let body = serde_json::to_vec(&q).unwrap();
    f.state
        .charge_test_session_v2_native_model_http_attempt(
            &f.session_id,
            SessionProofRequest {
                ingress_attempt_id: "small-first",
                original_bytes: &body,
                possession: &proof,
            },
            UnixMillis::new(13000),
        )
        .unwrap();
    f.state
        .reserve_test_session_v2_native_model_graph(
            &f.session_id,
            SessionProofRequest {
                ingress_attempt_id: "small-first",
                original_bytes: &body,
                possession: &proof,
            },
            UnixMillis::new(13000),
        )
        .unwrap();
    for i in 0..4 {
        f.state
            .charge_test_session_v2_native_model_http_attempt(
                &f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: &format!("small-paid-{i}"),
                    original_bytes: &body,
                    possession: &proof,
                },
                UnixMillis::new(13000),
            )
            .unwrap();
    }
    assert_eq!(remaining(&f), 4);
    let paid = f.state.clone();
    assert!(
        f.state
            .charge_test_session_v2_native_model_http_attempt(
                &f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: "small-reject",
                    original_bytes: &body,
                    possession: &proof
                },
                UnixMillis::new(13000)
            )
            .is_err()
    );
    assert_eq!(f.state, paid); // no partial fee, negative balance, graph or permit
}

#[test]
fn native_model_closed_profile_nonce_hint_preserves_old_parser_and_accepts_actual_new_tag() {
    use oasis7_custody_core::http_pop::{HttpHeader, http_pop_nonce_hint};
    let f = ready();
    let signed = query(&f);
    let snapshot = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    signed
        .verify_input_binding(snapshot.input(), 13000)
        .expect("actual N query signature and approved binding");
    let raw = raw_http(&f, &signed);
    let target = CanonicalHttpTarget::new(
        "https://custody.example",
        HttpPopRoute::ScopedClient {
            provision_id: f.state.draft().entry.provision_id.clone(),
            custody_account_id: f.state.draft().entry.custody_account_id.clone(),
            route: Box::new(HttpPopRoute::Actions),
        },
    )
    .unwrap();
    let token = URL_SAFE_NO_PAD.encode([31; 32]);
    let old_material = build_http_pop_signing_material(&HttpPopSigningInput {
        profile: HttpPopProfileV1::Custody,
        target: &target,
        key_id: &f.fact.registration.key_id,
        binding_id: &f.session_id,
        request_id: &signed.query.request_id,
        nonce: Some(&signed.query.query_nonce),
        token: Some(&token),
        created: UnixSeconds::new(13),
        expires: UnixSeconds::new(18),
        body: raw.body.as_bytes(),
    })
    .unwrap();
    let old_headers = old_material.signed_headers(
        &SigningKey::from_bytes(&[8; 32])
            .sign(old_material.signature_base())
            .to_bytes(),
    );
    let old_views: Vec<_> = old_headers
        .iter()
        .map(|h| HttpHeader {
            name: &h.name,
            value: &h.value,
        })
        .collect();
    assert_eq!(
        http_pop_nonce_hint(&old_views, HttpHeaderRepresentation::RawInstances).unwrap(),
        Some(signed.query.query_nonce.clone()),
        "old valid Custody tag positive"
    );
    let views: Vec<_> = raw
        .headers
        .iter()
        .map(|h| HttpHeader {
            name: &h.name,
            value: &h.value,
        })
        .collect();
    assert_eq!(
        http_pop_nonce_hint(&views, HttpHeaderRepresentation::RawInstances)
            .expect("actual closed NativeModelCustodyV2 signature tag must have a nonce hint"),
        Some(signed.query.query_nonce)
    );
}
