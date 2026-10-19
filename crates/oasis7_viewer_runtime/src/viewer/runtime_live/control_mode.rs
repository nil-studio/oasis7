use super::*;

impl ViewerRuntimeLiveServer {
    pub(super) fn apply_control_mode(
        &mut self,
        mode: ViewerControl,
        request_id: Option<u64>,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        if let Err(reason) = self.ensure_gameplay_ready_for_control(&mode) {
            return self.block_gameplay_control(
                session,
                writer,
                control_mode_label(&mode),
                "gameplay control rejected before world advance",
                reason,
                request_id,
                0,
                0,
                false,
            );
        }
        match mode {
            ViewerControl::Pause => {
                self.pause_auto_play(session);
                session.next_play_step_at = None;
                session.transient_play_failures = 0;
            }
            ViewerControl::Play => {
                self.resume_auto_play(session);
            }
            ViewerControl::Step { count } => {
                self.pause_auto_play(session);
                session.next_play_step_at = None;
                session.transient_play_failures = 0;
                self.advance_runtime(session, writer, "step", count.max(1), request_id, true)?;
            }
            ViewerControl::Seek { tick } => {
                self.pause_auto_play(session);
                session.next_play_step_at = None;
                session.transient_play_failures = 0;
                emit_stderr_or_event(
                    Level::INFO,
                    format!(
                        "viewer runtime live: ignore seek control in live mode (target_tick={tick})"
                    )
                    .as_str(),
                    "viewer runtime live ignored seek control in live mode",
                );
            }
        }
        Ok(())
    }
}
