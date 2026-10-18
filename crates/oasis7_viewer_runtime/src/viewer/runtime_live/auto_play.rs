use super::*;

/// Keep connection-driven playback scheduling and its low-noise emissions in a
/// dedicated slice. The parent server owns the world and session state; this
/// module only groups the scheduling behavior used by request handling.
impl ViewerRuntimeLiveServer {
    pub(super) fn enable_auto_play_for_session_if_available(
        &mut self,
        session: &mut RuntimeLiveSession,
    ) {
        // The configuration chooses initial playback. Explicit Play/Pause
        // controls the shared server thereafter, including new connections.
        session.playing = !self.auto_play_paused;
        session.next_play_step_at = None;
        session.transient_play_failures = 0;
    }

    pub(super) fn pause_auto_play(&mut self, session: &mut RuntimeLiveSession) {
        self.auto_play_paused = true;
        session.playing = false;
        session.next_play_step_at = None;
    }

    pub(super) fn resume_auto_play(&mut self, session: &mut RuntimeLiveSession) {
        self.auto_play_paused = false;
        session.playing = true;
        session.next_play_step_at = None;
        session.transient_play_failures = 0;
        self.next_auto_play_step_at = None;
    }

    pub(super) fn should_advance_auto_play_step(&mut self) -> bool {
        if self.auto_play_paused {
            self.next_auto_play_step_at = None;
            return false;
        }
        let now = Instant::now();
        if let Some(next_step_at) = self.next_auto_play_step_at
            && now < next_step_at
        {
            return false;
        }
        self.next_auto_play_step_at = Some(now + self.config.play_step_interval);
        true
    }

    pub(super) fn defer_next_auto_play_step_after_completion(&mut self, interval: Duration) {
        if !self.auto_play_paused {
            self.next_auto_play_step_at = Some(Instant::now() + interval);
        }
    }

    pub(super) fn emit_background_play_snapshot(
        &mut self,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        if session.explicitly_subscribed_to(ViewerStream::Snapshot)
            && should_emit_runtime_advance_snapshot(session, "play", false)
        {
            let snapshot = self.compat_snapshot_for_session(session);
            send_response(writer, &ViewerResponse::Snapshot { snapshot })?;
        }
        session.metrics = runtime_metrics(&self.world);
        if session.explicitly_subscribed_to(ViewerStream::Metrics) {
            send_response(
                writer,
                &ViewerResponse::Metrics {
                    time: Some(self.world.state().time),
                    metrics: session.metrics.clone(),
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn drive_auto_play(
        &mut self,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        if self.config.world_service.is_some() {
            // Periodic authenticated synchronization owns projection progress.
            // Playback emits that projection and never runs local actions.
            return self.emit_background_play_snapshot(session, writer);
        }
        if self.auto_play_paused || !session.initial_snapshot_sent {
            return Ok(());
        }
        session.playing = true;
        if self.should_advance_auto_play_step() {
            let play_step_interval = self.config.play_step_interval;
            self.advance_runtime(session, writer, "play", 1, None, false)?;
            self.defer_next_auto_play_step_after_completion(play_step_interval);
        } else {
            self.emit_background_play_snapshot(session, writer)?;
        }
        Ok(())
    }
}
