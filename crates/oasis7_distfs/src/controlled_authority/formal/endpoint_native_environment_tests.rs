//! Synthetic original-byte fixtures exercise durable/RPC contracts, not platform or S6 acceptance.
use super::*;
use crate::controlled_authority::formal::native_environment::*;
pub(super) fn attach(material: &mut ClosedRecord, descriptor: &NativeExecutionEnvironmentV1) {
    let bytes = descriptor.encode().unwrap();
    let id = descriptor.digest().unwrap();
    material
        .roots
        .insert(ArtifactRole::NativeExecutionEnvironment, id.clone());
    material.objects.push(ArtifactObject {
        content_hash: id,
        bytes,
        references: vec![],
    });
    material
        .objects
        .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
}
fn native() -> (
    NativeExecutionEnvironmentV1,
    Vec<NativeEnvironmentChunkBytesV1>,
) {
    let bytes = vec![73; 32 * 1024];
    let digest = blake3::hash(&bytes).to_hex().to_string();
    let descriptor = NativeExecutionEnvironmentV1 {
        schema_version: 1,
        target: "fixture-linux".into(),
        os: "linux".into(),
        arch: "fixture".into(),
        components: [
            NativeEnvironmentRole::Executable,
            NativeEnvironmentRole::BuildMetadata,
        ]
        .into_iter()
        .enumerate()
        .map(|(n, role)| NativeEnvironmentComponentV1 {
            role,
            name: format!("original-{n}"),
            byte_len: bytes.len() as u64,
            content_digest: digest.clone(),
            chunks: vec![NativeEnvironmentChunkV1 {
                digest: digest.clone(),
                byte_len: bytes.len() as u32,
            }],
        })
        .collect(),
    };
    (
        descriptor,
        vec![NativeEnvironmentChunkBytesV1 { digest, bytes }],
    )
}
#[test]
fn native_c0_staging_qualification_and_reopen_preserve_external_original_bytes() {
    let (descriptor, chunks) = native();
    let fixture = Fixture::new_with_native(Some(&descriptor));
    let mut coordinator = fixture.coordinator();
    let (primary, replica) = coordinator.endpoints_mut();
    for endpoint in [primary, replica] {
        assert!(endpoint.prepare(fixture.initial.clone()).is_err());
        endpoint
            .stage_native_environment(&descriptor, &chunks)
            .unwrap();
        assert!(
            endpoint
                .advance_native_environment_verification(&descriptor)
                .unwrap()
                .complete
        );
    }
    let proof = coordinator.submit(fixture.initial.clone()).unwrap();
    assert_eq!(proof.decision.record.schema_version, 5);
    drop(coordinator);
    let mut reopened = fixture.endpoint(EndpointRole::Primary);
    assert_eq!(reopened.head().unwrap().position, 1);
    assert!(matches!(
        reopened
            .query(&fixture.initial.candidate.record.request_digest)
            .unwrap(),
        FormalQueryOutcomeV1::Recorded(_)
    ));
    assert!(reopened.native_environment_jobs.is_empty());
    assert!(
        reopened
            .advance_native_environment_verification(&descriptor)
            .unwrap()
            .complete
    );
}
#[test]
fn native_authenticated_rpc_stages_and_verifies_original_chunks() {
    use crate::controlled_authority::formal::transport::{
        FormalEndpointServer, RemoteFormalEndpoint,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::{
        net::{TcpListener, TcpStream},
        time::Duration,
    };
    let (descriptor, chunks) = native();
    let fixture = Fixture::new_with_native(Some(&descriptor));
    let (endpoint, _oracle) = super::protected_tests::native_rpc_endpoint(&fixture);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let server = std::thread::spawn(move || {
        let mut server = FormalEndpointServer::new(endpoint, Duration::from_secs(3)).unwrap();
        while !done.load(Ordering::Acquire) {
            let result = server.serve_one(&listener);
            if done.load(Ordering::Acquire) {
                break;
            }
            result.unwrap();
        }
    });
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut client = RemoteFormalEndpoint::new(
            address,
            Duration::from_secs(3),
            fixture.authority.clone(),
            EndpointRole::Primary,
            key(4),
            fixture.authority.definition().world.genesis_head().unwrap(),
        )
        .unwrap();
        client
            .stage_native_environment(&descriptor, &chunks)
            .unwrap();
        let progress = client
            .advance_native_environment_verification(&descriptor)
            .unwrap();
        assert!(progress.complete);
        assert_eq!(progress.descriptor_digest, descriptor.digest().unwrap());
        assert_eq!(progress.verified_chunks, 2);
    }));
    stop.store(true, Ordering::Release);
    let _wake = TcpStream::connect(address);
    server.join().unwrap();
    outcome.unwrap();
}

#[test]
fn native_incomplete_readback_pin_allows_registered_append_and_blocks_reclamation() {
    let bytes = vec![61; MAX_NATIVE_CHUNK_BYTES];
    let digest = blake3::hash(&bytes).to_hex().to_string();
    let mut complete_bytes = blake3::Hasher::new();
    for _ in 0..9 {
        complete_bytes.update(&bytes);
    }
    let descriptor = NativeExecutionEnvironmentV1 {
        schema_version: 1,
        target: "fixture-linux".into(),
        os: "linux".into(),
        arch: "fixture".into(),
        components: vec![
            NativeEnvironmentComponentV1 {
                role: NativeEnvironmentRole::Executable,
                name: "original-exe".into(),
                byte_len: (9 * MAX_NATIVE_CHUNK_BYTES) as u64,
                content_digest: complete_bytes.finalize().to_hex().to_string(),
                chunks: vec![
                    NativeEnvironmentChunkV1 {
                        digest: digest.clone(),
                        byte_len: MAX_NATIVE_CHUNK_BYTES as u32
                    };
                    9
                ],
            },
            NativeEnvironmentComponentV1 {
                role: NativeEnvironmentRole::BuildMetadata,
                name: "original-build".into(),
                byte_len: bytes.len() as u64,
                content_digest: digest.clone(),
                chunks: vec![NativeEnvironmentChunkV1 {
                    digest: digest.clone(),
                    byte_len: MAX_NATIVE_CHUNK_BYTES as u32,
                }],
            },
        ],
    };
    let fixture = Fixture::new_with_native(Some(&descriptor));
    let mut endpoint = fixture.endpoint(EndpointRole::Primary);
    endpoint
        .stage_native_environment(
            &descriptor,
            &[NativeEnvironmentChunkBytesV1 { digest, bytes }],
        )
        .unwrap();
    assert!(
        !endpoint
            .advance_native_environment_verification(&descriptor)
            .unwrap()
            .complete
    );
    // This is the registered Store path, while the original native readback is live.
    let id = endpoint
        .store
        .put(&("new typed publication during native verification", 7u64))
        .unwrap();
    assert_eq!(endpoint.store.get::<(String, u64)>(&id).unwrap().1, 7);
    let directory = fixture.pdir.join("formal-objects-v1");
    assert!(crate::controlled_authority::formal::physical::pin(&directory, true).is_err());
    assert!(endpoint.collect_physical_orphans().is_err());
    assert!(
        endpoint
            .advance_native_environment_verification(&descriptor)
            .unwrap()
            .complete
    );
    drop(
        endpoint
            .take_native_environment(&fixture.initial.material)
            .unwrap(),
    );
    assert!(
        crate::controlled_authority::formal::physical::pin(&directory, true)
            .unwrap()
            .is_some()
    );
}
