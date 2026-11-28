//! Closed Request DATA layout; decoding never grants execution authority.
use super::*;
use crate::controlled_authority::formal::native_environment::{
    NativeEnvironmentChunkBytesV1, NativeEnvironmentChunkV1, NativeEnvironmentComponentV1,
    NativeExecutionEnvironmentV1,
};
use crate::controlled_authority::replicated_protocol::{ArtifactObject, ArtifactRole};
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLayoutV1;
use std::mem::size_of;

pub(super) fn layout() -> CborDataLayoutV1 {
    // Every Box in the closed Request graph is an immediate Command field.
    // Their pointees contain inline structs/Options, Strings and the repeated
    // containers below, with no nested Box. Count the largest simultaneous
    // fixed roots once, not once per CBOR byte-array element or scalar token.
    let boxes = [
        size_of::<NativeExecutionEnvironmentV1>(),
        size_of::<SignedFormalFrozenV1>() + size_of::<ClosedRecord>(),
        size_of::<FormalFrozenCertificateV1>() + size_of::<ClosedRecord>(),
        size_of::<FormalCandidateMaterialV1>(),
        2 * size_of::<FormalPrepareReceiptV1>(),
        size_of::<FormalDecisionProofV1>(),
        size_of::<FormalQualifiedCommitProofV1>(),
        size_of::<SignedNormalDurableSetTransitionV1>(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    CborDataLayoutV1 {
        root: size_of::<Request>() + boxes,
        // ClosedRecord objects/references/u8 arrays and native components,
        // chunk descriptors and byte-string carriers are the complete Vec set.
        // The shared decoder additionally admits Serde Content and phase peaks.
        sequence_element: size_of::<ArtifactObject>()
            .max(size_of::<NativeEnvironmentComponentV1>())
            .max(size_of::<NativeEnvironmentChunkV1>())
            .max(size_of::<NativeEnvironmentChunkBytesV1>())
            .max(size_of::<String>()),
        map_pair: size_of::<(ArtifactRole, String)>(),
        set_element: 0,
        box_pointee: 0,
        // All reachable defaults are Option::None; no allocating defaults.
        default_heap_per_value: 0,
        symbols_bytes: 0,
        visits: 1,
        missing: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controlled_authority::formal::endpoint::FormalArchiveReadWorkV1;
    use crate::controlled_authority::formal::native_environment::NativeEnvironmentRole;
    use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::PaidCborDataV1;
    use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
    use std::borrow::Borrow;
    use std::io::Write;
    use std::net::TcpStream;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn hash() -> String {
        "ab".repeat(32)
    }
    fn request(command: Command) -> Request {
        // These are codec DATA fixtures, not signed or qualified authority.
        Request {
            body: RequestBody {
                schema_version: 1,
                world: FormalWorldIdentityV1 {
                    world_id: "request-layout".into(),
                    chain_id: "chain".into(),
                    genesis_digest: hash(),
                },
                authority_id: hash(),
                instance_digest: hash(),
                challenge: hash(),
                minimum: FormalHeadRefV1 {
                    position: 0,
                    record_id: hash(),
                },
                command,
                archive_read_grant: None,
            },
            signature: "cd".repeat(64),
        }
    }
    fn work() -> FormalArchiveReadWorkV1 {
        FormalArchiveReadWorkV1::new(100_000_000, 1 << 34).unwrap()
    }
    fn receive<T: serde::de::DeserializeOwned>(
        bytes: &[u8],
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(PaidCborDataV1<T>, ReservedValue<Vec<u8>>), FormalError> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let bytes = bytes.to_vec();
        let sender = std::thread::spawn(move || {
            let (mut peer, _) = listener.accept().unwrap();
            let _ = peer.write_all(&(bytes.len() as u32).to_be_bytes());
            let _ = peer.write_all(&bytes);
        });
        let mut stream = TcpStream::connect(address).unwrap();
        wire::configure(&stream, Duration::from_secs(2)).unwrap();
        let result = wire::read_paid_before_deadline(
            &mut stream,
            Instant::now() + Duration::from_secs(2),
            memory,
            work,
            layout(),
            cancelled,
        );
        drop(stream);
        sender.join().unwrap();
        result
    }
    fn prepare() -> Command {
        let world = request(Command::Head).body.world;
        let record = CanonicalRecordV1 {
            schema_version: 1,
            durable_set_transition_digest: None,
            kind: FormalRecordKindV1::InitialActivation,
            world,
            position: 1,
            parent_record_id: hash(),
            original_authority_id: hash(),
            original_epoch: 1,
            activation_ref: hash(),
            request_digest: hash(),
            payload_digest: hash(),
            frozen_reservation_id: None,
            runtime_manifest_digest: hash(),
            governing_manifest_digest: hash(),
            execution_policy_digest: hash(),
            before_state_root: hash(),
            after_state_root: hash(),
            action_root: hash(),
            event_root: hash(),
            result_root: hash(),
            projection_root: hash(),
            artifact_roots: [(ArtifactRole::Input, hash())].into(),
            handoff: None,
        };
        let material = ClosedRecord {
            payload_digest: hash(),
            before_state_root: hash(),
            after_state_root: hash(),
            roots: [(ArtifactRole::Input, hash())].into(),
            objects: vec![ArtifactObject {
                content_hash: hash(),
                bytes: vec![0, 127, 128, 255],
                references: vec![hash(), "reference".into()],
            }],
        };
        Command::Prepare(Box::new(FormalCandidateMaterialV1 {
            candidate: SignedCanonicalCandidateV1 {
                record,
                writer_signature_hex: "cd".repeat(64),
            },
            frozen_material: Some(material.clone()),
            material,
            frozen: None,
            branch: FormalCompletionBranchV1::Normal,
        }))
    }
    fn native() -> Command {
        let descriptor = NativeExecutionEnvironmentV1 {
            schema_version: 1,
            target: "x86_64-unknown-linux-gnu".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            components: [
                NativeEnvironmentRole::Executable,
                NativeEnvironmentRole::BuildMetadata,
            ]
            .into_iter()
            .map(|role| NativeEnvironmentComponentV1 {
                role,
                name: format!("{role:?}"),
                byte_len: 4,
                content_digest: hash(),
                chunks: vec![NativeEnvironmentChunkV1 {
                    digest: hash(),
                    byte_len: 4,
                }],
            })
            .collect(),
        };
        Command::StageNativeEnvironment(
            Box::new(descriptor),
            vec![NativeEnvironmentChunkBytesV1 {
                digest: hash(),
                bytes: vec![0, 127, 128, 255],
            }],
        )
    }

    #[test]
    fn prepaid_request_preserves_query_prepare_and_native_original_codecs() {
        for command in [Command::Query(hash()), prepare(), native()] {
            let request = request(command);
            let named = serde_cbor::to_vec(&request).unwrap();
            let mut packed = Vec::new();
            request
                .serialize(&mut serde_cbor::Serializer::new(&mut packed).packed_format())
                .unwrap();
            let mut legacy = Vec::new();
            request
                .serialize(&mut serde_cbor::Serializer::new(&mut legacy).legacy_enums())
                .unwrap();
            assert_eq!(named[0], 0xa2);
            let mut indefinite = vec![0xbf];
            indefinite.extend_from_slice(&named[1..]);
            indefinite.push(0xff);
            for bytes in [named.clone(), packed, legacy, indefinite] {
                let original: Request = serde_cbor::from_slice(&bytes).unwrap();
                assert_eq!(serde_cbor::to_vec(&original).unwrap(), named);
                let memory = MemoryBudget::new(128 << 20, 0).unwrap();
                let work = work();
                let (paid, raw) =
                    receive::<Request>(&bytes, &memory, &work, &mut || false).unwrap();
                let decoded: &Request = paid.borrow();
                assert_eq!(serde_cbor::to_vec(decoded).unwrap(), named);
                paid.validate_context(&mut work.bounded_context(&memory, &mut || false))
                    .unwrap();
                assert!(raw.uses_budget(&memory));
                assert!(memory.used() > raw.reserved_bytes());
                drop(raw);
                assert!(
                    memory.used() > 0,
                    "typed children remain charged after raw drops"
                );
                drop(paid);
                assert_eq!(memory.used(), 0);
            }
        }
    }

    #[test]
    fn prepaid_request_rejects_before_dto_decode_when_only_raw_frame_fits() {
        static DECODED: AtomicBool = AtomicBool::new(false);
        struct Probe;
        impl<'de> Deserialize<'de> for Probe {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                DECODED.store(true, Ordering::SeqCst);
                Request::deserialize(d).map(|_| Self)
            }
        }
        let bytes = serde_cbor::to_vec(&request(Command::Query(hash()))).unwrap();
        assert!(serde_cbor::from_slice::<Request>(&bytes).is_ok());
        let memory = MemoryBudget::new(bytes.len() + size_of::<Vec<u8>>() + 4096, 0).unwrap();
        let work = work();
        let before = work.remaining_read_bytes();
        let error = receive::<Probe>(&bytes, &memory, &work, &mut || false)
            .err()
            .unwrap();
        assert!(
            error.to_string().contains("memory reservation"),
            "{error:?}"
        );
        assert!(!DECODED.load(Ordering::SeqCst));
        assert!(work.remaining_read_bytes() < before);
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn prepaid_request_keeps_pool_work_identity_cancellation_and_exhaustion() {
        let bytes = serde_cbor::to_vec(&request(Command::Query(hash()))).unwrap();
        let memory = MemoryBudget::new(128 << 20, 0).unwrap();
        let original = work();
        let (paid, raw) = receive::<Request>(&bytes, &memory, &original, &mut || false).unwrap();
        let foreign_pool = MemoryBudget::new(128 << 20, 0).unwrap();
        assert!(
            paid.validate_context(&mut original.bounded_context(&foreign_pool, &mut || false))
                .is_err()
        );
        assert!(
            paid.validate_context(&mut work().bounded_context(&memory, &mut || false))
                .is_err()
        );
        drop((paid, raw));
        assert_eq!(memory.used(), 0);
        original
            .consume_steps(original.remaining_steps(), &mut || false)
            .unwrap();
        assert!(receive::<Request>(&bytes, &memory, &original, &mut || false).is_err());
        assert_eq!(original.remaining_steps(), 0);
        let fresh = work();
        let before = fresh.remaining_steps();
        let error = receive::<Request>(&bytes, &memory, &fresh, &mut || true)
            .err()
            .unwrap();
        assert!(error.to_string().contains("cancelled"), "{error:?}");
        assert_eq!(fresh.remaining_steps(), before);
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn prepaid_request_original_codec_rejects_trailing_duplicates_and_hostile_hints() {
        let bytes = serde_cbor::to_vec(&request(Command::Query(hash()))).unwrap();
        let mut trailing = bytes.clone();
        trailing.push(0);
        let mut duplicate = bytes;
        duplicate[0] = 0xa3;
        duplicate.extend(serde_cbor::to_vec(&"signature").unwrap());
        duplicate.extend(serde_cbor::to_vec(&"duplicate").unwrap());
        for bad in [
            trailing,
            duplicate,
            vec![0xff],
            vec![0x9b, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
        ] {
            assert!(serde_cbor::from_slice::<Request>(&bad).is_err());
            let memory = MemoryBudget::new(128 << 20, 0).unwrap();
            assert!(receive::<Request>(&bad, &memory, &work(), &mut || false).is_err());
            assert_eq!(memory.used(), 0);
        }
    }
}
