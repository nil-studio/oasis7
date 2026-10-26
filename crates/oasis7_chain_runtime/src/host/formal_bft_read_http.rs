//! Loopback transport for a durable, independently verified fixed BFT cut.
//! No controlled-profile route or write operation is installed by this server.
use ed25519_dalek::SigningKey;
use oasis7::world_service::{
    authority, formal_bft_read_service::FormalBftReadPublicationV2,
    formal_read_error::FormalReadError,
};
use oasis7_client_api::world_service::{
    FORMAL_BFT_READ_PATH_V2, FormalBftReadRequestV2, SignedReadRequest,
};
use oasis7_runtime::World;
use serde::Serialize;
use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use super::formal_live_service::{
    http::{read_http_request, write_http_reply},
    parse_strict,
};

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(2);
const HTTP_STACK_BYTES: usize = 8 * 1024 * 1024;
const MAX_SIGNED_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// The owner must construct the publication from the execution driver's
/// verified durable cut. A current authorization World is optional only for
/// public reads; private reads fail closed without it. Its lock serializes
/// authorization changes with response construction, before bytes are released
/// to transport. The historical publication is never used as that authority.
pub(crate) struct FormalBftReadHttpServiceV2 {
    publication: Mutex<FormalBftReadPublicationV2>,
    signer: SigningKey,
    current_authorization: Option<Arc<Mutex<World>>>,
}

impl FormalBftReadHttpServiceV2 {
    pub(crate) fn new(
        publication: FormalBftReadPublicationV2,
        signer: SigningKey,
        current_authorization: Option<Arc<Mutex<World>>>,
    ) -> Self {
        Self {
            publication: Mutex::new(publication),
            signer,
            current_authorization,
        }
    }

    pub(crate) fn serve_loopback(
        self: Arc<Self>,
        bind: SocketAddr,
    ) -> Result<FormalBftReadHttpServerV2, String> {
        if !bind.ip().is_loopback() || bind.port() == 0 {
            return Err("formal BFT read bind must be a nonzero loopback address".into());
        }
        let listener = TcpListener::bind(bind).map_err(|error| error.to_string())?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let join = thread::Builder::new()
            .name("oasis7-bft-fixed-read-http".into())
            .stack_size(HTTP_STACK_BYTES)
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, peer)) if peer.ip().is_loopback() => {
                            if let Err(error) = self.serve_connection(&mut stream) {
                                let (_, body) = error_body(400, "invalid_http_request", &error);
                                let _ = write_http_reply(&mut stream, 400, &body);
                            }
                        }
                        Ok(_) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => return Err(format!("formal BFT read listener: {error}")),
                    }
                }
                Ok(())
            })
            .map_err(|error| error.to_string())?;
        Ok(FormalBftReadHttpServerV2 {
            stop,
            join: Some(join),
        })
    }

    fn serve_connection(&self, stream: &mut TcpStream) -> Result<(), String> {
        stream
            .set_nonblocking(false)
            .map_err(|error| error.to_string())?;
        stream
            .set_write_timeout(Some(CONNECTION_TIMEOUT))
            .map_err(|error| error.to_string())?;
        let (method, path, body) = read_http_request(stream, Instant::now() + CONNECTION_TIMEOUT)?;
        let (status, reply) = if method == "POST" && path == FORMAL_BFT_READ_PATH_V2 {
            self.read_reply(&body)
        } else {
            error_body(
                404,
                "unsupported_route",
                "only the fixed-cut BFT read route is available",
            )
        };
        write_http_reply(stream, status, &reply)
    }

    fn request_error(
        &self,
        request: &SignedReadRequest<FormalBftReadRequestV2>,
        status: u16,
        code: &str,
        message: &str,
    ) -> (u16, Vec<u8>) {
        let payload = serde_json::json!({
            "schema_version": 2,
            "code": code,
            "message": message.chars().take(384).collect::<String>(),
        });
        let signed = authority::request_digest(FORMAL_BFT_READ_PATH_V2, request)
            .and_then(|digest| {
                authority::sign_service_response(
                    FORMAL_BFT_READ_PATH_V2,
                    digest,
                    &payload,
                    &hex::encode(self.signer.to_bytes()),
                )
            })
            .and_then(|reply| serde_json::to_vec(&reply).map_err(|error| error.to_string()));
        match signed {
            Ok(body) => (status, body),
            Err(_) => error_body(500, "signature_failure", "formal BFT error signing failed"),
        }
    }

    fn read_reply(&self, body: &[u8]) -> (u16, Vec<u8>) {
        let request: SignedReadRequest<FormalBftReadRequestV2> = match parse_strict(body) {
            Ok(request) => request,
            Err(error) => return error_body(400, "invalid_request", &error),
        };
        // Keep the current authorization guard through both checking the
        // request and constructing the exact signed response.
        let authorization = if request.request.scope_id == "public" {
            None
        } else if let Some(world) = &self.current_authorization {
            match world.try_lock() {
                Ok(guard) => Some(guard),
                Err(_) => {
                    return self.request_error(
                        &request,
                        503,
                        "authorization_unavailable",
                        "current authorization is unavailable or busy",
                    );
                }
            }
        } else {
            None
        };
        let publication = match self.publication.try_lock() {
            Ok(publication) => publication,
            Err(_) => {
                return self.request_error(
                    &request,
                    503,
                    "publication_unavailable",
                    "fixed-cut publication is unavailable or busy",
                );
            }
        };
        let payload = match publication.read(&request, authorization.as_deref()) {
            Ok(payload) => payload,
            Err(error) => {
                let (status, code) = match &error {
                    FormalReadError::RequestRejected(_) => (400, "request_rejected"),
                    FormalReadError::GrantRejected(_) => (403, "grant_rejected"),
                    FormalReadError::NotReady(_) => (503, "not_ready"),
                    FormalReadError::HistoryUnavailable(_) => (410, "history_unavailable"),
                    FormalReadError::Internal(_) => (500, "internal_error"),
                    FormalReadError::Integrity(_) => (400, "integrity_rejected"),
                };
                return self.request_error(&request, status, code, &error.to_string());
            }
        };
        let signed =
            authority::request_digest(FORMAL_BFT_READ_PATH_V2, &request).and_then(|digest| {
                authority::sign_service_response(
                    FORMAL_BFT_READ_PATH_V2,
                    digest,
                    &payload,
                    &hex::encode(self.signer.to_bytes()),
                )
            });
        match signed
            .and_then(|response| serde_json::to_vec(&response).map_err(|error| error.to_string()))
        {
            Ok(body) if body.len() <= MAX_SIGNED_RESPONSE_BYTES => (200, body),
            Ok(_) => error_body(
                500,
                "response_too_large",
                "signed response exceeds transport budget",
            ),
            Err(_) => error_body(
                500,
                "signature_failure",
                "formal BFT response signing failed",
            ),
        }
    }
}

fn error_body(status: u16, code: &str, message: &str) -> (u16, Vec<u8>) {
    #[derive(Serialize)]
    struct ErrorBody<'a> {
        schema_version: u32,
        code: &'a str,
        message: String,
    }
    let error = ErrorBody {
        schema_version: 2,
        code,
        message: message.chars().take(384).collect(),
    };
    (
        status,
        serde_json::to_vec(&error).expect("serializing a bounded error DTO"),
    )
}

pub(crate) struct FormalBftReadHttpServerV2 {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<Result<(), String>>>,
}
impl FormalBftReadHttpServerV2 {
    pub(crate) fn is_finished(&self) -> bool {
        self.join.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub(crate) fn stop(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::Release);
        self.join.take().map_or(Ok(()), |join| {
            join.join()
                .map_err(|_| "formal BFT read HTTP thread panicked".to_owned())?
        })
    }
}
impl Drop for FormalBftReadHttpServerV2 {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
