use super::*;
#[path = "physical_port_counter.rs"]
mod physical_port_counter;
use std::{io::Read, net::TcpStream};
thread_local! {
    static LIVE_CONTEXT_PORTS: std::cell::RefCell<Option<Arc<[std::sync::atomic::AtomicUsize; 2]>>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn observe_live_context_ports(counts: Arc<[std::sync::atomic::AtomicUsize; 2]>) {
    LIVE_CONTEXT_PORTS.with(|slot| {
        slot.replace(Some(counts));
    });
}
fn count_live_context_port(method: &str, path: &str) {
    if method != "POST" {
        return;
    }
    let index = match path {
        oasis7_client_api::world_service::VIEW_JSON_PATH => 0,
        oasis7_client_api::world_service::task_context::TASK_CONTEXT_LIVE_READ_PATH => 1,
        _ => return,
    };
    LIVE_CONTEXT_PORTS.with(|slot| {
        if let Some(counts) = slot.borrow().as_ref() {
            counts[index].fetch_add(1, Ordering::SeqCst);
        }
    });
}
pub(super) fn serve(
    listener: TcpListener,
    node: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    root: &Path,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    stop: &AtomicBool,
    dropped: &AtomicBool,
) {
    serve_with_private(listener, node, root, signer, stop, dropped, None)
}
pub(super) fn serve_with_private(
    listener: TcpListener,
    node: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    root: &Path,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    stop: &AtomicBool,
    dropped: &AtomicBool,
    private: Option<&oasis7::world_service::private_goal_store::PrivateGoalStore>,
) {
    serve_with_private_settlement(
        listener, node, root, signer, stop, dropped, private, false, None,
    )
}

// Opt-in scheduling for a happy-path custody test, not a World API guarantee.
// Other fixtures retain asynchronous Received/Unknown behavior.
#[expect(
    clippy::too_many_arguments,
    reason = "Test-only opt-in preserves existing transport arguments and defaults"
)]
pub(super) fn serve_with_private_settlement(
    listener: TcpListener,
    node: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    root: &Path,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    stop: &AtomicBool,
    dropped: &AtomicBool,
    private: Option<&oasis7::world_service::private_goal_store::PrivateGoalStore>,
    settle_submits: bool,
    baseline_marker: Option<&Path>,
) {
    serve_with_private_settlement_observed(listener,node,root,signer,stop,dropped,private,settle_submits,baseline_marker,None);
}
#[expect(clippy::too_many_arguments, reason = "Test-only physical receiver evidence preserves the existing transport and defaults")]
pub(super) fn serve_with_private_settlement_observed(
    listener: TcpListener,
    node: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    root: &Path,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    stop: &AtomicBool,
    dropped: &AtomicBool,
    private: Option<&oasis7::world_service::private_goal_store::PrivateGoalStore>,
    settle_submits: bool,
    baseline_marker: Option<&Path>,
    physical_ports: Option<&Path>,
) {
    let mut physical = physical_port_counter::PhysicalPortCounter::new(physical_ports);
    while !stop.load(Ordering::SeqCst) {
        let (mut stream, _) = match listener.accept() {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => panic!("{e}"),
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            if stream.read_exact(&mut byte).is_err() {
                break;
            }
            bytes.push(byte[0]);
            assert!(bytes.len() < 65536);
        }
        let headers = std::str::from_utf8(&bytes).unwrap();
        let mut first = headers.split_whitespace();
        let method = first.next().unwrap().to_string();
        let path = first.next().unwrap().to_string();
        physical.record(&method, &path);
        count_live_context_port(&method, &path);
        let length = headers
            .split("\r\n")
            .filter_map(|line| line.split_once(':'))
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
            .unwrap_or(0);
        assert!(length <= 256 * 1024);
        let mut body = vec![0; length];
        stream.read_exact(&mut body).unwrap();
        let baseline_settlement = baseline_marker.is_some_and(|marker| marker.is_file());
        let settling =
            ((settle_submits || baseline_settlement) && path == SUBMIT_PATH).then(|| {
                serde_json::from_slice::<SubmitIntentRequest<WorldServicePayloadV1>>(&body)
                    .expect("happy fixture submit must be a typed original")
            });
        bytes.extend(body);
        if crate::world_service_api::private_goal::maybe_handle(
            &mut stream,
            &bytes,
            node,
            &method,
            &path,
            "w1",
            &root.join("world"),
            &root.join("records"),
            &root.join("store"),
            signer,
            private,
        )
        .unwrap()
        {
            continue;
        }
        if path == SUBMIT_PATH && !dropped.swap(true, Ordering::SeqCst) {
            // Run the genuine handler/admission, but discard its response at the transport boundary.
            let sink = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut writer = TcpStream::connect(sink.local_addr().unwrap()).unwrap();
            let (_reader, _) = sink.accept().unwrap();
            assert!(
                crate::world_service_api::maybe_handle(
                    &mut writer,
                    &bytes,
                    node,
                    &method,
                    &path,
                    "w1",
                    &root.join("world"),
                    &root.join("records"),
                    &root.join("store"),
                    signer
                )
                .unwrap()
            );
            drop(stream);
            continue;
        }
        assert!(
            crate::world_service_api::maybe_handle(
                &mut stream,
                &bytes,
                node,
                &method,
                &path,
                "w1",
                &root.join("world"),
                &root.join("records"),
                &root.join("store"),
                signer
            )
            .unwrap()
        );
        // Release the genuine response before awaiting execution. No reply bytes,
        // receipt, correlation or protocol lookup are generated by this barrier.
        drop(stream);
        if let Some(original) = settling {
            await_original_commit(node, root, &original);
        }
    }
}

fn await_original_commit(
    node: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    root: &Path,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) {
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let handle = node.lock().unwrap().committed_action_batches_handle();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        // Notifications only wake this observer; only the exact installed
        // World result below can establish settlement.
        node.lock().unwrap().drain_committed_action_batches();
        let current = pin(root, &original.correlation.key.world);
        if let Some(value) = current
            .world
            .capability_revocation_state()
            .world_service_results
            .get(&key)
        {
            correlation::validate_result(&key, value).unwrap();
            let result: CanonicalIntentResultV1 = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(
                result.request, *original,
                "fixture must settle the exact original"
            );
            assert!(
                result.rejected.is_none(),
                "happy fixture original was rejected"
            );
            return;
        }
        // This private fixture owns the batch observer. Draining its notification
        // cache does not remove installed canonical records or runtime actions.
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("happy fixture original did not commit within its bounded schedule");
        assert!(
            handle.wait_for_batches(remaining),
            "happy fixture commit notification timed out"
        );
    }
}
