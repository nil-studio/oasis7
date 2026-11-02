//! Genuine activated/claimed Book + strong Native Source/LF, not a fake grant.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
fn hexbytes(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
use oasis7_custody_core::native_cleanup_v2::*;
struct PreparedService {
    f: SessionFixture,
    proofs: SignedExecutionPreparePermitsV2,
    bundle: ExecutionSigningBundleV2,
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    fact: SignedManagedNativeSessionLifetimeV1,
}
fn prepared_service(
    remaining: Option<u64>,
    before_source: impl FnOnce(&mut SessionFixture),
) -> PreparedService {
    let ReadyPrepare {
        mut f,
        body,
        pop,
        fact,
    } = ready_with_current_ticket_and_hook(None, None, false, before_source);
    let paid = prepay(&mut f, &body, &pop);
    let ports = f
        .state
        .claim_test_session_v2_prepare_current_ports(&f.session_id, &paid, UnixMillis::new(14_006))
        .unwrap()
        .unwrap();
    assert_eq!(ports.read().source_ticket, 5);
    let authority = admit(&mut f, &fact, &paid, 5, 14_007).unwrap();
    let bundle = authority.bundle().clone();
    let service = SigningKey::from_bytes(&[0x71; 32]);
    let public = hexbytes(&service.verifying_key().to_bytes());
    assert_ne!(public, authority.executor_key().public_key);
    assert_ne!(public, bundle.fence.owner_public_key);
    let issuer = AccountMachineIssuerV2 {
        namespace: "fixture-account-issuer".into(),
        service_id: "fixture-account-service".into(),
        environment: authority
            .executor_scope()
            .context
            .identity
            .environment
            .clone(),
        key_id: "fixture-account-issuer-key".into(),
        issuer_epoch: DecimalU64(7),
        config_generation: DecimalU64(11),
        native_audience_origin: "https://native.fixture.test".into(),
    };
    let pin = AccountMachineIssuerPinV2 {
        namespace: issuer.namespace.clone(),
        service_id: issuer.service_id.clone(),
        environment: issuer.environment.clone(),
        key_id: issuer.key_id.clone(),
        public_key: public,
        issuer_epoch: issuer.issuer_epoch,
        config_generation: issuer.config_generation,
        not_before: DecimalU64(1000),
        not_after: DecimalU64(80_000),
        native_audience_origin: issuer.native_audience_origin.clone(),
        allowed_purpose: AccountMachinePurposeV2::ExecutionPrepareSignerV2,
    };
    let floor = AccountMachineIssuerFloorV2 {
        issuer_epoch: issuer.issuer_epoch,
        config_generation: issuer.config_generation,
    };
    // These timestamps describe historical Core fixture Current, not actual
    // Worker Directory/RF HTTP qualification or an external clock override.
    let permits = authority
        .into_service_permits(
            issuer,
            DecimalU64(14_006),
            DecimalU64(14_006),
            UnixMillis::new(19_000),
        )
        .unwrap();
    let sign_one =
        |permit: AccountExecutionPreparePermitV2| SignedAccountExecutionPreparePermitV2 {
            signature_hex: hexbytes(&service.sign(&permit.signing_bytes().unwrap()).to_bytes()),
            permit,
        };
    let proofs = SignedExecutionPreparePermitsV2 {
        initialize: sign_one(permits.initialize),
        sign: sign_one(permits.sign),
    };
    if let Some(remaining) = remaining {
        spend_to(&mut f, &body, &pop, remaining);
    }
    PreparedService {
        f,
        proofs,
        bundle,
        pin,
        floor,
        fact,
    }
}
fn spend_to(
    f: &mut SessionFixture,
    action_bytes: &[u8],
    action_pop: &oasis7_custody_core::http_pop::HttpPossessionReceipt,
    remaining: u64,
) {
    let action: SessionActionRequestV2 = serde_json::from_slice(action_bytes).unwrap();
    let metadata = SessionChallengeRequestV1 {
        api_version: 1,
        session_id: f.session_id.clone(),
        request_id: "rkv-threshold-metadata".into(),
        purpose: SessionPurposeV1::Execution,
        kind: SessionChallengeKindV1::Tool,
        logical_request_id: action.request_id.clone(),
        operation_request_digest: action.digest().unwrap(),
    };
    let metadata_bytes = serde_json::to_vec(&metadata).unwrap();
    let http = signed_http(
        f,
        &metadata_bytes,
        &metadata.request_id,
        (
            HttpPopRoute::ProofChallenges,
            HttpPopProfileV1::CustodyChallenge,
        ),
        None,
        SessionPurposeV1::Execution,
        14_007,
    );
    let c = SessionClientHttpContext {
        public_origin: &f.fact.registration.public_origin,
        custody_account_id: &f.state.draft().entry.custody_account_id,
        provision_id: &f.state.draft().entry.provision_id,
        operation: SessionClientOperation::ToolNonce,
        representation: HttpHeaderRepresentation::RawInstances,
    };
    let prepared = f
        .state
        .prepare_test_session_v2_tool_nonce_http(&http, &c, UnixMillis::new(14_007))
        .unwrap();
    let metadata_pop =
        verify_http_possession(prepared, UnixMillis::new(14_007), &mut StrictCrypto).unwrap();
    let left = |f: &SessionFixture| {
        f.state
            .test_session_v2_snapshot(&f.session_id)
            .unwrap()
            .roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap()
    };
    let mut i = 0u64;
    while left(f) > remaining {
        let before = left(f);
        let ingress = format!("rkv-threshold-physical-{i}");
        let units = if before - remaining >= 3 {
            // This is a genuine metadata ingress, not an Actions original.
            f.state
                .charge_test_session_v2_tool_nonce_http_attempt(
                    SessionProofRequest {
                        ingress_attempt_id: &ingress,
                        original_bytes: &metadata_bytes,
                        possession: &metadata_pop,
                    },
                    UnixMillis::new(14_007),
                )
                .unwrap();
            3
        } else {
            charge(
                f,
                action_bytes,
                action_pop,
                &ingress,
                SessionPurposeV1::Execution,
                14_007,
            )
            .unwrap();
            1
        };
        assert_eq!(left(f), before - units);
        i = i.checked_add(1).unwrap();
    }
    assert_eq!(left(f), remaining);
}
fn packet() -> (
    SignedExecutionPreparePermitsV2,
    ExecutionSigningBundleV2,
    AccountMachineIssuerPinV2,
    AccountMachineIssuerFloorV2,
) {
    let PreparedService {
        mut f,
        proofs,
        bundle,
        pin,
        floor,
        ..
    } = prepared_service(None, |_| {});
    let before = f.state.clone();
    let mut invalid = proofs.clone();
    invalid.sign.signature_hex = "00".repeat(64);
    assert!(
        f.state
            .record_test_session_v2_prepare_service_permits(
                &f.session_id,
                invalid,
                &pin,
                &floor,
                UnixMillis::new(14_007)
            )
            .is_err()
    );
    assert_eq!(before, f.state);
    let dispatch = f
        .state
        .record_test_session_v2_prepare_service_permits(
            &f.session_id,
            proofs.clone(),
            &pin,
            &floor,
            UnixMillis::new(14_007),
        )
        .unwrap()
        .expect("first durable pair must return its unique dispatch token");
    assert!(
        dispatch.permits() == &proofs,
        "exact original permits retained"
    );
    assert_ne!(dispatch.port_ids()[0], dispatch.port_ids()[1]);
    let raw_before = serde_json::to_value(&before).unwrap();
    let raw_after = serde_json::to_value(&f.state).unwrap();
    let spent = |raw: &serde_json::Value, purpose: &str| -> u64 {
        serde_json::from_value::<DecimalU64>(raw["test_sessions_v2"][&f.session_id]
            ["roots"][purpose]["root"]["budgets"]["transport"]["spent"].clone()).unwrap().0
    };
    assert_eq!(
        spent(&raw_after, "execution"),
        spent(&raw_before, "execution") + 4
    );
    assert_eq!(
        spent(&raw_after, "reconcile_read"),
        spent(&raw_before, "reconcile_read")
    );
    for field in ["control", "identity_transport", "connector_transport"] {
        assert_eq!(
            raw_after["test_sessions_v2"][&f.session_id][field],
            raw_before["test_sessions_v2"][&f.session_id][field]
        );
    }
    let committed = f.state.clone();
    let duplicate = f
        .state
        .record_test_session_v2_prepare_service_permits(
            &f.session_id,
            proofs.clone(),
            &pin,
            &floor,
            UnixMillis::new(14_007),
        )
        .unwrap();
    assert!(
        duplicate.is_none(),
        "history cannot create another outbound token"
    );
    assert_eq!(committed, f.state);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
    assert_eq!(cold, f.state);
    assert!(cold.test_session_v2_prepare_service_permits(&f.session_id) == Some(&proofs));
    for (index, key) in dispatch.port_ids().iter().enumerate() {
        let mut stripped = serde_json::to_value(&cold).unwrap();
        assert!(
            stripped["test_sessions_v2"][&f.session_id]["purpose_transports"]
                .as_object_mut()
                .unwrap()
                .remove(key)
                .is_some()
        );
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(stripped).is_err());
        let mut downgraded = serde_json::to_value(&cold).unwrap();
        downgraded["test_sessions_v2"][&f.session_id]["purpose_transports"][key] =
            serde_json::json!("execution");
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(downgraded).is_err());
        let mut altered = serde_json::to_value(&cold).unwrap();
        altered["test_sessions_v2"][&f.session_id]["execution_book"]["stages"]["prepare"]["service_record"]
            ["port_ids"][index] = serde_json::json!(dispatch.port_ids()[(index + 1) % 4]);
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(altered).is_err());
    }
    let mut raw = serde_json::to_value(&cold).unwrap();
    let events = raw["test_sessions_v2"][&f.session_id]["events"]
        .as_array_mut()
        .unwrap();
    let allocated = events
        .iter()
        .position(|e| e["event"]["kind"] == "execution_prepare_allocated")
        .unwrap();
    let recorded = events
        .iter()
        .position(|e| e["event"]["kind"] == "execution_prepare_permits_recorded")
        .unwrap();
    assert!(allocated < recorded);
    assert_eq!(events[allocated]["at"], events[recorded]["at"]);
    events.swap(allocated, recorded);
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err());
    let mut raw = serde_json::to_value(&cold).unwrap();
    raw["test_sessions_v2"][&f.session_id]["execution_book"]["stages"]["prepare"]
        .as_object_mut()
        .unwrap()
        .remove("service_record");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err());
    (proofs, bundle, pin, floor)
}
#[test]
fn session_v2_prepare_signer_strong_pin_exact_candidate_and_recipient() {
    let (proofs, bundle, pin, floor) = packet();
    let proof = proofs.sign;
    let scope = proof.permit.recipient_scope.clone();
    let public = proof.permit.recipient_key.clone();
    let actual = proof
        .clone()
        .verify_owned(
            &pin,
            &floor,
            bundle.clone(),
            ExecutionPrepareRecipientV2 {
                scope: &scope,
                key: &public,
                operation: ExecutionPrepareSignerOperationV2::SignPrepare,
            },
            UnixMillis::new(14_008),
        )
        .unwrap();
    assert!(actual.bundle() == &bundle);
    assert_eq!(actual.once_operation_digest().unwrap().len(), 64);
    let mut wrong = pin.clone();
    wrong.allowed_purpose = AccountMachinePurposeV2::NativeTermCurrentAttempt;
    assert!(
        proof
            .verify(
                &wrong,
                &floor,
                &bundle,
                ExecutionPrepareRecipientV2 {
                    scope: &scope,
                    key: &public,
                    operation: ExecutionPrepareSignerOperationV2::SignPrepare
                },
                UnixMillis::new(14_008)
            )
            .is_err()
    );
    let mut retired = floor.clone();
    retired.config_generation = DecimalU64(12);
    assert!(
        proof
            .verify(
                &pin,
                &retired,
                &bundle,
                ExecutionPrepareRecipientV2 {
                    scope: &scope,
                    key: &public,
                    operation: ExecutionPrepareSignerOperationV2::SignPrepare
                },
                UnixMillis::new(14_008)
            )
            .is_err()
    );
    let mut changed = proof.clone();
    changed.permit.recipient_route.push_str("-other");
    assert!(
        changed
            .verify(
                &pin,
                &floor,
                &bundle,
                ExecutionPrepareRecipientV2 {
                    scope: &scope,
                    key: &public,
                    operation: ExecutionPrepareSignerOperationV2::SignPrepare
                },
                UnixMillis::new(14_008)
            )
            .is_err()
    );
    let mut raw = serde_json::to_value(&proof).unwrap();
    raw["verified"] = serde_json::json!(true);
    assert!(serde_json::from_value::<SignedAccountExecutionPreparePermitV2>(raw).is_err());
    assert!(
        proof
            .verify(
                &pin,
                &floor,
                &bundle,
                ExecutionPrepareRecipientV2 {
                    scope: &scope,
                    key: &public,
                    operation: ExecutionPrepareSignerOperationV2::SignPrepare
                },
                proof.permit.expires_at
            )
            .is_err()
    );
    assert!(
        proofs
            .initialize
            .verify(
                &pin,
                &floor,
                &bundle,
                ExecutionPrepareRecipientV2 {
                    scope: &scope,
                    key: &public,
                    operation: ExecutionPrepareSignerOperationV2::SignPrepare
                },
                UnixMillis::new(14_008)
            )
            .is_err()
    );
}
#[test]
fn session_v2_prepare_signer_complete_sdk_signatures_and_fixture_export() {
    let (proofs, bundle, pin, floor) = packet();
    let proof = proofs.sign;
    let ExecutionSigningActionV2::Prepare {
        original,
        observation_nonce,
    } = bundle.action.as_ref()
    else {
        panic!("Prepare");
    };
    let output = ExecutionPrepareSignedOriginalV2 {
        access: oasis7_client_api::world_service::entry_executor::EntryExecutorAccessV1 {
            read: oasis7_client_api::world_service::sign_read_request(oasis7_client_api::world_service::VIEW_JSON_PATH, bundle.observation.clone(), &"41".repeat(32)).unwrap(),
            fence: bundle.fence.clone(),
        },
        request: oasis7_client_api::world_service::task_context_projection::TaskContextPrepareAttestationRequestV2 {
            original: oasis7_client_api::world_service::sign_read_request(oasis7_client_api::world_service::task_context::TASK_CONTEXT_PREPARE_PATH, original.as_ref().clone(), &"41".repeat(32)).unwrap(),
            observation_nonce: observation_nonce.clone(),
        },
    };
    bundle.verify_prepare_original(&output).unwrap();
    let mut bad = output.clone();
    bad.access.read.request.observation_nonce = Some("ab".repeat(32));
    assert!(bundle.verify_prepare_original(&bad).is_err());
    let mut bad = output.clone();
    bad.request.observation_nonce = "ef".repeat(32);
    assert!(bundle.verify_prepare_original(&bad).is_err());
    let proof_digest = proof
        .clone()
        .verify_owned(
            &pin,
            &floor,
            bundle.clone(),
            ExecutionPrepareRecipientV2 {
                scope: &proof.permit.recipient_scope,
                key: &proof.permit.recipient_key,
                operation: ExecutionPrepareSignerOperationV2::SignPrepare,
            },
            UnixMillis::new(14_008),
        )
        .unwrap()
        .proof_digest()
        .unwrap();
    if let Ok(path) = std::env::var("OASIS7_EXECUTION_PREPARE_SIGNER_FIXTURE") {
        let public = serde_json::json!({"profile":"historical-core-fixture-not-paid-worker-current", "proof":proof,"initialize_proof":proofs.initialize,"bundle":bundle,"pin":pin,"floor":floor,"actor_name":proof.permit.recipient_scope.task_digest().unwrap(),"expected_original":output,"expected_original_digest":oasis7_custody_core::approval::bytes_digest(&oasis7_wasm_abi::encode_canonical_cbor(&output).unwrap()),"proof_digest":proof_digest});
        std::fs::write(path, serde_json::to_vec(&public).unwrap()).unwrap();
    }
}

#[path = "test_session_v2_execution_prepare_last_paid_tests.rs"]
mod last_paid_tests;

#[path = "test_session_v2_prepare_original_archive_tests.rs"]
mod original_archive_tests;
