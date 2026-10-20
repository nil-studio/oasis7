use super::*;
use std::io;

#[derive(Clone, Copy)]
pub(super) struct ResponseLimits {
    frame_bytes: usize,
    turn_bytes: usize,
    frames: usize,
    deadline: Duration,
}

impl ResponseLimits {
    pub(super) fn from_config(config: &ViewerRuntimeLiveServerConfig) -> Self {
        Self {
            frame_bytes: config.response_frame_max_bytes,
            turn_bytes: config.response_turn_max_bytes,
            frames: config.response_turn_max_frames,
            deadline: config.response_write_timeout,
        }
    }
}

/// `flush` commits a complete protocol frame; it never accesses a socket.
pub(super) struct ResponseOutbox {
    limits: ResponseLimits,
    bytes: Vec<u8>,
    complete: usize,
    frames: usize,
    failed: bool,
}

impl ResponseOutbox {
    pub(super) fn new(limits: ResponseLimits) -> Self {
        Self {
            limits,
            bytes: Vec::new(),
            complete: 0,
            frames: 0,
            failed: false,
        }
    }

    fn overflow(&mut self) -> io::Error {
        self.bytes.truncate(self.complete);
        self.failed = true;
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Viewer response output limit exceeded",
        )
    }

    /// Called only after releasing the Viewer guard. One deadline covers all writes.
    pub(super) fn deliver(&mut self, socket: &mut TcpStream) -> io::Result<()> {
        let until = Instant::now()
            .checked_add(self.limits.deadline)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Viewer output deadline overflow",
                )
            })?;
        let mut offset = 0;
        while offset < self.complete {
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Viewer response output deadline exceeded",
                ));
            }
            socket.set_write_timeout(Some(remaining))?;
            match socket.write(&self.bytes[offset..self.complete]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "Viewer response socket closed",
                    ));
                }
                Ok(written) => offset += written,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        if self.complete > 0 && Instant::now() > until {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Viewer response output deadline exceeded",
            ));
        }
        if self.failed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Viewer response output limit exceeded",
            ));
        }
        Ok(())
    }
}

impl Write for ResponseOutbox {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(self.overflow());
        }
        let Some(total) = self.bytes.len().checked_add(bytes.len()) else {
            return Err(self.overflow());
        };
        if total > self.limits.turn_bytes || total - self.complete > self.limits.frame_bytes {
            return Err(self.overflow());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.failed || self.frames >= self.limits.frames {
            return Err(self.overflow());
        }
        self.frames += 1;
        self.complete = self.bytes.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn outbox(frame_bytes: usize, turn_bytes: usize, frames: usize) -> ResponseOutbox {
        ResponseOutbox::new(ResponseLimits {
            frame_bytes,
            turn_bytes,
            frames,
            deadline: Duration::from_secs(1),
        })
    }
    #[test]
    fn overflow_retains_complete_frames_and_discards_only_partial_frame() {
        let mut output = outbox(8, 16, 2);
        output.write_all(b"one\n").unwrap();
        output.flush().unwrap();
        output.write_all(b"partial").unwrap();
        assert!(output.write_all(b"too big").is_err());
        assert_eq!(output.bytes, b"one\n");
        assert_eq!(output.complete, 4);
    }
    #[test]
    fn turn_and_frame_count_limits_preserve_prior_complete_frames() {
        let mut turn = outbox(8, 8, 4);
        turn.write_all(b"first\n").unwrap();
        turn.flush().unwrap();
        assert!(turn.write_all(b"next\n").is_err());
        assert_eq!(turn.bytes, b"first\n");
        let mut count = outbox(8, 32, 1);
        count.write_all(b"first\n").unwrap();
        count.flush().unwrap();
        count.write_all(b"next\n").unwrap();
        assert!(count.flush().is_err());
        assert_eq!(count.bytes, b"first\n");
    }

    #[test]
    fn browser_initial_snapshot_replays_full_retained_history_with_default_limits() {
        for config in [
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal),
            ViewerRuntimeLiveServerConfig::formal_release_default(),
        ] {
            let limits = ResponseLimits::from_config(&config);
            let mut server = ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(
                WorldScenario::Minimal,
            ))
            .unwrap();
            for _ in 0..MAX_AUTHORITATIVE_BATCH_HISTORY {
                server.register_authoritative_batch(&[]).unwrap();
            }
            let batch_id = server
                .authoritative_batches
                .back()
                .unwrap()
                .batch_id
                .clone();
            for index in 0..MAX_AUTHORITATIVE_CHALLENGE_HISTORY {
                server
                    .authoritative_challenges
                    .push_back(RuntimeAuthoritativeChallengeRecord {
                        challenge_id: format!("challenge-{index}"),
                        batch_id: batch_id.clone(),
                        watcher_id: "watcher".into(),
                        recomputed_state_root: "0".repeat(64),
                        recomputed_data_root: "0".repeat(64),
                        status: AuthoritativeChallengeStatus::Challenged,
                        submitted_at_tick: 0,
                        resolved_at_tick: None,
                        slash_applied: false,
                        slash_reason: None,
                    });
            }
            let mut session = RuntimeLiveSession::new_with_playing(false);
            server
                .handle_request(
                    ViewerRequest::Subscribe {
                        streams: vec![
                            ViewerStream::Snapshot,
                            ViewerStream::Events,
                            ViewerStream::Metrics,
                        ],
                        event_kinds: vec![],
                    },
                    &mut session,
                    &mut Vec::new(),
                )
                .unwrap();
            let mut output = ResponseOutbox::new(limits);
            server.handle_request(ViewerRequest::RequestSnapshot, &mut session, &mut output)
                .expect("a browser subscription must receive all retained history without disconnecting");
            assert!(!output.failed);
            let responses: Vec<ViewerResponse> = output
                .bytes
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| serde_json::from_slice(line).unwrap())
                .collect();
            assert_eq!(
                responses
                    .iter()
                    .filter(|response| matches!(
                        response,
                        ViewerResponse::AuthoritativeBatch { .. }
                    ))
                    .count(),
                MAX_AUTHORITATIVE_BATCH_HISTORY
            );
            assert_eq!(
                responses
                    .iter()
                    .filter(|response| matches!(
                        response,
                        ViewerResponse::AuthoritativeChallengeAck { .. }
                    ))
                    .count(),
                MAX_AUTHORITATIVE_CHALLENGE_HISTORY
            );
            assert_eq!(
                responses.len(),
                MAX_AUTHORITATIVE_BATCH_HISTORY + MAX_AUTHORITATIVE_CHALLENGE_HISTORY + 3
            );
            assert!(session.initial_snapshot_sent);
        }
    }
}
