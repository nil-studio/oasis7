//! Real Linux GEN → accepted namespace owner → native Host admission.
//! Only source distribution is shared here: no Runtime/Chain library dependency,
//! copied worker implementation, synthetic accepted owner or WorldReady claim.
#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
extern crate self as oasis7_runtime;
#[path = "../../oasis7_runtime/src/world/incremental_module_worker_allocator.rs"]
mod actual_allocator;
#[path = "../../oasis7_runtime/src/world/incremental_cache_move.rs"]
pub mod incremental_cache_move;
pub mod incremental_module_execution_worker {
    pub use crate::actual_allocator::RootModuleWorkerAllocatorV1;
}
#[path = "../../oasis7_chain_runtime/tests/support/genesis_endpoint_work.rs"]
mod genesis_work;
#[path = "../../oasis7_chain_runtime/src/iws_genesis.rs"]
mod iws_genesis;
#[path = "../../oasis7_chain_runtime/src/iws_host_runtime.rs"]
mod iws_host_runtime;
use ed25519_dalek::SigningKey;
use oasis7_distfs::{
    controlled_authority::{
        fencing::{ControlledFileProvider, TrustedFencingPolicyV1, directory_fingerprint},
        formal::endpoint::{
            FileFormalEndpoint, FormalArchiveReadWorkV1, FormalCandidateMaterialV1,
            FormalEndpointAuthorizationV1, IwsPageStoreCapabilityV1,
        },
    },
    incremental_page_store::{IncrementalPageStore, PageCompression},
};
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{self, InitialActivationTrustV1, SignedInitialActivationV1},
        replicated_protocol::EndpointRole,
    },
    native_environment::{NativeEnvironmentRole, NativeExecutionEnvironmentV1},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_executor::bounded_worker::WorkerLimitsV1;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

fn bundle() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("OASIS7_IWS_HOST_BUNDLE")
            .expect("OASIS7_IWS_HOST_BUNDLE is required; missing inputs are not a skipped test"),
    );
    assert!(
        root.is_absolute(),
        "bundle must name a fresh absolute Linux data directory"
    );
    root
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
/// This mode measures only fresh directories and a genuine provider resource.
/// Its successful return is preparation evidence, not Host acceptance evidence.
fn fingerprints(root: &Path, output: &Path) {
    fs::create_dir_all(root).unwrap();
    let primary = root.join("primary");
    let replica = root.join("replica");
    fs::create_dir(&primary).expect("primary must be fresh; never replace an existing world");
    fs::create_dir(&replica).expect("replica must be fresh; never replace an existing world");
    let provider =
        ControlledFileProvider::new(root.join("resource"), SigningKey::from_bytes(&[3; 32]))
            .unwrap();
    let value = serde_json::json!({"primary":directory_fingerprint(&primary).unwrap(),
        "replica":directory_fingerprint(&replica).unwrap(),"resource":provider.resource_binding()});
    original(output, &serde_json::to_vec(&value).unwrap());
    eprintln!(
        "GEN input preparation only: original Linux directory fingerprints {}",
        output.display()
    );
}
fn metadata(
    component: &oasis7_formal_protocol::native_environment::NativeEnvironmentComponentV1,
    outs: &[PathBuf],
) -> Vec<u8> {
    let name = match component.name.as_str() {
        "build/metadata" => "native-build-metadata.txt",
        "build/toolchain" => "native-toolchain.txt",
        "build/Cargo.lock" => "native-Cargo.lock",
        other => panic!("unknown original build metadata component {other}"),
    };
    for out in outs {
        let path = out.join(name);
        if !path.is_file() {
            continue;
        }
        let bytes = input(&path, 8 << 20);
        if blake3::hash(&bytes).to_hex().as_str() == component.content_digest {
            return bytes;
        }
    }
    panic!(
        "original matching build bytes missing for {} ({})",
        component.name, component.content_digest
    );
}
/// The old actual ELF supplies the descriptor. Every later recovered original
/// must match that descriptor's original component/chunk hashes exactly.
fn capture(root: &Path, image: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(root).unwrap();
    let archive = root.join("native_archive");
    fs::create_dir(&archive).expect("native archive must be fresh");
    let stable = root.join("native-worker");
    let mut source = fs::File::open(image).expect("actual old Linux worker ELF required");
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
    let outs_file = PathBuf::from(
        std::env::var_os("OASIS7_IWS_OLD_BUILD_OUTS").expect("original OUT_DIR JSON list required"),
    );
    let outs: Vec<PathBuf> = serde_json::from_slice(&input(&outs_file, 1 << 20)).unwrap();
    assert!(!outs.is_empty(), "original OUT_DIR list cannot be empty");
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
            NativeEnvironmentRole::BuildMetadata => metadata(component, &outs),
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
    let mut limits = genesis_work::limits();
    limits.current.nodes_remaining = nodes;
    FileFormalEndpoint::with_genesis_remote_pair_for_fixture(&authority,|primary_provider,replica_provider|{
        let authorization=||FormalEndpointAuthorizationV1{authority:authority.clone(),original:authority.clone(),recovery:None,parent:None};
        let mut primary=FileFormalEndpoint::open_protected(&root.join("primary"),authorization(),EndpointRole::Primary,primary_provider.clone(),None).unwrap();
        let mut replica=FileFormalEndpoint::open_protected(&root.join("replica"),authorization(),EndpointRole::Replica,replica_provider.clone(),None).unwrap();
        for (endpoint,pages) in [(&mut primary,&mut primary_pages),(&mut replica,&mut replica_pages)] {
            endpoint.install_iws_page_store(IwsPageStoreCapabilityV1::new(pages.take().unwrap(),genesis_work::budgets()).unwrap().with_current_limits(limits.clone()).unwrap()).unwrap();
            endpoint.supply_iws_candidate_work(genesis_work::source_work()).unwrap();
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
#[test]
#[ignore = "manual real Linux input directory preparation"]
fn prepare_linux_host_fingerprints() {
    let output =
        std::env::var_os("OASIS7_IWS_HOST_FINGERPRINTS").expect("fingerprint output required");
    fingerprints(&bundle(), Path::new(&output));
}
#[test]
#[ignore = "manual original ELF/archive collection"]
fn capture_original_linux_native_archive() {
    let image = std::env::var_os("OASIS7_IWS_NATIVE_CAPTURE").expect("original ELF path required");
    capture(&bundle(), Path::new(&image));
}
#[test]
fn real_linux_accepted_genesis_installs_native_host() {
    accepted_host(&bundle());
}
