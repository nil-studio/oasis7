use super::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};
const LIMIT: usize = 64 * 1024 * 1024;
fn decode_native_batch<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Vec<super::super::NativeEnvironmentChunkBytesV1>, D::Error> {
    struct Batch;
    impl<'de> serde::de::Visitor<'de> for Batch {
        type Value = Vec<super::super::NativeEnvironmentChunkBytesV1>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("at most eight native chunks")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|length| length > 8) {
                return Err(serde::de::Error::custom("native batch count bound"));
            }
            let mut chunks = Vec::new();
            while let Some(chunk) = sequence.next_element()? {
                if chunks.len() == 8 {
                    return Err(serde::de::Error::custom("native batch count bound"));
                }
                chunks.push(chunk);
            }
            Ok(chunks)
        }
    }
    decoder.deserialize_seq(Batch)
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum Command {
    StageNativeEnvironment(
        Box<super::super::native_environment::NativeExecutionEnvironmentV1>,
        #[serde(deserialize_with = "decode_native_batch")]
        Vec<super::super::NativeEnvironmentChunkBytesV1>,
    ),
    VerifyNativeEnvironment(Box<super::super::native_environment::NativeExecutionEnvironmentV1>),
    ReleaseNativeEnvironmentVerification {
        descriptor_digest: String,
    },
    Head,
    Query(String),
    Freeze(Box<SignedFormalFrozenV1>, Box<ClosedRecord>),
    CompleteFrozen(Box<FormalFrozenCertificateV1>),
    InstallRecoveryFrozen(Box<FormalFrozenCertificateV1>, Box<ClosedRecord>),
    Prepare(Box<FormalCandidateMaterialV1>),
    Decide(
        String,
        Box<FormalPrepareReceiptV1>,
        Box<FormalPrepareReceiptV1>,
    ),
    InstallProof(Box<FormalDecisionProofV1>),
    PublishQualified(Box<FormalQualifiedCommitProofV1>),
    VerifyPublishedClosure {
        request: String,
        candidate_digest: String,
    },
    ReleasePublishedClosure {
        candidate_digest: String,
    },
    ArchiveManifest,
    ArchiveObject(String),
    ArchiveChunk {
        id: String,
        offset: u64,
        limit: u32,
    },
    Seal {
        operation: String,
        next_epoch: u64,
        checkpoint: fencing::EndpointCheckpointBindingV1,
    },
    SealDurableSet {
        transition: Box<SignedNormalDurableSetTransitionV1>,
        checkpoint: fencing::EndpointCheckpointBindingV1,
    },
    QueryDurableSet(Box<SignedNormalDurableSetTransitionV1>),
    BusinessIndexPath {
        root: super::super::business_index::BusinessIndexRootV1,
        key_digest: String,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestBody {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub authority_id: String,
    pub instance_digest: String,
    pub challenge: String,
    pub minimum: FormalHeadRefV1,
    pub command: Command,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_read_grant: Option<SignedArchiveReadGrantV1>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub body: RequestBody,
    pub signature: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum Value {
    Complete,
    Query(Box<FormalQueryOutcomeV1>),
    Frozen(Box<FormalFrozenReceiptV1>),
    Prepare(Box<FormalPrepareReceiptV1>),
    Final(Box<FormalFinalReceiptV1>),
    ProofComplete(Box<FormalProofCompleteReceiptV1>),
    NativeEnvironment(super::super::NativeEnvironmentVerificationProgressV1),
    PublishedClosure(super::super::coordinator::FormalPublishedClosureReceiptV1),
    ArchiveManifest(Box<SignedFormalArchiveTransferV1>),
    ArchiveObject(Option<Vec<u8>>),
    ArchiveChunk(Option<FormalArchiveChunkV1>),
    Seal(Box<fencing::EpochSealReceiptV1>),
    DurableSetSeal(Option<Box<fencing::EpochSealReceiptV1>>),
    BusinessIndexPath(Box<super::super::business_index::BusinessIndexPathV1>),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResponseBody {
    pub schema_version: u32,
    pub request_digest: String,
    pub challenge: String,
    pub world: FormalWorldIdentityV1,
    pub authority_id: String,
    pub instance_digest: String,
    pub minimum: FormalHeadRefV1,
    pub head: FormalHeadRefV1,
    pub checkpoint: fencing::EndpointCheckpointBindingV1,
    pub checkpoint_bytes: Vec<u8>,
    pub ancestry: Vec<Vec<u8>>,
    pub provider: fencing::MonotonicProviderEvidenceV1,
    pub result: Result<Value, FormalError>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Response {
    pub body: ResponseBody,
    pub signature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_signature: Option<fencing::ProtectedEndpointSignatureV1>,
}
pub(super) fn address(address: SocketAddr) -> Result<(), FormalError> {
    if !address.ip().is_loopback() {
        return Err(invalid(
            "formal RPC requires authenticated encrypted-tunnel loopback termination",
        ));
    }
    Ok(())
}
pub(super) fn configure(stream: &TcpStream, timeout: Duration) -> Result<(), FormalError> {
    if timeout.is_zero() || timeout > Duration::from_secs(60) {
        return Err(invalid("formal RPC timeout in (0,60s]"));
    }
    stream.set_nonblocking(false).map_err(io)?;
    // A frame's length and CBOR body are separate writes. Avoid delaying the
    // body behind the small length packet during multi-object cold restore.
    stream.set_nodelay(true).map_err(io)?;
    stream.set_read_timeout(Some(timeout)).map_err(io)?;
    stream.set_write_timeout(Some(timeout)).map_err(io)
}
pub(super) fn io(error: impl std::fmt::Display) -> FormalError {
    FormalError::Io(error.to_string())
}
pub(super) fn write<T: Serialize>(stream: &mut TcpStream, value: &T) -> Result<(), FormalError> {
    let bytes = serde_cbor::to_vec(value).map_err(|e| invalid(e.to_string()))?;
    if bytes.len() > LIMIT {
        return Err(invalid("formal RPC bounded frame"));
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(&bytes))
        .map_err(io)
}
/// Explicit server-side admission of the original transport *frame* before
/// allocating the 64 MiB raw body. This deliberately does not certify the
/// subsequently decoded Request DTO, its signing, or an accepted GEN result.
///
/// One server-owned Ordinary pool and one monotonic Work meter are supplied at
/// process startup, never minted or reset per RPC. The retained frame fee stays
/// alive while the caller authenticates and signs its response.
/// Every physical network read attempt consumes the same original Work.
/// Short reads and Interrupted retries never evade the step/byte counter.
fn read_exact_prepaid(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    work: &super::super::endpoint::FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    while !bytes.is_empty() {
        work.charge(1, bytes.len() as u64, cancelled)?;
        match stream.read(bytes) {
            Ok(0) => return Err(io("formal RPC frame ended early")),
            Ok(n) => bytes = &mut bytes[n..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io(error)),
        }
    }
    Ok(())
}
pub(super) fn read_prepaid_frame<T: serde::de::DeserializeOwned>(
    stream: &mut TcpStream,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    work: &super::super::endpoint::FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<(T, oasis7_proto::memory_budget::ReservedValue<Vec<u8>>), FormalError> {
    use oasis7_proto::memory_budget::MemoryClass;
    let mut length = [0u8; 4];
    read_exact_prepaid(stream, &mut length, work, cancelled)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > LIMIT {
        return Err(invalid("formal RPC bounded frame"));
    }
    // Charge the full raw allocation before creating it. The allocator
    // capacity slack and the Vec header are counted independently.
    let layout = length
        .checked_add(std::mem::size_of::<Vec<u8>>())
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| invalid("formal RPC frame heap layout"))?;
    let fee = memory
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(|_| invalid("formal RPC frame heap admission"))?;
    let mut bytes = vec![0; length];
    if bytes.capacity() > length + 4096 {
        return Err(invalid("formal RPC frame allocation capacity"));
    }
    for chunk in bytes.chunks_mut(8192) {
        read_exact_prepaid(stream, chunk, work, cancelled)?;
    }
    let paid = fee.retain(bytes);
    work.consume_steps(1, cancelled)?;
    let request = serde_cbor::from_slice(paid.as_slice())
        .map_err(|_| invalid("formal RPC admitted frame decode"))?;
    Ok((request, paid))
}

pub(super) fn read<T: serde::de::DeserializeOwned>(
    stream: &mut TcpStream,
) -> Result<T, FormalError> {
    let mut length = [0; 4];
    stream.read_exact(&mut length).map_err(io)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > LIMIT {
        return Err(invalid("formal RPC bounded frame"));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).map_err(io)?;
    serde_cbor::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
}

fn remaining(deadline: Instant) -> Result<Duration, FormalError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|time| !time.is_zero())
        .ok_or_else(|| io("formal RPC frame deadline expired"))
}
fn read_exact_before(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    deadline: Instant,
) -> Result<(), FormalError> {
    while !bytes.is_empty() {
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(io)?;
        let count = stream.read(bytes).map_err(io)?;
        if count == 0 {
            return Err(io("formal RPC frame ended early"));
        }
        bytes = &mut bytes[count..];
    }
    Ok(())
}
pub(super) fn read_before_deadline<T: serde::de::DeserializeOwned>(
    stream: &mut TcpStream,
    deadline: Instant,
) -> Result<T, FormalError> {
    let mut length = [0; 4];
    read_exact_before(stream, &mut length, deadline)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > LIMIT {
        return Err(invalid("formal RPC bounded frame"));
    }
    let mut bytes = vec![0; length];
    read_exact_before(stream, &mut bytes, deadline)?;
    remaining(deadline)?;
    serde_cbor::from_slice(&bytes).map_err(|error| invalid(error.to_string()))
}
#[cfg(test)]
pub(super) fn write_before_deadline<T: Serialize>(
    stream: &mut TcpStream,
    value: &T,
    deadline: Instant,
) -> Result<(), FormalError> {
    let bytes = prepare_before_deadline(value, deadline)?;
    write_prepared_before_deadline(stream, &bytes, deadline)
}

fn prepare_before_deadline<T: Serialize>(
    value: &T,
    deadline: Instant,
) -> Result<Vec<u8>, FormalError> {
    remaining(deadline)?;
    let bytes = serde_cbor::to_vec(value).map_err(|error| invalid(error.to_string()))?;
    if bytes.len() > LIMIT {
        return Err(invalid("formal RPC bounded frame"));
    }
    remaining(deadline)?;
    Ok(bytes)
}

fn write_prepared_before_deadline(
    stream: &mut TcpStream,
    bytes: &[u8],
    deadline: Instant,
) -> Result<(), FormalError> {
    let header = (bytes.len() as u32).to_be_bytes();
    for mut remaining_bytes in [&header[..], bytes] {
        while !remaining_bytes.is_empty() {
            stream
                .set_write_timeout(Some(remaining(deadline)?))
                .map_err(io)?;
            let count = stream.write(remaining_bytes).map_err(io)?;
            if count == 0 {
                return Err(io("formal RPC frame write ended early"));
            }
            remaining_bytes = &remaining_bytes[count..];
        }
    }
    Ok(())
}

pub(super) fn connect_and_write_before_deadline<T: Serialize>(
    address: &SocketAddr,
    value: &T,
    deadline: Instant,
) -> Result<TcpStream, FormalError> {
    connect_and_write_using(address, value, deadline, TcpStream::connect_timeout)
}

fn connect_and_write_using<T: Serialize>(
    address: &SocketAddr,
    value: &T,
    deadline: Instant,
    connect: impl FnOnce(&SocketAddr, Duration) -> std::io::Result<TcpStream>,
) -> Result<TcpStream, FormalError> {
    // Finish local encoding before the peer begins its first-read timeout.
    // The caller's one absolute deadline includes this preparation.
    let bytes = prepare_before_deadline(value, deadline)?;
    let mut stream = connect(address, remaining(deadline)?).map_err(io)?;
    configure(&stream, remaining(deadline)?)?;
    write_prepared_before_deadline(&mut stream, &bytes, deadline)?;
    Ok(stream)
}

#[cfg(test)]
#[path = "wire_encode_order_tests.rs"]
mod encode_order_tests;

#[cfg(test)]
mod deadline_tests {
    use super::*;
    #[test]
    fn formal_rpc_frame_deadline_is_not_reset_by_slow_fragments() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let sender = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let payload = serde_cbor::to_vec(&serde_cbor::Value::Bytes(vec![7; 16])).unwrap();
            let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
            frame.extend(payload);
            for byte in frame {
                if stream.write_all(&[byte]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        });
        let mut stream = TcpStream::connect(address).unwrap();
        let result = read_before_deadline::<serde_cbor::Value>(
            &mut stream,
            Instant::now() + Duration::from_millis(60),
        );
        assert!(
            result.is_err(),
            "a slow peer must not renew the absolute frame deadline"
        );
        drop(stream);
        sender.join().unwrap();
        prepaid_frame_is_charged_before_allocation_and_retained_across_decode();
        prepaid_frame_work_cannot_reset_between_connections();
    }

    fn prepaid_frame_is_charged_before_allocation_and_retained_across_decode() {
        use oasis7_proto::memory_budget::MemoryBudget;
        let wire = serde_cbor::to_vec(&42u64).unwrap();
        for capacity in [0, 1 << 20] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let bytes = wire.clone();
            let sender = std::thread::spawn(move || {
                let (mut peer, _) = listener.accept().unwrap();
                let _ = peer.write_all(&(bytes.len() as u32).to_be_bytes());
                let _ = peer.write_all(&bytes);
            });
            let mut client = TcpStream::connect(addr).unwrap();
            configure(&client, Duration::from_secs(2)).unwrap();
            let memory = MemoryBudget::new(capacity, 0).unwrap();
            let work =
                super::super::super::endpoint::FormalArchiveReadWorkV1::new(100, 4096).unwrap();
            let original_steps = work.remaining_steps();
            let result: Result<_, FormalError> =
                read_prepaid_frame::<u64>(&mut client, &memory, &work, &mut || false);
            if capacity == 0 {
                assert!(result.unwrap_err().to_string().contains("heap admission"));
                assert_eq!(memory.used(), 0);
            } else {
                let (value, owner) = result.unwrap();
                assert_eq!(value, 42);
                assert!(owner.uses_budget(&memory));
                assert!(memory.used() > 0);
                drop(owner);
                assert_eq!(memory.used(), 0);
            }
            assert!(work.remaining_steps() < original_steps);
            sender.join().unwrap();
        }
    }
    fn prepaid_frame_work_cannot_reset_between_connections() {
        use oasis7_proto::memory_budget::MemoryBudget;
        let bytes = serde_cbor::to_vec(&77u64).unwrap();
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let work =
            super::super::super::endpoint::FormalArchiveReadWorkV1::new(100, 1024).unwrap();
        for attempt in 0..2 {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let frame = bytes.clone();
            let sender = std::thread::spawn(move || {
                let (mut peer, _) = listener.accept().unwrap();
                let _ = peer.write_all(&(frame.len() as u32).to_be_bytes());
                let _ = peer.write_all(&frame);
            });
            let mut stream = TcpStream::connect(addr).unwrap();
            configure(&stream, Duration::from_secs(2)).unwrap();
            let result = read_prepaid_frame::<u64>(&mut stream, &memory, &work, &mut || false);
            if attempt == 0 {
                let (value, retained) = result.unwrap();
                assert_eq!(value, 77);
                assert!(retained.uses_budget(&memory));
                drop(retained);
                // Exhaust the *same* original meter after the first successful
                // frame, independent of physical TCP short-read boundaries.
                assert!(work.remaining_read_bytes() < 1024);
                let remaining = work.remaining_read_bytes();
                work.consume_read_bytes(remaining, &mut || false).unwrap();
            } else {
                assert!(result.is_err(), "second RPC must not renew work quota");
            }
            assert_eq!(memory.used(), 0);
            sender.join().unwrap();
        }
        assert_eq!(work.remaining_read_bytes(), 0);
    }
}

#[cfg(test)]
mod native_batch_tests {
    use super::*;
    #[test]
    fn native_stage_decoder_rejects_oversized_declared_batch_before_chunk_values() {
        // CBOR externally tagged enum map: StageNativeEnvironment:[descriptor,batch].
        let descriptor = super::super::super::native_environment::NativeExecutionEnvironmentV1 {
            schema_version: 1,
            target: "test".into(),
            os: "linux".into(),
            arch: "test".into(),
            components: vec![],
        };
        let chunk = super::super::super::NativeEnvironmentChunkBytesV1 {
            digest: "ab".repeat(32),
            bytes: vec![1],
        };
        let value = Command::StageNativeEnvironment(Box::new(descriptor), vec![chunk; 9]);
        let bytes = serde_cbor::to_vec(&value).unwrap();
        let error = serde_cbor::from_slice::<Command>(&bytes)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("native batch count bound"));
        let mut hostile = bytes;
        // The final 9-item array is preceded by its declaration; replacing it
        // with an impossible count must hit the same count gate, not allocation.
        let position = hostile
            .windows(2)
            .position(|pair| pair == [0x89, 0xa2])
            .unwrap();
        hostile.splice(
            position..position + 1,
            [0x9b, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
        );
        assert!(serde_cbor::from_slice::<Command>(&hostile).is_err());
    }
}
