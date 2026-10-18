//! Test-only framing shared by the in-process and child-process oracle.
use std::{io::Read, net::TcpStream};
// An idle lease is legal while the client verifies immutable material. Only a
// frame that has started must finish inside the socket's bounded read timeout.
pub(super) fn recv<T: serde::de::DeserializeOwned>(
    stream: &mut TcpStream,
) -> Result<Option<T>, ()> {
    let mut len = [0; 4];
    match stream.read(&mut len[..1]) {
        Ok(0) => return Err(()),
        Ok(_) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            return Ok(None);
        }
        Err(_) => return Err(()),
    }
    let timeout = stream.read_timeout().map_err(|_| ())?.ok_or(())?;
    let deadline = std::time::Instant::now() + timeout;
    let result = (|| {
        frame_exact(stream, &mut len[1..], deadline)?;
        let len = u32::from_be_bytes(len) as usize;
        if len == 0 || len > 1024 * 1024 {
            return Err(());
        }
        let mut bytes = vec![0; len];
        frame_exact(stream, &mut bytes, deadline)?;
        serde_cbor::from_slice(&bytes).map(Some).map_err(|_| ())
    })();
    stream.set_read_timeout(Some(timeout)).map_err(|_| ())?;
    result
}
fn frame_exact(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    deadline: std::time::Instant,
) -> Result<(), ()> {
    while !bytes.is_empty() {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or(())?;
        if remaining.is_zero() {
            return Err(());
        }
        stream.set_read_timeout(Some(remaining)).map_err(|_| ())?;
        let count = stream.read(bytes).map_err(|_| ())?;
        if count == 0 {
            return Err(());
        }
        bytes = &mut bytes[count..];
    }
    Ok(())
}
