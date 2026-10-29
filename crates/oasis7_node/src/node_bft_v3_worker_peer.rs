//! Bounded public ingress and nonblocking, separately authorized private fetches.
use super::*;
use crate::node_bft_v3_runtime::private_exchange::{
    NodeBftPrivateCandidateFetchV3, NodeBftPrivateFetchKindV3, NodeBftPrivateFetchRequestV3,
};
use std::collections::{BTreeMap, BTreeSet};
const QUEUED_BYTES: usize = 16 * 1024 * 1024;
const QUEUED_COUNT: usize = 64;
const HISTORY_RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);
struct Incoming {
    peer: String,
    message: BftPublicPeerMessageV3,
    bytes: usize,
}
struct Fetching {
    input: Incoming,
    operation: Box<dyn NodeBftPrivateCandidateFetchV3>,
    deadline: Instant,
    context: NodeBftExecutionContextV3,
}
struct HistoryRequest {
    peer: String,
    request: BftPeerHistoryRequestV3,
    deadline: Instant,
}
#[derive(Default)]
pub(crate) struct PeerRuntime {
    queued: VecDeque<Incoming>,
    queued_bytes: usize,
    fetching: Option<Fetching>,
    history: VecDeque<(String, FormalBftCertifiedHeaderV3)>,
    request: Option<HistoryRequest>,
    seen: BTreeSet<String>,
    fetch_budget: FetchBudget,
    last_send: Option<Instant>,
    last_request: Option<Instant>,
    next_peer: usize,
}
impl PeerRuntime {
    pub(super) fn is_catching_up(&self) -> bool {
        !self.history.is_empty()
            || self
                .fetching
                .as_ref()
                .is_some_and(|f| matches!(f.input.message, BftPublicPeerMessageV3::Commit(_)))
    }
    pub(super) fn cancel(&mut self) {
        if let Some(mut fetching) = self.fetching.take() {
            fetching.operation.cancel();
        }
    }
    pub(super) fn height_advanced(&mut self) {
        self.cancel();
        self.queued.clear();
        self.queued_bytes = 0;
        self.seen.clear();
        self.fetch_budget = FetchBudget::default();
        self.request = None;
    }
}
impl Worker {
    pub(super) fn broadcast(&self, message: BftPublicPeerMessageV3) {
        if let Some(network) = &self.selected.network {
            let _ = network.transport.broadcast_public(&message);
        }
    }
    pub(super) fn poll_peers(&mut self) -> Result<(), NodeError> {
        let Some(network) = self.selected.network.clone() else {
            return Ok(());
        };
        // Defense in depth: do not poll a completed fetch or drain a peer
        // frame until the original journal decisions have been reconciled.
        if self
            .checkpoint
            .state
            .journal
            .as_ref()
            .is_some_and(|journal| {
                journal.pending_commit.is_some()
                    || journal.pending_proposal_intent.is_some()
                    || journal.pending_vote_intent.is_some()
            })
        {
            return Ok(());
        }
        // Local restored state is authoritative. An inbound peer cannot choose
        // a new parent or keep an old-parent acquisition alive after commit.
        if self.peers.fetching.as_ref().is_some_and(|f| {
            f.context != NodeBftExecutionContextV3::from_schedule(&self.schedule)
                || Instant::now() >= f.deadline
        }) {
            self.peers.cancel();
        }
        if self
            .peers
            .request
            .as_ref()
            .is_some_and(|r| Instant::now() >= r.deadline)
        {
            self.peers.request = None;
        }
        if let Some(mut fetching) = self.peers.fetching.take() {
            match fetching.operation.poll() {
                Ok(Some(package)) => {
                    if package.candidate.public_value
                        == message_value(&fetching.input.message)
                            .ok_or_else(|| error("wire-3 private fetch lost public value"))?
                    {
                        let accepted = {
                            let mut hook = lock_hook(&self.hook)?;
                            hook.retain_received_bft_v3_private_candidate(
                                &fetching.context,
                                &package,
                            )
                            .and_then(|digest| {
                                if canonical_digest(&digest) {
                                    Ok(())
                                } else {
                                    Err("wire-3 received retention receipt malformed".into())
                                }
                            })
                        };
                        if accepted.is_ok() {
                            // A successful hook import is still checked through the
                            // same exact-parent actual candidate gate as local work.
                            let value = package.candidate.public_value;
                            if startup::candidate(
                                &self.config,
                                &self.hook,
                                &self.schedule,
                                &value,
                                &self.selected,
                            )
                            .is_ok()
                            {
                                self.accept_materialized(fetching.input.message)?;
                            }
                        }
                    }
                    fetching.operation.cancel();
                }
                Ok(None) => self.peers.fetching = Some(fetching),
                Err(_) => fetching.operation.cancel(),
            }
        }
        // The adapter authenticates transport identities; protocol signatures
        // remain separately checked against the admitted current schedule.
        let configured_peers = network.transport.peers();
        for received in network.transport.drain_public(32).map_err(error)? {
            if !configured_peers.contains(&received.peer_id) {
                continue;
            }
            let encoded = match encode_bft_public_peer_message_v3(
                network.transport.context(),
                &received.message,
            ) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let digest = message_digest(&received.message)?;
            match received.message {
                BftPublicPeerMessageV3::HistoryRequest(request) => {
                    self.serve_history(&received.peer_id, request)?
                }
                BftPublicPeerMessageV3::HistoryReply(reply) => {
                    let Some(outstanding) = &self.peers.request else {
                        continue;
                    };
                    if outstanding.peer != received.peer_id
                        || reply.validate_for_request(&outstanding.request).is_err()
                    {
                        continue;
                    }
                    if outstanding.request.after_height != self.schedule.height().saturating_sub(1)
                        || outstanding.request.after_header_id != self.schedule.parent_commit_ref()
                    {
                        continue;
                    }
                    // Validate the entire page with a temporary capability chain;
                    // no schedule is installed until each actual private commit.
                    let mut schedule = self.schedule.clone();
                    let valid =
                        reply.certificates.iter().all(
                            |c| match verify_commit_certificate_token_v3(&schedule, c) {
                                Ok(token) => {
                                    schedule = token.next_height_schedule().clone();
                                    true
                                }
                                Err(_) => false,
                            },
                        );
                    self.peers.request = None;
                    if valid && self.peers.history.is_empty() {
                        self.peers.history.extend(
                            reply
                                .certificates
                                .into_iter()
                                .map(|c| (received.peer_id.clone(), c)),
                        );
                    }
                }
                message => {
                    // The protected journal is the sole recovery path for our
                    // own signing slots. A peer echo is never an external event.
                    if is_local_echo(&message, &self.selected.binding.local_validator_id) {
                        continue;
                    }
                    if super::super::membership::active_key(&self.selected, &self.schedule)
                        .is_none()
                        && !matches!(message, BftPublicPeerMessageV3::Commit(_))
                    {
                        continue;
                    }
                    if self.peers.seen.contains(&digest)
                        || self.peers.seen.len()
                            >= BFT_MAX_CACHED_VOTES_V3 + BFT_MAX_PROPOSAL_EQUIVOCATIONS_V3
                        || self.peers.queued.iter().any(|i| i.message == message)
                        || self
                            .peers
                            .fetching
                            .as_ref()
                            .is_some_and(|f| f.input.message == message)
                    {
                        continue;
                    }
                    let height = message_height(&message);
                    if height > self.schedule.height() {
                        self.request_history()?;
                        continue;
                    }
                    if height != self.schedule.height() {
                        continue;
                    }
                    if let BftPublicPeerMessageV3::Vote(vote) = message {
                        if super::super::membership::active_key(&self.selected, &self.schedule)
                            .is_none()
                        {
                            continue;
                        }
                        let now = self
                            .signer()?
                            .protected_time_ms()
                            .map_err(|_| error("wire-3 protected peer clock unavailable"))?;
                        let mut next = self.machine.clone();
                        if let Ok(effects) = next.receive_vote(*vote.clone(), now) {
                            let previous = self.journal()?;
                            let mut journal = previous.clone();
                            journal.events.push(BftSigningJournalEventV3::Vote {
                                vote,
                                local: false,
                                now_ms: now,
                            });
                            if !self.valid_peer_journal(&previous, &journal) {
                                continue;
                            }
                            self.advance(journal)?;
                            self.machine = next;
                            self.effects.extend(effects);
                            self.peers.seen.insert(digest);
                        }
                    } else if self.validate_public_input(&message).is_ok()
                        && self.peers.queued.len() < QUEUED_COUNT
                        && self.peers.queued_bytes.saturating_add(encoded.len()) <= QUEUED_BYTES
                    {
                        self.peers.queued_bytes += encoded.len();
                        self.peers.queued.push_back(Incoming {
                            peer: received.peer_id,
                            message,
                            bytes: encoded.len(),
                        });
                    }
                }
            }
        }
        if self.peers.fetching.is_none() {
            let input = if let Some((peer, certificate)) = self.peers.history.pop_front() {
                Some(Incoming {
                    peer,
                    message: BftPublicPeerMessageV3::Commit(Box::new(certificate)),
                    bytes: 0,
                })
            } else {
                let item = self.peers.queued.pop_front();
                if let Some(item) = &item {
                    self.peers.queued_bytes = self.peers.queued_bytes.saturating_sub(item.bytes);
                }
                item
            };
            if let Some(input) = input
                && self.validate_public_input(&input.message).is_ok()
            {
                let value = message_value(&input.message)
                    .ok_or_else(|| error("wire-3 missing public value"))?;
                if startup::candidate(
                    &self.config,
                    &self.hook,
                    &self.schedule,
                    &value,
                    &self.selected,
                )
                .is_ok()
                {
                    self.accept_materialized(input.message)?;
                } else if self.peers.fetch_budget.reserve(
                    &configured_peers,
                    &input.peer,
                    &value.block_id().map_err(error)?,
                ) {
                    let context = NodeBftExecutionContextV3::from_schedule(&self.schedule);
                    let kind = if matches!(input.message, BftPublicPeerMessageV3::Commit(_)) {
                        NodeBftPrivateFetchKindV3::CommittedState
                    } else {
                        NodeBftPrivateFetchKindV3::PendingCandidate
                    };
                    if let Ok(operation) = network.exchange.start_fetch(
                        NodeBftPrivateFetchRequestV3 {
                            peer_id: input.peer.clone(),
                            context: context.clone(),
                            value,
                            kind,
                        },
                        self.lease.clone(),
                    ) {
                        self.peers.fetching = Some(Fetching {
                            input,
                            operation,
                            context,
                            deadline: Instant::now() + Duration::from_secs(600),
                        });
                    }
                }
            }
        }
        if self
            .peers
            .last_send
            .is_none_or(|t| t.elapsed() >= Duration::from_millis(500))
        {
            self.peers.last_send = Some(Instant::now());
            for event in self
                .checkpoint
                .state
                .journal
                .iter()
                .flat_map(|j| j.events.iter())
                .rev()
                .take(512)
            {
                match event {
                    BftSigningJournalEventV3::Proposal {
                        proposal,
                        local: true,
                        ..
                    } if proposal.header.round == self.machine.round() => {
                        self.broadcast(BftPublicPeerMessageV3::Proposal(proposal.clone()))
                    }
                    BftSigningJournalEventV3::Vote {
                        vote, local: true, ..
                    } if vote.round == self.machine.round() => {
                        self.broadcast(BftPublicPeerMessageV3::Vote(vote.clone()))
                    }
                    _ => {}
                }
            }
            if let Some(c) = self.checkpoint.state.history.last() {
                self.broadcast(BftPublicPeerMessageV3::Commit(Box::new(c.clone())));
            }
        }
        self.request_history()?;
        Ok(())
    }
    fn validate_public_input(&self, message: &BftPublicPeerMessageV3) -> Result<(), String> {
        if let Some(value) = message_value(message)
            && !self.selected.membership_enabled
            && (value.execution_commitment.transition.is_some()
                || value.execution_commitment.transition_ref.is_some())
        {
            return Err("wire-3 Node membership installation is not enabled".into());
        }
        match message {
            BftPublicPeerMessageV3::Proposal(p) => {
                // This gate is also evaluated when dequeuing an older input,
                // before reserving any private-fetch budget. An inactive
                // follower may recover committed state, never pending input.
                if super::super::membership::active_key(&self.selected, &self.schedule).is_none() {
                    return Err("wire-3 inactive follower cannot fetch a pending proposal".into());
                }
                p.validate_binding()?;
                p.block.validate_for_schedule(&self.schedule)?;
                let expected = self.schedule.expected_proposer_for_round(p.header.round)?;
                if p.header.proposer_id != expected {
                    return Err("wire-3 wrong proposer".into());
                }
                let validator = self
                    .schedule
                    .active_set()
                    .validator(&expected)
                    .ok_or("wire-3 unknown proposer")?;
                if p.header.public_key_hex != validator.public_key_hex {
                    return Err("wire-3 proposer key mismatch".into());
                }
                p.header.verify_signature()
            }
            BftPublicPeerMessageV3::Commit(c) => {
                verify_commit_certificate_token_v3(&self.schedule, c).map(|_| ())
            }
            _ => Err("wire-3 private input requested for unsupported message".into()),
        }
    }
    fn accept_materialized(&mut self, message: BftPublicPeerMessageV3) -> Result<(), NodeError> {
        if is_local_echo(&message, &self.selected.binding.local_validator_id) {
            return Ok(());
        }
        let digest = message_digest(&message)?;
        match message {
            BftPublicPeerMessageV3::Proposal(proposal) => {
                if super::super::membership::active_key(&self.selected, &self.schedule).is_none() {
                    return Ok(());
                }
                let now = self
                    .signer()?
                    .protected_time_ms()
                    .map_err(|_| error("wire-3 protected peer clock unavailable"))?;
                let mut next = self.machine.clone();
                if let Ok(effects) = next.receive_proposal(*proposal.clone(), true, now) {
                    let previous = self.journal()?;
                    let mut journal = previous.clone();
                    journal.events.push(BftSigningJournalEventV3::Proposal {
                        proposal,
                        local: false,
                        now_ms: now,
                    });
                    if !self.valid_peer_journal(&previous, &journal) {
                        return Ok(());
                    }
                    self.advance(journal)?;
                    self.machine = next;
                    self.effects.extend(effects);
                    self.peers.seen.insert(digest);
                }
            }
            BftPublicPeerMessageV3::Commit(c) => self.apply_commit(*c)?,
            _ => {}
        }
        Ok(())
    }
    // Reject untrusted event shapes locally, before a protected-provider call.
    // A failure after this check still fail-stops: a provider response may have
    // an uncertain durable outcome and must never be treated as peer rejection.
    fn valid_peer_journal(
        &self,
        previous: &BftSigningJournalV3,
        next: &BftSigningJournalV3,
    ) -> bool {
        verify_bft_signing_journal_transition_v3(
            previous,
            next,
            self.schedule.active_set(),
            &self.schedule,
            self.selected.binding.timeouts,
        )
        .is_ok()
    }
    fn request_history(&mut self) -> Result<(), NodeError> {
        if self.peers.request.is_some()
            || self.peers.is_catching_up()
            || self
                .peers
                .last_request
                .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return Ok(());
        }
        let Some(network) = &self.selected.network else {
            return Ok(());
        };
        let peers = network.transport.peers();
        if peers.is_empty() {
            return Ok(());
        }
        // Unverified future-height hints never choose the next route. Every
        // configured peer keeps its deterministic turn despite hostile hints.
        let peer = peers[self.peers.next_peer % peers.len()].clone();
        self.peers.next_peer = self.peers.next_peer.wrapping_add(1);
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| error("wire-3 history nonce unavailable"))?;
        let request = BftPeerHistoryRequestV3 {
            nonce: hex::encode(nonce),
            after_height: self.schedule.height().saturating_sub(1),
            after_header_id: self.schedule.parent_commit_ref().into(),
            max_entries: (64 - self.checkpoint.state.history.len()) as u16,
        };
        if request.max_entries == 0 {
            return Ok(());
        }
        let started_at = Instant::now();
        if network
            .transport
            .send_public(
                &peer,
                &BftPublicPeerMessageV3::HistoryRequest(request.clone()),
            )
            .is_ok()
        {
            self.peers.request = Some(HistoryRequest {
                peer,
                request,
                deadline: started_at + HISTORY_RESPONSE_TIMEOUT,
            });
            self.peers.last_request = Some(started_at);
        }
        Ok(())
    }
    fn serve_history(&self, peer: &str, request: BftPeerHistoryRequestV3) -> Result<(), NodeError> {
        let Some(network) = &self.selected.network else {
            return Ok(());
        };
        let initial = self.selected.binding.initial_schedule();
        let anchor_valid = (request.after_height.checked_add(1) == Some(initial.height())
            && request.after_header_id == initial.parent_commit_ref())
            || self.checkpoint.state.history.iter().any(|c| {
                c.execution_commitment.height == request.after_height
                    && c.header
                        .block_id()
                        .is_ok_and(|id| id == request.after_header_id)
            });
        if !anchor_valid {
            return Ok(());
        }
        let certificates = self
            .checkpoint
            .state
            .history
            .iter()
            .filter(|c| c.execution_commitment.height > request.after_height)
            .take(usize::from(request.max_entries))
            .cloned()
            .collect();
        let reply = bounded_history_reply(network.transport.context(), request, certificates)?;
        let _ = network
            .transport
            .send_public(peer, &BftPublicPeerMessageV3::HistoryReply(reply));
        Ok(())
    }
}
fn message_height(message: &BftPublicPeerMessageV3) -> u64 {
    match message {
        BftPublicPeerMessageV3::Proposal(p) => p.header.height,
        BftPublicPeerMessageV3::Vote(v) => v.height,
        BftPublicPeerMessageV3::Commit(c) => c.execution_commitment.height,
        _ => 0,
    }
}
fn message_value(message: &BftPublicPeerMessageV3) -> Option<BftHeaderValueV3> {
    match message {
        BftPublicPeerMessageV3::Proposal(p) => Some(p.block.clone()),
        BftPublicPeerMessageV3::Commit(c) => Some(BftHeaderValueV3 {
            header: c.header.clone(),
            execution_commitment: c.execution_commitment.clone(),
        }),
        _ => None,
    }
}

fn message_digest(message: &BftPublicPeerMessageV3) -> Result<String, NodeError> {
    let mut message = message.clone();
    // Alternate valid signatures over one vote do not consume fresh journal
    // capacity or starve a different validator's legitimate slot.
    if let BftPublicPeerMessageV3::Vote(vote) = &mut message {
        vote.signature_hex.clear();
    }
    let bytes =
        serde_cbor::to_vec(&message).map_err(|_| error("wire-3 peer message index encode"))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

// Select the largest encodable contiguous prefix. Binary search avoids
// serializing a near-16MiB page once per removed certificate. No request
// identity, anchor or original deadline is changed to make a page fit.
fn bounded_history_reply(
    context: &BftPeerContextV3,
    request: BftPeerHistoryRequestV3,
    mut certificates: Vec<FormalBftCertifiedHeaderV3>,
) -> Result<BftPeerHistoryReplyV3, NodeError> {
    certificates.truncate(usize::from(request.max_entries));
    let mut low = 0usize;
    let mut high = certificates.len();
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        let message = BftPublicPeerMessageV3::HistoryReply(BftPeerHistoryReplyV3 {
            request: request.clone(),
            certificates: certificates[..mid].to_vec(),
        });
        if encode_bft_public_peer_message_v3(context, &message).is_ok() {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    if low == 0 && !certificates.is_empty() {
        return Err(error(
            "wire-3 first history certificate cannot fit public response",
        ));
    }
    certificates.truncate(low);
    let reply = BftPeerHistoryReplyV3 {
        request,
        certificates,
    };
    encode_bft_public_peer_message_v3(
        context,
        &BftPublicPeerMessageV3::HistoryReply(reply.clone()),
    )
    .map_err(error)?;
    Ok(reply)
}
#[cfg(test)]
#[path = "node_bft_v3_worker_peer_tests.rs"]
mod tests;

fn is_local_echo(message: &BftPublicPeerMessageV3, local: &str) -> bool {
    match message {
        BftPublicPeerMessageV3::Proposal(proposal) => proposal.header.proposer_id == local,
        BftPublicPeerMessageV3::Vote(vote) => vote.validator_id == local,
        // A verified QC remains admissible independently of its proposer. It
        // may be the only available catch-up proof after a local interruption.
        _ => false,
    }
}

#[derive(Default)]
struct FetchBudget {
    peers: BTreeMap<String, usize>,
    values: BTreeMap<(String, String), usize>,
}
impl FetchBudget {
    fn reserve(&mut self, configured: &[String], peer: &str, value: &str) -> bool {
        // Quotas derive only from fixed configured Noise routes, never labels
        // supplied in a protocol message. One peer cannot consume another's
        // share; all failed starts and fetches still consume their own share.
        if configured.len() > 256 || !configured.iter().any(|p| p == peer) {
            return false;
        }
        let key = (peer.to_owned(), value.to_owned());
        if self.peers.get(peer).copied().unwrap_or(0) >= 64
            || self.values.get(&key).copied().unwrap_or(0) >= 4
        {
            return false;
        }
        *self.peers.entry(peer.to_owned()).or_default() += 1;
        *self.values.entry(key).or_default() += 1;
        true
    }
}
