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
}
