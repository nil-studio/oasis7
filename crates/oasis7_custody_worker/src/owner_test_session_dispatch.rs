//! Static role matrix and prepayment precede every remote authority observation.
use super::*;
fn json<T: serde::Serialize>(value: T) -> Result<String> {
    Ok(serde_json::to_string(&value)?)
}
pub(crate) async fn dispatch(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionRuntimeRpc,
    role: Role,
) -> Result<String> {
    session_authority::enabled(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let id = &rpc.request.session_id;
    let input = session_authority::input(&saved, &rpc.request)?;
    if role != Role::Identity
        && !(role == Role::Owner
            && matches!(
                &rpc.action,
                TestSessionRuntimeAction::GetPermit { .. }
                    | TestSessionRuntimeAction::PublishOwnerRead { .. }
            ))
    {
        return Err(denied());
    }
    if matches!(
        &rpc.action,
        TestSessionRuntimeAction::ClaimApprovalVerification
    ) {
        return json(
            store
                .claim_test_session_approval(id)
                .map_err(|_| denied())?,
        );
    }
    if let TestSessionRuntimeAction::ConsumeApproval { original } = &rpc.action {
        if original.session_id != *id {
            return Err(denied());
        }
        let started = clock()?;
        let enrollment = session_authority::enrollment(env, enrollment_id(&input)?).await?;
        let observed =
            session_authority::observe(env, &saved, input.clone(), &original.signed, false).await?;
        store
            .consume_test_session((**original).clone(), || {
                session_authority::approval_current(state, env, &observed, &enrollment, started)
            })
            .await
            .map_err(|_| denied())?;
        return progress(&store, id);
    }
    let descriptor = saved
        .state
        .test_session_preflight_descriptor(id)
        .map_err(|_| denied())?;
    let source = &descriptor.source_operation_id;
    match rpc.action {
        TestSessionRuntimeAction::DescribePreflight => json(&descriptor),
        TestSessionRuntimeAction::ClaimApprovalVerification
        | TestSessionRuntimeAction::ConsumeApproval { .. } => Err(denied()),
        TestSessionRuntimeAction::ReserveSource {
            operation_id,
            funding,
        } => {
            if funding != SessionSourceFundingV1::Control || operation_id != *source {
                return Err(denied());
            }
            json(
                store
                    .reserve_test_session_source(id, &operation_id, funding)
                    .map_err(|_| denied())?,
            )
        }
        TestSessionRuntimeAction::ClaimSource { operation_id } => {
            if operation_id != *source {
                return Err(denied());
            }
            let observed =
                session_authority::runtime_observe(env, &saved, &rpc.request, source, true).await?;
            json(
                store
                    .claim_test_session_source(id, source, || {
                        session_authority::current(state, env, &observed, false)
                    })
                    .map_err(|_| denied())?,
            )
        }
        TestSessionRuntimeAction::RecordSource {
            operation_id,
            original,
        } => {
            if operation_id != *source {
                return Err(denied());
            }
            let observed = session_authority::observe(env, &saved, input, &original, false).await?;
            store
                .complete_test_session_source(id, source, *original, || {
                    session_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            json(())
        }
        TestSessionRuntimeAction::ClaimPreflight => json(
            store
                .claim_test_session_preflight(id)
                .map_err(|_| denied())?,
        ),
        TestSessionRuntimeAction::ClaimStep { step } => json(
            store
                .claim_test_session_preflight_step(id, step)
                .map_err(|_| denied())?,
        ),
        TestSessionRuntimeAction::GetPermit { access } => {
            let (source_ticket, oracle_ticket) = store
                .reserve_test_session_preflight_oracle(id, source, role)
                .map_err(|_| denied())?;
            let started = clock()?;
            let observed = if role == Role::Owner && oracle_ticket == 3 {
                session_authority::runtime_observe_paid_factory(env, &saved, &rpc.request, source)
                    .await?
            } else {
                session_authority::runtime_observe(env, &saved, &rpc.request, source, false).await?
            };
            let (permit, signing) = store
                .test_session_preflight_permit(id, role, access.map(|v| *v), || {
                    Ok(TestSessionPreflightOwned {
                        native: session_authority::current(state, env, &observed, true)?,
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
                Role::Owner => json(TestSessionSignerPermit {
                    permit,
                    signing: signing.ok_or_else(denied)?,
                }),
                _ => Err(denied()),
            }
        }
        TestSessionRuntimeAction::PublishOwnerRead { original } => {
            let (source_ticket, oracle_ticket) = store
                .reserve_test_session_preflight_oracle(id, source, role)
                .map_err(|_| denied())?;
            if oracle_ticket != 5 {
                return Err(denied());
            }
            let started = clock()?;
            let observed =
                session_authority::runtime_observe(env, &saved, &rpc.request, source, false)
                    .await?;
            json(
                store
                    .publish_test_session_read(id, *original, || {
                        Ok(TestSessionPreflightOwned {
                            native: session_authority::current(state, env, &observed, true)?,
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
        TestSessionRuntimeAction::RecordLifetime { original } => {
            let source_ticket = store
                .reserve_test_session_source_read(id, source)
                .map_err(|_| denied())?;
            let started = clock()?;
            let observed =
                session_authority::runtime_observe(env, &saved, &rpc.request, source, false)
                    .await?;
            store
                .record_test_session_lifetime(id, *original, || {
                    Ok(TestSessionPreflightOwned {
                        native: session_authority::current(state, env, &observed, true)?,
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
        TestSessionRuntimeAction::Activate {
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
            )
            .await
        }
    }
}
pub(super) fn enrollment_id(input: &ApprovalAdmissionInput) -> Result<&str> {
    match &input.request.intent {
        oasis7_custody_core::approval::CustodyIntentV1::TestSession {
            client_pairing: Some(pairing),
            ..
        } => Ok(&pairing.enrollment_id),
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
) -> Result<String> {
    let id = &request.session_id;
    let s = saved.state.test_sessions().get(id).ok_or_else(denied)?;
    let lifetime = s.preflight_lifetime.clone().ok_or_else(denied)?;
    let descriptor = saved
        .state
        .test_session_preflight_descriptor(id)
        .map_err(|_| denied())?;
    // Pay the final source oracle before private binding/DF/RF reads.
    store
        .reserve_test_session_source_read(id, &descriptor.source_operation_id)
        .map_err(|_| denied())?;
    let started = clock()?;
    #[cfg(feature = "approval-harness")]
    let diagnostic = crate::provision_approval_store::native::activation_diagnostic::Trace::new(
        started,
        s.sources
            .get(&descriptor.source_operation_id)
            .and_then(|source| source.verification.as_ref())
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
        session: s.session.clone(),
        lifetime: lifetime.clone(),
    })?;
    // These fixed readonly inputs depend only on the same saved originals. Start
    // all three after prepayment; retain the common earliest observation bound.
    let (attested, enrolled, observed) = activation_inputs(
        attest(env, &attestation_request),
        session_authority::enrollment(env, enrollment_id(&s.input)?),
        session_authority::runtime_observe(
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
    if attestation.session_ref != s.session.session_ref
        || attestation.private_sid_generation != s.session.private_sid_generation
        || attestation.private_sid_generation != generation
        || attestation.lifetime_digest != lifetime.lifetime.digest().map_err(|_| denied())?
    {
        return Err(report("attestation.exact", denied()));
    }
    store
        .activate_test_session(
            id,
            lifetime,
            world,
            attestation.private_sid_generation,
            descriptor.source_operation_id,
            || session_authority::approval_current(state, env, &observed, &enrolled, started),
            #[cfg(feature = "approval-harness")]
            &diagnostic,
        )
        .await
        .map_err(|_| report("activate.store", denied()))?;
    progress(store, id)
}

// Poll every independent read without spawning tasks or introducing a runtime
// dependency. Completed futures are never polled again, including error results.
async fn activation_inputs<A, B, C>(a: A, b: B, c: C) -> (A::Output, B::Output, C::Output)
where
    A: std::future::Future,
    B: std::future::Future,
    C: std::future::Future,
{
    use std::future::poll_fn;
    use std::task::Poll;
    let (mut a, mut b, mut c) = (std::pin::pin!(a), std::pin::pin!(b), std::pin::pin!(c));
    let (mut first, mut second, mut third) = (None, None, None);
    poll_fn(|cx| {
        if first.is_none() {
            if let Poll::Ready(value) = a.as_mut().poll(cx) {
                first = Some(value);
            }
        }
        if second.is_none() {
            if let Poll::Ready(value) = b.as_mut().poll(cx) {
                second = Some(value);
            }
        }
        if third.is_none() {
            if let Poll::Ready(value) = c.as_mut().poll(cx) {
                third = Some(value);
            }
        }
        match (&first, &second, &third) {
            (Some(_), Some(_), Some(_)) => Poll::Ready((
                first.take().expect("completed attestation read"),
                second.take().expect("completed enrollment read"),
                third.take().expect("completed authority read"),
            )),
            _ => Poll::Pending,
        }
    })
    .await
}

fn progress(store: &OwnerProvisionApprovalStore, id: &str) -> Result<String> {
    json(
        store
            .snapshot()
            .map_err(|_| denied())?
            .state
            .test_session_progress(
                id,
                oasis7_custody_core::time_units::UnixMillis::new(clock()?),
            )
            .map_err(|_| denied())?,
    )
}
