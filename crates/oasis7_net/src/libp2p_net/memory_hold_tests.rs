use super::super::memory_hold::*;
use super::*;
use futures::{StreamExt, io::Cursor};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
}
fn request(pool: &MemoryPool, deadline: Instant) -> LocalRequest {
    LocalRequest {
        message: NetworkRequest {
            protocol: "/test/held".into(),
            payload: vec![42],
        },
        deadline: Some(deadline),
        exchange: None,
        response_resources: Default::default(),
        memory: Some(pool.rpc().unwrap()),
    }
}
const USED: [usize; 2] = [RPC_INBOUND_BYTES, RPC_OUTBOUND_BYTES];

fn response_bytes(payload: Vec<u8>) -> Vec<u8> {
    let mut wire = Vec::new();
    cbor::codec::Codec::<NetworkRequest, NetworkResponse>::default()
        .write_response(
            &StreamProtocol::new("/test/held"),
            &mut wire,
            NetworkResponse { payload },
        )
        .now_or_never()
        .unwrap()
        .unwrap();
    wire
}

#[test]
fn held_memory_follows_queue_codec_event_and_caller_without_legacy_escape() {
    runtime().block_on(async {
        let pool = MemoryPool::default();
        let (mut queue, mut receiver) = futures::channel::mpsc::channel(0);
        let command = || {
            let (response, caller) = std::sync::mpsc::channel();
            drop(caller); // The queued command still owns its allocation after caller cancellation.
            Command::RequestToPeer {
                protocol: "/test/held".into(),
                payload: vec![42],
                peer: libp2p::PeerId::random(),
                context: OutboundContext {
                    deadline: None,
                    exchange: None,
                    memory: Some(pool.rpc().unwrap()),
                },
                response,
            }
        };
        assert!(queue.try_send(command()).is_ok());
        let rejected = queue.try_send(command()).unwrap_err();
        assert_eq!(pool.used(), USED.map(|n| n * 2));
        drop(rejected);
        assert_eq!(pool.used(), USED);
        drop(receiver.next().await);
        assert_eq!(pool.used(), [0, 0]);
        drop(receiver);
        assert!(queue.try_send(command()).is_err());
        assert_eq!(pool.used(), [0, 0]);

        let protocol = StreamProtocol::new("/test/held");
        let mut codec = RequestCodec::default();
        codec
            .write_request(
                &protocol,
                &mut Cursor::new(Vec::new()),
                request(&pool, deadline_in(Duration::from_secs(5))),
            )
            .await
            .unwrap();
        let unrelated = codec.clone();
        assert!(unrelated.outbound_memory.is_none());
        let wire = response_bytes(vec![255; 64 * 1024]);
        let response = codec
            .read_response(&protocol, &mut Cursor::new(wire))
            .await
            .unwrap();
        assert_eq!(response.payload.capacity(), 64 * 1024);
        drop(codec); // The actual decoded Event can outlive its codec.
        let (completion, caller) = std::sync::mpsc::channel();
        completion.send(response).unwrap();
        assert_eq!(pool.used(), USED);
        let response = caller.recv().unwrap();
        assert_eq!(response.as_slice().len(), 64 * 1024);
        assert_eq!(pool.used(), USED);
        assert!(response.into_unheld().is_err());
        assert_eq!(pool.used(), [0, 0]);
        drop(unrelated);
    });
}

#[test]
fn held_stream_errors_partial_timeout_and_half_close_keep_original_owner() {
    runtime().block_on(async {
        let pool = MemoryPool::default();
        let protocol = StreamProtocol::new("/test/held");
        let mut codec = RequestCodec::default();
        let mut partial = super::tests::PartialWriter::default();
        assert!(
            codec
                .write_request(
                    &protocol,
                    &mut partial,
                    request(&pool, deadline_in(Duration::from_millis(20)))
                )
                .await
                .is_err()
        );
        assert_eq!(partial.bytes, 1);
        assert_eq!(partial.close_calls, 0);
        assert_eq!(pool.used(), USED);
        let retry = pool.rpc().unwrap();
        assert_eq!(pool.used(), USED.map(|n| n * 2));
        drop(codec);
        drop(retry);
        assert_eq!(pool.used(), [0, 0]);
        for fail in [false, true] {
            let mut codec = RequestCodec::default();
            let mut writer = super::tests::ResponseWriter {
                bytes: Vec::new(),
                drops: Default::default(),
                close_ready: false,
                fail,
            };
            let result = codec
                .write_request(
                    &protocol,
                    &mut writer,
                    request(&pool, deadline_in(Duration::from_secs(5))),
                )
                .now_or_never();
            if fail {
                assert!(result.unwrap().is_err());
            } else {
                assert!(result.is_none());
            }
            assert_eq!(pool.used(), USED);
            drop(codec);
            assert_eq!(pool.used(), [0, 0]);
        }
        let mut codec = RequestCodec::default();
        let mut writer = Cursor::new(Vec::new());
        assert!(
            codec
                .write_request(&protocol, &mut writer, request(&pool, Instant::now()))
                .await
                .is_err()
        );
        assert!(writer.into_inner().is_empty());
        drop(codec);
        assert_eq!(pool.used(), [0, 0]);
    });
}

#[test]
fn held_pinned_codec_bounds_oversize_and_malformed_allocations_before_release() {
    runtime().block_on(async {
        let pool = MemoryPool::default();
        let protocol = StreamProtocol::new("/test/held");
        // Array count, bytestring types and duplicates exercise the actual serde Vec visitor.
        let prefix = b"\xa1\x67payload";
        let huge = [0x9b, 255, 255, 255, 255, 255, 255, 255, 255];
        let mut malformed = vec![
            response_bytes(vec![0; 64 * 1024 + 1]),
            [prefix.as_slice(), &huge].concat(),
            [prefix.as_slice(), b"\x41x"].concat(),
            [prefix.as_slice(), b"\x5f\x41x\xff"].concat(),
            [b"\xa2\x67payload\x80\x67payload".as_slice(), &huge].concat(),
        ];
        for kind in [0x81, 0xa1, 0xc0] {
            let mut deep = b"\xa2\x67payload\x80\x61x".to_vec();
            deep.extend(std::iter::repeat_n(kind, 300));
            malformed.push(deep);
        }
        let mut key = b"\xa2\x67payload\x80\x7f\x7a".to_vec();
        let length = RPC_RESPONSE_ENCODED_MAX - 40;
        key.extend_from_slice(&(length as u32).to_be_bytes());
        key.resize(key.len() + length, b'x');
        key.extend_from_slice(&[0x7b, 255, 255, 255, 255, 255, 255, 255, 255]);
        malformed.push(key); // An indefinite key's final bogus segment must remain bounded.
        malformed.push(b"\xa2\x67payload\x80\x7f\x61\xff\xff\x00".to_vec());
        for bytes in malformed {
            let mut codec = RequestCodec::default();
            codec.outbound_memory = Some(pool.rpc().unwrap());
            assert!(
                codec
                    .read_response(&protocol, &mut Cursor::new(bytes))
                    .await
                    .is_err()
            );
            assert_eq!(pool.used(), USED);
            drop(codec);
            assert_eq!(pool.used(), [0, 0]);
        }
        for extra in [0, 1] {
            // A valid unknown field pads the outer frame to the exact encoded bound.
            let mut bytes = b"\xa2\x67payload\x80\x61x\x5a".to_vec();
            let length = RPC_RESPONSE_ENCODED_MAX + extra - bytes.len() - 4;
            bytes.extend_from_slice(&(length as u32).to_be_bytes());
            bytes.resize(RPC_RESPONSE_ENCODED_MAX + extra, 0);
            let mut codec = RequestCodec::default();
            codec.outbound_memory = Some(pool.rpc().unwrap());
            let mut source = Cursor::new(bytes);
            let response = codec.read_response(&protocol, &mut source).await;
            if extra == 0 {
                assert!(response.unwrap().as_slice().is_empty());
            } else {
                assert!(response.unwrap_err().to_string().contains("encoded bound"));
            }
            assert_eq!(source.position() as usize, RPC_RESPONSE_ENCODED_MAX + extra);
            drop(codec);
            assert_eq!(pool.used(), [0, 0]);
        }
        // The legacy codec's existing large-frame acceptance is unchanged.
        let bytes = response_bytes(vec![0; 128 * 1024]);
        assert_eq!(
            RequestCodec::default()
                .read_response(&protocol, &mut Cursor::new(bytes))
                .await
                .unwrap()
                .into_unheld()
                .unwrap()
                .len(),
            128 * 1024
        );
    });
}
