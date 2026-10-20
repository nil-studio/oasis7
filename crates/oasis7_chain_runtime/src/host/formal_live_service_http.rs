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

pub(super) fn read_http_request(
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
    let mut request = request_line.split_ascii_whitespace();
    let method = request.next().ok_or("HTTP method missing")?;
    let path = request.next().ok_or("HTTP path missing")?;
    let version = request.next().ok_or("HTTP version missing")?;
    if request.next().is_some()
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || path.contains('?')
        || path.contains('#')
    {
        return Err("unsupported HTTP request line".into());
    }
    let mut content_length = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or("malformed HTTP header")?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err("transfer encoding is unsupported".into());
        }
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err("duplicate HTTP content length".into());
            }
            content_length = Some(
                value
                    .trim()
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

pub(super) fn write_http_reply(
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
    write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())
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
