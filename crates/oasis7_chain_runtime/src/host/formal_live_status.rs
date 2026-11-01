//! Parent-private formal HTTP service implementation.
use super::*;
impl FormalLiveService {
    pub(super) fn handle_health(&self) -> HttpReply {
        let status = self.runtime_status();
        signed_reply(
            200,
            SERVICE_READINESS_PATH,
            &(),
            &FormalHealthV1 {
                api_version: "oasis7.formal-live-health/v1".into(),
                ready: status.ready,
                profile: "formal_live_v1".into(),
            },
            &self.signer,
        )
    }

    pub(super) fn handle_status(&self) -> HttpReply {
        signed_reply(
            200,
            CHAIN_STATUS_PATH,
            &(),
            &self.runtime_status(),
            &self.signer,
        )
    }

    pub(super) fn runtime_status(&self) -> FormalLiveStatusV1 {
        let stopping = self.state.stopping.load(Ordering::Acquire);
        let (snapshot, runtime_error) = if stopping {
            (None, None)
        } else {
            match self.runtime.lock() {
                Ok(runtime) => (Some(runtime.snapshot()), None),
                Err(_) => (None, Some("formal Node runtime lock poisoned".to_owned())),
            }
        };
        if let Some(snapshot) = &snapshot {
            self.state
                .node_running
                .store(snapshot.running, Ordering::Release);
            self.state
                .tick_count
                .store(snapshot.tick_count, Ordering::Release);
        }
        let cut = self
            .published
            .observe(|cut| {
                Ok((
                    cut.anchor.head().head().record_id.clone(),
                    cut.anchor.head().record().position,
                ))
            })
            .ok();
        let state_error = self
            .state
            .last_error
            .lock()
            .ok()
            .and_then(|value| value.clone());
        let node_running = snapshot.as_ref().map_or_else(
            || self.state.node_running.load(Ordering::Acquire),
            |snapshot| snapshot.running,
        );
        let node_error = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.last_error.clone())
            .map(|error| error.chars().take(MAX_NODE_ERROR_CHARS).collect());
        let last_error = state_error.or(node_error).or(runtime_error);
        let ready = self.state.ready.load(Ordering::Acquire)
            && !stopping
            && node_running
            && cut.is_some()
            && last_error.is_none();
        let (record, position) = cut.unzip();
        FormalLiveStatusV1 {
            api_version: "oasis7.formal-live-status/v1".into(),
            runtime_profile: "formal_live_v1".into(),
            state: if stopping {
                "stopping"
            } else if last_error.is_some() {
                "degraded"
            } else if ready {
                "ready"
            } else if node_running {
                "starting"
            } else {
                "stopped"
            }
            .into(),
            ready,
            can_propose: ready,
            node_id: self.node_id.clone(),
            world: self.world.clone(),
            node_running,
            tick_count: snapshot.as_ref().map_or_else(
                || self.state.tick_count.load(Ordering::Acquire),
                |snapshot| snapshot.tick_count,
            ),
            committed_position: position,
            committed_record_id: record,
            last_error,
            effect_delivery: self
                .state
                .effect_delivery
                .lock()
                .ok()
                .and_then(|state| state.clone()),
        }
    }
}
