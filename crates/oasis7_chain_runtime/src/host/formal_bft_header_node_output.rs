//! Fallible output for public Node reports, after business execution has finished.
use std::io::Write;

pub(super) fn write_line(output: &mut impl Write, encoded: &str) -> Result<(), String> {
    output
        .write_all(encoded.as_bytes())
        .and_then(|()| output.write_all(b"\n"))
        .and_then(|()| output.flush())
        .map_err(|_| "public header Node report delivery failed; actions may already be committed; do not resubmit".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, ErrorKind};

    struct Sink {
        bytes: Vec<u8>,
        fail_after: Option<usize>,
        fail_flush: bool,
        flushes: usize,
    }

    impl Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_after == Some(self.bytes.len()) {
                return Err(io::Error::new(ErrorKind::BrokenPipe, "private sink detail"));
            }
            // Exercise short writes, including the separate newline write.
            let count = bytes.len().min(1);
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_flush {
                Err(io::Error::other("private flush detail"))
            } else {
                Ok(())
            }
        }
    }

    fn sink(fail_after: Option<usize>, fail_flush: bool) -> Sink {
        Sink {
            bytes: Vec::new(),
            fail_after,
            fail_flush,
            flushes: 0,
        }
    }

    #[test]
    fn short_writes_preserve_json_line_and_flush_once() {
        let mut sink = sink(None, false);
        write_line(&mut sink, "{\"receipts\":[]}").unwrap();
        assert_eq!(sink.bytes, b"{\"receipts\":[]}\n");
        assert_eq!(sink.flushes, 1);
    }

    #[test]
    fn broken_pipe_before_during_and_after_json_is_returned_without_retry() {
        for offset in [0, 1, 2] {
            let mut sink = sink(Some(offset), false);
            let error = write_line(&mut sink, "{}").unwrap_err();
            assert_eq!(sink.bytes, b"{}"[..offset]);
            assert_eq!(sink.flushes, 0);
            assert!(error.contains("do not resubmit"));
            assert!(!error.contains("private"));
        }
    }

    #[test]
    fn flush_failure_is_not_success_or_a_second_write() {
        let mut sink = sink(None, true);
        let error = write_line(&mut sink, "{}").unwrap_err();
        assert_eq!(sink.bytes, b"{}\n");
        assert_eq!(sink.flushes, 1);
        assert!(!error.contains("private"));
    }

    #[cfg(unix)]
    #[test]
    fn actual_closed_stream_is_a_returned_error_not_a_panic() {
        use std::os::unix::net::UnixStream;
        let (mut output, reader) = UnixStream::pair().unwrap();
        drop(reader);
        assert!(write_line(&mut output, "{}").is_err());
    }
}
