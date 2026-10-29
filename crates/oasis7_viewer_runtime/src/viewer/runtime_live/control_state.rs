//! Viewer-local scheduling observation, separate from canonical world effects.
use super::*;
use crate::viewer::protocol::{
    LIVE_CONTROL_STATE_CAPABILITY, LIVE_CONTROL_STATE_MAX_REVISION, LIVE_CONTROL_STATE_SCOPE,
    LiveControlState,
};

pub(super) struct ControlStatePublisher {
    epoch: String,
    revision: u64,
    paused: bool,
    invalidated: bool,
}
impl ControlStatePublisher {
    pub(super) fn new(paused: bool) -> Result<Self, ViewerRuntimeLiveServerError> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| {
            ViewerRuntimeLiveServerError::Init(
                "Viewer control observation entropy unavailable".into(),
            )
        })?;
        Ok(Self {
            epoch: hex::encode(bytes),
            revision: 1,
            paused,
            invalidated: false,
        })
    }
    pub(super) fn observe(&mut self, paused: bool) -> Result<(), ViewerRuntimeLiveServerError> {
        if self.invalidated {
            return Err(ViewerRuntimeLiveServerError::Init(
                "Viewer control observation invalidated".into(),
            ));
        }
        if self.paused != paused {
            let revision = self
                .revision
                .checked_add(1)
                .filter(|revision| *revision <= LIVE_CONTROL_STATE_MAX_REVISION)
                .ok_or_else(|| {
                    self.invalidated = true;
                    ViewerRuntimeLiveServerError::Init(
                        "Viewer control observation revision exhausted".into(),
                    )
                })?;
            self.revision = revision;
            self.paused = paused;
        }
        Ok(())
    }
}
impl ViewerRuntimeLiveServer {
    pub(super) fn publish_live_control_state(
        &mut self,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
        request_id: Option<u64>,
        force: bool,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        if session.negotiated_protocol.version < 2
            || !session
                .negotiated_protocol
                .capabilities
                .iter()
                .any(|capability| capability == LIVE_CONTROL_STATE_CAPABILITY)
        {
            return Ok(());
        }
        self.control_state.observe(self.auto_play_paused)?;
        if !force && session.last_live_control_revision == Some(self.control_state.revision) {
            return Ok(());
        }
        let state = LiveControlState {
            scope: LIVE_CONTROL_STATE_SCOPE.into(),
            epoch: self.control_state.epoch.clone(),
            revision: self.control_state.revision,
            paused: self.auto_play_paused,
            request_id,
        };
        state
            .validate()
            .map_err(ViewerRuntimeLiveServerError::Init)?;
        send_response(writer, &ViewerResponse::LiveControlState { state })?;
        session.last_live_control_revision = Some(self.control_state.revision);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhausted_control_revision_cannot_change_observed_policy() {
        let mut publisher = ControlStatePublisher::new(true).unwrap();
        publisher.revision = LIVE_CONTROL_STATE_MAX_REVISION;
        assert!(publisher.observe(false).is_err());
        assert!(publisher.paused);
        assert_eq!(publisher.revision, LIVE_CONTROL_STATE_MAX_REVISION);
        assert!(publisher.observe(true).is_err());
        assert!(publisher.invalidated);
    }

    #[test]
    fn exhausted_control_revision_blocked_paths_pause_and_invalidate_connection() {
        for runtime_failure in [false, true] {
            let mut server = ViewerRuntimeLiveServer::new(
                ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                    .with_auto_play_on_connect(true),
            )
            .unwrap();
            let mut session = RuntimeLiveSession::new();
            session.negotiated_protocol.version = 2;
            session.negotiated_protocol.capabilities =
                vec![LIVE_CONTROL_STATE_CAPABILITY.to_string()];
            server.control_state.revision = LIVE_CONTROL_STATE_MAX_REVISION;
            let mut initial = Vec::new();
            server
                .publish_live_control_state(&mut session, &mut initial, None, true)
                .unwrap();
            assert!(!server.auto_play_paused);
            let before_time = server.world.state().time;
            let before_events = server.world.journal().len();
            let mut output = Vec::new();
            let result = if runtime_failure {
                server.block_runtime_control(
                    &mut session,
                    &mut output,
                    "play",
                    "play blocked",
                    ViewerRuntimeLiveServerError::Init("test runtime failure".into()),
                    Some(99),
                    0,
                    0,
                    false,
                )
            } else {
                server.block_gameplay_control(
                    &mut session,
                    &mut output,
                    "play",
                    "play blocked",
                    "test unavailable provider".into(),
                    Some(99),
                    0,
                    0,
                    false,
                )
            };
            assert!(
                server.auto_play_paused,
                "safety pause must always stop new decisions"
            );
            assert!(
                result.is_err(),
                "blocked path must terminate the connection if its confirmed enabled state cannot be superseded; runtime_failure={runtime_failure}"
            );
            assert_eq!(server.world.state().time, before_time);
            assert_eq!(server.world.journal().len(), before_events);
            assert_eq!(
                server.control_state.revision,
                LIVE_CONTROL_STATE_MAX_REVISION
            );
            assert!(
                server
                    .publish_live_control_state(&mut session, &mut output, None, false)
                    .is_err()
            );
        }
    }

    #[test]
    fn exhausted_control_observation_keeps_legacy_pause_compatible() {
        let mut server = ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_auto_play_on_connect(true),
        )
        .unwrap();
        server.control_state.revision = LIVE_CONTROL_STATE_MAX_REVISION;
        let mut legacy = RuntimeLiveSession::new();
        server.pause_auto_play(&mut legacy).unwrap();
        assert!(server.auto_play_paused);
        let mut bytes = Vec::new();
        server
            .publish_live_control_state(&mut legacy, &mut bytes, None, true)
            .unwrap();
        assert!(bytes.is_empty());
        let mut negotiated = RuntimeLiveSession::new();
        negotiated.negotiated_protocol.version = 2;
        negotiated.negotiated_protocol.capabilities = vec![LIVE_CONTROL_STATE_CAPABILITY.into()];
        assert!(server.resume_auto_play(&mut negotiated).is_err());
        assert!(
            server.auto_play_paused,
            "failed observed Resume must not enable new decisions"
        );
        assert!(
            server
                .publish_live_control_state(&mut negotiated, &mut bytes, None, true)
                .is_err()
        );
        assert!(bytes.is_empty());
    }
}
