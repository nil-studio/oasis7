//! Transparent actual-wire oracle for the Ready-only cold test's metadata fees.
//! This does not use the service's response-bound or mutation helpers.
use super::*;
use serde_cbor::Value;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Clone)]
struct ChargedReply {
    actual_wire: u64,
    counter_max_bound: u64,
    counter: u64,
    exact_proof_frame: bool,
}
#[derive(Clone, Default)]
pub(super) struct Snapshot {
    gets: usize,
    proof_reads: usize,
    replies: Vec<ChargedReply>,
}
pub(super) struct ExpectedProof {
    operation: String,
    revision: u64,
    state_digest: String,
    bytes: Vec<u8>,
    object: Value,
}
impl ExpectedProof {
    pub(super) fn new(
        operation: String,
        revision: u64,
        state_digest: String,
        bytes: Vec<u8>,
    ) -> Self {
        assert!(!bytes.is_empty() && bytes.len() <= BFT_SIGNER_MAX_FRAME_BYTES_V3);
        let object = serde_cbor::value::to_value(std::collections::BTreeMap::from([
            (
                "digest",
                Value::Text(blake3::hash(&bytes).to_hex().to_string()),
            ),
            ("bytes", Value::Integer((bytes.len() as u64).into())),
        ]))
        .unwrap();
        Self {
            operation,
            revision,
            state_digest,
            bytes,
            object,
        }
    }
    fn count(&self) -> usize {
        self.bytes
            .len()
            .div_ceil(oasis7_distfs::protected_bft_v3::BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1)
    }
    fn read(&self, part: usize) -> Value {
        let offset =
            part * oasis7_distfs::protected_bft_v3::BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1;
        serde_cbor::value::to_value(std::collections::BTreeMap::from([
            (
                "target",
                serde_cbor::value::to_value(std::collections::BTreeMap::from([
                    ("kind", "SelectionOperation"),
                    ("digest", self.operation.as_str()),
                ]))
                .unwrap(),
            ),
            ("revision", Value::Integer(self.revision.into())),
            ("state_digest", Value::Text(self.state_digest.clone())),
            (
                "object",
                if part == 0 {
                    Value::Null
                } else {
                    self.object.clone()
                },
            ),
            ("offset", Value::Integer((offset as u64).into())),
        ]))
        .unwrap()
    }
    fn check_chunk(&self, chunk: &Value, part: usize) -> Result<(), String> {
        let width = oasis7_distfs::protected_bft_v3::BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1;
        let offset = part
            .checked_mul(width)
            .ok_or("cold proof offset overflow")?;
        let end = offset
            .checked_add(width)
            .ok_or("cold proof range overflow")?
            .min(self.bytes.len());
        if field(chunk, "read")? != &self.read(part)
            || field(chunk, "object")? != &self.object
            || field(chunk, "bytes")?
                != &serde_cbor::value::to_value(&self.bytes[offset..end])
                    .map_err(|e| e.to_string())?
        {
            return Err("cold original proof chunk differs from exact backend original".into());
        }
        Ok(())
    }
}
pub(super) struct Relay {
    address: SocketAddr,
    observed: Arc<Mutex<Snapshot>>,
    stopping: Arc<AtomicBool>,
    sockets: Arc<Mutex<Vec<TcpStream>>>,
    thread: Option<thread::JoinHandle<Result<(), String>>>,
    proof: Arc<OnceLock<ExpectedProof>>,
}
impl Relay {
    pub(super) fn start(service: SocketAddr, deadline: Instant) -> Self {
        let proof: Arc<OnceLock<ExpectedProof>> = Arc::new(OnceLock::new());
        let expected_proof = proof.clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let observed = Arc::new(Mutex::new(Snapshot::default()));
        let stopping = Arc::new(AtomicBool::new(false));
        let sockets = Arc::new(Mutex::new(Vec::new()));
        let (meter, stopped, controls) = (observed.clone(), stopping.clone(), sockets.clone());
        let thread = thread::spawn(move || {
            // Same test-only listener accept bound as the existing tcp helper.
            let accept_deadline = deadline.min(Instant::now() + Duration::from_secs(60));
            let mut front = loop {
                if stopped.load(Ordering::Acquire) {
                    return Ok(());
                }
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= accept_deadline {
                            return Err("cold meter client never connected".into());
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => return Err(e.to_string()),
                }
            };
            // Publish the accepted socket before connect, and recheck after
            // every publication: Drop may already have observed an empty list.
            *controls.lock().unwrap() = vec![front.try_clone().map_err(|e| e.to_string())?];
            check(deadline, &stopped)?;
            let mut back = TcpStream::connect_timeout(&service, remaining(deadline, &stopped)?)
                .map_err(|e| e.to_string())?;
            *controls.lock().unwrap() = vec![
                front.try_clone().map_err(|e| e.to_string())?,
                back.try_clone().map_err(|e| e.to_string())?,
            ];
            check(deadline, &stopped)?;
            loop {
                let Some(request) = read_frame(&mut front, deadline, &stopped)? else {
                    return Ok(());
                };
                let value = canonical(&request)?;
                let command = text(field(field(&value, "body")?, "command")?, "kind")?;
                // This test has no timer/signing/admission or other archive read.
                if !matches!(
                    command,
                    "PrepareAcquire"
                        | "Acquire"
                        | "Checkpoint"
                        | "ProtectedTime"
                        | "GetPrivateRetention"
                        | "ReadPrivateRetentionProof"
                ) {
                    return Err(format!("unexpected cold request family: {command}"));
                }
                if command == "GetPrivateRetention" {
                    let mut observed = meter.lock().unwrap();
                    if observed.gets >= 2 {
                        return Err(
                            "third cold metadata request/reply rejected before provider receive"
                                .into(),
                        );
                    }
                    observed.gets += 1;
                }
                let proof_part = if command == "ReadPrivateRetentionProof" {
                    let mut observed = meter.lock().unwrap();
                    let part = observed.proof_reads;
                    let proof = expected_proof
                        .get()
                        .ok_or("cold proof oracle not configured after acquire")?;
                    if part >= proof.count()
                        || field(field(field(&value, "body")?, "command")?, "body")
                            .and_then(|body| field(body, "read"))?
                            != &proof.read(part)
                    {
                        return Err("unexpected extra/foreign cold original proof request".into());
                    }
                    observed.proof_reads += 1;
                    Some(part)
                } else {
                    None
                };
                write_frame(&mut back, &request, deadline, &stopped)?;
                let reply = read_frame(&mut back, deadline, &stopped)?
                    .ok_or("provider closed before cold signed reply")?;
                let mut value = canonical(&reply)?;
                let kind = text(field(field(&value, "body")?, "result")?, "kind")?.to_owned();
                let charged = if kind == "PrivateRetention" || kind == "PrivateRetentionProofChunk"
                {
                    if (kind == "PrivateRetention" && command != "GetPrivateRetention")
                        || (kind == "PrivateRetentionProofChunk" && proof_part.is_none())
                    {
                        return Err("cold metadata reply did not match GetPrivateRetention".into());
                    }
                    let signature = text(&value, "provider_signature_hex")?;
                    if signature.len() != 128 {
                        return Err("cold signed reply signature width differs".into());
                    }
                    let payload =
                        field_mut(field_mut(field_mut(&mut value, "body")?, "result")?, "body")?;
                    let payload = if let Some(part) = proof_part {
                        let chunk = field_mut(payload, "chunk")?;
                        expected_proof
                            .get()
                            .ok_or("cold proof oracle absent")?
                            .check_chunk(chunk, part)?;
                        chunk
                    } else {
                        payload
                    };
                    let counter_value = field_mut(payload, "reserved_response_bytes")?;
                    let Value::Integer(counter) = counter_value else {
                        return Err("cold metadata counter type".into());
                    };
                    let counter =
                        u64::try_from(*counter).map_err(|_| "cold metadata counter range")?;
                    *counter_value = Value::Integer(u64::MAX.into());
                    // Independent reconstruction from the actual signed wire
                    // shape, with the protocol's maximum counter width only.
                    // Signature values have fixed 128-hex width; no signature
                    // or response is forged or forwarded from this reconstruction.
                    let bound = oasis7_cas::to_canonical_cbor(&value)
                        .map_err(|_| "cold canonical reply encoding failed")?
                        .len()
                        .checked_add(4)
                        .ok_or("cold frame bound overflow")?;
                    if bound
                        > BFT_SIGNER_MAX_FRAME_BYTES_V3
                            .checked_add(4)
                            .ok_or("cold maximum frame overflow")?
                    {
                        return Err("cold counterMAX reply exceeds original frame bound".into());
                    }
                    Some(ChargedReply {
                        actual_wire: reply.len() as u64,
                        counter_max_bound: bound as u64,
                        counter,
                        exact_proof_frame: proof_part.is_some(),
                    })
                } else {
                    if matches!(command, "GetPrivateRetention" | "ReadPrivateRetentionProof")
                        || matches!(kind.as_str(), "ArchiveChunk" | "PrivateRetentionProofChunk")
                    {
                        return Err("unexpected cold charged/rejected reply family".into());
                    }
                    None
                };
                if let Some(charged) = charged {
                    let mut observed = meter.lock().unwrap();
                    if observed.replies.len()
                        >= 2 + expected_proof.get().map_or(0, ExpectedProof::count)
                    {
                        return Err("third cold metadata reply rejected before release".into());
                    }
                    observed.replies.push(charged);
                }
                // Always forward original complete frame bytes unchanged. The
                // actual client independently checks every provider signature.
                write_frame(&mut front, &reply, deadline, &stopped)?;
            }
        });
        Self {
            address,
            observed,
            stopping,
            sockets,
            thread: Some(thread),
            proof,
        }
    }
    pub(super) fn configure_proof(&self, proof: ExpectedProof) {
        assert!(
            self.proof.set(proof).is_ok(),
            "cold proof oracle configured twice"
        );
    }
    pub(super) fn address(&self) -> SocketAddr {
        self.address
    }
    pub(super) fn snapshot(&self) -> Snapshot {
        self.observed.lock().unwrap().clone()
    }
    pub(super) fn assert_two_observations(
        &self,
        baseline: &Snapshot,
        before: &BftSuccessorArchiveTransferLedgerV1,
        after: &BftSuccessorArchiveTransferLedgerV1,
    ) {
        let actual = self.snapshot();
        assert_eq!(
            baseline.gets, 0,
            "acquire must not conceal a retention read"
        );
        assert!(baseline.replies.is_empty());
        assert_eq!(actual.gets.checked_sub(baseline.gets), Some(2));
        let replies = &actual.replies[baseline.replies.len()..];
        assert_eq!(baseline.proof_reads, 0);
        assert_eq!(
            actual.proof_reads,
            self.proof
                .get()
                .expect("cold proof oracle configured")
                .count()
        );
        assert_eq!(
            replies.len(),
            2 + self
                .proof
                .get()
                .expect("cold proof oracle configured")
                .count()
        );
        let mut expected = *before;
        for reply in replies {
            let actual_counter_width = match reply.counter {
                0..=23 => 1,
                24..=255 => 2,
                256..=65535 => 3,
                65536..=4294967295 => 5,
                _ => 9,
            };
            assert_eq!(
                reply.counter_max_bound.checked_sub(reply.actual_wire),
                Some(9 - actual_counter_width),
                "only the canonical counter's encoded width may differ from actual signed wire bytes"
            );
            // Get retains its historical counterMAX upper-bound contract.
            // Proof responses charge the final actual signed frame (including
            // the four-byte header), whose counter is already authenticated by
            // the real client. Derive this from captured bytes, independently
            // of the production fixed-point/reservation helper.
            let charged = if reply.exact_proof_frame {
                reply.actual_wire
            } else {
                reply.counter_max_bound
            };
            expected.reserved_response_bytes = expected
                .reserved_response_bytes
                .checked_add(charged)
                .unwrap();
            assert_eq!(
                reply.counter, expected.reserved_response_bytes,
                "signed counter differs from the exact response-family contract"
            );
            eprintln!(
                "cold actual metadata exact-proof={} charged={} wire={} counterMAX-bound={} difference={}",
                reply.exact_proof_frame,
                charged,
                reply.actual_wire,
                reply.counter_max_bound,
                reply
                    .counter_max_bound
                    .checked_sub(reply.actual_wire)
                    .unwrap()
            );
        }
        assert_eq!(
            &expected, after,
            "cold transfer delta must be exactly two Get counterMAX bounds plus one logical proof's actual signed frames"
        );
    }
    pub(super) fn finish(mut self) -> Result<(), String> {
        let result = self
            .thread
            .take()
            .unwrap()
            .join()
            .map_err(|_| "cold relay thread panicked")?;
        result
    }
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        // Panic/setup failure cleanup must not retain an idle test relay or its
        // service socket. Normal completion explicitly checks the join result.
        if self.thread.is_some() {
            for stream in self
                .sockets
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
            {
                let _ = stream.shutdown(Shutdown::Both);
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}
fn remaining(deadline: Instant, stopped: &AtomicBool) -> Result<Duration, String> {
    if stopped.load(Ordering::Acquire) {
        return Err("cold relay cancelled".into());
    }
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or_else(|| "cold relay original deadline expired".into())
}
fn check(deadline: Instant, stopped: &AtomicBool) -> Result<(), String> {
    remaining(deadline, stopped).map(|_| ())
}
fn retryable(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::Interrupted
    )
}
fn read_piece(
    stream: &mut TcpStream,
    bytes: &mut [u8],
    deadline: Instant,
    stopped: &AtomicBool,
) -> Result<usize, String> {
    loop {
        let left = remaining(deadline, stopped)?;
        stream
            .set_read_timeout(Some(left.min(Duration::from_millis(10))))
            .map_err(|e| e.to_string())?;
        match stream.read(bytes) {
            Ok(count) => {
                check(deadline, stopped)?;
                return Ok(count);
            }
            Err(error) if retryable(&error) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
}
fn read_exact(
    stream: &mut TcpStream,
    bytes: &mut [u8],
    deadline: Instant,
    stopped: &AtomicBool,
) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        let count = read_piece(stream, &mut bytes[offset..], deadline, stopped)?;
        if count == 0 {
            return Err("cold relay partial frame EOF".into());
        }
        offset += count;
    }
    Ok(())
}
fn write_frame(
    stream: &mut TcpStream,
    bytes: &[u8],
    deadline: Instant,
    stopped: &AtomicBool,
) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        let left = remaining(deadline, stopped)?;
        stream
            .set_write_timeout(Some(left.min(Duration::from_millis(10))))
            .map_err(|e| e.to_string())?;
        match stream.write(&bytes[offset..]) {
            Ok(0) => return Err("cold relay write closed".into()),
            Ok(count) => {
                check(deadline, stopped)?;
                offset += count;
            }
            Err(error) if retryable(&error) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}
fn read_frame(
    stream: &mut TcpStream,
    deadline: Instant,
    stopped: &AtomicBool,
) -> Result<Option<Vec<u8>>, String> {
    let mut prefix = [0; 4];
    match read_piece(stream, &mut prefix[..1], deadline, stopped)? {
        0 => return Ok(None),
        1 => {}
        _ => unreachable!(),
    }
    read_exact(stream, &mut prefix[1..], deadline, stopped)?;
    let length =
        usize::try_from(u32::from_be_bytes(prefix)).map_err(|_| "cold frame length conversion")?;
    if length == 0 || length > BFT_SIGNER_MAX_FRAME_BYTES_V3 {
        return Err("cold actual frame outside original bound".into());
    }
    let mut frame = vec![0; length.checked_add(4).ok_or("cold frame size overflow")?];
    frame[..4].copy_from_slice(&prefix);
    read_exact(stream, &mut frame[4..], deadline, stopped)?;
    Ok(Some(frame))
}
fn canonical(frame: &[u8]) -> Result<Value, String> {
    let value: Value = serde_cbor::from_slice(&frame[4..]).map_err(|e| e.to_string())?;
    if oasis7_cas::to_canonical_cbor(&value).map_err(|_| "cold canonical frame encoding failed")?
        != frame[4..]
    {
        return Err("cold actual signed frame is not canonical".into());
    }
    Ok(value)
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, String> {
    let Value::Map(map) = value else {
        return Err("cold wire expected map".into());
    };
    map.get(&Value::Text(name.into()))
        .ok_or_else(|| format!("cold wire field missing: {name}"))
}
fn field_mut<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Value, String> {
    let Value::Map(map) = value else {
        return Err("cold wire expected mutable map".into());
    };
    map.get_mut(&Value::Text(name.into()))
        .ok_or_else(|| format!("cold wire field missing: {name}"))
}
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    let Value::Text(text) = field(value, name)? else {
        return Err("cold wire expected text".into());
    };
    Ok(text)
}
