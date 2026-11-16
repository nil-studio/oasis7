//! Real libraries: Linux GEN → accepted namespace owner → native Host admission.
//! The current Cargo worker and its original build metadata create a fresh,
//! self-contained fixture. No synthetic accepted owner or WorldReady claim.
#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[path = "support/world_storage_cognition.rs"]
mod cognition;
#[path = "support/world_storage_current_work.rs"]
mod current_work;
#[path = "support/genesis_endpoint_material.rs"]
mod endpoint_material;
#[path = "support/genesis_endpoint_work.rs"]
mod endpoint_work;
#[path = "support/world_storage_genesis_work.rs"]
mod genesis_work;
#[path = "support/world_storage_physical_genesis.rs"]
mod physical_genesis;
#[path = "support/world_storage_scratch.rs"]
mod scratch;
use ed25519_dalek::SigningKey;
use endpoint_work::*;
use oasis7_chain_runtime::{iws_genesis, iws_host_runtime};
use oasis7_distfs::incremental_page_store::objects::{IncrementalObjectStore, ObjectIo};
use oasis7_distfs::{
    controlled_authority::{
        fencing::{ControlledFileProvider, TrustedFencingPolicyV1},
        formal::endpoint::{
            FileFormalEndpoint, FormalArchiveReadWorkV1, FormalCandidateMaterialV1,
            FormalEndpointAuthorizationV1, IwsPageStoreCapabilityV1,
        },
    },
    incremental_page_store::{IncrementalPageStore, PageCompression},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::test_fixtures::{self as signed, incremental},
    incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1, RootSetV1},
    incremental_history::GenesisConfigV1,
    incremental_state::*,
    incremental_value::*,
};
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{self, InitialActivationTrustV1, SignedInitialActivationV1},
        replicated_protocol::EndpointRole,
    },
    native_environment::{NativeEnvironmentRole, NativeExecutionEnvironmentV1},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{World, incremental_genesis::RuntimeGenesisWorkV1};
use oasis7_wasm_executor::bounded_worker::WorkerLimitsV1;
use oasis7_world_state::{
    WorldState, incremental_cognition_history::*, incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1, incremental_runtime_history_index::*,
    incremental_runtime_history_manifest::*, incremental_semantic_key::SemanticKeyBudget,
    incremental_world_current_complete::*,
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        // Match the actual native-worker tests: /proc/maps and stat must see
        // the same Linux device, rather than a host-bind filesystem mapping.
        let path = Path::new("/tmp").join(format!(
            "oasis7-iws-host-integration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).expect("fresh isolated Linux fixture directory");
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove only this test's isolated directory");
    }
}
fn put(objects: &IncrementalObjectStore<'_>, bytes: &[u8], io: &mut ObjectIo<'_>) {
    objects
        .write_object(
            &mut &bytes[..],
            bytes.len() as u64,
            Some(*blake3::hash(bytes).as_bytes()),
            io,
        )
        .unwrap();
}
fn input(path: &Path, cap: usize) -> Vec<u8> {
    let file =
        fs::File::open(path).unwrap_or_else(|e| panic!("required input {}: {e}", path.display()));
    let length = usize::try_from(file.metadata().unwrap().len()).unwrap();
    assert!(length <= cap, "fixture input cap: {}", path.display());
    let mut bytes = Vec::with_capacity(length);
    file.take((cap as u64) + 1).read_to_end(&mut bytes).unwrap();
    assert_eq!(
        bytes.len(),
        length,
        "fixture input changed: {}",
        path.display()
    );
    bytes
}
fn original(path: &Path, bytes: &[u8]) {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(bytes).unwrap();
            file.sync_all().unwrap();
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            assert_eq!(input(path, 256 << 20), bytes, "original collision")
        }
        Err(e) => panic!("new original {}: {e}", path.display()),
    }
}
fn metadata(
    component: &oasis7_formal_protocol::native_environment::NativeEnvironmentComponentV1,
) -> Vec<u8> {
    // These are the very same package build outputs embedded by the worker;
    // never scan another target directory or mix a previous build's originals.
    let bytes: &[u8] = match component.name.as_str() {
        "build/metadata" => include_bytes!(concat!(env!("OUT_DIR"), "/native-build-metadata.txt")),
        "build/toolchain" => include_bytes!(concat!(env!("OUT_DIR"), "/native-toolchain.txt")),
        "build/Cargo.lock" => include_bytes!(concat!(env!("OUT_DIR"), "/native-Cargo.lock")),
        other => panic!("unknown original build metadata component {other}"),
    };
    assert_eq!(
        blake3::hash(bytes).to_hex().as_str(),
        component.content_digest
    );
    bytes.to_vec()
}
/// The current actual ELF supplies the descriptor. Every archived original
/// must match that descriptor's original component/chunk hashes exactly.
fn capture(root: &Path) {
    let image = Path::new(env!("CARGO_BIN_EXE_oasis7-native-bound-fixture"));
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(root).unwrap();
    let archive = root.join("native_archive");
    fs::create_dir(&archive).expect("native archive must be fresh");
    let stable = root.join("native-worker");
    let mut source = fs::File::open(image).expect("actual current Cargo worker ELF required");
    let mut target = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stable)
        .unwrap();
    assert!(
        source.metadata().unwrap().len() <= 256 << 20,
        "actual ELF fixture input cap"
    );
    std::io::copy(&mut source, &mut target).unwrap();
    target.sync_all().unwrap();
    drop(target);
    fs::set_permissions(&stable, fs::Permissions::from_mode(0o700)).unwrap();
    let result = Command::new(&stable)
        .arg("--capture-native-fixture-v1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "original capture failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let descriptor =
        NativeExecutionEnvironmentV1::decode(&result.stdout).expect("actual descriptor codec");
    assert_eq!(descriptor.os, "linux");
    assert_eq!(descriptor.arch, std::env::consts::ARCH);
    let mut total = 0usize;
    for component in &descriptor.components {
        let bytes = match component.role {
            NativeEnvironmentRole::Executable => input(&stable, 256 << 20),
            NativeEnvironmentRole::LoadedLibrary => {
                let path = component
                    .name
                    .strip_prefix("mapped")
                    .expect("original mapped library name");
                input(Path::new(path), 256 << 20)
            }
            NativeEnvironmentRole::BuildMetadata => metadata(component),
        };
        assert_eq!(bytes.len() as u64, component.byte_len);
        assert_eq!(
            blake3::hash(&bytes).to_hex().as_str(),
            component.content_digest
        );
        total = total.checked_add(bytes.len()).unwrap();
        assert!(total <= 256 << 20, "native archive fixture cap");
        let mut offset = 0usize;
        for chunk in &component.chunks {
            let end = offset.checked_add(chunk.byte_len as usize).unwrap();
            let raw = bytes.get(offset..end).expect("original chunk range");
            assert_eq!(blake3::hash(raw).to_hex().as_str(), chunk.digest);
            original(&archive.join(&chunk.digest), raw);
            offset = end;
        }
        assert_eq!(offset, bytes.len());
    }
    descriptor
        .verify(|id| Ok(input(&archive.join(id), 1 << 20)))
        .expect("original complete native verifier");
    original(&archive.join("descriptor.cbor"), &result.stdout);
    fs::File::open(&archive).unwrap().sync_all().unwrap();
    eprintln!(
        "GEN input preparation only: original native archive {} bytes at {}",
        total,
        archive.display()
    );
}
fn accepted_host(root: &Path) {
    let certificate: SignedInitialActivationV1 =
        serde_cbor::from_slice(&input(&root.join("activation.cbor"), 8 << 20)).unwrap();
    let trust: InitialActivationTrustV1 =
        serde_cbor::from_slice(&input(&root.join("trust.cbor"), 8 << 20)).unwrap();
    let authority = formal::verify_initial_activation(&certificate, &trust)
        .expect("original signed activation and supplied trust");
    let candidate: FormalCandidateMaterialV1 =
        serde_cbor::from_slice(&input(&root.join("candidate.cbor"), 64 << 20)).unwrap();
    let nodes: u64 =
        serde_json::from_slice(&input(&root.join("current_nodes.json"), 1024)).unwrap();
    assert!(nodes > 0, "actual bootstrap scan bound missing");
    let memory = MemoryBudget::new(2usize << 30, 32 << 20).unwrap();
    let stop = AtomicBool::new(false);
    let primary_pages = IncrementalPageStore::open(
        &root.join("pages"),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let replica_pages = IncrementalPageStore::open(
        &root.join("pages"),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let mut primary_pages = Some(primary_pages);
    let mut replica_pages = Some(replica_pages);
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: authority.policy().fencing_provider_public_key.clone(),
        authorized_resource_bindings: authority
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let mut limits = endpoint_material::limits();
    limits.current.nodes_remaining = nodes;
    FileFormalEndpoint::with_genesis_remote_pair_for_fixture(&authority,|primary_provider,replica_provider|{
        let authorization=||FormalEndpointAuthorizationV1{authority:authority.clone(),original:authority.clone(),recovery:None,parent:None};
        let mut primary=FileFormalEndpoint::open_protected(&root.join("primary"),authorization(),EndpointRole::Primary,primary_provider.clone(),None).unwrap();
        let mut replica=FileFormalEndpoint::open_protected(&root.join("replica"),authorization(),EndpointRole::Replica,replica_provider.clone(),None).unwrap();
        for (endpoint,pages) in [(&mut primary,&mut primary_pages),(&mut replica,&mut replica_pages)] {
            endpoint.install_iws_page_store(IwsPageStoreCapabilityV1::new(pages.take().unwrap(),endpoint_material::budgets()).unwrap().with_current_limits(limits.clone()).unwrap()).unwrap();
            endpoint.supply_iws_candidate_work(endpoint_material::source_work()).unwrap();
        }
        let context=primary.stage_genesis_source(&candidate,||false).unwrap();
        let other=replica.stage_genesis_source(&candidate,||false).unwrap();
        assert_eq!(context.record(),other.record());
        let work=FormalArchiveReadWorkV1::new(1_000_000_000,16u64<<30).unwrap();
        let accepted=iws_genesis::submit_staged_genesis_bounded(&candidate,&context,iws_genesis::GenesisEndpointsV1{
            primary:&mut primary,replica:&mut replica,primary_provider:primary_provider.as_ref(),replica_provider:replica_provider.as_ref(),
            primary_policy:&policy,replica_policy:&policy,
        },&memory,&work,&mut||false).unwrap();
        let record=accepted.primary().head().record_id.clone();
        assert_eq!(record,accepted.replica().head().record_id);assert!(accepted.uses_budget(&memory));
        let source=primary.into_accepted_genesis_host_source_bounded(primary_provider.as_ref(),&policy,&memory,&work,&mut||false).unwrap();
        assert_eq!(hex::encode(source.record_id()),record);
        let fence=source.fence();let namespaces=(source.page_namespace_identity(),source.native_namespace_identity());
        assert_eq!(hex::encode(fence.parent_commit),candidate.candidate.record.artifact_roots[&oasis7_formal_protocol::controlled_authority::replicated_protocol::ArtifactRole::IncrementalCommitBody]);
        assert_eq!(hex::encode(fence.parent_roots),candidate.candidate.record.after_state_root);
        let mut host=iws_host_runtime::IwsHostRuntimeV1::default();
        host.install_accepted_genesis_source_bounded(source,WorkerLimitsV1{
            address_space_bytes:512<<20,max_request_bytes:1<<20,max_output_bytes:4096,wall_time_ms:10000,
        },&memory,&work,&stop).expect("real same-sealed-FD handshake AND native-bound environment admission");
        assert!(host.is_installed());assert_eq!(host.generation(),1);
        assert_eq!(host.current_fence(),Some(fence));assert_eq!(host.current_namespace_identities(),Some(namespaces));
        assert_eq!(host.current_record_id().map(hex::encode),Some(record.clone()));
        host.verify_current_worker_bounded(&memory,&work,&stop).expect("same installed real owner can re-admit its actual native worker");
        assert_eq!(host.current_fence(),Some(fence));assert_eq!(host.generation(),1);
        eprintln!("REAL_LINUX_ACCEPTED_GEN_HOST_INSTALLED record={record} generation=1");
        let retained=memory.used();drop(host);assert!(memory.used()<retained,"dropping Host releases its actual retained sources");
        drop(replica);drop(accepted);
    }).unwrap();
}
fn prepare_bundle(root: &Path) {
    let primary = root.join("primary");
    let replica = root.join("replica");
    fs::create_dir(&primary).unwrap();
    fs::create_dir(&replica).unwrap();
    let provider =
        ControlledFileProvider::new(root.join("resource"), SigningKey::from_bytes(&[3; 32]))
            .unwrap();
    capture(root);
    let fixture = endpoint_material::prepare_with_native_archive(
        root,
        &primary,
        &replica,
        provider.resource_binding(),
        &root.join("native_archive"),
        None,
    );
    // Only original bytes cross the prepare/reopen boundary. The consumer must
    // independently verify activation, close the quorum and obtain live leases.
    for (name, bytes) in [
        (
            "activation.cbor",
            serde_cbor::to_vec(&fixture.activation).unwrap(),
        ),
        ("trust.cbor", serde_cbor::to_vec(&fixture.trust).unwrap()),
        (
            "candidate.cbor",
            serde_cbor::to_vec(&fixture.candidate).unwrap(),
        ),
        (
            "current_nodes.json",
            serde_json::to_vec(&fixture.current_limits.current.nodes_remaining).unwrap(),
        ),
    ] {
        original(&root.join(name), &bytes);
    }
    drop(fixture);
    drop(provider);
}
#[test]
fn real_linux_accepted_genesis_installs_native_host() {
    let directory = Directory::new();
    prepare_bundle(&directory.0);
    accepted_host(&directory.0);
}
