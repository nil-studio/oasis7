//! HTTP framing and signed response transport for the formal service.
use super::*;

#[derive(Debug)]
pub(super) struct HttpReply {
    pub(super) status: u16,
    pub(super) body: Vec<u8>,
}
pub(super) fn json_reply<T: Serialize>(status: u16, value: &T) -> HttpReply {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    HttpReply { status, body }
}
pub(super) fn signed_reply<Q: Serialize, T: Serialize>(
    status: u16,
    path: &str,
    request: &Q,
    payload: &T,
    signer: &SigningKey,
) -> HttpReply {
    match authority::request_digest(path, request) {
        Ok(digest) => signed_reply_with_digest(status, path, Ok(digest), payload, signer),
        Err(_) => json_reply(
            500,
            &FormalServiceErrorV1 {
                api_version: "oasis7.formal-live-error/v1".into(),
                code: "signature_failure".into(),
                message: "response signing failed".into(),
            },
        ),
    }
}
pub(super) fn signed_reply_with_digest<T: Serialize>(
    status: u16,
    path: &str,
    digest: Result<String, String>,
    payload: &T,
    signer: &SigningKey,
) -> HttpReply {
    let Ok(digest) = digest else {
        return json_reply(
            500,
            &FormalServiceErrorV1 {
                api_version: "oasis7.formal-live-error/v1".into(),
                code: "signature_failure".into(),
                message: "request digest failed".into(),
            },
        );
    };
    match authority::sign_service_response(path, digest, payload, &hex::encode(signer.to_bytes())) {
        Ok(response) => json_reply(status, &response),
        Err(_) => json_reply(
            500,
            &FormalServiceErrorV1 {
                api_version: "oasis7.formal-live-error/v1".into(),
                code: "signature_failure".into(),
                message: "response signing failed".into(),
            },
        ),
    }
}
pub(super) fn signed_error_with_digest(
    status: u16,
    path: &str,
    digest: Result<String, String>,
    code: &str,
    message: &str,
    signer: &SigningKey,
) -> HttpReply {
    let payload = FormalServiceErrorV1 {
        api_version: "oasis7.formal-live-error/v1".into(),
        code: code.into(),
        message: message.chars().take(MAX_NODE_ERROR_CHARS).collect(),
    };
    signed_reply_with_digest(status, path, digest, &payload, signer)
}
pub(super) fn raw_error(
    path: &str,
    body: &[u8],
    code: &str,
    message: &str,
    signer: &SigningKey,
) -> HttpReply {
    let digest = authority::request_digest(path, &body);
    signed_error_with_digest(400, path, digest, code, message, signer)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalLiveStatusChallengeV1 {
    pub(crate) observation_nonce: String,
}

pub(super) fn handle_status_observation(service: &FormalLiveService, body: &[u8]) -> HttpReply {
    let challenge = (body.len() <= 128)
        .then(|| serde_json::from_slice::<FormalLiveStatusChallengeV1>(body).ok())
        .flatten()
        .filter(|challenge| {
            let nonce = &challenge.observation_nonce;
            nonce.len() == 64
                && nonce.bytes().all(|byte| {
                    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                })
        });
    let Some(challenge) = challenge else {
        return signed_error_with_digest(
            400,
            CHAIN_STATUS_PATH,
            authority::request_digest(CHAIN_STATUS_PATH, &()),
            "invalid_status_challenge",
            "formal status requires a bounded observation nonce",
            &service.signer,
        );
    };
    signed_reply(
        200,
        CHAIN_STATUS_PATH,
        &challenge,
        &service.runtime_status(),
        &service.signer,
    )
}

pub(super) fn plain_error(message: &str) -> Vec<u8> {
    serde_json::to_vec(&FormalServiceErrorV1 {
        api_version: "oasis7.formal-live-error/v1".into(),
        code: "invalid_http_request".into(),
        message: message.chars().take(MAX_NODE_ERROR_CHARS).collect(),
    })
    .unwrap_or_else(|_| b"{}".to_vec())
}

pub(super) fn serve_connection(
    service: &FormalLiveService,
    stream: &mut TcpStream,
) -> Result<(), String> {
    // BSD/macOS may inherit the listener's O_NONBLOCK on accept. Socket
    // timeouts do not clear it; restore blocking reads before applying the
    // single absolute request deadline to fragmented headers and body.
    stream
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(HTTP_CONNECTION_TIMEOUT))
        .map_err(|error| error.to_string())?;
    let deadline = std::time::Instant::now() + HTTP_CONNECTION_TIMEOUT;
    let (method, path, body) = read_http_request(stream, deadline)?;
    let reply = service.handle(&method, &path, &body);
    write_http_reply(stream, reply.status, &reply.body)
}

pub(crate) fn read_http_request(
    stream: &mut TcpStream,
    deadline: std::time::Instant,
) -> Result<(String, String, Vec<u8>), String> {
    let mut bytes = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    loop {
        let count = read_before_deadline(stream, &mut chunk, deadline)
            .map_err(|error| format!("HTTP read: {error}"))?;
        if count == 0 {
            return Err("HTTP request ended before complete headers/body".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
        let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            if bytes.len() > MAX_HTTP_HEADER {
                return Err("HTTP headers exceed bound".into());
            }
            continue;
        };
        if header_end > MAX_HTTP_HEADER {
            return Err("HTTP headers exceed bound".into());
        }
        let (method, path, content_length) = parse_http_headers(&bytes[..header_end])?;
        if content_length > MAX_REQUEST_BODY {
            return Err("HTTP body exceeds bound".into());
        }
        let body_start = header_end + 4;
        let total = body_start
            .checked_add(content_length)
            .ok_or("HTTP length overflow")?;
        if bytes.len() > total {
            return Err("HTTP pipelining is unsupported".into());
        }
        while bytes.len() < total {
            let count = read_before_deadline(stream, &mut chunk, deadline)
                .map_err(|error| format!("HTTP body read: {error}"))?;
            if count == 0 {
                return Err("HTTP body truncated".into());
            }
            bytes.extend_from_slice(&chunk[..count]);
            if bytes.len() > total {
                return Err("HTTP pipelining is unsupported".into());
            }
        }
        return Ok((method, path, bytes[body_start..total].to_vec()));
    }
}

pub(super) fn read_before_deadline(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    deadline: std::time::Instant,
) -> std::io::Result<usize> {
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    if remaining.is_zero() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "HTTP connection deadline expired",
        ));
    }
    stream.set_read_timeout(Some(remaining))?;
    stream.read(buffer)
}

pub(super) fn parse_http_headers(header: &[u8]) -> Result<(String, String, usize), String> {
    let header = std::str::from_utf8(header).map_err(|_| "HTTP headers are not UTF-8")?;
    let mut lines = header.split("\r\n");
    let request_line = lines.next().ok_or("HTTP request line missing")?;
    // This is a signed, mutation-facing service. Require the original HTTP
    // request line's exact SP delimiters instead of normalizing tabs or
    // repeated whitespace into an apparently canonical route.
    if request_line.bytes().any(|byte| byte.is_ascii_control()) {
        return Err("unsupported HTTP request line".into());
    }
    let mut request = request_line.split(' ');
    let method = request.next().ok_or("HTTP method missing")?;
    let path = request.next().ok_or("HTTP path missing")?;
    let version = request.next().ok_or("HTTP version missing")?;
    if method.is_empty()
        || path.is_empty()
        || request.next().is_some()
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || path.contains('?')
        || path.contains('#')
    {
        return Err("unsupported HTTP request line".into());
    }
    let mut content_length = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or("malformed HTTP header")?;
        // Never let whitespace before ':' hide Content-Length or
        // Transfer-Encoding as an unrelated field. Reject control bytes in
        // values before dispatching any signed status or mutation request.
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || !value
                .bytes()
                .all(|b| b == b'\t' || (b >= 0x20 && b != 0x7f))
        {
            return Err("malformed HTTP header field".into());
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err("transfer encoding is unsupported".into());
        }
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err("duplicate HTTP content length".into());
            }
            // HTTP Content-Length is an ASCII decimal, with only SP/HTAB
            // allowed as field OWS. Rust's usize parser accepts a leading '+',
            // and Unicode trim would erase non-HTTP whitespace. Neither form
            // is safe to normalize on a signed mutation-facing service.
            let decimal = value.trim_matches(|ch: char| ch == ' ' || ch == '\t');
            if decimal.is_empty() || !decimal.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("invalid HTTP content length".into());
            }
            content_length = Some(
                decimal
                    .parse::<usize>()
                    .map_err(|_| "invalid HTTP content length")?,
            );
        }
    }
    let length = content_length.unwrap_or(0);
    if method == "POST" && content_length.is_none() {
        return Err("POST requires content length".into());
    }
    if method == "GET" && length != 0 {
        return Err("GET body is unsupported".into());
    }
    Ok((method.to_owned(), path.to_owned(), length))
}

pub(crate) fn write_http_reply(
    stream: &mut TcpStream,
    status: u16,
    body: &[u8],
) -> Result<(), String> {
    if body.len() > MAX_HTTP_REPLY {
        return Err("HTTP response exceeds bound".into());
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
    write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())
        .and_then(|_| stream.write_all(body))
        .and_then(|_| stream.flush())
        .map_err(|error| format!("HTTP write: {error}"))
}

#[derive(Debug)]
pub(crate) struct FormalHttpServer {
    pub(super) stop: Arc<AtomicBool>,
    pub(super) join: Option<JoinHandle<()>>,
}
impl FormalHttpServer {
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| "formal HTTP server thread panicked".to_owned())?;
        }
        Ok(())
    }
}
impl Drop for FormalHttpServer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod framing_tests {
    use super::{parse_http_headers, read_http_request, write_http_reply};
    use std::{
        io::{Read, Write},
        net::{Shutdown, TcpListener, TcpStream},
        time::{Duration, Instant},
    };

    // Exercise the production bounded TCP reader, not only its parser helper.
    fn receive(raw: &[u8]) -> Result<(String, String, Vec<u8>), String> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client.write_all(raw).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        read_http_request(&mut server, Instant::now() + Duration::from_secs(2))
    }

    #[test]
    fn formal_http_replies_disable_caching_even_for_degraded_status() {
        // Use the production TCP response writer, not a synthetic header
        // builder. A previous successful status must never become a reusable
        // response after readiness or authorization changes.
        for status in [200, 400, 503] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (mut server, _) = listener.accept().unwrap();
            write_http_reply(&mut server, status, b"{}").unwrap();
            drop(server);
            let mut response = String::new();
            client.read_to_string(&mut response).unwrap();
            let (headers, body) = response.split_once("\r\n\r\n").unwrap();
            assert_eq!(body, "{}");
            assert!(headers.starts_with(&format!("HTTP/1.1 {status} ")));
            assert!(headers.split("\r\n").any(|line| line == "Cache-Control: no-store"));
            assert!(headers.split("\r\n").any(|line| line == "Content-Length: 2"));
            assert!(headers.split("\r\n").any(|line| line == "Connection: close"));
        }
    }

    #[test]
    fn signed_formal_http_rejects_ambiguous_framing_before_dispatch() {
        for malformed in [
            b"GET /v1/chain/status HTTP/1.1\r\nTransfer-Encoding : chunked\r\n\r\n".as_slice(),
            b"POST /v1/world/submit HTTP/1.1\r\nContent-Length: 2\r\nContent-Length : 9\r\n\r\n{}".as_slice(),
            b"GET /v1/chain/status HTTP/1.1\r\nX-Probe: bad\0value\r\n\r\n".as_slice(),
            b"GET /v1/chain/status HTTP/1.1\r\nX-Probe: bad\nvalue\r\n\r\n".as_slice(),
            b"GET /v1/chain/status HTTP/1.1\r\nX-Bad Name: value\r\n\r\n".as_slice(),
        ] {
            assert!(
                receive(malformed).is_err(),
                "accepted malformed request: {malformed:?}"
            );
        }
        let (method, path, body) = receive(
            b"POST /v1/world/submit HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nX-Probe: hi\tthere\r\n\r\n{}",
        )
        .expect("valid bounded formal HTTP request");
        assert_eq!(method, "POST");
        assert_eq!(path, "/v1/world/submit");
        assert_eq!(body, b"{}");
        assert!(parse_http_headers(b"GET /v1/chain/status HTTP/1.1\r\nContent-Length: 0").is_ok());
        // Valid UTF-8 observations are preserved; only C0/DEL control bytes
        // and ambiguous field names are rejected.
        assert!(parse_http_headers(
            "GET /v1/chain/status HTTP/1.1\r\nX-Note: café".as_bytes()
        )
        .is_ok());
    }

    #[test]
    fn signed_formal_http_requires_ascii_decimal_content_length_on_real_tcp() {
        // Content-Length must not acquire an alternative framing meaning
        // through Rust numeric parsing or Unicode whitespace normalization.
        for malformed in [
            b"POST /v1/world/submit HTTP/1.1\r\nContent-Length: +2\r\n\r\n{}".as_slice(),
            b"POST /v1/world/submit HTTP/1.1\r\nContent-Length: \xc2\xa02\r\n\r\n{}".as_slice(),
            b"POST /v1/world/submit HTTP/1.1\r\nContent-Length: 2\xc2\xa0\r\n\r\n{}".as_slice(),
            b"POST /v1/world/submit HTTP/1.1\r\nContent-Length: \t+2\r\n\r\n{}".as_slice(),
            b"POST /v1/world/submit HTTP/1.1\r\nContent-Length: 2 2\r\n\r\n{}".as_slice(),
        ] {
            assert!(
                receive(malformed).is_err(),
                "accepted non-decimal HTTP length: {malformed:?}"
            );
        }
        // Legal HTTP OWS and leading zeroes retain the same exact framing.
        let (method, path, body) =
            receive(b"POST /v1/world/submit HTTP/1.1\r\nContent-Length:\t002 \t\r\n\r\n{}")
                .expect("ASCII decimal with legal OWS");
        assert_eq!(method, "POST");
        assert_eq!(path, "/v1/world/submit");
        assert_eq!(body, b"{}");
    }

    #[test]
    fn signed_formal_http_rejects_noncanonical_request_line_before_dispatch() {
        for malformed in [
            b"GET\t/v1/chain/status HTTP/1.1\r\nHost: localhost\r\n\r\n".as_slice(),
            b"GET  /v1/chain/status HTTP/1.1\r\nHost: localhost\r\n\r\n".as_slice(),
            b"GET /v1/chain/status\tHTTP/1.1\r\nHost: localhost\r\n\r\n".as_slice(),
            b"POST /v1/world/submit  HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}".as_slice(),
            b"GET /v1/chain/status\0 HTTP/1.1\r\nHost: localhost\r\n\r\n".as_slice(),
        ] {
            assert!(
                receive(malformed).is_err(),
                "accepted noncanonical request line: {malformed:?}"
            );
        }

        let (method, path, body) =
            receive(b"GET /v1/chain/status HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .expect("canonical HTTP request line");
        assert_eq!(method, "GET");
        assert_eq!(path, "/v1/chain/status");
        assert!(body.is_empty());
    }
}
