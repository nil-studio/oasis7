use super::*;

pub(super) struct InitialRuntimeState {
    pub(super) runtime: Option<FormalLiveRuntime>,
    pub(super) initial_client: RemoteWorldServiceClient,
    pub(super) initial_view: Option<oasis7::world_service::verified_view::VerifiedWorldView>,
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
