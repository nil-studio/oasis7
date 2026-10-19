//! Blocking stream session parsing and dispatch.
use super::*;

impl ViewerRuntimeLiveServer {
    pub(super) fn serve_stream(
        &mut self,
        stream: TcpStream,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_millis(50)))?;

        let reader_stream = stream.try_clone()?;
        let mut reader = BufReader::new(reader_stream);
        let mut socket = stream;
        let limits = response_outbox::ResponseLimits::from_config(&self.config);
        let mut session = RuntimeLiveSession::new_with_playing(false);

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => return Ok(()),
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        let mut output = response_outbox::ResponseOutbox::new(limits);
                        let mut handled = Ok(());
                        match agency_control::parse_agency_control_frame(trimmed) {
                            agency_control::ParsedAgencyControlFrame::Request(request) => {
                                let response = self.handle_agency_control_request(request);
                                agency_control::write_agency_control_response(
                                    &mut output,
                                    &response,
                                )?;
                            }
                            agency_control::ParsedAgencyControlFrame::Invalid(response) => {
                                agency_control::write_agency_control_response(
                                    &mut output,
                                    &response,
                                )?;
                            }
                            agency_control::ParsedAgencyControlFrame::NotAgencyControl => {
                                if let Ok(request) = serde_json::from_str::<ViewerRequest>(trimmed)
                                {
                                    handled =
                                        self.handle_request(request, &mut session, &mut output);
                                }
                            }
                        }
                        output.deliver(&mut socket)?;
                        handled?;
                    }
                }
                Err(err) if is_timeout_error(&err) => {}
                Err(err) if is_expected_disconnect_error(&err) => return Ok(()),
                Err(err) => return Err(ViewerRuntimeLiveServerError::Io(err)),
            }

            let mut output = response_outbox::ResponseOutbox::new(limits);
            if self.authoritative_recovery_write_fence.is_none()
                && self.chain_link_enabled()
                && session.initial_snapshot_sent
                && session.should_poll_chain(self.config.chain_poll_interval)
                && let Err(err) = self.sync_chain_linked_runtime(&mut session, &mut output)
            {
                emit_stderr_or_event(
                    Level::WARN,
                    format!("viewer runtime live: chain sync skipped: {err:?}").as_str(),
                    "viewer runtime live chain sync skipped",
                );
            }

            let driven = if self.authoritative_recovery_write_fence.is_none() {
                self.drive_auto_play(&mut session, &mut output)
            } else {
                Ok(())
            };
            output.deliver(&mut socket)?;
            driven?;
        }
    }
}
