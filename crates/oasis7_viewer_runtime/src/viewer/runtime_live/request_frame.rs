//! Bounded JSON-line transport state, independent of request parsing/authority.
use std::io::{self, BufRead};

// Match tungstenite's default message limit used by the WebSocket bridge.
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;

pub(super) struct RequestFrame {
    pending: Vec<u8>,
    limit: usize,
}
impl Default for RequestFrame {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            limit: MAX_REQUEST_BYTES,
        }
    }
}
impl RequestFrame {
    #[cfg(test)]
    pub(super) fn buffered_bytes(&self) -> usize {
        self.pending.len()
    }

    /// A timeout returns to periodic work without discarding any consumed bytes.
    /// EOF is clean only between complete frames; decoding occurs after LF.
    pub(super) fn read(&mut self, reader: &mut impl BufRead) -> io::Result<Option<String>> {
        loop {
            let available = match reader.fill_buf() {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if available.is_empty() {
                return if self.pending.is_empty() {
                    Ok(None)
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "truncated Viewer request frame",
                    ))
                };
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let payload = newline.unwrap_or(available.len());
            let length = self
                .pending
                .len()
                .checked_add(payload)
                .filter(|length| *length <= self.limit)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Viewer request frame exceeds 64 MiB",
                    )
                })?;
            // Bound the allocation as well as the payload, before append.
            if length > self.pending.capacity() {
                let capacity = length
                    .max(self.pending.capacity().saturating_mul(2))
                    .min(self.limit);
                self.pending
                    .try_reserve_exact(capacity - self.pending.len())
                    .map_err(|_| io::Error::other("Viewer request frame allocation failed"))?;
            }
            self.pending.extend_from_slice(&available[..payload]);
            reader.consume(payload + usize::from(newline.is_some()));
            if newline.is_some() {
                return String::from_utf8(std::mem::take(&mut self.pending))
                    .map(Some)
                    .map_err(|_| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid UTF-8 Viewer request frame",
                        )
                    });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::{BufReader, Read};
    struct Script(VecDeque<Result<Vec<u8>, io::ErrorKind>>);
    impl Read for Script {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            match self.0.pop_front() {
                None => Ok(0),
                Some(Err(kind)) => Err(io::Error::from(kind)),
                Some(Ok(bytes)) => {
                    let count = output.len().min(bytes.len());
                    output[..count].copy_from_slice(&bytes[..count]);
                    if count < bytes.len() {
                        self.0.push_front(Ok(bytes[count..].to_vec()));
                    }
                    Ok(count)
                }
            }
        }
    }
    #[test]
    fn timed_out_and_would_block_utf8_prefix_survives_until_complete_frame() {
        for kind in [io::ErrorKind::TimedOut, io::ErrorKind::WouldBlock] {
            let script = Script(VecDeque::from([
                Ok(vec![b'a', 0xc3]),
                Err(kind),
                Ok(vec![0xa9, b'\n', b'b', b'\n']),
            ]));
            let mut reader = BufReader::new(script);
            let mut frame = RequestFrame::default();
            assert_eq!(frame.read(&mut reader).unwrap_err().kind(), kind);
            assert_eq!(frame.buffered_bytes(), 2);
            assert_eq!(frame.read(&mut reader).unwrap().as_deref(), Some("aé"));
            assert_eq!(frame.read(&mut reader).unwrap().as_deref(), Some("b"));
            assert!(frame.read(&mut reader).unwrap().is_none());
        }
    }
    #[test]
    fn exact_payload_bound_allows_lf_and_overflow_is_rejected_before_append() {
        let mut frame = RequestFrame {
            pending: Vec::new(),
            limit: 4,
        };
        assert_eq!(
            frame.read(&mut &b"abcd\n"[..]).unwrap().as_deref(),
            Some("abcd")
        );
        assert_eq!(
            frame.read(&mut &b"abcde\n"[..]).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(frame.pending.is_empty());
        assert!(frame.pending.capacity() <= frame.limit);
    }
    #[test]
    fn eof_does_not_execute_unterminated_frame_and_invalid_utf8_fails_closed() {
        let mut frame = RequestFrame::default();
        assert_eq!(
            frame.read(&mut &b"partial"[..]).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        let mut frame = RequestFrame::default();
        assert_eq!(
            frame.read(&mut &b"\xff\n"[..]).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(
            RequestFrame::default()
                .read(&mut &b""[..])
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn interrupted_read_retries_without_losing_the_frame() {
        let script = Script(VecDeque::from([
            Ok(b"a".to_vec()),
            Err(io::ErrorKind::Interrupted),
            Ok(b"b\n".to_vec()),
        ]));
        assert_eq!(
            RequestFrame::default()
                .read(&mut BufReader::new(script))
                .unwrap()
                .as_deref(),
            Some("ab")
        );
    }
}
