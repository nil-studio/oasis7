use super::*;

pub(super) struct InitialRuntimeState {
    pub(super) runtime: Option<FormalLiveRuntime>,
    pub(super) initial_client: RemoteWorldServiceClient,
    pub(super) initial_view: Option<oasis7_world_service::verified_view::VerifiedWorldView>,
}

/// Start the non-shipping fixture runtime and obtain the initial signed view.
/// Special startup probes complete here exactly as they did in the caller.
pub(super) fn open_and_read_initial<F>(
    shipping_mode: bool,
    crash_phase: Option<&str>,
    runtime_config_path: &Path,
    world_identity: &WorldIdentity,
    status_bind: std::net::SocketAddr,
    scenario_deadline: Option<ScenarioDeadline>,
    client_config: &F,
) -> Result<Option<InitialRuntimeState>, String>
where
    F: Fn() -> WorldServiceClientConfig,
{
    let runtime = if shipping_mode {
        None
    } else {
        let mut runtime = FormalLiveRuntime::open_with_native_history(runtime_config_path)?;
        if crash_phase == Some("v3_read_gate_no_adapter") {
            let error = runtime
                .start()
                .expect_err("V3Required without evidence dependencies must fail before startup");
            if !error.contains("no production witness/history and protected-clock adapter") {
                return Err(format!(
                    "V3Required startup failed for the wrong reason: {error}"
                ));
            }
            if runtime.started || runtime.server.is_some() {
                return Err(
                    "V3Required without evidence dependencies opened the host listener".into(),
                );
            }
            let listener = TcpListener::bind(status_bind).map_err(|error| {
                format!("status socket was already bound after rejected startup: {error}")
            })?;
            drop(listener);
            // After the missing-dependency rejection, use the ordinary config
            // loader and start path, never explicit gate injection. The RPC
            // fixture signs real protocol objects but is NOT a protected-clock
            // or production witness qualification.
            let fixture = crate::node_lifecycle::StartupV3Fixture::new(
                runtime.validated.trust.initial_trust.clone(),
            );
            let mut configured = runtime.validated.config.clone();
            configured.full_sync_read_authorization_v3 = Some(fixture.configuration());
            runtime.validated.config = configured.clone();
            runtime
                .validated
                .config
                .full_sync_provider_read_authorization =
                crate::formal_live_config::FullSyncProviderReadAuthorizationModeV1::LegacyV1;
            let error = runtime
                .start()
                .expect_err("legacy mode must not accept V3 dependencies");
            if !error.contains("conflict with legacy_v1") || runtime.server.is_some() {
                return Err(format!(
                    "legacy/V3 configuration did not fail closed: {error}"
                ));
            }
            drop(TcpListener::bind(status_bind).map_err(|error| error.to_string())?);
            let replacement = runtime_config_path.with_extension("v3-pending.json");
            oracle_support::write_private(
                &replacement,
                &serde_json::to_vec(&configured).map_err(|error| error.to_string())?,
            )?;
            std::fs::rename(&replacement, runtime_config_path)
                .map_err(|error| error.to_string())?;
            std::fs::File::open(
                runtime_config_path
                    .parent()
                    .ok_or("config parent missing")?,
            )
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
            drop(runtime);
            for attempt in 0..2 {
                let mut reopened =
                    FormalLiveRuntime::open_with_native_history(runtime_config_path)?;
                reopened.start()?;
                let cut = reopened.published.current()?;
                let endpoint = format!("http://{status_bind}");
                if attempt == 0 {
                    let (status, body) = fixture.read_manifest(&endpoint, &cut.anchor);
                    let response: serde_json::Value =
                        serde_json::from_slice(&body).map_err(|error| error.to_string())?;
                    if status != 410 || response["payload"]["code"] != "history_unavailable" {
                        return Err(format!(
                            "configured ordinary V3 returned unexpected status during issuer outage: {status}"
                        ));
                    }
                }
                let (status, _) = fixture.read_manifest(&endpoint, &cut.anchor);
                reopened.stop(Duration::from_secs(10))?;
                drop(reopened);
                if status != 200 {
                    return Err(format!(
                        "configured ordinary V3 manifest rejected: {status}"
                    ));
                }
            }
            fixture.finish();
            eprintln!("configured ordinary V3 HTTP manifest withheld during issuer outage and restored on the same runtime");
            return Ok(None);
        }
        if crash_phase == Some("identity_runtime_lease") {
            identity_runtime_tests::exercise(runtime, runtime_config_path)?;
            return Ok(None);
        }
        if crash_phase == Some("controlled_stop_budget") {
            controlled_stop_tests::exercise(runtime)?;
            return Ok(None);
        }
        runtime.start()?;
        Some(runtime)
    };
    if matches!(
        crash_phase,
        Some(
            "healthy_set"
                | "healthy_set_long"
                | "healthy_operation"
                | "healthy_fault_writer_lock"
                | "healthy_fault_old_owner_sigkill"
        )
    ) && !shipping_mode
    {
        scenario_deadline
            .ok_or("healthy durable-set fixture lacks its scenario deadline")?
            .report_stage("original C0 runtime started")?;
    }
    let initial_client =
        RemoteWorldServiceClient::new(client_config()).map_err(|error| error.to_string())?;
    let initial_view = if shipping_mode {
        None
    } else {
        let view = read_current(&initial_client, world_identity)
            .map_err(|error| format!("initial externally pinned read: {error}"))?;
        assert_eq!(view.version().commit.position, 1);
        assert!(view.formal_commit().is_some());
        Some(view)
    };
    Ok(Some(InitialRuntimeState {
        runtime,
        initial_client,
        initial_view,
    }))
}
