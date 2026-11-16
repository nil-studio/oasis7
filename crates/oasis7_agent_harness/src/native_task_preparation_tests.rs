use super::*;

struct ObservedHost {
    inner: FakeHost,
    state: NativeTaskRecovery,
}
impl NativeTaskHost for ObservedHost {
    fn initialize_run(&self, config: &NativeTaskRunConfig) -> Result<u32, String> {
        self.inner.initialize_run(config)
    }
    fn reserve_model_call(&self, _: &NativeTaskRunConfig, _: &str) -> Result<u32, String> {
        panic!("blocked preparation must not reserve inference")
    }
    fn recover(&self, operation: &str) -> Result<NativeTaskRecovery, String> {
        self.inner.recover(operation)
    }
    fn settle_exhausted_context(&self, _: &str) -> Result<NativeTaskRecovery, String> {
        panic!("blocked preparation must not terminate a context")
    }
    fn prepare(&self, _: &str) -> Result<NativeTaskContext, String> {
        panic!("the runner must consume the typed preparation observation")
    }
    fn prepare_observed(&self, operation: &str) -> Result<NativeTaskPreparation, String> {
        self.inner
            .state
            .lock()
            .unwrap()
            .events
            .push(format!("observed:{operation}"));
        Ok(NativeTaskPreparation::Blocked(self.state))
    }
    fn current_control(&self) -> Result<NativeTaskControl, String> {
        self.inner.current_control()
    }
    fn invoke(
        &self,
        _: &NativeTaskContext,
        _: &NativeTaskTool,
    ) -> Result<NativeTaskToolResult, String> {
        panic!("blocked preparation must not execute a tool")
    }
}

#[test]
fn verified_preparation_states_stop_without_model_spend_or_later_slot() {
    for (state, expected) in [
        (
            NativeTaskRecovery::Pending,
            NativeTaskStopReason::PendingOriginal,
        ),
        (
            NativeTaskRecovery::Rejected,
            NativeTaskStopReason::RejectedOriginal,
        ),
        (
            NativeTaskRecovery::Expired,
            NativeTaskStopReason::ExpiredOriginal,
        ),
        (NativeTaskRecovery::Aborted, NativeTaskStopReason::Aborted),
        // These are invalid blocked preparations, never completed business effects.
        (
            NativeTaskRecovery::Vacant,
            NativeTaskStopReason::HostFailure,
        ),
        (NativeTaskRecovery::Ready, NativeTaskStopReason::HostFailure),
        (
            NativeTaskRecovery::Settled,
            NativeTaskStopReason::HostFailure,
        ),
        (
            NativeTaskRecovery::ContextRetired,
            NativeTaskStopReason::HostFailure,
        ),
    ] {
        let host = Arc::new(ObservedHost {
            inner: FakeHost::new(),
            state,
        });
        let report = NativeTaskRunner::new(host.clone(), provider(vec![]), config())
            .unwrap()
            .run()
            .unwrap();
        assert_eq!(report.stop_reason, expected);
        assert_eq!((report.turns_completed, report.model_calls_spent), (0, 0));
        assert_eq!(report.last_operation.as_deref(), Some("operator-run:0"));
        assert_eq!(
            host.inner
                .events()
                .iter()
                .filter(|event| event.starts_with("observed:"))
                .count(),
            1
        );
    }
}

#[test]
fn legacy_prepare_errors_are_preserved_not_parsed_as_verified_pending() {
    for error in [
        "context prefix has not committed; retain original operation and lookup",
        "signature invalid",
        "transport unavailable",
        "corrupt original cache",
    ] {
        let host = Arc::new(FakeHost::new());
        host.state.lock().unwrap().prepare_error = Some(error.into());
        let result = NativeTaskRunner::new(host.clone(), provider(vec![]), config())
            .unwrap()
            .run();
        assert_eq!(result.unwrap_err(), error);
        assert_eq!(host.state.lock().unwrap().spent, 0);
        assert!(!host.events().iter().any(|event| event.starts_with("tool:")));
    }
}
