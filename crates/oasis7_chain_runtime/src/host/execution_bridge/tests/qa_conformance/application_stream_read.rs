//! One absolute read window retains both partial frames and ordered exchange progress.
use super::*;

impl Session {
    pub(super) fn read_window(
        &mut self,
        budget: Duration,
        require_hello: bool,
        stop_on_ordered_completion: bool,
    ) -> (bool, usize) {
        let deadline = Instant::now() + budget;
        let read_started = Instant::now();
        let mut snapshot_frames = 0;
        if !self.ordered_response.started {
            self.ordered_response.started = true;
            self.ordered_response.hello_seen = !require_hello;
        }
        if stop_on_ordered_completion && self.ordered_response.complete {
            return (true, snapshot_frames);
        }
        while Instant::now() < deadline {
            match self.read_frame_until(deadline) {
                Ok(false) => return (false, snapshot_frames),
                Ok(true) => {
                    if let Ok(value) =
                        serde_json::from_slice::<serde_json::Value>(&self.partial_line)
                    {
                        if self.diagnostic {
                            println!(
                                "stream_observer_frame connection={} type={} bytes={} elapsed_us={} read_elapsed_us={}",
                                self.connection,
                                value["type"],
                                self.partial_line.len(),
                                self.created.elapsed().as_micros(),
                                read_started.elapsed().as_micros()
                            );
                        }
                        if value["type"] == "hello_ack" {
                            self.ordered_response.hello_seen = true;
                        }
                        if self.ordered_response.hello_seen
                            && value["type"] == "snapshot"
                            && value["snapshot"]["runtime_snapshot"].is_object()
                        {
                            println!("actual_stream_snapshot_bytes={}", self.partial_line.len());
                            self.ordered_response.snapshot_seen = true;
                            snapshot_frames += 1;
                        }
                        if self.ordered_response.snapshot_seen
                            && value["type"] == "authoritative_recovery_ack"
                        {
                            self.ordered_response.complete = true;
                            if stop_on_ordered_completion {
                                self.partial_line.clear();
                                return (true, snapshot_frames);
                            }
                        }
                    } else if self.diagnostic {
                        println!(
                            "stream_observer_invalid_frame connection={} bytes={} elapsed_us={}",
                            self.connection,
                            self.partial_line.len(),
                            self.created.elapsed().as_micros()
                        );
                    }
                    self.partial_line.clear();
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    if self.diagnostic {
                        println!(
                            "stream_observer_timeout connection={} partial_bytes={} partial_type={} elapsed_us={} read_elapsed_us={}",
                            self.connection,
                            self.partial_line.len(),
                            if self.partial_line.is_empty() {
                                "empty"
                            } else {
                                "incomplete_frame"
                            },
                            self.created.elapsed().as_micros(),
                            read_started.elapsed().as_micros()
                        );
                    }
                }
                Err(_) => return (false, snapshot_frames),
            }
        }
        (self.ordered_response.complete, snapshot_frames)
    }

    // read_line can remain inside one fragmented frame while each socket read
    // succeeds. Check the same deadline between actual buffer fills instead.
    // Retain raw bytes so a UTF-8 character may span multiple read windows.
    pub(super) fn read_frame_until(&mut self, deadline: Instant) -> std::io::Result<bool> {
        while Instant::now() < deadline {
            self.reader.get_mut().set_read_timeout(Some(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(50))
                    .max(Duration::from_millis(1)),
            ))?;
            let bytes = match self.reader.fill_buf() {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if bytes.is_empty() {
                return Ok(false);
            }
            let end = bytes.iter().position(|byte| *byte == b'\n');
            let consumed = end.map_or(bytes.len(), |index| index + 1);
            self.partial_line.extend_from_slice(&bytes[..consumed]);
            self.reader.consume(consumed);
            if end.is_some() {
                std::str::from_utf8(&self.partial_line).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "stream frame is not UTF-8",
                    )
                })?;
                return Ok(true);
            }
        }
        Err(std::io::ErrorKind::TimedOut.into())
    }
}
