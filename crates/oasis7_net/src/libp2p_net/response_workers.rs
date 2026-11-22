use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use futures::SinkExt;
use futures::channel::mpsc as futures_mpsc;
use libp2p::request_response;
use libp2p::swarm::Swarm;
use oasis7_proto::distributed::DistributedErrorCode;
#[cfg(test)]
use oasis7_proto::distributed_net::FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW;
use oasis7_proto::distributed_net::{
    FETCH_BLOB_MAX_RAW_CHUNK_BYTES, fetch_blob_legacy_json_encoded_upper_bound,
};

use crate::error::WorldError;
use oasis7_proto::distributed_net::NetworkResponse;

use super::response_budget::{
    FETCH_BLOB_PROTOCOL, FetchBlobResponseBudget, budgeted_response_bytes,
};
use super::{Behaviour, Handler};

const CONTROL_QUEUE_CAPACITY: usize = 32;
const HEAD_QUEUE_CAPACITY: usize = 8;
const BLOB_QUEUE_CAPACITY: usize = 8;
const CONTROL_WORKERS: usize = 2;
const HEAD_WORKERS: usize = 1;
const BLOB_WORKERS: usize = 2;
const COMPLETION_QUEUE_CAPACITY: usize = CONTROL_WORKERS + HEAD_WORKERS + BLOB_WORKERS;
const CONTROL_MAX_IN_FLIGHT: usize = CONTROL_QUEUE_CAPACITY + CONTROL_WORKERS;
const HEAD_MAX_IN_FLIGHT: usize = HEAD_QUEUE_CAPACITY + HEAD_WORKERS;
const BLOB_MAX_IN_FLIGHT: usize = BLOB_QUEUE_CAPACITY + BLOB_WORKERS;
const BLOB_MAX_IN_FLIGHT_BYTES: usize = 80 * 1024 * 1024;
const FETCH_BLOB_MAX_RESPONSE_BYTES: usize = FETCH_BLOB_MAX_RAW_CHUNK_BYTES;
const FETCH_COMMIT_HEAD_PROTOCOL: &str = "/aw/node/replication/fetch-commit/head/1.0.0";

#[derive(Clone, Copy)]
enum ResponseLane {
    Control,
    Head,
    Blob,
}

struct InFlightState {
    control_jobs: usize,
    head_jobs: usize,
    blob_jobs: usize,
    blob_bytes: usize,
}

struct InFlightReservation {
    state: Arc<Mutex<InFlightState>>,
    lane: ResponseLane,
    bytes: usize,
}

impl Drop for InFlightReservation {
    fn drop(&mut self) {
        let mut state = self.state.lock().expect("response worker in-flight state");
        match self.lane {
            ResponseLane::Control => state.control_jobs = state.control_jobs.saturating_sub(1),
            ResponseLane::Head => state.head_jobs = state.head_jobs.saturating_sub(1),
            ResponseLane::Blob => {
                state.blob_jobs = state.blob_jobs.saturating_sub(1);
                state.blob_bytes = state.blob_bytes.saturating_sub(self.bytes);
            }
        }
    }
}

pub(super) struct CompletedResponse {
    pub channel: request_response::ResponseChannel<NetworkResponse>,
    pub protocol: String,
    pub peer: libp2p::PeerId,
    pub queued_at_ms: i64,
    deadline: std::time::Instant,
    pub reply: Result<Vec<u8>, WorldError>,
    auth_release: Option<super::authenticated_reply::ReplyGuard>,
    _reservation: InFlightReservation,
}

pub(super) struct Job {
    channel: request_response::ResponseChannel<NetworkResponse>,
    protocol: String,
    peer: libp2p::PeerId,
    payload: Vec<u8>,
    queued_at_ms: i64,
    deadline: std::time::Instant,
    handler: Handler,
    reservation: InFlightReservation,
    cancelled: Arc<AtomicBool>,
    auth_release: Option<super::authenticated_reply::ReplyGuard>,
}

pub(super) struct RejectedResponse {
    pub channel: request_response::ResponseChannel<NetworkResponse>,
    pub protocol: String,
    pub peer: libp2p::PeerId,
    pub reply: WorldError,
}

pub(super) struct ResponseWorkers {
    control: mpsc::SyncSender<Job>,
    head: mpsc::SyncSender<Job>,
    blob: mpsc::SyncSender<Job>,
    in_flight: Arc<Mutex<InFlightState>>,
    response_deadline: std::time::Duration,
}

impl ResponseWorkers {
    pub(super) fn new(
        response_deadline: std::time::Duration,
    ) -> (
        Self,
        futures_mpsc::Receiver<CompletedResponse>,
        FetchBlobResponseBudget,
    ) {
        let (completed, receiver) = futures_mpsc::channel(COMPLETION_QUEUE_CAPACITY);
        let (control, control_rx) = mpsc::sync_channel(CONTROL_QUEUE_CAPACITY);
        let (head, head_rx) = mpsc::sync_channel(HEAD_QUEUE_CAPACITY);
        let (blob, blob_rx) = mpsc::sync_channel(BLOB_QUEUE_CAPACITY);
        spawn_workers(control_rx, CONTROL_WORKERS, completed.clone());
        spawn_workers(head_rx, HEAD_WORKERS, completed.clone());
        spawn_workers(blob_rx, BLOB_WORKERS, completed);
        (
            Self {
                control,
                head,
                blob,
                in_flight: Arc::new(Mutex::new(InFlightState {
                    control_jobs: 0,
                    head_jobs: 0,
                    blob_jobs: 0,
                    blob_bytes: 0,
                })),
                response_deadline,
            },
            receiver,
            FetchBlobResponseBudget::default(),
        )
    }

    #[expect(
        clippy::result_large_err,
        clippy::too_many_arguments,
        reason = "RejectedResponse carries the response channel and wire context together for immediate rejection"
    )]
    pub(super) fn schedule(
        &self,
        channel: request_response::ResponseChannel<NetworkResponse>,
        protocol: String,
        peer: libp2p::PeerId,
        payload: Vec<u8>,
        queued_at_ms: i64,
        handler: Handler,
        auth_release: Option<super::authenticated_reply::ReplyGuard>,
    ) -> Result<(), RejectedResponse> {
        let response_bytes = match response_reservation_bytes(protocol.as_str(), payload.as_slice())
        {
            Ok(bytes) => bytes,
            Err(reply) => {
                return Err(RejectedResponse {
                    channel,
                    protocol,
                    peer,
                    reply,
                });
            }
        };
        // Take the original deadline on a monotonic clock, independently of
        // the wall-clock queued_at_ms used only for diagnostics. A wall-clock
        // rollback must not extend an already queued handler's work window.
        let Some(deadline) = std::time::Instant::now().checked_add(self.response_deadline) else {
            return Err(RejectedResponse {
                channel,
                protocol,
                peer,
                reply: Self::overload_response(),
            });
        };
        let Some(reservation) = self.reserve(protocol.as_str(), response_bytes) else {
            return Err(RejectedResponse {
                channel,
                protocol,
                peer,
                reply: Self::overload_response(),
            });
        };
        self.submit(Job {
            channel,
            protocol,
            peer,
            payload,
            queued_at_ms,
            deadline,
            handler,
            reservation,
            cancelled: Arc::new(AtomicBool::new(false)),
            auth_release,
        })
        .map_err(|job| RejectedResponse {
            channel: job.channel,
            protocol: job.protocol,
            peer: job.peer,
            reply: Self::overload_response(),
        })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Completion keeps the response channel, budget, metrics, and swarm borrows explicit"
    )]
    pub(super) fn complete(
        &self,
        completed: CompletedResponse,
        budget: &mut FetchBlobResponseBudget,
        now_ms: i64,
        traffic_metrics: &super::SharedLibp2pTrafficMetrics,
        swarm: &mut Swarm<Behaviour>,
        event_errors: &Arc<Mutex<Vec<String>>>,
        max_error_messages: usize,
    ) {
        let reply = if completed
            .auth_release
            .as_ref()
            .is_some_and(|guard| guard.check().is_err())
        {
            Err(WorldError::NetworkRequestFailed {
                code: oasis7_proto::distributed::DistributedErrorCode::ErrNotAvailable,
                message: "authenticated reply release conditions changed".into(),
                retryable: true,
            })
        } else {
            completed.reply
        };
        // Even after dequeuing before its deadline, a non-cooperative handler
        // or a backed-up completion channel can finish too late. Do not send
        // or charge such a success as a fresh paid response.
        let reply = fail_expired_success(reply, completed.deadline, std::time::Instant::now());
        if !send_charged_response(
            budget,
            completed.peer,
            completed.protocol.as_str(),
            reply,
            now_ms,
            traffic_metrics,
            |response| {
                swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(completed.channel, response)
            },
        ) {
            super::push_bounded_clone(
                event_errors,
                format!(
                    "libp2p response cancelled before send protocol={} peer={} queue_wait_ms={}",
                    completed.protocol,
                    completed.peer,
                    now_ms.saturating_sub(completed.queued_at_ms)
                ),
                max_error_messages,
                "lock errors",
            );
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Immediate responses keep the response channel, budget, metrics, and swarm borrows explicit"
    )]
    pub(super) fn send_immediate(
        &self,
        channel: request_response::ResponseChannel<NetworkResponse>,
        protocol: &str,
        peer: libp2p::PeerId,
        reply: Result<Vec<u8>, WorldError>,
        budget: &mut FetchBlobResponseBudget,
        now_ms: i64,
        traffic_metrics: &super::SharedLibp2pTrafficMetrics,
        swarm: &mut Swarm<Behaviour>,
    ) -> bool {
        send_charged_response(
            budget,
            peer,
            protocol,
            reply,
            now_ms,
            traffic_metrics,
            |response| {
                swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, response)
            },
        )
    }

    pub(super) fn overload_response() -> WorldError {
        WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrOverloaded,
            message: "response worker overloaded; retry request".to_string(),
            retryable: true,
        }
    }

    #[cfg(test)]
    fn pending_limits(&self) -> (usize, usize, usize, usize) {
        let state = self
            .in_flight
            .lock()
            .expect("response worker in-flight state");
        (
            state.control_jobs,
            state.head_jobs,
            state.blob_jobs,
            state.blob_bytes,
        )
    }

    fn reserve(&self, protocol: &str, bytes: usize) -> Option<InFlightReservation> {
        let lane = response_lane(protocol);
        let mut state = self
            .in_flight
            .lock()
            .expect("response worker in-flight state");
        match lane {
            ResponseLane::Control if state.control_jobs >= CONTROL_MAX_IN_FLIGHT => return None,
            ResponseLane::Head if state.head_jobs >= HEAD_MAX_IN_FLIGHT => return None,
            ResponseLane::Blob
                if state.blob_jobs >= BLOB_MAX_IN_FLIGHT
                    || state.blob_bytes > BLOB_MAX_IN_FLIGHT_BYTES.saturating_sub(bytes) =>
            {
                return None;
            }
            _ => {}
        }
        match lane {
            ResponseLane::Control => state.control_jobs += 1,
            ResponseLane::Head => state.head_jobs += 1,
            ResponseLane::Blob => {
                state.blob_jobs += 1;
                state.blob_bytes += bytes;
            }
        }
        Some(InFlightReservation {
            state: Arc::clone(&self.in_flight),
            lane,
            bytes,
        })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Returning the rejected Job preserves reservation ownership for the caller's recovery path"
    )]
    fn submit(&self, job: Job) -> Result<(), Job> {
        let sender = match response_lane(job.protocol.as_str()) {
            ResponseLane::Control => &self.control,
            ResponseLane::Head => &self.head,
            ResponseLane::Blob => &self.blob,
        };
        sender.try_send(job).map_err(|error| match error {
            mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job) => job,
        })
    }
}

fn response_lane(protocol: &str) -> ResponseLane {
    if protocol.contains("fetch-blob") {
        ResponseLane::Blob
    } else if protocol == FETCH_COMMIT_HEAD_PROTOCOL {
        ResponseLane::Head
    } else {
        ResponseLane::Control
    }
}

#[derive(serde::Deserialize)]
struct FetchBlobReservationRequest {
    #[serde(default)]
    limit_bytes: Option<u64>,
}

fn response_reservation_bytes(protocol: &str, payload: &[u8]) -> Result<usize, WorldError> {
    if !matches!(response_lane(protocol), ResponseLane::Blob) {
        return Ok(0);
    }
    let Ok(request) = serde_json::from_slice::<FetchBlobReservationRequest>(payload) else {
        // Generic handlers can use this protocol label with non-JSON payloads; the registered
        // replication handler still rejects malformed requests before any disk read.
        return Ok(FETCH_BLOB_MAX_RESPONSE_BYTES);
    };
    let requested = request
        .limit_bytes
        .map(|value| usize::try_from(value).unwrap_or(usize::MAX))
        .unwrap_or(FETCH_BLOB_MAX_RESPONSE_BYTES);
    if requested == 0 || requested > FETCH_BLOB_MAX_RESPONSE_BYTES {
        return Err(WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrBadRequest,
            message: format!(
                "fetch-blob requested response bytes must be within 1..={FETCH_BLOB_MAX_RESPONSE_BYTES}"
            ),
            retryable: false,
        });
    }
    Ok(fetch_blob_legacy_json_encoded_upper_bound(requested))
}

fn response(
    budget: &mut FetchBlobResponseBudget,
    peer: libp2p::PeerId,
    protocol: &str,
    reply: Result<Vec<u8>, WorldError>,
    now_ms: i64,
    _traffic_metrics: &super::SharedLibp2pTrafficMetrics,
) -> NetworkResponse {
    NetworkResponse {
        payload: budgeted_response_bytes(budget, peer, protocol, reply, now_ms),
    }
}

/// The libp2p channel can synchronously reject a response when the caller
/// canceled. This is known zero delivery and must not charge a successful
/// FetchBlob response against the peer or global paid byte window.
#[expect(
    clippy::too_many_arguments,
    reason = "Explicit true send outcome fences shared responder byte quotas"
)]
fn send_charged_response(
    budget: &mut FetchBlobResponseBudget,
    peer: libp2p::PeerId,
    protocol: &str,
    reply: Result<Vec<u8>, WorldError>,
    now_ms: i64,
    traffic_metrics: &super::SharedLibp2pTrafficMetrics,
    send: impl FnOnce(NetworkResponse) -> Result<(), NetworkResponse>,
) -> bool {
    // Only positive successful FetchBlob bytes can mutate the ledger. The
    // bounded 256-peer clone is not needed for ordinary/control/error paths.
    let (response, proposed) = if protocol == FETCH_BLOB_PROTOCOL
        && reply.as_ref().is_ok_and(|bytes| !bytes.is_empty())
    {
        let mut proposed = budget.clone();
        (
            response(&mut proposed, peer, protocol, reply, now_ms, traffic_metrics),
            Some(proposed),
        )
    } else {
        (response(budget, peer, protocol, reply, now_ms, traffic_metrics), None)
    };
    let size = response.payload.len();
    if send(response).is_err() {
        return false;
    }
    if let Some(proposed) = proposed {
        *budget = proposed;
    }
    super::record_response_outbound(traffic_metrics, protocol, size);
    true
}

// The worker's admit/expire decision uses only the original monotonic
// deadline. Neither a local UTC rollback nor delayed queue servicing can
// silently grant a fresh execution window.
fn response_deadline_elapsed(deadline: std::time::Instant, now: std::time::Instant) -> bool {
    now >= deadline
}

fn fail_expired_success(
    reply: Result<Vec<u8>, WorldError>,
    deadline: std::time::Instant,
    now: std::time::Instant,
) -> Result<Vec<u8>, WorldError> {
    if reply.is_ok() && response_deadline_elapsed(deadline, now) {
        return Err(WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrTimeout,
            message: "response worker original deadline elapsed before release".into(),
            retryable: true,
        });
    }
    reply
}

fn spawn_workers(
    rx: mpsc::Receiver<Job>,
    count: usize,
    completed: futures_mpsc::Sender<CompletedResponse>,
) {
    let rx = Arc::new(std::sync::Mutex::new(rx));
    for _ in 0..count {
        let rx = Arc::clone(&rx);
        let mut completed = completed.clone();
        thread::spawn(move || {
            loop {
                let Ok(job) = rx.lock().expect("response worker receiver").recv() else {
                    break;
                };
                if response_deadline_elapsed(job.deadline, std::time::Instant::now()) {
                    // Dropping the queued job releases its reservation without handler work.
                    continue;
                }
                // Synchronous handlers are run only by this fixed worker pool. A remote client
                // may time out, but a non-cooperative callback retains its execution slot until
                // it returns; this keeps capacity accounting honest and OS-thread count bounded.
                let context = oasis7_proto::distributed_net::NetworkRequestContext::new(
                    job.deadline,
                    Arc::clone(&job.cancelled),
                );
                let reply = if let Some(guard) = &job.auth_release {
                    guard
                        .initialize(&context)
                        .and_then(|()| (job.handler)(&context, &job.payload))
                } else {
                    (job.handler)(&context, &job.payload)
                };
                // Backpressure is confined to this worker; the swarm loop drains completions.
                if futures::executor::block_on(completed.send(CompletedResponse {
                    channel: job.channel,
                    protocol: job.protocol,
                    peer: job.peer,
                    queued_at_ms: job.queued_at_ms,
                    deadline: job.deadline,
                    reply,
                    auth_release: job.auth_release,
                    _reservation: job.reservation,
                }))
                .is_err()
                {
                    break;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_blob_send_does_not_spend_a_peer_or_global_success_window() {
        let peer = libp2p::PeerId::random();
        let mut budget = FetchBlobResponseBudget::default();
        let metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
        let bytes = vec![0_u8; FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW];
        // Real Swarm::send_response uses this same Result callback shape.
        // Cancellation before acceptance returns the unused original response.
        let accepted = send_charged_response(
            &mut budget,
            peer,
            FETCH_BLOB_PROTOCOL,
            Ok(bytes.clone()),
            1_000,
            &metrics,
            Err,
        );
        assert!(!accepted);

        let accepted = send_charged_response(
            &mut budget,
            peer,
            FETCH_BLOB_PROTOCOL,
            Ok(bytes),
            1_000,
            &metrics,
            |reply| {
                assert_eq!(reply.payload.len(), FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW);
                Ok(())
            },
        );
        assert!(accepted);
        let accepted = send_charged_response(
            &mut budget,
            peer,
            FETCH_BLOB_PROTOCOL,
            Ok(vec![1]),
            1_000,
            &metrics,
            |reply| {
                let error: oasis7_proto::distributed::ErrorResponse =
                    serde_cbor::from_slice(&reply.payload).unwrap();
                assert_eq!(error.code, DistributedErrorCode::ErrRateLimited);
                assert!(error.retryable);
                Ok(())
            },
        );
        assert!(accepted);
    }

    #[test]
    fn expired_completed_success_is_not_released_or_charged() {
        let now = std::time::Instant::now();
        let peer = libp2p::PeerId::random();
        let mut budget = FetchBlobResponseBudget::default();
        let metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
        let reply = fail_expired_success(Ok(b"late success".to_vec()), now, now);
        assert!(matches!(
            reply,
            Err(WorldError::NetworkRequestFailed {
                code: DistributedErrorCode::ErrTimeout,
                retryable: true,
                ..
            })
        ));
        let response = response(
            &mut budget,
            peer,
            "/aw/node/replication/fetch-blob/1.0.0",
            reply,
            1_000,
            &metrics,
        );
        let rejection: oasis7_proto::distributed::ErrorResponse =
            serde_cbor::from_slice(&response.payload).unwrap();
        assert_eq!(rejection.code, DistributedErrorCode::ErrTimeout);
        assert!(rejection.retryable);
        let full = vec![0_u8; FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW];
        let accepted = budgeted_response_bytes(
            &mut budget,
            peer,
            "/aw/node/replication/fetch-blob/1.0.0",
            Ok(full),
            1_000,
        );
        assert_eq!(
            accepted.len(),
            FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW,
            "expired successful reply did not consume original full peer quota"
        );

        let on_time = fail_expired_success(
            Ok(b"ok".to_vec()),
            now + std::time::Duration::from_nanos(1),
            now,
        )
        .expect("success before deadline remains successful");
        assert_eq!(on_time, b"ok");
        let prior_failure = WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrNotAvailable,
            message: "real authorization failure".into(),
            retryable: true,
        };
        assert!(matches!(
            fail_expired_success(Err(prior_failure), now, now),
            Err(WorldError::NetworkRequestFailed {
                code: DistributedErrorCode::ErrNotAvailable,
                ..
            })
        ));
    }

    #[test]
    fn queued_worker_deadline_is_absolute_and_monotonic() {
        let started = std::time::Instant::now();
        let deadline = started + std::time::Duration::from_millis(250);
        assert!(!response_deadline_elapsed(deadline, started));
        assert!(!response_deadline_elapsed(
            deadline,
            deadline - std::time::Duration::from_nanos(1),
        ));
        assert!(response_deadline_elapsed(deadline, deadline));
        assert!(response_deadline_elapsed(
            deadline,
            deadline + std::time::Duration::from_millis(1),
        ));
        // A wall-clock diagnostic stamp can move backward or forward without
        // changing the already fixed queue execution deadline.
        for _diagnostic_wall_ms in [i64::MIN, i64::MAX] {
            assert!(response_deadline_elapsed(deadline, deadline));
        }
    }

    #[test]
    fn in_flight_reservations_bound_blob_jobs_and_actual_bytes_until_release() {
        let workers = ResponseWorkers {
            control: mpsc::sync_channel(1).0,
            head: mpsc::sync_channel(1).0,
            blob: mpsc::sync_channel(1).0,
            in_flight: Arc::new(Mutex::new(InFlightState {
                control_jobs: 0,
                head_jobs: 0,
                blob_jobs: 0,
                blob_bytes: 0,
            })),
            response_deadline: std::time::Duration::from_millis(1),
        };
        let mut reservations = Vec::new();
        let full_response_count = BLOB_MAX_IN_FLIGHT_BYTES / FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW;
        for _ in 0..full_response_count {
            reservations.push(
                workers
                    .reserve("fetch-blob", FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW)
                    .expect("within blob capacity"),
            );
        }
        assert!(
            workers
                .reserve("fetch-blob", FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW)
                .is_none()
        );
        let small_reservation = workers
            .reserve("fetch-blob", 1)
            .expect("remaining byte capacity can fill the final job slot");
        assert!(workers.reserve("fetch-blob", 1).is_none());
        assert_eq!(
            workers.pending_limits(),
            (
                0,
                0,
                BLOB_MAX_IN_FLIGHT,
                full_response_count * FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW + 1,
            )
        );
        drop(small_reservation);
        reservations.pop();
        assert!(
            workers
                .reserve("fetch-blob", FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW)
                .is_some()
        );
    }

    #[test]
    fn blob_reservation_uses_the_validated_requested_range() {
        let workers = ResponseWorkers {
            control: mpsc::sync_channel(1).0,
            head: mpsc::sync_channel(1).0,
            blob: mpsc::sync_channel(1).0,
            in_flight: Arc::new(Mutex::new(InFlightState {
                control_jobs: 0,
                head_jobs: 0,
                blob_jobs: 0,
                blob_bytes: 0,
            })),
            response_deadline: std::time::Duration::from_millis(1),
        };
        let request = br#"{"content_hash":"blob","offset_bytes":1024,"limit_bytes":4096,"requester_public_key_hex":"key","requester_signature_hex":"signature"}"#;
        let requested_bytes =
            response_reservation_bytes("/aw/node/replication/fetch-blob/1.0.0", request)
                .expect("valid range");
        let reservation = workers
            .reserve("/aw/node/replication/fetch-blob/1.0.0", requested_bytes)
            .expect("validated range fits response budget");
        assert_eq!(workers.pending_limits(), (0, 0, 1, 4 * 4096 + 1024));
        drop(reservation);
        assert_eq!(workers.pending_limits(), (0, 0, 0, 0));
        assert!(
            response_reservation_bytes(
                "/aw/node/replication/fetch-blob/1.0.0",
                format!(r#"{{"limit_bytes":{}}}"#, FETCH_BLOB_MAX_RESPONSE_BYTES + 1).as_bytes(),
            )
            .is_err()
        );
    }

    #[test]
    fn oversized_concurrent_blob_reservations_hit_the_80_mib_cap_without_payload_allocation() {
        let workers = ResponseWorkers {
            control: mpsc::sync_channel(1).0,
            head: mpsc::sync_channel(1).0,
            blob: mpsc::sync_channel(1).0,
            in_flight: Arc::new(Mutex::new(InFlightState {
                control_jobs: 0,
                head_jobs: 0,
                blob_jobs: 0,
                blob_bytes: 0,
            })),
            response_deadline: std::time::Duration::from_millis(1),
        };
        let reservation = workers
            .reserve("fetch-blob", 64 * 1024 * 1024)
            .expect("first validated response range fits");
        assert!(
            workers.reserve("fetch-blob", 17 * 1024 * 1024).is_none(),
            "the 80 MiB cap must reject a concurrent range before handler allocation"
        );
        drop(reservation);
        assert!(workers.reserve("fetch-blob", 17 * 1024 * 1024).is_some());
    }

    #[test]
    fn pending_blob_budget_reserves_json_encoded_bytes_not_raw_range_bytes() {
        let workers = ResponseWorkers {
            control: mpsc::sync_channel(1).0,
            head: mpsc::sync_channel(1).0,
            blob: mpsc::sync_channel(1).0,
            in_flight: Arc::new(Mutex::new(InFlightState {
                control_jobs: 0,
                head_jobs: 0,
                blob_jobs: 0,
                blob_bytes: 0,
            })),
            response_deadline: std::time::Duration::from_millis(1),
        };
        let encoded = response_reservation_bytes(
            "/aw/node/replication/fetch-blob/1.0.0",
            format!(r#"{{"limit_bytes":{FETCH_BLOB_MAX_RESPONSE_BYTES}}}"#).as_bytes(),
        )
        .expect("valid maximum raw range");
        assert!(encoded > FETCH_BLOB_MAX_RESPONSE_BYTES);
        let reservation_count = BLOB_MAX_IN_FLIGHT_BYTES / encoded;
        let reservations = (0..reservation_count)
            .map(|_| {
                workers
                    .reserve("fetch-blob", encoded)
                    .expect("encoded range fits")
            })
            .collect::<Vec<_>>();
        assert!(workers.reserve("fetch-blob", encoded).is_none());
        drop(reservations);
    }
}
