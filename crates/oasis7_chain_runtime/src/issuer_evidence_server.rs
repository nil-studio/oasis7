//! Authenticated, bounded HTTP ingress for the durable V3 full-sync issuer.
//!
//! Deployment supplies the independent client certificate ACL and the actual
//! protected witness. This transport never constructs a witness or an issuer
//! policy, and a certificate alone does not authorize disclosure.
use oasis7_distfs::controlled_authority::formal::full_sync_issuer::{
    DurableFullSyncIssuerV3, FullSyncIssuerHistoryAccessV3, FullSyncIssuerTargetV2,
    FullSyncIssuerWitnessV3, FullSyncReadEvidenceQueryV3,
};
use rustls::{
    RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer},
    server::WebPkiClientVerifier,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_HEADER: usize = 16 * 1024;
const MAX_REQUEST: usize = 64 * 1024;
const MAX_RESPONSE: usize = 68 * 1024 * 1024;
const MAX_GRANTS: usize = 1024;
const MAX_CONNECTION_TIME: Duration = Duration::from_secs(60);

/// Independently provisioned complete-history disclosure authorization.
/// The SHA-256 pin is for the authenticated leaf certificate's DER bytes.
/// Values from the HTTP body cannot create or broaden this grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerEvidenceGrantV3 {
    pub client_certificate_sha256: String,
    pub provider_public_key: String,
    /// Independently authenticated active process incarnation; a retained
    /// certificate alone cannot refresh an old provider's history access.
    pub provider_incarnation: String,
    pub policy_digest: String,
    pub target: FullSyncIssuerTargetV2,
    pub target_generation: u64,
}

fn hex32(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn certificate_pin(certificate: &[u8]) -> String {
    hex::encode(Sha256::digest(certificate))
}

/// The listener owner supplies a provisioned issuer and its actual witness.
/// No credential, history, or failure detail is echoed in rejected replies.
pub struct IssuerEvidenceServerV3 {
    tls: Arc<ServerConfig>,
    grants: Vec<IssuerEvidenceGrantV3>,
}

impl IssuerEvidenceServerV3 {
    pub fn new(
        client_roots: RootCertStore,
        server_chain: Vec<CertificateDer<'static>>,
        server_key: PrivateKeyDer<'static>,
        grants: Vec<IssuerEvidenceGrantV3>,
    ) -> Result<Self, String> {
        if client_roots.is_empty() || grants.is_empty() || grants.len() > MAX_GRANTS {
            return Err("issuer evidence requires client trust and bounded grants".into());
        }
        for (index, grant) in grants.iter().enumerate() {
            if !hex32(&grant.client_certificate_sha256)
                || !hex32(&grant.provider_public_key)
                || !hex32(&grant.provider_incarnation)
                || !hex32(&grant.policy_digest)
                || !hex32(&grant.target.node_public_key)
                || grant.target_generation == 0
                || grants[..index].iter().any(|old| {
                    old.client_certificate_sha256 == grant.client_certificate_sha256
                        && (old.provider_public_key != grant.provider_public_key
                            || old.provider_incarnation != grant.provider_incarnation
                            || old == grant)
                })
            {
                return Err("invalid or ambiguous issuer evidence grant".into());
            }
        }
        let crypto = Arc::new(rustls::crypto::ring::default_provider());
        let verifier =
            WebPkiClientVerifier::builder_with_provider(Arc::new(client_roots), crypto.clone())
                .build()
                .map_err(|_| "invalid issuer client trust")?;
        let tls = ServerConfig::builder_with_provider(crypto)
            .with_safe_default_protocol_versions()
            .map_err(|_| "invalid issuer TLS protocol configuration")?
            .with_client_cert_verifier(verifier)
            .with_single_cert(server_chain, server_key)
            .map_err(|_| "invalid issuer TLS identity")?;
        Ok(Self {
            tls: Arc::new(tls),
            grants,
        })
    }

    /// Serve exactly one POST /read on an accepted TCP connection. The same
    /// deadline bounds TLS, framing, and release of the response; a blocked
    /// external witness must also obey its own protected transport deadline.
    pub fn serve_connection(
        &self,
        socket: TcpStream,
        deadline: Instant,
        issuer: &mut DurableFullSyncIssuerV3,
        witness: &mut dyn FullSyncIssuerWitnessV3,
    ) -> Result<(), String> {
        self.serve_with(socket, deadline, |grant, query| {
            let access = FullSyncIssuerHistoryAccessV3 {
                provider_public_key: &grant.provider_public_key,
                policy_digest: &grant.policy_digest,
                target: &grant.target,
                target_generation: grant.target_generation,
            };
            let evidence = issuer
                .read_evidence_for_provider(&access, query, witness)
                .map_err(|_| "issuer evidence unavailable".to_string())?;
            let mut output = BoundedBody(Vec::new());
            serde_json::to_writer(&mut output, &evidence)
                .map_err(|_| "issuer evidence exceeded response bound".to_string())?;
            Ok(output.0)
        })
    }

    fn serve_with(
        &self,
        socket: TcpStream,
        deadline: Instant,
        dispatch: impl FnOnce(
            &IssuerEvidenceGrantV3,
            FullSyncReadEvidenceQueryV3,
        ) -> Result<Vec<u8>, String>,
    ) -> Result<(), String> {
        let deadline = deadline.min(Instant::now() + MAX_CONNECTION_TIME);
        socket
            .set_nonblocking(false)
            .map_err(|_| "issuer socket configuration failed")?;
        let mut io = DeadlineSocket { socket, deadline };
        let mut tls =
            ServerConnection::new(self.tls.clone()).map_err(|_| "issuer TLS setup failed")?;
        while tls.is_handshaking() {
            tls.complete_io(&mut io)
                .map_err(|_| "issuer client authentication failed")?;
        }
        let leaf = tls
            .peer_certificates()
            .and_then(|chain| chain.first())
            .ok_or("issuer client certificate missing")?;
        let pin = certificate_pin(leaf.as_ref());
        let mut stream = StreamOwned::new(tls, io);
        let query = match read_query(&mut stream) {
            Ok(query) => query,
            Err(_) => return write_reply(&mut stream, "400 Bad Request", b"{}"),
        };
        let grant = self.grants.iter().find(|grant| {
            query.schema_version == 3
                && grant.client_certificate_sha256 == pin
                && grant.provider_public_key == query.provider_public_key
                && grant.provider_incarnation == query.provider_incarnation
                && grant.policy_digest == query.minimum_anchor.policy_digest
                && grant.target == query.target
                && grant.target_generation == query.target_generation
        });
        let Some(grant) = grant else {
            return write_reply(&mut stream, "403 Forbidden", b"{}");
        };
        remaining(deadline).map_err(|_| "issuer connection expired")?;
        match dispatch(grant, query) {
            Ok(body) if body.len() <= MAX_RESPONSE => write_reply(&mut stream, "200 OK", &body),
            _ => write_reply(&mut stream, "503 Service Unavailable", b"{}"),
        }
    }
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|value| !value.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "issuer deadline"))
}

struct DeadlineSocket {
    socket: TcpStream,
    deadline: Instant,
}

impl Read for DeadlineSocket {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(remaining(self.deadline)?))?;
        self.socket.read(buffer)
    }
}

impl Write for DeadlineSocket {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.socket
            .set_write_timeout(Some(remaining(self.deadline)?))?;
        self.socket.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        remaining(self.deadline)?;
        self.socket.flush()
    }
}

struct BoundedBody(Vec<u8>);

impl Write for BoundedBody {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_RESPONSE.saturating_sub(self.0.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "issuer evidence response too large",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn read_query(stream: &mut impl Read) -> Result<FullSyncReadEvidenceQueryV3, String> {
    let mut header = Vec::with_capacity(1024);
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= MAX_HEADER {
            return Err("issuer request header too large".into());
        }
        let mut byte = [0_u8; 1];
        stream
            .read_exact(&mut byte)
            .map_err(|_| "issuer request header unavailable")?;
        header.push(byte[0]);
    }
    let header = std::str::from_utf8(&header).map_err(|_| "invalid issuer header")?;
    let mut lines = header.trim_end_matches("\r\n").split("\r\n");
    if lines.next() != Some("POST /read HTTP/1.1") {
        return Err("invalid issuer request line".into());
    }
    let mut length = None;
    let mut json = false;
    let mut host = false;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or("invalid issuer header field")?;
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || !value.bytes().all(|b| b.is_ascii_graphic() || b == b' ' || b == b'\t')
        {
            return Err("invalid issuer header field".into());
        }
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err("ambiguous issuer content length".into());
            }
            length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| "invalid issuer content length")?,
            );
        } else if name.eq_ignore_ascii_case("content-type") {
            if json || !value.eq_ignore_ascii_case("application/json") {
                return Err("invalid issuer content type".into());
            }
            json = true;
        } else if name.eq_ignore_ascii_case("host") {
            if host || value.is_empty() {
                return Err("invalid issuer host header".into());
            }
            host = true;
        } else if ["transfer-encoding", "content-encoding", "expect", "trailer"]
            .iter()
            .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
        {
            return Err("unsupported issuer request framing".into());
        }
    }
    let length = length
        .filter(|value| *value > 0 && *value <= MAX_REQUEST && json && host)
        .ok_or("invalid issuer request bounds")?;
    let mut body = vec![0; length];
    stream
        .read_exact(&mut body)
        .map_err(|_| "issuer request body unavailable")?;
    serde_json::from_slice(&body).map_err(|_| "invalid issuer request body".into())
}

fn write_reply(stream: &mut impl Write, status: &str, body: &[u8]) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|()| stream.write_all(body))
        .and_then(|()| stream.flush())
        .map_err(|_| "issuer reply unavailable".into())
}

#[cfg(test)]
#[path = "issuer_evidence_server_tests.rs"]
mod tests;
