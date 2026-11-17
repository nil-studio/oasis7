//! Controlled HTTP/1.1 body-I/O primitive for explicitly approved socket targets.
//! URL/DNS/TLS policy and signature verification remain caller responsibilities.
//! This primitive is not yet exposed as a full-sync V2 configuration capability.
use super::metered_io::{read_window, remaining, write_window};
use super::operation_ledger::{AttemptPermit, Ledger};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

const HEADERS: usize = 8 * 1024;
const REQUEST_BODY: usize = 256 * 1024;

pub(super) struct Request<'a> {
    pub address: SocketAddr,
    pub path: &'a str,
    pub body: &'a [u8],
    pub response_bound: usize,
    pub request_timeout: Duration,
}

pub(super) fn post(
    input: Request<'_>,
    ledger: &mut Ledger,
    attempt: &AttemptPermit,
    deadline: Instant,
) -> Result<(u16, Vec<u8>), String> {
    post_with_headers(input, ledger, attempt, deadline)
        .map(|response| (response.status, response.body))
}

pub(super) fn post_with_headers(
    input: Request<'_>,
    ledger: &mut Ledger,
    attempt: &AttemptPermit,
    deadline: Instant,
) -> Result<oasis7_game_client::formal_sync_exchange::FullSyncHttpResponse, String> {
    let Request {
        address,
        path,
        body,
        response_bound,
        request_timeout,
    } = input;
    if !path.starts_with('/')
        || path.bytes().any(|c| !(b'!'..127).contains(&c))
        || body.len() > REQUEST_BODY
        || response_bound > 16 * 1024 * 1024
        || request_timeout.is_zero()
        || request_timeout > Duration::from_secs(60)
    {
        return Err("invalid controlled HTTP request bounds".into());
    }
    let header = format!(
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    if header.len() > HEADERS {
        return Err("request headers exceed 8 KiB".into());
    }
    let deadline = deadline.min(attempt.deadline());
    remaining(deadline, request_timeout)?;
    let request = ledger.begin_exchange(
        attempt,
        body.len() as u64,
        response_bound.min(64 * 1024) as u64,
    )?;
    let result = (|| {
        // The stream is deliberately scoped inside the closure: the ledger's
        // request permit is not released until its socket has been dropped.
        let mut stream =
            TcpStream::connect_timeout(&address, remaining(deadline, request_timeout)?)
                .map_err(|e| format!("controlled HTTP connect: {e}"))?;
        let mut unsent = header.as_bytes();
        while !unsent.is_empty() {
            stream
                .set_write_timeout(Some(remaining(deadline, request_timeout)?))
                .map_err(|e| e.to_string())?;
            let n = stream.write(unsent).map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("HTTP header writer made no progress".into());
            }
            unsent = &unsent[n..];
        }
        let mut unsent = body;
        while !unsent.is_empty() {
            stream
                .set_write_timeout(Some(remaining(deadline, request_timeout)?))
                .map_err(|e| e.to_string())?;
            let n = write_window(
                &mut stream,
                ledger,
                attempt,
                &request,
                unsent,
                deadline,
                request_timeout,
            )?;
            unsent = &unsent[n..];
        }
        let mut headers = Vec::with_capacity(1024);
        while !headers.ends_with(b"\r\n\r\n") {
            if headers.len() == HEADERS {
                return Err("response headers exceed 8 KiB".into());
            }
            stream
                .set_read_timeout(Some(remaining(deadline, request_timeout)?))
                .map_err(|e| e.to_string())?;
            let mut byte = [0];
            if stream.read(&mut byte).map_err(|e| e.to_string())? == 0 {
                return Err("response ended within headers".into());
            }
            headers.push(byte[0]);
        }
        let (status, length, retry_after) = parse_full_headers(&headers, response_bound)?;
        let mut output = vec![0; length];
        let mut offset = 0;
        while offset < length {
            stream
                .set_read_timeout(Some(remaining(deadline, request_timeout)?))
                .map_err(|e| e.to_string())?;
            let n = read_window(
                &mut stream,
                ledger,
                attempt,
                &request,
                &mut output[offset..],
                deadline,
                request_timeout,
            )?;
            if n == 0 {
                return Err("response ended within declared body".into());
            }
            offset += n;
        }
        remaining(deadline, request_timeout)?;
        Ok(
            oasis7_game_client::formal_sync_exchange::FullSyncHttpResponse {
                status,
                retry_after,
                body: output,
            },
        )
    })();
    // On uncertain persistence this fails closed and preserves the token. On
    // ordinary socket failures, known actual prefixes remain charged.
    ledger.finish_exchange(attempt, &request)?;
    remaining(deadline, request_timeout)?;
    result
}

fn parse_headers(bytes: &[u8], bound: usize) -> Result<(u16, usize), String> {
    parse_full_headers(bytes, bound).map(|(status, length, _)| (status, length))
}
pub(super) fn parse_full_headers(
    bytes: &[u8],
    bound: usize,
) -> Result<(u16, usize, Option<String>), String> {
    if bytes
        .iter()
        .any(|&c| c >= 127 || (c < b' ' && !matches!(c, b'\r' | b'\n' | b'\t')))
    {
        return Err("invalid HTTP header octet".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let mut lines = text.split("\r\n");
    let status_line = lines.next().ok_or("missing HTTP status")?;
    if status_line.contains(['\r', '\n', '\t']) {
        return Err("invalid HTTP status line".into());
    }
    let mut parts = status_line.splitn(3, ' ');
    if !matches!(parts.next(), Some("HTTP/1.1" | "HTTP/1.0")) {
        return Err("unsupported HTTP version".into());
    }
    let code = parts.next().ok_or("missing HTTP status code")?;
    if code.len() != 3 || !code.bytes().all(|c| c.is_ascii_digit()) {
        return Err("invalid HTTP status code".into());
    }
    let status: u16 = code.parse().map_err(|_| "invalid HTTP status code")?;
    if !(200..=599).contains(&status) {
        return Err("unsupported interim HTTP response".into());
    }
    let mut length = None;
    let mut retry_after = None;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if line.contains(['\r', '\n']) {
            return Err("bare CR/LF in HTTP header".into());
        }
        let (name, value) = line.split_once(':').ok_or("malformed HTTP header")?;
        if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
            return Err("invalid HTTP header name".into());
        }
        if name.eq_ignore_ascii_case("retry-after") {
            let value = value.trim();
            if retry_after.is_some() || value.is_empty() || value.len() > 128 {
                return Err("invalid or duplicate Retry-After".into());
            }
            retry_after = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("content-length") {
            let value = value.trim();
            if length.is_some() || value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
                return Err("ambiguous HTTP Content-Length".into());
            }
            length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| "invalid HTTP Content-Length")?,
            );
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err("unsupported HTTP Transfer-Encoding".into());
        } else if name.eq_ignore_ascii_case("content-encoding")
            && !value.trim().eq_ignore_ascii_case("identity")
        {
            return Err("unsupported HTTP Content-Encoding".into());
        }
    }
    let length = length.ok_or("HTTP Content-Length required")?;
    if length > bound {
        return Err("encoded response body exceeds route bound".into());
    }
    Ok((status, length, retry_after))
}

#[cfg(test)]
mod tests {
    use super::parse_headers;
    #[test]
    fn framing_rejects_ambiguous_or_transformed_body() {
        for header in [
            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Encoding: gzip\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nX-Test: x\ny\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nX-Test: x\ry\r\n\r\n",
        ] {
            assert!(parse_headers(header.as_bytes(), 16).is_err());
        }
        assert_eq!(
            parse_headers(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 5\r\n\r\n", 16).unwrap(),
            (403, 5)
        );
    }
}
