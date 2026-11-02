//! Genuine HTTP prefix, real shared locks and ordinary UTC. No fake success.
use super::*;
use oasis7_client_api::world_service::task_mutation_qualified_v2::*;
fn wrapper(close: &TaskCleanupSubmitV2, millis: u64) -> TaskMutationWaitSubmitV2 {
    close.verify_owner_signature().unwrap();
    let q = TaskMutationWaitSubmitV2 {
        api_revision: 2,
        reply_profile: TaskMutationReplyProfileV2::QualifiedEffectV2,
        submitted: TaskMutationSubmittedV2::RevokeAndClose(Box::new(close.clone())),
        delivery_nonce: "de".repeat(32),
        delivery_until_ms: DecimalU64(
            now_unix_ms()
                .checked_add(millis)
                .unwrap()
                .min(close.original.request.cleanup_until_ms.0),
        ),
    };
    q.validate().unwrap();
    q
}
fn send(
    service: &Arc<FormalLiveService>,
    q: &TaskMutationWaitSubmitV2,
) -> (
    std::sync::mpsc::Receiver<(u16, Vec<u8>)>,
    std::thread::JoinHandle<()>,
) {
    let service = Arc::clone(service);
    let body = serde_json::to_vec(q).unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        tx.send(exchange(&service, TASK_CLEANUP_SUBMIT_V2, &body))
            .unwrap();
    });
    (rx, worker)
}
fn strong_ack(
    service: &FormalLiveService,
    close: &TaskCleanupSubmitV2,
    ack: &SignedServiceResponse<TaskMutationAdmissionV2>,
) {
    verify_service_response(
        TASK_CLEANUP_SUBMIT_V2,
        &authority::request_digest(TASK_CLEANUP_SUBMIT_V2, close).unwrap(),
        ack,
        &hex::encode(service.signer.verifying_key().to_bytes()),
    )
    .unwrap();
    ack.payload
        .validate(
            TaskMutationKindV2::RevokeAndClose,
            &close.expected_world,
            &close.original_digest().unwrap(),
        )
        .unwrap();
}
pub(super) fn runtime_admission(
    service: &Arc<FormalLiveService>,
    node: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    control: &test_control::BoundaryControl,
    close: &TaskCleanupSubmitV2,
) {
    let task = &close
        .original
        .request
        .permitted_branches
        .exact_initial_active;
    let before = service.published.current().unwrap();
    let before_task = before
        .world
        .current_game_task_state_v2(&task.agent_id)
        .unwrap();
    let before_signer = before
        .world
        .capability_revocation_state()
        .agent_signer_delegations
        .get(&task.agent_id)
        .cloned();
    let before_stop = before
        .world
        .capability_revocation_state()
        .task_stops_v2
        .clone();
    assert!(before_task.is_some() && before_stop.is_some());
    assert!(
        before
            .world
            .closed_game_task_v2(&task.task_id)
            .unwrap()
            .is_none()
    );
    let q = wrapper(close, 300);
    let guard = node.lock().unwrap();
    let (rx, worker) = send(service, &q);
    // Probe only local locks; no business retry or second Submit is made.
    let until = Instant::now() + Duration::from_millis(100);
    let mut gate_available = false;
    while Instant::now() < until {
        if service.admission_gate.try_lock().is_ok() {
            gate_available = true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let early = rx.recv_timeout(Duration::from_millis(600)).ok();
    let returned_while_held = early.is_some();
    drop(guard);
    // A broken implementation may have queued only after the lock is released.
    // Release and join before any semantic assertion, so the test cannot strand it.
    control.release().unwrap();
    let response = early.or_else(|| rx.recv_timeout(Duration::from_secs(10)).ok());
    worker.join().unwrap();
    let (status, raw) = response.expect("actual HTTP reply after release/join");
    assert!(
        returned_while_held,
        "qualified admission must end at the original deadline while the real Node lock remains held"
    );
    assert!(
        gate_available,
        "qualified admission must not retain the shared gate while waiting for a real Node lock"
    );
    assert_ne!(
        status, 200,
        "no admission occurred before its delivery window ended"
    );
    let error: SignedServiceResponse<FormalServiceErrorV1> = serde_json::from_slice(&raw).unwrap();
    assert_eq!(error.payload.code, "mutation_not_queued");
    // raw_error signs the exact incoming bytes, independently of a bare request.
    let sent = serde_json::to_vec(&q).unwrap();
    verify_service_response(
        TASK_CLEANUP_SUBMIT_V2,
        &authority::request_digest(TASK_CLEANUP_SUBMIT_V2, &sent.as_slice()).unwrap(),
        &error,
        &hex::encode(service.signer.verifying_key().to_bytes()),
    )
    .unwrap();
    // No second Submit belongs to this expired, unadmitted invocation. Stop the
    // actual Node before checking that the genuine Stop and signer HWM remain.
    node.lock().unwrap().stop().unwrap();
    let after = service.published.current().unwrap();
    assert!(
        after
            .world
            .current_game_task_state_v2(&task.agent_id)
            .unwrap()
            == before_task
    );
    assert!(
        after
            .world
            .capability_revocation_state()
            .agent_signer_delegations
            .get(&task.agent_id)
            .cloned()
            == before_signer
    );
    assert!(after.world.capability_revocation_state().task_stops_v2 == before_stop);
    assert!(
        after
            .world
            .closed_game_task_v2(&task.task_id)
            .unwrap()
            .is_none()
    );
}
pub(super) fn publication_wait(
    service: &Arc<FormalLiveService>,
    publication: &Arc<PublishedReadState>,
    control: &test_control::BoundaryControl,
    close: &TaskCleanupSubmitV2,
    pause_deadline: Instant,
) -> SignedServiceResponse<TaskMutationAdmissionV2> {
    let q = wrapper(close, 5000);
    let (rx, worker) = send(service, &q);
    // The actual Node reaches the exact admitted Close before we hold this lock.
    control.wait(pause_deadline).unwrap();
    let guard = publication.current.lock().unwrap();
    let deadline_open_when_held = now_unix_ms() < q.delivery_until_ms.0;
    let gate_available = service.admission_gate.try_lock().is_ok();
    let early = rx.recv_timeout(Duration::from_millis(5500)).ok();
    let returned_while_held = early.is_some();
    drop(guard);
    control.release().unwrap();
    let response = early.or_else(|| rx.recv_timeout(Duration::from_secs(10)).ok());
    worker.join().unwrap();
    let (status, raw) = response.expect("actual admitted HTTP reply after release/join");
    assert!(
        deadline_open_when_held,
        "genuine admission must reach the actual paused Close before the original delivery deadline"
    );
    assert!(
        returned_while_held,
        "qualified wait must end while the real publication lock remains held"
    );
    assert!(
        gate_available,
        "waiting for publication must not retain admission gate"
    );
    assert_eq!(status, 200);
    let reply = TaskMutationSubmitReplyV2::decode_strict(&raw).unwrap();
    let TaskMutationSubmitReplyV2::Queued { admission, .. } = reply else {
        panic!("paused Close cannot already be qualified");
    };
    strong_ack(service, close, &admission);
    admission
}
