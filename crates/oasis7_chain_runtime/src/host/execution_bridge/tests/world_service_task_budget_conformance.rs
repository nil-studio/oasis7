//! A single unpaid read-only model call must release its original Runtime turn.
use super::*;
use oasis7_agent_harness::{
    ModelProvider, ModelProviderError, ModelProviderRequest, ModelProviderResponse,
    ModelProviderTrace, ModelProviderTurn, ModelProviderUsage, NativeTaskRunConfig,
    NativeTaskRunner, NativeTaskStopReason, SdkNativeTaskHost,
};
use oasis7_game_client::{device_client::Client, operation_store::ClientStateKind};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::atomic::AtomicUsize};
struct ReadOnlyProvider(Arc<AtomicUsize>);
impl ModelProvider for ReadOnlyProvider {
    fn provider_id(&self) -> &str {
        "unpaid-read-only-budget-conformance"
    }
    fn complete(
        &mut self,
        request: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        assert_eq!(self.0.fetch_add(1, Ordering::SeqCst), 0);
        Ok(ModelProviderResponse {
            request_digest: request.request_digest.clone(),
            turns: vec![ModelProviderTurn::ToolCall {
                name: "observe".into(),
                arguments: json!({}),
            }],
            output: String::new(),
            model: Some("unpaid-conformance".into()),
            usage: ModelProviderUsage::default(),
            trace: ModelProviderTrace::default(),
        })
    }
}
struct StopFixture(PathBuf);
impl Drop for StopFixture {
    fn drop(&mut self) {
        let _ = fs::write(self.0.join("service-stop"), b"stop");
    }
}
#[test]
fn native_read_only_budget_terminates_original_and_releases_turn_before_expiry() {
    let _slot = prepared_world_probe_slot();
    let ready = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-budget-release-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let deadline = Instant::now() + Duration::from_secs(180);
    std::thread::scope(|scope| {
        let service = scope.spawn(|| {
            run_prepared_world_service_kind(
                ready.clone(),
                Duration::from_secs(5),
                false,
                None,
                false,
                Duration::from_secs(30),
                PreparedWorldKind::StarterChain,
                Duration::ZERO,
            )
        });
        while !ready.exists() {
            assert!(
                Instant::now() < deadline,
                "budget conformance readiness deadline"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let metadata: Value = serde_json::from_slice(&fs::read(&ready).unwrap()).unwrap();
        let root = PathBuf::from(metadata["root"].as_str().unwrap());
        let stop = StopFixture(root.clone());
        let device = PathBuf::from(metadata["config"].as_str().unwrap());
        let client = Arc::new(Client::open(&device).unwrap());
        let config = NativeTaskRunConfig {
            run_id: "read-only-budget-release".into(),
            model: "unpaid-conformance".into(),
            max_turns: 1,
            max_model_calls: 1,
            model_timeout_ms: 10_000,
            ..Default::default()
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let run = || {
            NativeTaskRunner::new(
                Arc::new(SdkNativeTaskHost::new(Arc::new(
                    Client::open(&device).unwrap(),
                ))),
                Box::new(ReadOnlyProvider(calls.clone())),
                config.clone(),
            )
            .unwrap()
            .run()
            .unwrap()
        };
        let report = run();
        assert_eq!(report.stop_reason, NativeTaskStopReason::BudgetExhausted);
        assert_eq!(report.model_calls_spent, 1);
        assert_eq!(report.turns_completed, 0);
        let operation = config.operation_id(0);
        assert!(
            client
                .cached_intent(ClientStateKind::Action, &operation)
                .unwrap()
                .is_none()
        );
        let prefix = client
            .cached_intent(ClientStateKind::Prefix, &operation)
            .unwrap()
            .unwrap();
        let oasis7_client_api::world_service_wire::WorldServicePayloadV1::Scheduler(signed) =
            prefix.signed_payload
        else {
            unreachable!()
        };
        let oasis7_client_api::world_service_wire::SchedulerOperationV1::TaskProviderPrefix {
            context,
        } = signed.request.operation
        else {
            unreachable!()
        };
        let original = client
            .cached_intent(ClientStateKind::Outcome, &operation)
            .unwrap()
            .unwrap();
        let oasis7_client_api::world_service_wire::WorldServicePayloadV1::Scheduler(failure) =
            &original.signed_payload
        else {
            unreachable!()
        };
        let oasis7_client_api::world_service_wire::SchedulerOperationV1::TaskProviderFailure {
            context: terminated,
            reason,
        } = &failure.request.operation
        else {
            unreachable!()
        };
        assert_eq!(terminated, &context);
        assert_eq!(reason, "transport_exhausted");
        assert!(matches!(
            client.poll_original(&original).unwrap().outcome,
            oasis7_client_api::world_service::IntentOutcome::Committed { .. }
        ));
        // Reopen the same durable budget namespace with a fresh host/provider.
        assert_eq!(run().stop_reason, NativeTaskStopReason::Aborted);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            client
                .cached_intent(ClientStateKind::Outcome, &operation)
                .unwrap()
                .unwrap(),
            original
        );
        // A new ordinary-client prefix is admitted while the old context lease
        // would still be valid: only the committed failure released its turn.
        let next = client.prefix("after-budget-termination").unwrap();
        assert!(matches!(
            client.submit_and_poll(&next).unwrap().outcome,
            oasis7_client_api::world_service::IntentOutcome::Committed { .. }
        ));
        let current = client.public_view().unwrap();
        assert!(current.logical_tick() < context.valid_until_tick.0);
        assert_eq!(current.task_fence().unwrap(), &context.fence);
        drop(stop);
        assert_eq!(service.join().unwrap(), root);
        fs::remove_file(&ready).unwrap();
        fs::remove_dir_all(root).unwrap();
    });
}
