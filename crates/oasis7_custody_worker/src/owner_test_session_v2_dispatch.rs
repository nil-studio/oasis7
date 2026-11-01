//! Static role matrix and prepayment precede every remote authority observation.
use super::*;
fn json<T: serde::Serialize>(value: T) -> Result<String> {
    Ok(serde_json::to_string(&value)?)
}
pub(crate) async fn dispatch(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionRuntimeRpcV2,
    role: Role,
) -> Result<String> {
    enabled_v2(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let id = &rpc.request.session_id;
    let input = input_v2(&saved, &rpc.request)?;
    if role != Role::Identity
        && !(role == Role::Owner
            && matches!(
                &rpc.action,
                TestSessionRuntimeActionV2::GetPermit { .. }
                    | TestSessionRuntimeActionV2::PublishOwnerRead { .. }
            ))
    {
        return Err(denied());
    }
    if matches!(
        &rpc.action,
        TestSessionRuntimeActionV2::ClaimApprovalVerification
    ) {
        return json(
            store
                .claim_test_session_v2_approval(id)
                .map_err(|_| denied())?,
        );
    }
    if let TestSessionRuntimeActionV2::ConsumeApproval { original } = &rpc.action {
        if original.session_id != *id {
            return Err(denied());
        }
        let started = clock()?;
        let enrollment = session_authority::enrollment(env, enrollment_id(&input)?).await?;
        let observed =
            session_authority::observe_v2(env, &saved, input.clone(), &original.signed, false)
                .await?;
        store
            .consume_test_session_v2((**original).clone(), || {
                session_authority::approval_current_v2(state, env, &observed, &enrollment, started)
            })
            .await
            .map_err(|_| denied())?;
        return progress(&store, id);
    }
    let descriptor = saved
        .state
        .test_session_v2_preflight_descriptor(id)
        .map_err(|_| denied())?;
    let source = &descriptor.source_operation_id;
    match rpc.action {
        TestSessionRuntimeActionV2::DescribePreflight => json(&descriptor),
        TestSessionRuntimeActionV2::ClaimApprovalVerification
        | TestSessionRuntimeActionV2::ConsumeApproval { .. } => Err(denied()),
        TestSessionRuntimeActionV2::ReserveSource {
            operation_id,
            funding,
        } => {
            if funding != SessionSourceFundingV1::Control || operation_id != *source {
                return Err(denied());
            }
            json(
                store
                    .reserve_test_session_v2_source(id, &operation_id, funding)
                    .map_err(|_| denied())?,
            )
        }
        TestSessionRuntimeActionV2::ClaimSource { operation_id } => {
            if operation_id != *source {
                return Err(denied());
            }
            let observed =
                session_authority::runtime_observe_v2(env, &saved, &rpc.request, source, true)
                    .await?;
            json(
                store
                    .claim_test_session_v2_source(id, source, || {
                        session_authority::current_v2(state, env, &observed, false)
                    })
                    .map_err(|_| denied())?,
            )
        }
        TestSessionRuntimeActionV2::RecordSource {
            operation_id,
            original,
        } => {
            if operation_id != *source {
                return Err(denied());
            }
            let observed =
                session_authority::observe_v2(env, &saved, input, &original, false).await?;
            store
                .complete_test_session_v2_source(id, source, *original, || {
                    session_authority::current_v2(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            json(())
        }
        TestSessionRuntimeActionV2::ClaimPreflight => json(
            store
                .claim_test_session_v2_preflight(id)
                .map_err(|_| denied())?,
        ),
        TestSessionRuntimeActionV2::ClaimStep { step } => json(
            store
                .claim_test_session_v2_preflight_step(id, step)
                .map_err(|_| denied())?,
        ),
        TestSessionRuntimeActionV2::GetPermit { access } => {
            let (source_ticket, oracle_ticket) = store
                .reserve_test_session_v2_preflight_oracle(id, source, role)
                .map_err(|_| denied())?;
            let started = clock()?;
            let observed = if role == Role::Owner && matches!(oracle_ticket, 3 | 4) {
                session_authority::runtime_observe_paid_factory_v2(
                    env,
                    &saved,
                    &rpc.request,
                    source,
                )
                .await?
            } else {
                session_authority::runtime_observe_v2(env, &saved, &rpc.request, source, false)
                    .await?
            };
            let (permit, signing) = store
                .test_session_v2_preflight_permit(id, role, access.map(|v| *v), || {
                    Ok(TestSessionPreflightOwned {
                        native: session_authority::current_v2(state, env, &observed, true)?,
                        source_operation_id: source.clone(),
                        source_ticket,
                        oracle_ticket,
                        observed_at: started,
                    })
                })
                .await
                .map_err(|_| denied())?;
            match role {
                Role::Identity => {
                    if signing.is_some() {
                        return Err(denied());
                    }
                    json(permit)
                }
                Role::Owner => json(TestSessionSignerPermitV2 {
                    permit,
                    signing: signing.ok_or_else(denied)?,
                }),
                _ => Err(denied()),
            }
        }
        TestSessionRuntimeActionV2::PublishOwnerRead { original } => {
            let (source_ticket, oracle_ticket) = store
                .reserve_test_session_v2_preflight_oracle(id, source, role)
                .map_err(|_| denied())?;
            if role != Role::Owner || oracle_ticket != 5 {
                return Err(denied());
            }
            let started = clock()?;
            let observed = session_authority::runtime_observe_paid_factory_v2(
                env,
                &saved,
                &rpc.request,
                source,
            )
            .await?;
            json(
                store
                    .publish_test_session_v2_read(id, *original, || {
                        Ok(TestSessionPreflightOwned {
                            native: session_authority::current_v2(state, env, &observed, true)?,
                            source_operation_id: source.clone(),
                            source_ticket,
                            oracle_ticket,
                            observed_at: started,
                        })
                    })
                    .await
                    .map_err(|_| denied())?,
            )
        }
        TestSessionRuntimeActionV2::RecordLifetime { original } => {
            let source_ticket = store
                .reserve_test_session_v2_source_read(id, source)
                .map_err(|_| denied())?;
            let started = clock()?;
            let observed =
                session_authority::runtime_observe_v2(env, &saved, &rpc.request, source, false)
                    .await?;
            store
                .record_test_session_v2_lifetime(id, *original, || {
                    Ok(TestSessionPreflightOwned {
                        native: session_authority::current_v2(state, env, &observed, true)?,
                        source_operation_id: source.clone(),
                        source_ticket,
                        oracle_ticket: 5,
                        observed_at: started,
                    })
                })
                .await
                .map_err(|_| denied())?;
            json(())
        }
        TestSessionRuntimeActionV2::RecordLifetimeAndClaimMachineRead { original } => {
            enabled_machine_v2(env)?;
            // Same paid observation and source ticket as the old RecordLifetime.
            // The issuer cannot sign caller bytes; only a Core-produced token.
            let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)?;
            let source_ticket = store
                .reserve_test_session_v2_source_read(id, source)
                .map_err(|_| denied())?;
            let started = clock()?;
            let observed =
                session_authority::runtime_observe_v2(env, &saved, &rpc.request, source, false)
                    .await?;
            let request = store
                .record_lifetime_and_claim_machine_read_v2(id, *original, &issuer, || {
                    Ok(TestSessionPreflightOwned {
                        native: session_authority::current_v2(state, env, &observed, true)?,
                        source_operation_id: source.clone(),
                        source_ticket,
                        oracle_ticket: 5,
                        observed_at: started,
                    })
                })
                .await
                .map_err(|_| denied())?;
            json(request)
        }
        TestSessionRuntimeActionV2::ActivateRegisteredMachine {
            reply,
            private_sid_generation,
        } => {
            enabled_machine_v2(env)?;
            let original = saved
                .state
                .test_session_v2_machine_read_original(id)
                .map_err(|_| denied())?;
            let world = OwnerSetupWorldEvidenceV1 {
                request: original.owner_read,
                response: reply.world_response.clone(),
            };
            activate(
                state,
                env,
                &store,
                &saved,
                &rpc.request,
                world,
                private_sid_generation,
                Some(*reply),
            )
            .await
        }
        TestSessionRuntimeActionV2::Activate {
            world,
            private_sid_generation,
        } => {
            activate(
                state,
                env,
                &store,
                &saved,
                &rpc.request,
                *world,
                private_sid_generation,
                None,
            )
            .await
        }
    }
}
fn enrollment_id(input: &ApprovalAdmissionInput) -> Result<&str> {
    match &input.request.intent {
        oasis7_custody_core::approval::CustodyIntentV1::TestSessionV2 { scope, .. } => {
            Ok(&scope.client_pairing.enrollment_id)
        }
        _ => Err(denied()),
    }
}

#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/test_session_rpc.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch,js_name=readTestSessionAttestation)]
    async fn attest(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
async fn activate(
    state: &JsValue,
    env: &Env,
    store: &OwnerProvisionApprovalStore,
    saved: &ProvisionApprovalSnapshot,
    request: &TestSessionSelectorV1,
    world: OwnerSetupWorldEvidenceV1,
    generation: DecimalU64,
    registration: Option<oasis7_custody_core::native_cleanup_v2::NativeReadAndRegisterReplyV2>,
) -> Result<String> {
    let id = &request.session_id;
    let s = saved
        .state
        .test_session_v2_snapshot(id)
        .map_err(|_| denied())?;
    let lifetime = s.preflight_lifetime().cloned().ok_or_else(denied)?;
    let descriptor = saved
        .state
        .test_session_v2_preflight_descriptor(id)
        .map_err(|_| denied())?;
    // Pay the final source oracle before private binding/DF/RF reads.
    store
        .reserve_test_session_v2_source_read(id, &descriptor.source_operation_id)
        .map_err(|_| denied())?;
    let started = clock()?;
    #[cfg(feature = "approval-harness")]
    let diagnostic = crate::provision_approval_store::native::activation_diagnostic::Trace::new(
        started,
        saved
            .state
            .test_session_v2_source(id, &descriptor.source_operation_id)
            .and_then(|source| source.source.verification.as_ref())
            .map(|signed| signed.verification.observed_at.0),
    );
    let report = |stage: &'static str, error| {
        #[cfg(feature = "approval-harness")]
        diagnostic.fail(stage);
        #[cfg(not(feature = "approval-harness"))]
        let _ = stage;
        error
    };
    let attestation_request = serde_json::to_string(&TestSessionAttestationRequest {
        session: s.session().clone(),
        lifetime: lifetime.clone(),
    })?;
    // These fixed readonly inputs depend only on the same saved originals. Start
    // all three after prepayment; retain the common earliest observation bound.
    let (attested, enrolled, observed) = session_dispatch::activation_inputs(
        attest(env, &attestation_request),
        session_authority::enrollment(env, enrollment_id(s.input())?),
        session_authority::runtime_observe_v2(
            env,
            saved,
            request,
            &descriptor.source_operation_id,
            false,
        ),
    )
    .await;
    let attestation: TestSessionPrivateAttestation = serde_json::from_str(
        &super::super::super::text(attested.map_err(|_| report("join.attestation", denied()))?)
            .map_err(|e| report("attestation.text", e))?,
    )
    .map_err(|_| report("attestation.decode", denied()))?;
    let enrolled = enrolled.map_err(|e| report("join.enrollment", e))?;
    let observed = observed.map_err(|e| report("join.observation", e))?;
    if attestation.session_ref != s.session().session_ref
        || attestation.private_sid_generation != s.session().private_sid_generation
        || attestation.private_sid_generation != generation
        || attestation.lifetime_digest != lifetime.lifetime.digest().map_err(|_| denied())?
    {
        return Err(report("attestation.exact", denied()));
    }
    if let Some(reply) = registration {
        let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)?;
        let dispatch = store
            .claim_registered_native_activation_v2(
                id,
                reply,
                lifetime.clone(),
                world.clone(),
                attestation.private_sid_generation,
                descriptor.source_operation_id.clone(),
                &issuer,
                || {
                    session_authority::approval_current_v2(
                        state, env, &observed, &enrolled, started,
                    )
                },
            )
            .await
            .map_err(|_| denied())?;
        // Core has persisted the three inert Roots, Await gate and Native1601.
        // No Identity/Owner frontend round trip or local self fetch occurs.
        let ack = crate::native_machine_activation_transport::send(dispatch).await?;
        store
            .admit_native_activation_v2(
                id,
                ack,
                lifetime,
                world,
                attestation.private_sid_generation,
                descriptor.source_operation_id,
                || {
                    session_authority::approval_current_v2(
                        state, env, &observed, &enrolled, started,
                    )
                },
            )
            .await
            .map_err(|_| denied())?;
        return progress(store, id);
    }
    store
        .activate_test_session_v2(
            id,
            lifetime,
            world,
            attestation.private_sid_generation,
            descriptor.source_operation_id,
            || session_authority::approval_current_v2(state, env, &observed, &enrolled, started),
            #[cfg(feature = "approval-harness")]
            &diagnostic,
        )
        .await
        .map_err(|_| report("activate.store", denied()))?;
    progress(store, id)
}

fn progress(store: &OwnerProvisionApprovalStore, id: &str) -> Result<String> {
    let saved = store.snapshot().map_err(|_| denied())?;
    let session = saved
        .state
        .test_session_v2_snapshot(id)
        .map_err(|_| denied())?;
    json(TestSessionActivationProgressV2 {
        api_version: 2,
        custody_account_id: saved.state.draft().entry.custody_account_id.clone(),
        provision_id: saved.state.draft().entry.provision_id.clone(),
        session_id: id.into(),
        activated_at: session.approved_at(),
        preflight_pending: session.approved_at().is_none()
            && session.preflight_claimed_at().is_some(),
        closing: session.closing_metadata().is_some(),
    })
}
fn enabled_v2(env: &Env) -> Result<()> {
    if env
        .var("CUSTODY_TEST_SESSION_V2_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(denied());
    }
    Ok(())
}
fn input_v2(
    saved: &ProvisionApprovalSnapshot,
    request: &TestSessionSelectorV1,
) -> Result<ApprovalAdmissionInput> {
    let draft = saved.state.draft();
    if request.api_version != 2
        || request.custody_account_id != draft.entry.custody_account_id
        || request.provision_id != draft.entry.provision_id
    {
        return Err(denied());
    }
    saved
        .state
        .test_session_v2_snapshot(&request.session_id)
        .map(|s| s.input().clone())
        .map_err(|_| denied())
}

fn enabled_machine_v2(env: &Env) -> Result<()> {
    if env
        .var("CUSTODY_CLEANUP_MACHINE_V2_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(denied());
    }
    Ok(())
}
