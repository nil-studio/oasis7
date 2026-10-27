//! Bounded framing for private formal-owner IPC.
use super::MAX_CONTROL_BYTES;
#[cfg(test)]
use super::configure_control_stream;
use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

pub(super) fn write_message<T: Serialize>(
    stream: &mut impl Write,
    value: &T,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_CONTROL_BYTES {
        return Err("formal runtime owner control message size bound".into());
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .map_err(|error| error.to_string())?;
    stream.write_all(&bytes).map_err(|error| error.to_string())
}

pub(super) fn read_message<T: DeserializeOwned>(stream: &mut impl Read) -> Result<T, String> {
    let mut len = [0u8; 4];
    stream
        .read_exact(&mut len)
        .map_err(|error| error.to_string())?;
    let len = u32::from_be_bytes(len) as usize;
    if len == 0 || len > MAX_CONTROL_BYTES {
        return Err("formal runtime owner control message size bound".into());
    }
    let mut bytes = vec![0u8; len];
    stream
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

/// All framing operations share one caller deadline, including short reads,
/// interrupted syscalls and the transition from the length header to its body.
pub(super) struct DeadlineStream<'a> {
    stream: &'a mut UnixStream,
    deadline: Instant,
}

pub(super) fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "formal runtime owner control deadline exhausted",
            )
        })
}

impl<'a> DeadlineStream<'a> {
    pub(super) fn new(stream: &'a mut UnixStream, deadline: Instant) -> Self {
        Self { stream, deadline }
    }
}

impl Read for DeadlineStream<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(remaining(self.deadline)?))?;
        let count = self.stream.read(buffer)?;
        remaining(self.deadline)?;
        Ok(count)
    }
}

impl Write for DeadlineStream<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(remaining(self.deadline)?))?;
        let count = self.stream.write(buffer)?;
        remaining(self.deadline)?;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        remaining(self.deadline)?;
        self.stream.flush()?;
        remaining(self.deadline)?;
        Ok(())
    }
}

pub(super) fn bounded_reason(error: &str) -> String {
    error
        .chars()
        .filter(|character| !character.is_control())
        .take(1024)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_delayed_frame_on_inherited_nonblocking_stream() {
        let (mut accepted, mut client) = UnixStream::pair().unwrap();
        accepted.set_nonblocking(true).unwrap();
        configure_control_stream(&accepted).unwrap();
        let sender = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(25));
            write_message(&mut client, &serde_json::json!({"action": "query"})).unwrap();
        });
        let frame: serde_json::Value = read_message(&mut accepted).unwrap();
        assert_eq!(frame, serde_json::json!({"action": "query"}));
        sender.join().unwrap();
    }

    #[test]
    fn expired_deadline_rejects_ready_read_and_write() {
        let (mut accepted, mut client) = UnixStream::pair().unwrap();
        write_message(&mut client, &serde_json::json!({"ready": true})).unwrap();
        let mut bounded = DeadlineStream::new(&mut accepted, Instant::now());
        let result: Result<serde_json::Value, _> = read_message(&mut bounded);
        assert!(result.unwrap_err().contains("deadline exhausted"));
        assert!(
            write_message(&mut bounded, &true)
                .unwrap_err()
                .contains("deadline exhausted")
        );
    }

    #[test]
    fn fragmented_frame_cannot_renew_the_original_deadline() {
        let (mut accepted, mut client) = UnixStream::pair().unwrap();
        let deadline = Instant::now() + Duration::from_millis(200);
        let sender = std::thread::spawn(move || {
            // Each individual pause fits the old per-read socket timeout, but
            // the complete header/body does not fit the one exchange deadline.
            for byte in [0, 0, 0, 4, b't', b'r', b'u', b'e'] {
                if client.write_all(&[byte]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(80));
            }
        });
        let mut bounded = DeadlineStream::new(&mut accepted, deadline);
        let result: Result<serde_json::Value, _> = read_message(&mut bounded);
        assert!(result.is_err());
        drop(accepted);
        sender.join().unwrap();
    }
}
