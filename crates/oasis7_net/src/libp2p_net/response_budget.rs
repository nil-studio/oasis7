use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use libp2p::PeerId;
use oasis7_proto::distributed::DistributedErrorCode;
use oasis7_proto::distributed_net::{
    FETCH_BLOB_GLOBAL_RESPONSE_BYTES_PER_WINDOW, FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS,
    FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW, FETCH_BLOB_RESPONSE_WINDOW_MS,
};

use crate::error::WorldError;
use crate::util::to_canonical_cbor;

use super::error_mapping::error_response_from_world_error;

const FETCH_BLOB_PROTOCOL: &str = "/aw/node/replication/fetch-blob/1.0.0";
const RESPONSE_WINDOW: Duration = Duration::from_millis(FETCH_BLOB_RESPONSE_WINDOW_MS as u64);
#[derive(Default)]
pub(super) struct FetchBlobResponseBudget {
    by_peer: HashMap<PeerId, ResponseWindow>,
    global: Option<ResponseWindow>,
}

#[derive(Clone, Copy)]
struct ResponseWindow {
    started_at: Instant,
    bytes: usize,
}

impl FetchBlobResponseBudget {
    pub(super) fn admit(
        &mut self,
        peer: PeerId,
        protocol: &str,
        response_bytes: usize,
        now: Instant,
    ) -> Result<(), WorldError> {
        // A successful zero-length response transfers no blob bytes. It must
        // neither create a fresh peer window nor evict another peer's charged
        // quota at the bounded table cap. A future positive response still
        // performs the normal expiry and admission checks.
        if protocol != FETCH_BLOB_PROTOCOL || response_bytes == 0 {
            return Ok(());
        }

        // Expire old windows first, but do not evict an active peer merely
        // because a different peer proposed a response that will be rejected.
        self.prune_stale(now);

        {
            let global = self.global.get_or_insert(ResponseWindow {
                started_at: now,
                bytes: 0,
            });
            if now.saturating_duration_since(global.started_at) >= RESPONSE_WINDOW {
                *global = ResponseWindow {
                    started_at: now,
                    bytes: 0,
                };
            }
            if response_bytes
                > FETCH_BLOB_GLOBAL_RESPONSE_BYTES_PER_WINDOW.saturating_sub(global.bytes)
            {
                return Err(WorldError::NetworkRequestFailed {
                    code: DistributedErrorCode::ErrRateLimited,
                    message:
                        "global fetch-blob response budget exhausted; retry after window reset"
                            .to_string(),
                    retryable: true,
                });
            }
        }

        // Preflight the peer's existing in-window usage before changing the
        // capacity table. An oversized or otherwise rejected response must
        // never evict another peer's charged budget.
        let used = self.by_peer.get(&peer).map_or(0, |window| {
            if now.saturating_duration_since(window.started_at) >= RESPONSE_WINDOW {
                0
            } else {
                window.bytes
            }
        });
        if response_bytes > FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW.saturating_sub(used) {
            return Err(WorldError::NetworkRequestFailed {
                code: DistributedErrorCode::ErrRateLimited,
                message: format!(
                    "fetch-blob response budget exhausted for peer={peer}; retry after window reset"
                ),
                retryable: true,
            });
        }
        // Evicting a still-charged peer would silently reset its per-peer
        // 60-second allowance. Fail closed at the bounded table cap instead;
        // an existing peer retains its own window, and an expired peer can
        // be pruned normally on the next admission.
        if !self.by_peer.contains_key(&peer)
            && self.by_peer.len() >= FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS
        {
            return Err(WorldError::NetworkRequestFailed {
                code: DistributedErrorCode::ErrRateLimited,
                message: "fetch-blob response peer budget table full; retry after window reset"
                    .to_string(),
                retryable: true,
            });
        }
        let window = self.by_peer.entry(peer).or_insert(ResponseWindow {
            started_at: now,
            bytes: 0,
        });
        if now.saturating_duration_since(window.started_at) >= RESPONSE_WINDOW {
            *window = ResponseWindow {
                started_at: now,
                bytes: 0,
            };
        }

        if response_bytes > FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW.saturating_sub(window.bytes) {
            return Err(WorldError::NetworkRequestFailed {
                code: DistributedErrorCode::ErrRateLimited,
                message: format!(
                    "fetch-blob response budget exhausted for peer={peer}; retry after window reset"
                ),
                retryable: true,
            });
        }

        window.bytes = window.bytes.saturating_add(response_bytes);
        let global = self.global.as_mut().expect("global window initialized");
        global.bytes = global.bytes.saturating_add(response_bytes);
        Ok(())
    }

    fn prune_stale(&mut self, now: Instant) {
        self.by_peer
            .retain(|_, window| now.saturating_duration_since(window.started_at) < RESPONSE_WINDOW);
    }
}

pub(super) fn budgeted_response_bytes(
    budget: &mut FetchBlobResponseBudget,
    peer: PeerId,
    protocol: &str,
    reply: Result<Vec<u8>, WorldError>,
    _diagnostic_wall_ms: i64,
) -> Vec<u8> {
    // Wall time remains an observation only. The charged response window
    // uses local monotonic elapsed time; a clock jump cannot refund quota.
    let response_bytes = match reply {
        Ok(response_bytes) => response_bytes,
        Err(err) => {
            // Rejections are intentionally not charged to a peer's successful blob
            // budget.  In particular, unauthorised requests must not manufacture
            // unbounded peer state before admission rejects them.
            return to_canonical_cbor(&error_response_from_world_error(&err)).unwrap_or_default();
        }
    };
    budget
        .admit(peer, protocol, response_bytes.len(), Instant::now())
        .map(|()| response_bytes)
        .unwrap_or_else(|err| {
            to_canonical_cbor(&error_response_from_world_error(&err)).unwrap_or_default()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_blob_budget_is_per_peer_and_resets_after_its_window() {
        let mut budget = FetchBlobResponseBudget::default();
        let first_peer = PeerId::random();
        let second_peer = PeerId::random();
        let first_window = Instant::now();

        budget
            .admit(
                first_peer,
                FETCH_BLOB_PROTOCOL,
                FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW,
                first_window,
            )
            .expect("first peer fits exactly once");
        let err = budget
            .admit(first_peer, FETCH_BLOB_PROTOCOL, 1, first_window)
            .expect_err("same peer is limited inside its window");
        assert!(matches!(
            err,
            WorldError::NetworkRequestFailed {
                code: DistributedErrorCode::ErrRateLimited,
                retryable: true,
                ..
            }
        ));

        budget
            .admit(second_peer, FETCH_BLOB_PROTOCOL, 1, first_window)
            .expect("another peer has an independent budget");
        budget
            .admit(
                first_peer,
                FETCH_BLOB_PROTOCOL,
                1,
                first_window + RESPONSE_WINDOW,
            )
            .expect("expired window resets budget");
    }

    #[test]
    fn fetch_blob_global_budget_admits_eight_full_peer_windows() {
        let mut budget = FetchBlobResponseBudget::default();
        let now = Instant::now();

        for _ in 0..8 {
            budget
                .admit(
                    PeerId::random(),
                    FETCH_BLOB_PROTOCOL,
                    FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW,
                    now,
                )
                .expect("eight full peer windows fit the global budget");
        }

        assert!(
            budget
                .admit(
                    PeerId::random(),
                    FETCH_BLOB_PROTOCOL,
                    FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW,
                    now,
                )
                .is_err()
        );
    }

    #[test]
    fn fetch_blob_budget_does_not_apply_to_other_protocols() {
        let mut budget = FetchBlobResponseBudget::default();
        budget
            .admit(
                PeerId::random(),
                "/aw/node/replication/fetch-commit/1.0.0",
                FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW.saturating_add(1),
                Instant::now(),
            )
            .expect("other protocols are unaffected");
    }

    #[test]
    fn fetch_blob_budget_bounds_peers_preserves_paid_windows_and_ignores_rejections() {
        let mut budget = FetchBlobResponseBudget::default();
        let now = Instant::now();
        for _ in 0..FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS {
            budget
                .admit(PeerId::random(), FETCH_BLOB_PROTOCOL, 1, now)
                .expect("bounded peer entry");
        }
        let retained = budget
            .by_peer
            .keys()
            .min_by_key(|peer| peer.to_bytes())
            .copied()
            .expect("oldest deterministic peer");
        assert!(
            budget
                .admit(PeerId::random(), FETCH_BLOB_PROTOCOL, 1, now)
                .is_err(),
            "new peer must not evict an unexpired charged window"
        );
        assert_eq!(budget.by_peer.len(), FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS);
        assert!(budget.by_peer.contains_key(&retained));

        let rejected_peer = PeerId::random();
        let _ = budgeted_response_bytes(
            &mut budget,
            rejected_peer,
            FETCH_BLOB_PROTOCOL,
            Err(WorldError::NetworkRequestFailed {
                code: DistributedErrorCode::ErrUnauthorized,
                message: "denied".to_string(),
                retryable: false,
            }),
            1_000,
        );
        assert!(!budget.by_peer.contains_key(&rejected_peer));

        budget
            .admit(
                PeerId::random(),
                FETCH_BLOB_PROTOCOL,
                1,
                now + RESPONSE_WINDOW,
            )
            .expect("stale entries are pruned before inserting");
        assert_eq!(budget.by_peer.len(), 1);
    }

    #[test]
    fn rejected_new_peer_cannot_evict_an_existing_charged_window() {
        let mut budget = FetchBlobResponseBudget::default();
        let now = Instant::now();
        let mut peers = std::collections::HashSet::new();
        while peers.len() < FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS {
            peers.insert(PeerId::random());
        }
        let victim = *peers.iter().min_by_key(|peer| peer.to_bytes()).unwrap();
        for peer in peers {
            let bytes = if peer == victim {
                FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW
            } else {
                1
            };
            budget
                .admit(peer, FETCH_BLOB_PROTOCOL, bytes, now)
                .expect("existing per-peer window is admitted");
        }
        assert_eq!(budget.by_peer.len(), FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS);
        let newcomer = loop {
            let candidate = PeerId::random();
            if !budget.by_peer.contains_key(&candidate) {
                break candidate;
            }
        };
        // The newcomer has no valid per-peer budget for this response.
        // Previously the cap handler first evicted the victim, then rejected
        // this response, letting that victim incorrectly restart its window.
        assert!(
            budget
                .admit(
                    newcomer,
                    FETCH_BLOB_PROTOCOL,
                    FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW + 1,
                    now,
                )
                .is_err()
        );
        assert!(!budget.by_peer.contains_key(&newcomer));
        assert!(budget.by_peer.contains_key(&victim));
        assert_eq!(budget.by_peer.len(), FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS);
        assert!(budget.admit(victim, FETCH_BLOB_PROTOCOL, 1, now).is_err());

        // Even a small valid new response cannot evict an in-window peer
        // and grant that same peer a second full response allowance.
        assert!(budget.admit(newcomer, FETCH_BLOB_PROTOCOL, 1, now).is_err());
        assert_eq!(budget.by_peer.len(), FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS);
        assert!(budget.by_peer.contains_key(&victim));
        assert!(!budget.by_peer.contains_key(&newcomer));
        assert!(budget.admit(victim, FETCH_BLOB_PROTOCOL, 1, now).is_err());
        budget
            .admit(newcomer, FETCH_BLOB_PROTOCOL, 1, now + RESPONSE_WINDOW)
            .expect("new peer can enter once the original window expires");
        assert_eq!(budget.by_peer.len(), 1);
        assert!(!budget.by_peer.contains_key(&victim));
    }

    #[test]
    fn empty_blob_response_cannot_evict_another_peers_paid_window() {
        let mut budget = FetchBlobResponseBudget::default();
        let now = Instant::now();
        let mut peers = std::collections::HashSet::new();
        while peers.len() < FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS {
            peers.insert(PeerId::random());
        }
        let victim = *peers.iter().min_by_key(|peer| peer.to_bytes()).unwrap();
        for peer in peers {
            let charged = if peer == victim {
                FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW
            } else {
                1
            };
            budget
                .admit(peer, FETCH_BLOB_PROTOCOL, charged, now)
                .unwrap();
        }
        let before = budget.global.unwrap().bytes;
        let newcomer = loop {
            let peer = PeerId::random();
            if !budget.by_peer.contains_key(&peer) {
                break peer;
            }
        };
        // Exercise the real responder-side helper, not only a synthetic
        // ledger call. The empty success should leave the paid owner intact.
        assert!(
            budgeted_response_bytes(
                &mut budget,
                newcomer,
                FETCH_BLOB_PROTOCOL,
                Ok(vec![]),
                1_000
            )
            .is_empty()
        );
        assert_eq!(budget.global.unwrap().bytes, before);
        assert_eq!(budget.by_peer.len(), FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS);
        assert!(budget.by_peer.contains_key(&victim));
        assert!(!budget.by_peer.contains_key(&newcomer));
        assert!(budget.admit(victim, FETCH_BLOB_PROTOCOL, 1, now).is_err());

        // A nonempty response is denied rather than evicting a paid window.
        assert!(budget.admit(newcomer, FETCH_BLOB_PROTOCOL, 1, now).is_err());
        assert_eq!(budget.by_peer.len(), FETCH_BLOB_RESPONSE_BUDGET_MAX_PEERS);
        assert!(budget.by_peer.contains_key(&victim));
        assert!(!budget.by_peer.contains_key(&newcomer));
        assert_eq!(budget.global.unwrap().bytes, before);
    }

    #[test]
    fn wall_clock_jumps_do_not_refund_a_charged_blob_window() {
        let mut budget = FetchBlobResponseBudget::default();
        let peer = PeerId::random();
        let now = Instant::now();
        budget
            .admit(
                peer,
                FETCH_BLOB_PROTOCOL,
                FETCH_BLOB_RESPONSE_BYTES_PER_WINDOW,
                now,
            )
            .unwrap();
        let original = budget.by_peer[&peer];
        // The production responder receives a wall-clock diagnostic stamp,
        // but only monotonic elapsed time can expire its paid byte window.
        for wall in [i64::MAX, i64::MIN] {
            let response =
                budgeted_response_bytes(&mut budget, peer, FETCH_BLOB_PROTOCOL, Ok(vec![1]), wall);
            assert!(!response.is_empty(), "rate-limit error is encoded");
            assert_eq!(budget.by_peer[&peer].bytes, original.bytes);
            assert_eq!(budget.by_peer[&peer].started_at, original.started_at);
            assert_eq!(budget.global.unwrap().bytes, original.bytes);
        }
        budget
            .admit(peer, FETCH_BLOB_PROTOCOL, 1, now + RESPONSE_WINDOW)
            .expect("only monotonic window expiry resets a charged quota");
        assert_eq!(budget.by_peer[&peer].bytes, 1);
    }

    #[test]
    fn rotating_peers_cannot_bypass_global_window_and_window_recovers() {
        let mut budget = FetchBlobResponseBudget::default();
        let now = Instant::now();
        let chunk = FETCH_BLOB_GLOBAL_RESPONSE_BYTES_PER_WINDOW / 300;
        let mut admitted = 0usize;
        for _ in 0..300 {
            if budget
                .admit(PeerId::random(), FETCH_BLOB_PROTOCOL, chunk, now)
                .is_ok()
            {
                admitted += chunk;
            }
        }
        assert!(admitted <= FETCH_BLOB_GLOBAL_RESPONSE_BYTES_PER_WINDOW);
        assert!(
            budget
                .admit(PeerId::random(), FETCH_BLOB_PROTOCOL, chunk, now)
                .is_err()
        );
        budget
            .admit(
                PeerId::random(),
                FETCH_BLOB_PROTOCOL,
                chunk,
                now + RESPONSE_WINDOW,
            )
            .expect("global deterministic window resets");
    }
}
