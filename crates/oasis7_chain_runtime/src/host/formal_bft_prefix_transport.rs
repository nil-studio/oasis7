//! V3 response-body budget and absolute socket-write deadline. The V1/V2
//! writer retains its original 8 MiB cap and timeout behavior.
use std::io::{ErrorKind, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};
const MAX_BODY: usize = oasis7_client_api::world_service::MAX_FORMAL_BFT_READ_RESPONSE_BYTES_V3;
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn write_http_reply(
    stream: &mut TcpStream,
    status: u16,
    body: &[u8],
) -> Result<(), String> {
    write_http_reply_until(stream, status, body, Instant::now() + WRITE_TIMEOUT)
}
pub(super) fn write_http_reply_until(
    stream: &mut TcpStream,
    status: u16,
    body: &[u8],
    deadline: Instant,
) -> Result<(), String> {
    if body.len() > MAX_BODY {
        return Err("V3 HTTP response exceeds 40 MiB body budget".into());
    }
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        410 => "Gone",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    write_until(stream, header.as_bytes(), deadline)?;
    write_until(stream, body, deadline)
}
fn write_until(stream: &mut TcpStream, mut bytes: &[u8], deadline: Instant) -> Result<(), String> {
    while !bytes.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or("V3 HTTP absolute write deadline exceeded")?;
        stream
            .set_write_timeout(Some(remaining))
            .map_err(|e| format!("V3 HTTP write timeout: {e}"))?;
        let end = bytes.len().min(64 * 1024);
        match stream.write(&bytes[..end]) {
            Ok(0) => return Err("V3 HTTP peer closed during response".into()),
            Ok(written) => bytes = &bytes[written..],
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::Interrupted | ErrorKind::WouldBlock | ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(error) => return Err(format!("V3 HTTP write: {error}")),
        }
    }
    Ok(())
}
