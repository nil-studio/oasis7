//! Owned transport attempt admission, handshake deadlines, and muxer-bound permits.
//!
//! Pairs [`NetworkBehaviour`](libp2p::swarm::NetworkBehaviour) pending-connection
//! hooks with [`BudgetedTransport`] so upstream `libp2p-swarm` remains unpatched:
//! - Inbound listener upgrades are gated in `handle_pending_inbound_connection`
//!   before the upgrade future is ever polled, transferring the permit into
//!   [`PermittedMuxer`] on completion.
//! - Outbound dials preflight candidate admission in `handle_pending_outbound_connection`
//!   (returning `DialError::Denied` synchronously when all candidates are denied)
//!   and bind per-address permits in `BudgetedTransport::dial` without invoking
//!   the inner transport for budget-denied candidates.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    io,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll, ready},
    time::Instant,
};

use futures::{
    FutureExt,
    future::{BoxFuture, poll_fn},
};
use libp2p::{
    Multiaddr, PeerId, Transport,
    core::{
        muxing::{StreamMuxer, StreamMuxerBox, StreamMuxerEvent, SubstreamBox},
        transport::{self, ListenerId, TransportError, TransportEvent},
    },
    swarm::{ConnectionDenied, ConnectionId, FromSwarm},
};

/// An admission token whose destructor releases the caller's transport attempt resources.
pub(super) type TransportAttemptPermit = Box<dyn Send>;

/// Resources and an optional absolute deadline for a transport handshake.
pub(super) struct TransportAdmission {
    permit: TransportAttemptPermit,
    deadline: Option<Instant>,
}

impl TransportAdmission {
    /// Retain the permit while the attempt runs and terminate at the given instant.
    pub(super) fn until(permit: TransportAttemptPermit, deadline: Instant) -> Self {
        Self {
            permit,
            deadline: Some(deadline),
        }
    }

    #[cfg(test)]
    pub(super) fn unbounded(permit: TransportAttemptPermit) -> Self {
        Self {
            permit,
            deadline: None,
        }
    }

    fn check(self) -> Result<Self, ConnectionDenied> {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(ConnectionDenied::new(TransportHandshakeDeadlineElapsed));
        }
        Ok(self)
    }
}

/// The caller's absolute handshake deadline elapsed before or during negotiation.
#[derive(Debug)]
pub(super) struct TransportHandshakeDeadlineElapsed;

impl std::fmt::Display for TransportHandshakeDeadlineElapsed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("transport handshake deadline expired")
    }
}

impl std::error::Error for TransportHandshakeDeadlineElapsed {}

/// Marker error for an individual outbound candidate skipped by local admission
/// while at least one sibling candidate in the same `Swarm::dial` was admitted.
#[derive(Debug)]
pub(super) struct OutboundAttemptBudgetDenied;

impl std::fmt::Display for OutboundAttemptBudgetDenied {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("outbound transport attempt denied by local budget")
    }
}

impl std::error::Error for OutboundAttemptBudgetDenied {}

/// Inspects an `io::Error` and any `libp2p::core::transport::Boxed` `io::Error::other`
/// wrappers around it for a typed local transport admission or deadline marker `T`.
pub(super) fn has_transport_error_cause<T: std::error::Error + 'static>(error: &io::Error) -> bool {
    let mut current: Option<&(dyn std::error::Error + 'static)> = error
        .get_ref()
        .map(|inner| inner as &(dyn std::error::Error + 'static));
    while let Some(cause) = current {
        if cause.is::<T>() {
            return true;
        }
        if let Some(nested_io) = cause.downcast_ref::<io::Error>() {
            current = nested_io
                .get_ref()
                .map(|inner| inner as &(dyn std::error::Error + 'static));
        } else {
            current = cause.source();
        }
    }
    false
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, TransportHandshakeDeadlineElapsed)
}

async fn until<F: Future + Unpin>(mut future: F, deadline: Instant) -> io::Result<F::Output> {
    let mut timer = futures_timer::Delay::new(deadline.saturating_duration_since(Instant::now()));
    poll_fn(|cx| {
        if Instant::now() >= deadline {
            return Poll::Ready(Err(timed_out()));
        }
        if Pin::new(&mut timer).poll(cx).is_ready() {
            // Re-arm if timer granularity wakes the task before the absolute deadline.
            timer.reset(deadline.saturating_duration_since(Instant::now()));
            cx.waker().wake_by_ref();
        }
        let result = ready!(Pin::new(&mut future).poll(cx));
        if Instant::now() >= deadline {
            return Poll::Ready(Err(timed_out()));
        }
        Poll::Ready(Ok(result))
    })
    .await
}

type InboundAdmitFn = Box<
    dyn FnMut(ConnectionId, &Multiaddr, &Multiaddr) -> Result<TransportAdmission, ConnectionDenied>
        + Send,
>;

type OutboundAdmitFn = Box<
    dyn FnMut(
            ConnectionId,
            Option<PeerId>,
            &Multiaddr,
        ) -> Result<TransportAdmission, ConnectionDenied>
        + Send,
>;

type InboundSlot = Mutex<Option<TransportAdmission>>;

struct OutboundDialSession {
    connection_id: ConnectionId,
    peer_id: Option<PeerId>,
    staged: VecDeque<(Multiaddr, Result<TransportAdmission, ()>)>,
}

#[derive(Default)]
struct GateState {
    inbound_admit: Option<InboundAdmitFn>,
    outbound_admit: Option<OutboundAdmitFn>,
    listened_addrs: HashMap<ListenerId, Vec<Multiaddr>>,
    pending_inbound: VecDeque<Weak<InboundSlot>>,
    outbound_session: Option<OutboundDialSession>,
}

impl GateState {
    fn is_listened_addr(&self, candidate: &Multiaddr) -> bool {
        self.listened_addrs
            .values()
            .flatten()
            .any(|addr| addr == candidate)
    }

    fn pop_next_inbound_slot(&mut self) -> Option<Arc<InboundSlot>> {
        while let Some(weak) = self.pending_inbound.pop_front() {
            if let Some(slot) = weak.upgrade() {
                return Some(slot);
            }
        }
        None
    }
}

/// Shared coordinator connecting `NetworkBehaviour` pending-connection callbacks
/// with [`BudgetedTransport`].
#[derive(Clone, Default)]
pub(super) struct TransportAdmissionGate(Arc<Mutex<GateState>>);

impl TransportAdmissionGate {
    pub(super) fn with_callbacks(
        inbound: impl FnMut(
            ConnectionId,
            &Multiaddr,
            &Multiaddr,
        ) -> Result<TransportAdmission, ConnectionDenied>
        + Send
        + 'static,
        outbound: impl FnMut(
            ConnectionId,
            Option<PeerId>,
            &Multiaddr,
        ) -> Result<TransportAdmission, ConnectionDenied>
        + Send
        + 'static,
    ) -> Self {
        Self(Arc::new(Mutex::new(GateState {
            inbound_admit: Some(Box::new(inbound)),
            outbound_admit: Some(Box::new(outbound)),
            ..GateState::default()
        })))
    }

    #[cfg(test)]
    pub(super) fn inbound_only(
        mut inbound: impl FnMut(
            ConnectionId,
            &Multiaddr,
            &Multiaddr,
        ) -> Result<TransportAttemptPermit, ConnectionDenied>
        + Send
        + 'static,
    ) -> Self {
        Self(Arc::new(Mutex::new(GateState {
            inbound_admit: Some(Box::new(move |id, local, remote| {
                inbound(id, local, remote).map(TransportAdmission::unbounded)
            })),
            ..GateState::default()
        })))
    }

    #[cfg(test)]
    pub(super) fn outbound_only(
        mut outbound: impl FnMut(
            ConnectionId,
            Option<PeerId>,
            &Multiaddr,
        ) -> Result<TransportAttemptPermit, ConnectionDenied>
        + Send
        + 'static,
    ) -> Self {
        Self(Arc::new(Mutex::new(GateState {
            outbound_admit: Some(Box::new(move |id, peer, addr| {
                outbound(id, peer, addr).map(TransportAdmission::unbounded)
            })),
            ..GateState::default()
        })))
    }

    pub(super) fn wrap_transport<T>(&self, inner: T) -> BudgetedTransport<T> {
        BudgetedTransport {
            inner,
            gate: self.clone(),
        }
    }

    pub(super) fn clear_outbound_session(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.outbound_session = None;
        }
    }

    pub(super) fn on_swarm_event(&self, event: FromSwarm<'_>) {
        if let FromSwarm::DialFailure(failure) = event
            && let Ok(mut state) = self.0.lock()
            && state
                .outbound_session
                .as_ref()
                .is_some_and(|session| session.connection_id == failure.connection_id)
        {
            state.outbound_session = None;
        }
    }

    pub(super) fn handle_pending_inbound(
        &self,
        id: ConnectionId,
        local: &Multiaddr,
        remote: &Multiaddr,
        inner_check: impl FnOnce() -> Result<(), ConnectionDenied>,
    ) -> Result<(), ConnectionDenied> {
        // Always pop the queued listener slot before running inner behaviour checks so
        // an inner rejection never leaves a stale slot behind for a future connection.
        let slot = {
            let mut state = self
                .0
                .lock()
                .map_err(|_| ConnectionDenied::new(io::Error::other("admission gate poisoned")))?;
            if state.inbound_admit.is_some() {
                Some(state.pop_next_inbound_slot().ok_or_else(|| {
                    ConnectionDenied::new(io::Error::other(
                        "missing inbound transport upgrade slot",
                    ))
                })?)
            } else {
                None
            }
        };
        inner_check()?;
        if let Some(slot) = slot {
            let admission = {
                let mut state = self.0.lock().map_err(|_| {
                    ConnectionDenied::new(io::Error::other("admission gate poisoned"))
                })?;
                let Some(admit) = state.inbound_admit.as_mut() else {
                    return Ok(());
                };
                admit(id, local, remote)?.check()?
            };
            let mut guard = slot
                .lock()
                .map_err(|_| ConnectionDenied::new(io::Error::other("inbound slot poisoned")))?;
            *guard = Some(admission);
        }
        Ok(())
    }

    pub(super) fn handle_pending_outbound(
        &self,
        id: ConnectionId,
        peer: Option<PeerId>,
        opts_addresses: &[Multiaddr],
        augmented_addresses: Vec<Multiaddr>,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| ConnectionDenied::new(io::Error::other("admission gate poisoned")))?;
        state.outbound_session = None;
        if state.outbound_admit.is_none() {
            return Ok(augmented_addresses);
        }

        let mut seen = HashSet::new();
        let mut opts_dialable = Vec::new();
        let mut opts_has_conflicting_peer_addr = false;
        for addr in opts_addresses {
            if state.is_listened_addr(addr) || !seen.insert(addr.clone()) {
                continue;
            }
            match peer.map_or_else(|| Ok(addr.clone()), |p| addr.clone().with_p2p(p)) {
                Ok(dial_addr) => opts_dialable.push(dial_addr),
                Err(_) => opts_has_conflicting_peer_addr = true,
            }
        }

        // `DialOpts::unknown_peer_id()` always sets `extend_addresses_through_behaviour = false`.
        let aug_candidates = if let Some(p) = peer {
            let mut list = Vec::new();
            for addr in augmented_addresses {
                if state.is_listened_addr(&addr) || !seen.insert(addr.clone()) {
                    continue;
                }
                let resolved = addr.clone().with_p2p(p);
                list.push((addr, resolved));
            }
            list
        } else {
            Vec::new()
        };

        let Some(mut admit) = state.outbound_admit.take() else {
            return Ok(Vec::new());
        };
        let result = (|| {
            let mut staged = VecDeque::new();
            let mut last_denied: Option<ConnectionDenied> = None;

            if !opts_dialable.is_empty() || opts_has_conflicting_peer_addr || peer.is_none() {
                // `DialOpts` with explicit addresses defaults `extend_addresses_through_behaviour`
                // to `false` (causing `Swarm::dial` to discard `augmented_addresses`). Never
                // pre-reserve `aug_candidates` here; if `extend_addresses_through_behaviour` was
                // explicitly enabled, `BudgetedTransport::dial` admits passthrough addresses on demand.
                let mut any_opts_admitted = false;
                for dial_addr in opts_dialable {
                    match admit(id, peer, &dial_addr).and_then(TransportAdmission::check) {
                        Ok(admission) => {
                            any_opts_admitted = true;
                            staged.push_back((dial_addr, Ok(admission)));
                        }
                        Err(denied) => {
                            last_denied = Some(denied);
                            staged.push_back((dial_addr, Err(())));
                        }
                    }
                }

                if !any_opts_admitted
                    && !opts_has_conflicting_peer_addr
                    && let Some(denied) = last_denied
                {
                    return Err(denied);
                }

                let passthrough: Vec<Multiaddr> =
                    aug_candidates.into_iter().map(|(raw, _)| raw).collect();
                if !staged.is_empty() || !passthrough.is_empty() {
                    state.outbound_session = Some(OutboundDialSession {
                        connection_id: id,
                        peer_id: peer,
                        staged,
                    });
                }
                return Ok(passthrough);
            }

            // `opts` carried no non-listened addresses for a known `peer` (e.g. `DialOpts::peer_id(p).build()`).
            let mut any_aug_admitted = false;
            let mut aug_has_conflicting_peer_addr = false;
            let mut all_aug_raw = Vec::new();
            for (raw_addr, resolved) in aug_candidates {
                match resolved {
                    Ok(dial_addr) => {
                        match admit(id, peer, &dial_addr).and_then(TransportAdmission::check) {
                            Ok(admission) => {
                                any_aug_admitted = true;
                                staged.push_back((dial_addr, Ok(admission)));
                                all_aug_raw.push(raw_addr);
                            }
                            Err(denied) => {
                                last_denied = Some(denied);
                                staged.push_back((dial_addr, Err(())));
                                all_aug_raw.push(raw_addr);
                            }
                        }
                    }
                    Err(_) => {
                        aug_has_conflicting_peer_addr = true;
                        all_aug_raw.push(raw_addr);
                    }
                }
            }

            if !any_aug_admitted
                && !aug_has_conflicting_peer_addr
                && let Some(denied) = last_denied
            {
                return Err(denied);
            }

            if !staged.is_empty() {
                state.outbound_session = Some(OutboundDialSession {
                    connection_id: id,
                    peer_id: peer,
                    staged,
                });
            }
            Ok(all_aug_raw)
        })();
        state.outbound_admit = Some(admit);
        result
    }
}

/// Transport wrapper that enforces per-attempt outbound admission, handshake deadlines,
/// and muxer-bound permit lifetimes over an unpatched `libp2p` transport.
pub(super) struct BudgetedTransport<T> {
    inner: T,
    gate: TransportAdmissionGate,
}

type BoxedConnectionOutput = (PeerId, StreamMuxerBox);
type BoxedDialFuture = BoxFuture<'static, Result<BoxedConnectionOutput, io::Error>>;

impl<T> Transport for BudgetedTransport<T>
where
    T: Transport<Output = BoxedConnectionOutput, Error = io::Error> + Unpin,
    T::Dial: Send + 'static,
    T::ListenerUpgrade: Send + 'static,
{
    type Output = BoxedConnectionOutput;
    type Error = io::Error;
    type ListenerUpgrade = BudgetedListenerUpgrade<T::ListenerUpgrade>;
    type Dial = BoxedDialFuture;

    fn listen_on(
        &mut self,
        id: ListenerId,
        addr: Multiaddr,
    ) -> Result<(), TransportError<Self::Error>> {
        self.inner.listen_on(id, addr)
    }

    fn remove_listener(&mut self, id: ListenerId) -> bool {
        // Defer removing `id` from `GateState::listened_addrs` until `TransportEvent::ListenerClosed`
        // so the set stays synchronized with `Swarm::listened_addrs`.
        self.inner.remove_listener(id)
    }

    fn dial(
        &mut self,
        addr: Multiaddr,
        opts: transport::DialOpts,
    ) -> Result<Self::Dial, TransportError<Self::Error>> {
        let admission = {
            let mut state = self.gate.0.lock().map_err(|_| {
                TransportError::Other(io::Error::other("admission gate lock poisoned"))
            })?;
            if state.outbound_admit.is_none() {
                None
            } else {
                let Some(session) = state.outbound_session.as_mut() else {
                    return Err(TransportError::Other(io::Error::other(
                        "unassociated outbound transport dial",
                    )));
                };
                let conn_id = session.connection_id;
                let peer_id = session.peer_id;
                if let Some(pos) = session
                    .staged
                    .iter()
                    .position(|(staged_addr, _)| staged_addr == &addr)
                {
                    let (_, staged_decision) = session
                        .staged
                        .remove(pos)
                        .ok_or_else(|| TransportError::Other(io::Error::other("missing stage")))?;
                    match staged_decision {
                        Ok(admission) => Some(admission),
                        Err(()) => {
                            return Err(TransportError::Other(io::Error::other(
                                OutboundAttemptBudgetDenied,
                            )));
                        }
                    }
                } else {
                    let Some(admit) = state.outbound_admit.as_mut() else {
                        return Err(TransportError::Other(io::Error::other(
                            "missing outbound admission callback",
                        )));
                    };
                    match admit(conn_id, peer_id, &addr).and_then(TransportAdmission::check) {
                        Ok(admission) => Some(admission),
                        Err(_) => {
                            return Err(TransportError::Other(io::Error::other(
                                OutboundAttemptBudgetDenied,
                            )));
                        }
                    }
                }
            }
        };

        let dial_fut = self.inner.dial(addr, opts)?;
        Ok(match admission {
            Some(admission) => hold_outbound_permit(dial_fut.boxed(), admission),
            None => dial_fut.boxed(),
        })
    }

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<TransportEvent<Self::ListenerUpgrade, Self::Error>> {
        let event = ready!(Pin::new(&mut self.inner).poll(cx));
        match event {
            TransportEvent::NewAddress {
                listener_id,
                listen_addr,
            } => {
                if let Ok(mut state) = self.gate.0.lock() {
                    let addrs = state.listened_addrs.entry(listener_id).or_default();
                    if !addrs.contains(&listen_addr) {
                        addrs.push(listen_addr.clone());
                    }
                }
                Poll::Ready(TransportEvent::NewAddress {
                    listener_id,
                    listen_addr,
                })
            }
            TransportEvent::AddressExpired {
                listener_id,
                listen_addr,
            } => {
                if let Ok(mut state) = self.gate.0.lock()
                    && let Some(addrs) = state.listened_addrs.get_mut(&listener_id)
                {
                    addrs.retain(|existing| existing != &listen_addr);
                }
                Poll::Ready(TransportEvent::AddressExpired {
                    listener_id,
                    listen_addr,
                })
            }
            TransportEvent::ListenerClosed {
                listener_id,
                reason,
            } => {
                if let Ok(mut state) = self.gate.0.lock() {
                    state.listened_addrs.remove(&listener_id);
                }
                Poll::Ready(TransportEvent::ListenerClosed {
                    listener_id,
                    reason,
                })
            }
            TransportEvent::ListenerError { listener_id, error } => {
                Poll::Ready(TransportEvent::ListenerError { listener_id, error })
            }
            TransportEvent::Incoming {
                listener_id,
                upgrade,
                local_addr,
                send_back_addr,
            } => {
                let upgrade = match self.gate.0.lock() {
                    Ok(mut state) if state.inbound_admit.is_some() => {
                        while state
                            .pending_inbound
                            .front()
                            .is_some_and(|weak| weak.strong_count() == 0)
                        {
                            state.pending_inbound.pop_front();
                        }
                        let slot = Arc::new(Mutex::new(None));
                        state.pending_inbound.push_back(Arc::downgrade(&slot));
                        BudgetedListenerUpgrade {
                            state: InboundUpgradeState::Gated(upgrade),
                            permit: None,
                            slot: Some(slot),
                        }
                    }
                    Ok(_) => BudgetedListenerUpgrade {
                        state: InboundUpgradeState::Passthrough(upgrade.boxed()),
                        permit: None,
                        slot: None,
                    },
                    Err(_) => BudgetedListenerUpgrade {
                        state: InboundUpgradeState::Failed("admission gate lock poisoned"),
                        permit: None,
                        slot: None,
                    },
                };
                Poll::Ready(TransportEvent::Incoming {
                    listener_id,
                    upgrade,
                    local_addr,
                    send_back_addr,
                })
            }
        }
    }
}

fn hold_outbound_permit(future: BoxedDialFuture, admission: TransportAdmission) -> BoxedDialFuture {
    let future = if let Some(deadline) = admission.deadline {
        async move { until(future, deadline).await? }.boxed()
    } else {
        future
    };
    Box::pin(PermittedAttempt {
        future,
        permit: Some(admission.permit),
    })
}

struct PermittedAttempt {
    // Declared first so cancelling the attempt drops the transport future before its permit.
    future: BoxedDialFuture,
    permit: Option<TransportAttemptPermit>,
}

impl Future for PermittedAttempt {
    type Output = Result<BoxedConnectionOutput, io::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let result = ready!(Pin::new(&mut self.future).poll(cx));
        Poll::Ready(retain_connected(result, &mut self.permit))
    }
}

enum InboundUpgradeState<U> {
    Passthrough(BoxedDialFuture),
    Gated(U),
    Running(BoxedDialFuture),
    Failed(&'static str),
    Done,
}

pub(super) struct BudgetedListenerUpgrade<U> {
    // Declared first so dropping an unpolled or in-flight upgrade drops `U` before the permit.
    state: InboundUpgradeState<U>,
    permit: Option<TransportAttemptPermit>,
    slot: Option<Arc<InboundSlot>>,
}

impl<U> Unpin for BudgetedListenerUpgrade<U> {}

impl<U> Future for BudgetedListenerUpgrade<U>
where
    U: Future<Output = Result<BoxedConnectionOutput, io::Error>> + Send + 'static,
{
    type Output = Result<BoxedConnectionOutput, io::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().get_mut();
        if matches!(this.state, InboundUpgradeState::Gated(_)) {
            let InboundUpgradeState::Gated(upgrade) =
                std::mem::replace(&mut this.state, InboundUpgradeState::Done)
            else {
                return Poll::Ready(Err(io::Error::other("invalid listener upgrade state")));
            };
            let admission = this
                .slot
                .take()
                .and_then(|slot| slot.lock().ok().and_then(|mut guard| guard.take()));
            let Some(admission) = admission else {
                return Poll::Ready(Err(io::Error::other(
                    "unadmitted inbound transport upgrade",
                )));
            };
            this.permit = Some(admission.permit);
            let run_future = if let Some(deadline) = admission.deadline {
                async move { until(upgrade.boxed(), deadline).await? }.boxed()
            } else {
                upgrade.boxed()
            };
            this.state = InboundUpgradeState::Running(run_future);
        }

        match &mut this.state {
            InboundUpgradeState::Passthrough(future) => {
                let result = ready!(Pin::new(future).poll(cx));
                this.state = InboundUpgradeState::Done;
                Poll::Ready(result)
            }
            InboundUpgradeState::Running(future) => {
                let result = ready!(Pin::new(future).poll(cx));
                this.state = InboundUpgradeState::Done;
                Poll::Ready(retain_connected(result, &mut this.permit))
            }
            InboundUpgradeState::Failed(reason) => {
                let reason = *reason;
                this.state = InboundUpgradeState::Done;
                Poll::Ready(Err(io::Error::other(reason)))
            }
            InboundUpgradeState::Gated(_) | InboundUpgradeState::Done => Poll::Ready(Err(
                io::Error::other("inbound transport upgrade polled after completion"),
            )),
        }
    }
}

fn retain_connected(
    result: Result<BoxedConnectionOutput, io::Error>,
    permit: &mut Option<TransportAttemptPermit>,
) -> Result<BoxedConnectionOutput, io::Error> {
    let (peer, muxer) = result?;
    let Some(permit) = permit.take() else {
        return Err(io::Error::other("transport attempt permit missing"));
    };
    Ok((
        peer,
        StreamMuxerBox::new(PermittedMuxer {
            muxer,
            _permit: permit,
        }),
    ))
}

struct PermittedMuxer {
    // Declared first so dropping the connection drops the underlying muxer before its permit.
    muxer: StreamMuxerBox,
    _permit: TransportAttemptPermit,
}

impl StreamMuxer for PermittedMuxer {
    type Substream = SubstreamBox;
    type Error = io::Error;

    fn poll_inbound(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        Pin::new(&mut self.muxer).poll_inbound(cx)
    }

    fn poll_outbound(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        Pin::new(&mut self.muxer).poll_outbound(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.muxer).poll_close(cx)
    }

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
        Pin::new(&mut self.muxer).poll(cx)
    }
}

#[cfg(test)]
pub(super) struct AdmissionBehaviour<B> {
    inner: B,
    gate: TransportAdmissionGate,
}

#[cfg(test)]
impl<B> AdmissionBehaviour<B> {
    pub(super) fn new(inner: B, gate: TransportAdmissionGate) -> Self {
        Self { inner, gate }
    }
}

#[cfg(test)]
impl<B: libp2p::swarm::NetworkBehaviour> libp2p::swarm::NetworkBehaviour for AdmissionBehaviour<B> {
    type ConnectionHandler = B::ConnectionHandler;
    type ToSwarm = B::ToSwarm;

    fn handle_pending_inbound_connection(
        &mut self,
        id: ConnectionId,
        local: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        let inner = &mut self.inner;
        self.gate.handle_pending_inbound(id, local, remote, || {
            inner.handle_pending_inbound_connection(id, local, remote)
        })
    }

    fn handle_established_inbound_connection(
        &mut self,
        id: ConnectionId,
        peer: PeerId,
        local: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, ConnectionDenied> {
        self.inner
            .handle_established_inbound_connection(id, peer, local, remote)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        id: ConnectionId,
        peer: Option<PeerId>,
        addresses: &[Multiaddr],
        role: libp2p::core::Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let augmented = self
            .inner
            .handle_pending_outbound_connection(id, peer, addresses, role)?;
        self.gate
            .handle_pending_outbound(id, peer, addresses, augmented)
    }

    fn handle_established_outbound_connection(
        &mut self,
        id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role: libp2p::core::Endpoint,
        port_use: transport::PortUse,
    ) -> Result<libp2p::swarm::THandler<Self>, ConnectionDenied> {
        self.inner
            .handle_established_outbound_connection(id, peer, addr, role, port_use)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        self.gate.on_swarm_event(event);
        self.inner.on_swarm_event(event);
    }

    fn on_connection_handler_event(
        &mut self,
        peer: PeerId,
        id: ConnectionId,
        event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        self.inner.on_connection_handler_event(peer, id, event);
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>> {
        self.gate.clear_outbound_session();
        self.inner.poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libp2p_net::transport_paths::{
        DialedTransportPath, bootstrap_transport_path_state, take_retryable_transport_path,
    };
    use libp2p::swarm::{DialError, dial_opts::DialOpts};

    struct DropTrace {
        label: &'static str,
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    impl Drop for DropTrace {
        fn drop(&mut self) {
            self.order.lock().unwrap().push(self.label);
        }
    }

    #[test]
    fn cancelling_outbound_and_inbound_attempts_drops_future_before_permit() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let fut_trace = DropTrace {
            label: "outbound_future",
            order: Arc::clone(&order),
        };
        let permit_trace = DropTrace {
            label: "outbound_permit",
            order: Arc::clone(&order),
        };
        let pending_outbound: BoxedDialFuture = async move {
            let _hold = fut_trace;
            futures::future::pending().await
        }
        .boxed();
        let attempt = hold_outbound_permit(
            pending_outbound,
            TransportAdmission::unbounded(Box::new(permit_trace)),
        );
        drop(attempt);
        assert_eq!(
            order.lock().unwrap().as_slice(),
            &["outbound_future", "outbound_permit"]
        );

        order.lock().unwrap().clear();
        let in_fut_trace = DropTrace {
            label: "inbound_future",
            order: Arc::clone(&order),
        };
        let in_permit_trace = DropTrace {
            label: "inbound_permit",
            order: Arc::clone(&order),
        };
        let pending_inbound: BoxedDialFuture = async move {
            let _hold = in_fut_trace;
            futures::future::pending().await
        }
        .boxed();
        let mut upgrade = BudgetedListenerUpgrade::<BoxedDialFuture> {
            state: InboundUpgradeState::Running(pending_inbound),
            permit: Some(Box::new(in_permit_trace)),
            slot: None,
        };
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert!(Pin::new(&mut upgrade).poll(&mut cx).is_pending());
        drop(upgrade);
        assert_eq!(
            order.lock().unwrap().as_slice(),
            &["inbound_future", "inbound_permit"]
        );
    }

    #[test]
    fn inner_inbound_rejection_pops_listener_slot_without_shifting_next_connection() {
        let admitted_ids = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&admitted_ids);
        let gate = TransportAdmissionGate::inbound_only(move |id, _, _| {
            recorded.lock().unwrap().push(id);
            Ok(Box::new(()))
        });

        let slot_one = Arc::new(Mutex::new(None));
        let slot_two = Arc::new(Mutex::new(None));
        {
            let mut state = gate.0.lock().unwrap();
            state.pending_inbound.push_back(Arc::downgrade(&slot_one));
            state.pending_inbound.push_back(Arc::downgrade(&slot_two));
        }

        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();
        let first_id = DialOpts::unknown_peer_id()
            .address(addr.clone())
            .build()
            .connection_id();
        let second_id = DialOpts::unknown_peer_id()
            .address(addr.clone())
            .build()
            .connection_id();

        assert!(
            gate.handle_pending_inbound(first_id, &addr, &addr, || {
                Err(ConnectionDenied::new(io::Error::other("inner rejected")))
            })
            .is_err()
        );
        assert!(slot_one.lock().unwrap().is_none());
        drop(slot_one);

        gate.handle_pending_inbound(second_id, &addr, &addr, || Ok(()))
            .unwrap();
        assert!(slot_two.lock().unwrap().is_some());
        assert_eq!(admitted_ids.lock().unwrap().as_slice(), &[second_id]);
    }

    #[test]
    fn local_outbound_budget_denial_does_not_mark_tracked_path_retryable() {
        let peer = PeerId::random();
        let addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/4001/p2p/{peer}")
            .parse()
            .unwrap();
        let (known, _, _) = bootstrap_transport_path_state(std::slice::from_ref(&addr));
        let path = known[&peer][0].clone();
        let connection_id = DialOpts::peer_id(peer)
            .addresses(vec![addr.clone()])
            .build()
            .connection_id();
        let mut pending = HashMap::from([(
            peer,
            DialedTransportPath {
                connection_id,
                path,
            },
        )]);

        // Simulate `libp2p::core::transport::Boxed` wrapping `io::Error::other(OutboundAttemptBudgetDenied)`.
        let boxed_err = io::Error::other(io::Error::other(OutboundAttemptBudgetDenied));
        let dial_err = DialError::Transport(vec![(addr, TransportError::Other(boxed_err))]);

        assert!(
            take_retryable_transport_path(&mut pending, peer, connection_id, &dial_err).is_none()
        );
        assert!(!pending.contains_key(&peer));
    }

    #[test]
    fn explicit_opts_denial_and_conflicting_peer_addrs_never_pre_reserve_augmented_candidates() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct LivePermit(Arc<AtomicUsize>);
        impl Drop for LivePermit {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }

        let live_permits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&live_permits);
        let peer = PeerId::random();
        let other_peer = PeerId::random();
        let denied_raw: Multiaddr = "/ip4/127.0.0.1/tcp/4101".parse().unwrap();
        let denied_dial = denied_raw.clone().with_p2p(peer).unwrap();
        let aug_raw: Multiaddr = "/ip4/127.0.0.1/tcp/4102".parse().unwrap();
        let conflicting: Multiaddr = format!("/ip4/127.0.0.1/tcp/4103/p2p/{other_peer}")
            .parse()
            .unwrap();

        let denied_target = denied_dial.clone();
        let gate = TransportAdmissionGate::outbound_only(move |_, _, addr| {
            if addr == &denied_target {
                return Err(ConnectionDenied::new(io::Error::other("denied opts")));
            }
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(LivePermit(Arc::clone(&counter))))
        });

        // 1. Explicit `opts` denied while `augmented` has an admissible candidate:
        //    must return synchronous `Err(ConnectionDenied)` without reserving `aug_raw`.
        let id_denied = DialOpts::peer_id(peer)
            .addresses(vec![denied_raw.clone()])
            .build()
            .connection_id();
        assert!(
            gate.handle_pending_outbound(
                id_denied,
                Some(peer),
                &[denied_raw],
                vec![aug_raw.clone()]
            )
            .is_err()
        );
        assert_eq!(live_permits.load(Ordering::SeqCst), 0);
        assert!(gate.0.lock().unwrap().outbound_session.is_none());

        // 2. Explicit `opts` has only a conflicting `/p2p/<other>` multiaddr while `augmented`
        //    has an admissible candidate: must NOT pre-reserve `aug_raw`.
        let id_conflict = DialOpts::peer_id(peer)
            .addresses(vec![conflicting.clone()])
            .build()
            .connection_id();
        let passthrough = gate
            .handle_pending_outbound(
                id_conflict,
                Some(peer),
                &[conflicting],
                vec![aug_raw.clone()],
            )
            .unwrap();
        assert_eq!(passthrough, vec![aug_raw.clone()]);
        assert_eq!(live_permits.load(Ordering::SeqCst), 0);

        // 3. Unknown-peer dial (`peer = None`) never reserves or forwards `augmented`.
        let unknown_addr: Multiaddr = "/ip4/127.0.0.1/tcp/4104".parse().unwrap();
        let id_unknown = DialOpts::unknown_peer_id()
            .address(unknown_addr.clone())
            .build()
            .connection_id();
        let unknown_aug = gate
            .handle_pending_outbound(id_unknown, None, &[unknown_addr], vec![aug_raw])
            .unwrap();
        assert!(unknown_aug.is_empty());
        assert_eq!(live_permits.load(Ordering::SeqCst), 1);
        gate.clear_outbound_session();
        assert_eq!(live_permits.load(Ordering::SeqCst), 0);
    }
}
