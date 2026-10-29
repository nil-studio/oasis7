//! Real fixed Native signatures; no actor/network/Current authority is inferred.
use super::*;
struct StorageFixture {
    f: SessionFixture,
    original: ws::SignedReadRequest<TaskContextPrepareV1>,
    prepared: ws::SignedServiceResponse<PreparedTaskContextV1>,
    scope: ContextBodyScopeV1,
    lifetime: SignedManagedNativeSessionLifetimeV1,
}
fn fixture() -> StorageFixture {
    let (mut f, _, _) = claimed();
    let (original, prepared) = prepared_artifact(&f);
    let lifetime = exec_source(&mut f, "storage-factory-source", "e1", 12_000);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let pin = f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &lifetime, "storage-factory-source", 12_002);
    f.state
        .register_test_session_context(
            ID,
            original.clone(),
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap();
    let scope = f
        .state
        .validate_test_session_prepared_body(
            ID,
            &prepared,
            &current,
            UnixMillis::new(12_002),
            &mut StrictCrypto,
        )
        .unwrap()
        .scope()
        .clone();
    StorageFixture {
        f,
        original,
        prepared,
        scope,
        lifetime,
    }
}
fn prepared_input<'a>(f: &'a StorageFixture) -> PreparedContextStorageInput<'a> {
    PreparedContextStorageInput {
        scope: &f.scope,
        trusted_world: &f.f.state.draft().world_identity,
        original: &f.original,
        response: &f.prepared,
    }
}
#[test]
fn context_storage_factory_prepared_and_captured_exact_manifests_without_account_grant() {
    let f = fixture();
    let before = f.f.state.clone();
    let input = prepared_input(&f);
    let checks = prepared_context_storage_checks(&input).unwrap();
    assert_eq!(checks.checks().len(), 3);
    let body =
        validate_prepared_context_storage(&input, &mut ExactCache::verified(checks)).unwrap();
    assert!(body.scope() == &f.scope);
    let (get, prefix, captured) = captured_artifact(&f.f, &f.prepared.payload, &f.original);
    let mut captured_scope = f.scope.clone();
    captured_scope.slot = ContextBodySlotKindV1::Captured;
    let input = CapturedContextStorageInput {
        scope: &captured_scope,
        prepared: prepared_input(&f),
        original: &get,
        prefix: &prefix,
        response: &captured,
    };
    let checks = captured_context_storage_checks(&input).unwrap();
    assert_eq!(checks.checks().len(), 7);
    let prepared_outer = ws::signing_bytes(
        TASK_CONTEXT_PREPARE_PATH,
        &(&f.prepared.request_digest, &f.prepared.payload),
    )
    .unwrap();
    assert!(
        checks
            .checks()
            .iter()
            .any(|v| v.canonical_bytes() == prepared_outer)
    );
    let body =
        validate_captured_context_storage(&input, &mut ExactCache::verified(checks)).unwrap();
    assert!(body.scope() == &captured_scope);
    assert!(
        body.canonical_bytes()
            == oasis7_wasm_abi::encode_canonical_cbor(&captured.payload).unwrap()
    );
    assert_eq!(f.f.state, before);
    // A valid historical body does not register Get, consume an actor port,
    // allocate a source, or authorize a write through Account.
    assert!(
        f.f.state.test_sessions()[ID]
            .context
            .as_ref()
            .unwrap()
            .get
            .is_none()
    );
    assert!(
        f.f.state.test_sessions()[ID]
            .context
            .as_ref()
            .unwrap()
            .slots
            .is_empty()
    );
}
#[test]
fn context_storage_factory_exact_cache_rejects_missing_replaced_jobs_and_changed_scope() {
    let f = fixture();
    let input = prepared_input(&f);
    let original_checks = prepared_context_storage_checks(&input).unwrap();
    let (get, prefix, captured) = captured_artifact(&f.f, &f.prepared.payload, &f.original);
    let mut captured_scope = f.scope.clone();
    captured_scope.slot = ContextBodySlotKindV1::Captured;
    let captured_input = CapturedContextStorageInput {
        scope: &captured_scope,
        prepared: prepared_input(&f),
        original: &get,
        prefix: &prefix,
        response: &captured,
    };
    // This real verified subset has three jobs, not the complete seven.
    assert!(
        validate_captured_context_storage(
            &captured_input,
            &mut ExactCache::verified(original_checks.clone()),
        )
        .is_err()
    );
    let mut changed_scope = f.scope.clone();
    changed_scope.session_intent_digest = "ed".repeat(32);
    let changed = PreparedContextStorageInput {
        scope: &changed_scope,
        ..prepared_input(&f)
    };
    // The scope is structural storage metadata, not approval. Its independent
    // manifest must change even though the same historic signatures are valid.
    let changed_checks = prepared_context_storage_checks(&changed).unwrap();
    StrictCrypto.verify(&changed_checks).unwrap();
    assert!(changed_checks != original_checks);
    assert!(
        validate_prepared_context_storage(
            &changed,
            &mut ExactCache::verified(original_checks.clone()),
        )
        .is_err()
    );
    let mut alternate = f.prepared.payload.clone();
    alternate.snapshot.actor_observation["storage-alternate"] = true.into();
    resign(&mut alternate);
    alternate.validate_against(&f.original).unwrap();
    let alternate = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        f.prepared.request_digest.clone(),
        alternate,
        &"3f".repeat(32),
    )
    .unwrap();
    let changed = PreparedContextStorageInput {
        response: &alternate,
        ..prepared_input(&f)
    };
    let alternate_checks = prepared_context_storage_checks(&changed).unwrap();
    StrictCrypto.verify(&alternate_checks).unwrap();
    assert!(
        validate_prepared_context_storage(&input, &mut ExactCache::verified(alternate_checks),)
            .is_err()
    );
    // An independently valid but different Prepared cannot supply the old
    // Captured's original WorldPrepared signature provenance.
    let wrong_dependency = CapturedContextStorageInput {
        prepared: changed,
        ..captured_input
    };
    assert!(captured_context_storage_checks(&wrong_dependency).is_err());
}
fn resign(v: &mut PreparedTaskContextV1) {
    v.identity.request_commitment = v.commitment("oasis7/private-task-request/v1").unwrap();
    v.identity.context_commitment = v.commitment("oasis7/private-task-context/v1").unwrap();
    v.identity.provider_invocation_commitment =
        v.commitment("oasis7/private-task-invocation/v1").unwrap();
}
#[test]
fn context_storage_factory_fixed_pin_nested_signatures_and_strong_wrong_semantics_reject() {
    let f = fixture();
    let mut wrong_pin = f.f.state.draft().world_identity.clone();
    wrong_pin.branch_id = "wrong-fixed-branch".into();
    assert!(
        prepared_context_storage_checks(&PreparedContextStorageInput {
            trusted_world: &wrong_pin,
            ..prepared_input(&f)
        })
        .is_err()
    );
    let mut nested = f.prepared.payload.clone();
    nested.snapshot.active_goal.owner_proof.signature_hex = "00".repeat(64);
    resign(&mut nested);
    let nested = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        f.prepared.request_digest.clone(),
        nested,
        &"3f".repeat(32),
    )
    .unwrap();
    ws::private_goal::verify_owner_signature_strict(
        TASK_CONTEXT_PREPARE_PATH,
        &(&nested.request_digest, &nested.payload),
        &f.f.state.draft().world_identity.service_public_key,
        &nested.signature_hex,
    )
    .unwrap();
    assert!(
        prepared_context_storage_checks(&PreparedContextStorageInput {
            response: &nested,
            ..prepared_input(&f)
        })
        .is_err()
    );
    let mut semantic = f.prepared.payload.clone();
    semantic.identity.fence.branch_id = "foreign-branch".into();
    let semantic = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        f.prepared.request_digest.clone(),
        semantic,
        &"3f".repeat(32),
    )
    .unwrap();
    verify_private_reply(
        TASK_CONTEXT_PREPARE_PATH,
        &f.original,
        &semantic,
        &f.f.state.draft().world_identity.service_public_key,
    )
    .unwrap();
    assert!(
        prepared_context_storage_checks(&PreparedContextStorageInput {
            response: &semantic,
            ..prepared_input(&f)
        })
        .is_err()
    );
    let mut bad_outer = f.prepared.clone();
    bad_outer.signature_hex = "00".repeat(64);
    assert!(
        prepared_context_storage_checks(&PreparedContextStorageInput {
            response: &bad_outer,
            ..prepared_input(&f)
        })
        .is_err()
    );
    let (get, mut prefix, captured) = captured_artifact(&f.f, &f.prepared.payload, &f.original);
    prefix.response.signature_hex = "00".repeat(64);
    let mut captured_scope = f.scope.clone();
    captured_scope.slot = ContextBodySlotKindV1::Captured;
    assert!(
        captured_context_storage_checks(&CapturedContextStorageInput {
            scope: &captured_scope,
            prepared: prepared_input(&f),
            original: &get,
            prefix: &prefix,
            response: &captured,
        })
        .is_err()
    );
}
#[test]
fn context_storage_factory_opaque_still_cannot_bypass_account_source_or_scope() {
    let mut f = fixture();
    let body = validate_prepared_context_storage(&prepared_input(&f), &mut StrictCrypto).unwrap();
    let record = envelope(body.scope(), body.canonical_bytes().len());
    let life = exec_source(&mut f.f, "different-storage-source", "e2", 12_010);
    let policy = Policy::new(f.f.state.draft());
    let provision = policy.current(&f.f.context);
    let pin = f.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &life, "different-storage-source", 12_012);
    StrictCrypto
        .verify(
            &f.f.state
                .test_session_live_checks(
                    ID,
                    SessionPurposeV1::Execution,
                    &current,
                    UnixMillis::new(12_012),
                )
                .unwrap(),
        )
        .unwrap();
    let before = f.f.state.clone();
    assert!(
        f.f.state
            .claim_test_session_context_write(
                ID,
                ContextBodyWrite {
                    attempt: "wrong-storage-source",
                    body: &body,
                    candidate: &record
                },
                &current,
                UnixMillis::new(12_012),
                &mut StrictCrypto,
            )
            .is_err()
    );
    assert_eq!(f.f.state, before);
    let correct = live(
        &provision,
        &ns,
        &f.lifetime,
        "storage-factory-source",
        12_012,
    );
    StrictCrypto
        .verify(
            &f.f.state
                .test_session_live_checks(
                    ID,
                    SessionPurposeV1::Execution,
                    &correct,
                    UnixMillis::new(12_012),
                )
                .unwrap(),
        )
        .unwrap();
    let mut wrong_scope = record.clone();
    wrong_scope.scope.session_id = "foreign-storage-session".into();
    assert!(
        f.f.state
            .claim_test_session_context_write(
                ID,
                ContextBodyWrite {
                    attempt: "wrong-storage-scope",
                    body: &body,
                    candidate: &wrong_scope
                },
                &correct,
                UnixMillis::new(12_012),
                &mut StrictCrypto,
            )
            .is_err()
    );
    assert_eq!(f.f.state, before);
}

#[test]
fn context_storage_factory_strong_wrong_domain_and_oversized_body_reject() {
    let f = fixture();
    let wrong_domain = ws::sign_service_response(
        TASK_CONTEXT_READ_PATH,
        f.prepared.request_digest.clone(),
        f.prepared.payload.clone(),
        &"3f".repeat(32),
    )
    .unwrap();
    ws::private_goal::verify_owner_signature_strict(
        TASK_CONTEXT_READ_PATH,
        &(&wrong_domain.request_digest, &wrong_domain.payload),
        &f.f.state.draft().world_identity.service_public_key,
        &wrong_domain.signature_hex,
    )
    .unwrap();
    assert!(
        prepared_context_storage_checks(&PreparedContextStorageInput {
            response: &wrong_domain,
            ..prepared_input(&f)
        })
        .is_err()
    );
    let mut oversized = f.prepared.payload.clone();
    oversized.snapshot.actor_observation["storage-full-body-padding"] =
        serde_json::Value::String("x".repeat(CONTEXT_BODY_MAX_BYTES));
    resign(&mut oversized);
    oversized.validate_against(&f.original).unwrap();
    let oversized = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        f.prepared.request_digest.clone(),
        oversized,
        &"3f".repeat(32),
    )
    .unwrap();
    verify_private_reply(
        TASK_CONTEXT_PREPARE_PATH,
        &f.original,
        &oversized,
        &f.f.state.draft().world_identity.service_public_key,
    )
    .unwrap();
    assert_eq!(
        prepared_context_storage_checks(&PreparedContextStorageInput {
            response: &oversized,
            ..prepared_input(&f)
        })
        .err(),
        Some(ContextStorageError::Artifact(
            ContextBodyError::ContextTooLarge
        ))
    );
}
