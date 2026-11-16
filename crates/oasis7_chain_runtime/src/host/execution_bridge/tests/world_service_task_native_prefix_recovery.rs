//! A received prefix may remain unknown until the real worker publishes it.
use super::*;
use oasis7_agent_harness::{
    ModelProvider, ModelProviderError, ModelProviderRequest, ModelProviderResponse,
    NativeTaskRunConfig, NativeTaskRunner, NativeTaskStopReason, SdkNativeTaskHost,
};
use oasis7_game_client::{
    device_client::{Client, OperationDisposition},
    operation_store::ClientStateKind,
};
use std::sync::{atomic::AtomicUsize, mpsc};

type Original = SubmitIntentRequest<WorldServicePayloadV1>;

fn is_prefix(original: &Original) -> bool {
    matches!(&original.signed_payload, WorldServicePayloadV1::Scheduler(signed)
        if matches!(signed.request.operation, SchedulerOperationV1::TaskProviderPrefix { .. }))
}
pub(super) fn is_prefix_body(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Original>(bytes).is_ok_and(|original| is_prefix(&original))
}

pub(super) struct PrefixCommitGate {
    entered: mpsc::SyncSender<Original>,
    release: mpsc::Receiver<()>,
    pub(super) submits: Arc<AtomicUsize>,
}
pub(super) struct GatedPrefixHook {
    driver: Box<NodeRuntimeExecutionDriver>,
    gate: Option<PrefixCommitGate>,
}
impl GatedPrefixHook {
    pub(super) fn new(driver: NodeRuntimeExecutionDriver, gate: PrefixCommitGate) -> Self {
        Self {
            driver: Box::new(driver),
            gate: Some(gate),
        }
    }
}
impl oasis7_node::NodeExecutionHook for GatedPrefixHook {
    fn retained_local_commit_time(
        &mut self,
        context: &oasis7_node::NodeExecutionCommitContext,
    ) -> Result<Option<i64>, String> {
        self.driver.retained_local_commit_time(context)
    }
    fn on_commit(
        &mut self,
        context: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        self.on_commit_with_expected(context, None, None)
    }
    fn on_commit_with_expected(
        &mut self,
        context: oasis7_node::NodeExecutionCommitContext,
        expected_hash: Option<&str>,
        expected_root: Option<&str>,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        use oasis7::consensus_action_payload::{
            ConsensusActionPayloadBody, ConsensusActionPayloadEnvelope,
        };
        let prefix = context.committed_actions.iter().find_map(|action| {
            let envelope: ConsensusActionPayloadEnvelope =
                serde_cbor::from_slice(&action.payload_cbor).ok()?;
            let ConsensusActionPayloadBody::RuntimeAction {
                action: oasis7_runtime::Action::WorldServiceIntent { request },
            } = envelope.body
            else {
                return None;
            };
            let original: Original = serde_json::from_value(request).ok()?;
            is_prefix(&original).then_some(original)
        });
        if let Some(original) = prefix
            && let Some(gate) = self.gate.take()
        {
            gate.entered
                .send(original)
                .map_err(|_| "prefix observer closed")?;
            gate.release
                .recv_timeout(Duration::from_secs(90))
                .map_err(|_| "prefix release missing")?;
        }
        self.driver
            .on_commit_with_expected(context, expected_hash, expected_root)
    }
}

struct ReleasePrefix(Option<mpsc::Sender<()>>);
impl Drop for ReleasePrefix {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.send(());
        }
    }
}
struct StopService(std::path::PathBuf);
impl Drop for StopService {
    fn drop(&mut self) {
        let _ = fs::write(self.0.join("service-stop"), b"stop");
    }
}
struct NoInference;
impl ModelProvider for NoInference {
    fn provider_id(&self) -> &str {
        "unpaid-prefix-recovery-negative-control"
    }
    fn complete(
        &mut self,
        _: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        panic!("an unresolved prefix must not authorize inference")
    }
}

#[test]
fn native_prefix_pending_stops_without_resubmit_then_recovers_exact_committed_original() {
    let _slot = prepared_world_probe_slot();
    let ready = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-native-prefix-recovery-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let deadline = Instant::now() + Duration::from_secs(120);
    let submits = Arc::new(AtomicUsize::new(0));
    let (entered, observed) = mpsc::sync_channel(1);
    let (release, held) = mpsc::channel();
    let gate = PrefixCommitGate {
        entered,
        release: held,
        submits: submits.clone(),
    };
    std::thread::scope(|scope| {
        let service = scope.spawn(|| {
            run_prepared_world_service_configured(
                ready.clone(),
                Duration::from_secs(5),
                false,
                None,
                false,
                Duration::from_secs(30),
                PreparedWorldKind::StarterChain,
                Duration::ZERO,
                false,
                Some(gate),
            )
        });
        // Release the worker on assertion failure before the scoped service joins.
        let release = ReleasePrefix(Some(release));
        while !ready.exists() {
            assert!(
                Instant::now() < deadline,
                "prefix service readiness deadline"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(&ready).unwrap()).unwrap();
        let root = std::path::PathBuf::from(metadata["root"].as_str().unwrap());
        let stop = StopService(root.clone());
        let client =
            Arc::new(Client::open(Path::new(metadata["config"].as_str().unwrap())).unwrap());
        let host = Arc::new(SdkNativeTaskHost::new(client.clone()));
        let config = NativeTaskRunConfig {
            run_id: "native-prefix-pending".into(),
            model: "unpaid-prefix-test".into(),
            max_turns: 1,
            max_model_calls: 1,
            model_timeout_ms: 1000,
            ..NativeTaskRunConfig::default()
        };
        let operation = config.operation_id(0);
        let first = NativeTaskRunner::new(host.clone(), Box::new(NoInference), config.clone())
            .unwrap()
            .run();
        let enqueued = observed.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(submits.load(Ordering::SeqCst), 1);
        let original = client
            .cached_intent(ClientStateKind::Prefix, &operation)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&original).unwrap(),
            serde_json::to_value(enqueued).unwrap(),
            "the real worker must hold the exact SDK original"
        );
        assert!(matches!(
            client.lookup(&original).unwrap().outcome,
            IntentOutcome::Unknown
        ));
        assert!(
            first.is_ok(),
            "verified pending prefix must be a recoverable run report, not a host error: {first:?}"
        );
        let first = first.unwrap();
        assert_eq!(first.stop_reason, NativeTaskStopReason::PendingOriginal);
        assert_eq!(first.last_operation.as_deref(), Some(operation.as_str()));
        assert_eq!((first.turns_completed, first.model_calls_spent), (0, 0));
        let second = NativeTaskRunner::new(host, Box::new(NoInference), config.clone())
            .unwrap()
            .run()
            .unwrap();
        assert_eq!(second.stop_reason, NativeTaskStopReason::PendingOriginal);
        assert_eq!((second.turns_completed, second.model_calls_spent), (0, 0));
        assert_eq!(
            submits.load(Ordering::SeqCst),
            1,
            "unknown restart may only query"
        );
        for kind in [ClientStateKind::Action, ClientStateKind::Outcome] {
            assert!(client.cached_intent(kind, &operation).unwrap().is_none());
        }
        assert!(
            client
                .cached_intent(ClientStateKind::Prefix, &config.operation_id(1))
                .unwrap()
                .is_none()
        );
        drop(release);
        loop {
            assert!(
                Instant::now() < deadline,
                "original prefix recovery deadline"
            );
            match client.recover_operation(&operation).unwrap().disposition {
                OperationDisposition::ContextReady => break,
                OperationDisposition::Unknown
                | OperationDisposition::Pending
                | OperationDisposition::Received => (),
                other => panic!("original prefix did not commit: {other:?}"),
            }
        }
        assert_eq!(
            client
                .cached_intent(ClientStateKind::Prefix, &operation)
                .unwrap()
                .unwrap(),
            original
        );
        assert_eq!(submits.load(Ordering::SeqCst), 1);
        drop(client);
        drop(stop);
        assert_eq!(service.join().unwrap(), root);
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(&ready).unwrap();
    });
}
