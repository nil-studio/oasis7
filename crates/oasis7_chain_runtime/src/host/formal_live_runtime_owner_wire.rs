//! Bounded framing for private formal-owner IPC.
use super::MAX_CONTROL_BYTES;
#[cfg(test)]
use super::configure_control_stream;
use serde::{Serialize, de::DeserializeOwned};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
#[cfg(test)]
use std::time::Duration;

pub(super) fn write_message<T: Serialize>(
    stream: &mut UnixStream,
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

pub(super) fn read_message<T: DeserializeOwned>(stream: &mut UnixStream) -> Result<T, String> {
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
}
