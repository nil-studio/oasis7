//! Strict HTTPS exchange over an admitted address and independently configured
//! TLS name/root policy. DNS uses the captured configuration and original
//! exchange deadline, with no blocking system resolver fallback.
use super::{
    direct_http,
    metered_io::remaining,
    operation_ledger::{AttemptPermit, Ledger},
    tls_io::TlsBodyIo,
};
use oasis7_game_client::formal_sync_exchange::FullSyncHttpResponse;
use rustls::{ClientConfig, pki_types::ServerName};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) struct Request<'a> {
    pub address: super::dns::Target,
    pub server_name: ServerName<'static>,
    pub tls_config: Arc<ClientConfig>,
    pub authority: &'a str,
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
) -> Result<FullSyncHttpResponse, String> {
    if input.authority.is_empty()
        || input.authority.len() > 512
        || input.authority.bytes().any(|c| !(b'!'..127).contains(&c))
        || !input.path.starts_with('/')
        || input.path.bytes().any(|c| !(b'!'..127).contains(&c))
        || input.body.len() > 256 * 1024
        || input.response_bound > 16 * 1024 * 1024
        || input.request_timeout.is_zero()
        || input.request_timeout > Duration::from_secs(60)
    {
        return Err("invalid controlled HTTPS request bounds".into());
    }
    let headers = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        input.path,
        input.authority,
        input.body.len()
    );
    if headers.len() > 8 * 1024 {
        return Err("HTTPS request headers exceed bound".into());
    }
    let deadline = deadline.min(attempt.deadline());
    remaining(deadline, input.request_timeout)?;
    // Atomically reserve both directions before TCP/TLS handshake, just like
    // the plaintext adapter. Failed handshakes still consume this exchange.
    let request = ledger.begin_exchange(
        attempt,
        input.body.len() as u64,
        input.response_bound.min(64 * 1024) as u64,
    )?;
    let result = (|| {
        let address = input.address.resolve(deadline, input.request_timeout)?;
        let mut stream = TlsBodyIo::connect(
            address,
            input.server_name,
            input.tls_config,
            deadline,
            input.request_timeout,
        )?;
        stream.write_headers(headers.as_bytes())?;
        let mut rest = input.body;
        while !rest.is_empty() {
            let n = stream.write_body(ledger, attempt, &request, rest)?;
            rest = &rest[n..];
        }
        let mut response_headers = Vec::with_capacity(1024);
        while !response_headers.ends_with(b"\r\n\r\n") {
            if response_headers.len() == 8 * 1024 {
                return Err("HTTPS response headers exceed bound".into());
            }
            response_headers.push(stream.read_header_byte()?);
        }
        let (status, length, retry_after) =
            direct_http::parse_full_headers(&response_headers, input.response_bound)?;
        let mut output = vec![0; length];
        let mut offset = 0;
        while offset < length {
            let n = stream.read_body(ledger, attempt, &request, &mut output[offset..])?;
            if n == 0 {
                return Err("HTTPS response ended within declared body".into());
            }
            offset += n;
        }
        remaining(deadline, input.request_timeout)?;
        Ok(FullSyncHttpResponse {
            status,
            retry_after,
            body: output,
        })
        // The socket and TLS buffers are dropped before releasing the durable
        // exchange. An unknown settlement leaves its reservation unreleased.
    })();
    ledger.finish_exchange(attempt, &request)?;
    remaining(deadline, input.request_timeout)?;
    result
}
