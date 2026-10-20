//! Real localhost receiver fixture shared by protocol and protected-runtime
//! tests. It is deliberately cfg(test) and cannot grant formal qualification.

use super::*;
use std::{
    collections::HashMap,
    io,
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
};

const FIXTURE_RECEIVER_SEED: [u8; 32] = [0xC1; 32];
const FIXTURE_IO_DEADLINE: Duration = Duration::from_secs(2);
const FIXTURE_RECEIPT: &str = "formal-effect-receiver-fixture";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ObservedEffectRequestV1 {
    pub(crate) operation: String,
    pub(crate) identity: ExternalEffectIdentityV1,
    pub(crate) stable_effect_digest: String,
    pub(crate) sender_public_key_hex: String,
    pub(crate) committed_at: Option<super::super::formal_effect_outbox::QualifiedEffectCommitRefV1>,
}

#[derive(Default)]
struct ReceiverState {
    ledger: Mutex<HashMap<String, (String, JsonValue)>>,
    observations: Mutex<Vec<ObservedEffectRequestV1>>,
    executions: AtomicUsize,
    posts: AtomicUsize,
    queries: AtomicUsize,
    dropped_first_post_reply: AtomicBool,
    last_error: Mutex<Option<String>>,
}

/// Persistent in-process receiver used while actual runtime children are
/// started, killed and reopened. Its ledger survives those child lifetimes.
pub(crate) struct LocalEffectReceiverFixture {
    contract: FormalEffectReceiverContractV1,
    state: Arc<ReceiverState>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl LocalEffectReceiverFixture {
    pub(crate) fn start(
        authorized_sender_public_keys_hex: Vec<String>,
        first_post_lost_reply: bool,
        idempotent_post: bool,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("bind effect test receiver: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("configure effect test receiver: {error}"))?;
        let receiver = SigningKey::from_bytes(&FIXTURE_RECEIVER_SEED);
        let mut senders = authorized_sender_public_keys_hex;
        senders.sort();
        senders.dedup();
        let contract = FormalEffectReceiverContractV1 {
            schema_version: 1,
            target_id: "local.receiver".into(),
            receiver_id: "formal-effect-test-receiver".into(),
            address: listener
                .local_addr()
                .map_err(|error| format!("read effect test receiver address: {error}"))?,
            authorized_sender_public_keys_hex: senders,
            receiver_public_key_hex: hex::encode(receiver.verifying_key().to_bytes()),
            idempotent_post,
            complete_signed_query: true,
            signed_rejection_proves_no_effect: true,
            timeout_ms: FIXTURE_IO_DEADLINE.as_millis() as u64,
        };
        contract.validate()?;
        let contract_digest = contract.canonical_digest()?;
        let state = Arc::new(ReceiverState::default());
        let stop = Arc::new(AtomicBool::new(false));
        let thread_state = state.clone();
        let thread_stop = stop.clone();
        let thread_contract = contract.clone();
        let drop_first_reply = first_post_lost_reply;
        let worker = thread::Builder::new()
            .name("formal-effect-test-receiver".into())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if let Err(error) = handle_connection(
                                stream,
                                &thread_contract,
                                &contract_digest,
                                &receiver,
                                &thread_state,
                                drop_first_reply,
                            ) {
                                set_last_error(&thread_state, error);
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => {
                            set_last_error(
                                &thread_state,
                                format!("effect test receiver accept failed: {error}"),
                            );
                            thread::sleep(Duration::from_millis(5));
                        }
                    }
                }
            })
            .map_err(|error| format!("start effect test receiver: {error}"))?;
        Ok(Self {
            contract,
            state,
            stop,
            worker: Some(worker),
        })
    }

    pub(crate) fn contract(&self) -> &FormalEffectReceiverContractV1 {
        &self.contract
    }

    pub(crate) fn executions(&self) -> usize {
        self.state.executions.load(Ordering::Acquire)
    }

    pub(crate) fn posts(&self) -> usize {
        self.state.posts.load(Ordering::Acquire)
    }

    pub(crate) fn queries(&self) -> usize {
        self.state.queries.load(Ordering::Acquire)
    }

    pub(crate) fn wait_for_queries(&self, expected: usize, deadline: Instant) -> bool {
        while Instant::now() < deadline {
            if self.queries() >= expected {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        self.queries() >= expected
    }

    pub(crate) fn observations(&self) -> Vec<ObservedEffectRequestV1> {
        self.state
            .observations
            .lock()
            .map(|observations| observations.clone())
            .unwrap_or_default()
    }

    pub(crate) fn last_error(&self) -> Option<String> {
        self.state
            .last_error
            .lock()
            .ok()
            .and_then(|error| error.clone())
    }

    pub(crate) fn wait_for_execution(&self, deadline: Instant) -> bool {
        while Instant::now() < deadline {
            if self.executions() > 0 {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        self.executions() > 0
    }

    /// The accept loop is nonblocking, and each accepted HTTP exchange has a
    /// strict deadline, so joining does not depend on a client response.
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| "effect test receiver thread panicked".to_owned())?;
        }
        Ok(())
    }
}

impl Drop for LocalEffectReceiverFixture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn handle_connection(
    mut stream: TcpStream,
    contract: &FormalEffectReceiverContractV1,
    contract_digest: &str,
    receiver: &SigningKey,
    state: &ReceiverState,
    drop_first_post_reply: bool,
) -> Result<(), String> {
    stream
        .set_nonblocking(false)
        .map_err(|error| format!("configure accepted test stream: {error}"))?;
    stream
        .set_write_timeout(Some(FIXTURE_IO_DEADLINE))
        .map_err(|error| format!("set test stream write timeout: {error}"))?;
    let deadline = Instant::now() + FIXTURE_IO_DEADLINE;
    let raw = read_bounded_http_request(&mut stream, deadline)?;
    let request: SignedEffectRequestV1 = serde_cbor::from_slice(&raw)
        .map_err(|error| format!("decode signed test receiver request: {error}"))?;
    validate_request(&request, contract, contract_digest)?;
    let request_rpc_digest = request_digest(&request.body, &request.sender_public_key_hex)?;
    state
        .observations
        .lock()
        .map_err(|_| "effect test receiver observation lock poisoned")?
        .push(ObservedEffectRequestV1 {
            operation: request.body.operation.clone(),
            identity: request.body.identity.clone(),
            stable_effect_digest: request.body.effect_request_digest.clone(),
            sender_public_key_hex: request.sender_public_key_hex.clone(),
            committed_at: request.body.committed_at.clone(),
        });

    let outcome = match request.body.operation.as_str() {
        "deliver" => {
            state.posts.fetch_add(1, Ordering::AcqRel);
            let _intent = request
                .body
                .intent
                .as_ref()
                .ok_or("delivery request omitted its typed intent")?;
            let receipt = JsonValue::from(FIXTURE_RECEIPT);
            let mut ledger = state
                .ledger
                .lock()
                .map_err(|_| "effect test receiver ledger lock poisoned")?;
            let stored = match ledger.get(&request.body.delivery_key) {
                Some((prior_digest, prior_receipt)) => {
                    if prior_digest != &request.body.effect_request_digest {
                        return Err("receiver identity replay changed stable effect digest".into());
                    }
                    prior_receipt.clone()
                }
                None => {
                    state.executions.fetch_add(1, Ordering::AcqRel);
                    ledger.insert(
                        request.body.delivery_key.clone(),
                        (request.body.effect_request_digest.clone(), receipt.clone()),
                    );
                    receipt
                }
            };
            drop(ledger);
            if drop_first_post_reply && !state.dropped_first_post_reply.swap(true, Ordering::AcqRel)
            {
                return Ok(());
            }
            EffectResponseOutcomeV1::Delivered {
                receipt_payload: stored,
            }
        }
        "query" => {
            state.queries.fetch_add(1, Ordering::AcqRel);
            let ledger = state
                .ledger
                .lock()
                .map_err(|_| "effect test receiver ledger lock poisoned")?;
            match ledger.get(&request.body.delivery_key) {
                Some((prior_digest, receipt)) => {
                    if prior_digest != &request.body.effect_request_digest {
                        return Err("receiver query stable effect digest mismatch".into());
                    }
                    EffectResponseOutcomeV1::Delivered {
                        receipt_payload: receipt.clone(),
                    }
                }
                None => EffectResponseOutcomeV1::NotFound { complete: true },
            }
        }
        _ => return Err("unsupported effect fixture operation".into()),
    };

    let response = sign_response(
        receiver,
        EffectResponseBodyV1 {
            schema_version: 1,
            receiver_id: contract.receiver_id.clone(),
            contract_digest: contract_digest.to_owned(),
            operation: request.body.operation,
            response_to_digest: request_rpc_digest,
            effect_request_digest: request.body.effect_request_digest,
            identity: request.body.identity,
            outcome,
        },
    )?;
    let bytes = serde_cbor::to_vec(&response)
        .map_err(|error| format!("encode signed test receiver response: {error}"))?;
    if bytes.is_empty() || bytes.len() > MAX_SIGNED_REPLY_BYTES {
        return Err("signed test receiver response exceeds size bound".into());
    }
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/cbor\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(&bytes))
        .map_err(|error| format!("write signed test receiver response: {error}"))?;
    Ok(())
}

fn validate_request(
    request: &SignedEffectRequestV1,
    contract: &FormalEffectReceiverContractV1,
    contract_digest: &str,
) -> Result<(), String> {
    let body = &request.body;
    if !contract
        .authorized_sender_public_keys_hex
        .contains(&request.sender_public_key_hex)
        || body.schema_version != 1
        || body.receiver_id != contract.receiver_id
        || body.contract_digest != contract_digest
        || body.target_id != contract.target_id
        || body.delivery_key != delivery_identity_key(&body.identity)?
        || !valid_digest(&body.effect_request_digest)
    {
        return Err("effect test receiver request binding rejected".into());
    }
    let key = decode_verifying_key(&request.sender_public_key_hex)?;
    let signature = decode_signature(&request.signature_hex)?;
    let signed = serde_cbor::to_vec(&(REQUEST_DOMAIN, &request.body))
        .map_err(|error| format!("encode test receiver request signature: {error}"))?;
    key.verify_strict(&signed, &signature)
        .map_err(|_| "effect test receiver sender signature invalid".to_owned())?;

    match body.operation.as_str() {
        "deliver" => {
            let intent = body
                .intent
                .as_ref()
                .ok_or("delivery request omitted typed intent")?;
            let committed_at = body
                .committed_at
                .as_ref()
                .ok_or("delivery request omitted qualified provenance")?;
            if intent.intent_id != body.identity.intent_id
                || intent.birth_era != Some(body.identity.birth_era)
                || intent
                    .payload_digest()
                    .map_err(|error| format!("{error:?}"))?
                    != body.payload_digest
            {
                return Err("effect test receiver intent identity or digest mismatch".into());
            }
            let params = intent
                .params
                .as_object()
                .ok_or("delivery-aware effect params are not an object")?;
            if params.len() != 4
                || params.get("schema_version").and_then(JsonValue::as_u64) != Some(1)
                || params.get("target_id").and_then(JsonValue::as_str)
                    != Some(contract.target_id.as_str())
                || params
                    .get("receiver_contract_digest")
                    .and_then(JsonValue::as_str)
                    != Some(contract_digest)
                || !params.contains_key("payload")
                || body.effect_request_digest
                    != stable_effect_request_digest_fields(
                        &body.identity,
                        intent,
                        &body.payload_digest,
                        &body.target_id,
                        contract_digest,
                    )?
            {
                return Err("effect test receiver committed route selectors mismatch".into());
            }
            if committed_at.position == 0 || committed_at.record_id.len() != 64 {
                return Err("effect test receiver provenance is invalid".into());
            }
        }
        "query" => {
            if !contract.complete_signed_query
                || body.intent.is_some()
                || body.committed_at.is_some()
            {
                return Err("effect test receiver query shape rejected".into());
            }
        }
        _ => return Err("effect test receiver operation is unsupported".into()),
    }
    Ok(())
}

fn sign_response(
    receiver: &SigningKey,
    body: EffectResponseBodyV1,
) -> Result<SignedEffectResponseV1, String> {
    let bytes = serde_cbor::to_vec(&(RESPONSE_DOMAIN, &body))
        .map_err(|error| format!("encode test receiver response signature: {error}"))?;
    Ok(SignedEffectResponseV1 {
        body,
        signature_hex: hex::encode(receiver.sign(&bytes).to_bytes()),
    })
}

fn read_bounded_http_request(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut headers = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    loop {
        if headers.len() >= MAX_HTTP_HEADER_BYTES {
            return Err("effect test receiver request headers exceed bound".into());
        }
        read_exact_before(stream, &mut byte, deadline)
            .map_err(|error| format!("read effect test receiver headers: {error}"))?;
        headers.push(byte[0]);
        if headers.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let text = std::str::from_utf8(&headers)
        .map_err(|_| "effect test receiver headers are not valid ASCII")?;
    let mut lines = text.split("\r\n");
    let request_line = lines.next();
    if !matches!(
        request_line,
        Some("POST /v1/effects/deliver HTTP/1.1") | Some("POST /v1/effects/query HTTP/1.1")
    ) {
        return Err("effect test receiver request line rejected".into());
    }
    let mut content_length = None;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or("malformed effect test receiver HTTP header")?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err("effect test receiver rejects transfer encoding".into());
        }
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err("duplicate effect test receiver content length".into());
            }
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "invalid effect test receiver content length")?,
            );
        }
    }
    let length = content_length.ok_or("effect test receiver content length missing")?;
    if length == 0 || length > MAX_HTTP_BODY_BYTES {
        return Err("effect test receiver request body exceeds bound".into());
    }
    let mut body = vec![0u8; length];
    read_exact_before(stream, &mut body, deadline)
        .map_err(|error| format!("read effect test receiver body: {error}"))?;
    Ok(body)
}

fn read_exact_before(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    deadline: Instant,
) -> io::Result<()> {
    let mut read = 0;
    while read < buffer.len() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "fixture I/O deadline",
            ));
        }
        stream.set_read_timeout(Some(remaining.min(Duration::from_millis(100))))?;
        match stream.read(&mut buffer[read..]) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "peer closed")),
            Ok(count) => read += count,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn set_last_error(state: &ReceiverState, error: String) {
    if let Ok(mut current) = state.last_error.lock() {
        *current = Some(error);
    }
}
