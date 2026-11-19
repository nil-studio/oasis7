#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{self, test_fixtures::incremental},
    incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1, RootSetV1},
    incremental_material::*,
    incremental_state::*,
    incremental_value::*,
};
use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::{incremental_cognition_history::*, incremental_world_result_router::*};
use oasis7_world_state::{
    incremental_cognition_history_cbor::CognitionHistoryCborWorkV1,
    incremental_runtime_history_decode::RuntimeHistoryDecodeWorkV1,
};
use oasis7_world_state::{
    incremental_field_member::*, incremental_runtime_history_index::*,
    incremental_runtime_history_manifest::*,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-runtime-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
#[path = "support/genesis_endpoint_work.rs"]
mod endpoint_work;
use endpoint_work::*;
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

use oasis7_formal_protocol::incremental_history::GenesisConfigV1;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{World, incremental_genesis::RuntimeGenesisWorkV1};
use oasis7_world_state::incremental_semantic_key::SemanticKeyBudget;
use oasis7_world_state::{
    WorldState, incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_source::*, incremental_world_current_complete::*,
};
#[path = "support/world_storage_cognition.rs"]
mod cognition;
#[path = "support/world_storage_current_work.rs"]
mod current_work;
#[path = "support/world_storage_genesis_work.rs"]
mod genesis_work;
#[path = "support/world_storage_physical_genesis.rs"]
mod physical_genesis;
#[path = "support/world_storage_scratch.rs"]
mod scratch;

#[path = "support/genesis_endpoint_material.rs"]
mod endpoint_material;
use oasis7_distfs::controlled_authority::fencing::ControlledFileProvider;
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FileFormalEndpoint, FormalEndpointAuthorizationV1, IwsPageStoreCapabilityV1,
};
use oasis7_formal_protocol::controlled_authority::formal::test_fixtures as signed;
use oasis7_formal_protocol::controlled_authority::replicated_protocol::EndpointRole;
use std::sync::Arc;
#[test]
fn real_genesis_endpoint_stages_complete_evidence_but_never_issues_prepare_receipt() {
    let directory = Directory::new();
    let pdir = directory.0.join("primary");
    let rdir = directory.0.join("replica");
    std::fs::create_dir(&pdir).unwrap();
    std::fs::create_dir(&rdir).unwrap();
    let provider = Arc::new(
        ControlledFileProvider::new(directory.0.join("provider"), signed::key(3)).unwrap(),
    );
    let fixture =
        endpoint_material::prepare(&directory.0, &pdir, &rdir, provider.resource_binding());
    let mut endpoint = FileFormalEndpoint::open_controlled_for_fixture(
        &pdir,
        FormalEndpointAuthorizationV1 {
            authority: fixture.authority.clone(),
            original: fixture.authority.clone(),
            recovery: None,
            parent: None,
        },
        EndpointRole::Primary,
        signed::key(5),
        provider,
    )
    .unwrap();
    endpoint
        .install_iws_page_store(
            IwsPageStoreCapabilityV1::new(fixture.pages, endpoint_material::budgets())
                .unwrap()
                .with_current_limits(fixture.current_limits)
                .unwrap(),
        )
        .unwrap();
    let request = fixture.candidate.candidate.record.request_digest.clone();
    // Cancellation precedes source verification and cannot stage an issuer.
    let cancelled = endpoint
        .prepare_with_cancel(fixture.candidate.clone(), || true)
        .unwrap_err();
    assert!(
        cancelled
            .to_string()
            .contains("cancelled before verification")
    );
    endpoint
        .supply_iws_candidate_work(endpoint_material::source_work())
        .unwrap();
    let error = endpoint.prepare(fixture.candidate).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("IWS typed owner coverage incomplete"),
        "actual issuer did not reach its post-stage guard: {error}"
    );
    assert!(matches!(
        endpoint.query(&request).unwrap(),
        oasis7_distfs::controlled_authority::formal::endpoint::FormalQueryOutcomeV1::NotRecorded
    ));
}

struct AcceptedGenesisFixture<'a> {
    primary: FileFormalEndpoint,
    replica: FileFormalEndpoint,
    primary_provider:
        &'a Arc<oasis7_distfs::controlled_authority::fencing::RemoteMonotonicEndpointProvider>,
    replica_provider:
        &'a Arc<oasis7_distfs::controlled_authority::fencing::RemoteMonotonicEndpointProvider>,
    policy: &'a oasis7_distfs::controlled_authority::fencing::TrustedFencingPolicyV1,
    memory: &'a MemoryBudget,
    work: &'a oasis7_distfs::controlled_authority::formal::endpoint::FormalArchiveReadWorkV1,
    authority: &'a formal::VerifiedAuthorityV1,
    candidate: &'a oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1,
    expected: &'a formal::VerifiedFormalCommitV1,
    primary_root: &'a std::path::Path,
    replica_root: &'a std::path::Path,
}

#[path = "support/genesis_endpoint_acceptance.rs"]
mod accepted_genesis;

/// One actual physical GEN is shared by both endpoint verifiers. No accepted
/// setter, staged flag, local-key receipt or profile-2 persistence is used.
/// The callback borrows the original meter and runs inside the real oracle scope.
fn with_real_accepted_genesis(run: impl FnOnce(AcceptedGenesisFixture<'_>)) {
    use oasis7_distfs::controlled_authority::fencing::TrustedFencingPolicyV1;
    let directory = Directory::new();
    let pdir = directory.0.join("primary");
    let rdir = directory.0.join("replica");
    std::fs::create_dir(&pdir).unwrap();
    std::fs::create_dir(&rdir).unwrap();
    // This supplies only the original externally authorized resource binding.
    // Every receipt/native signature below is made by the real TCP oracle lane.
    let resource =
        ControlledFileProvider::new(directory.0.join("resource"), signed::key(3)).unwrap();
    let fixture =
        endpoint_material::prepare(&directory.0, &pdir, &rdir, resource.resource_binding());
    let memory = fixture.pages.memory_budget().clone();
    let second = IncrementalPageStore::open(
        fixture.pages.root(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: fixture
            .authority
            .policy()
            .fencing_provider_public_key
            .clone(),
        authorized_resource_bindings: fixture
            .authority
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let candidate = &fixture.candidate;
    let authority = &fixture.authority;
    let limits = fixture.current_limits.clone();
    let mut primary_pages = Some(fixture.pages);
    let mut replica_pages = Some(second);
    FileFormalEndpoint::with_genesis_remote_pair_for_fixture(
        authority,
        |primary_provider, replica_provider| {
            let accepted = accepted_genesis::prepare(
                accepted_genesis::Preparation {
                    primary_root: &pdir,
                    replica_root: &rdir,
                    authority,
                    candidate,
                    limits: &limits,
                    primary_provider: &primary_provider,
                    replica_provider: &replica_provider,
                    policy: &policy,
                    memory: &memory,
                },
                &mut primary_pages,
                &mut replica_pages,
            );
            run(AcceptedGenesisFixture {
                primary: accepted.primary,
                replica: accepted.replica,
                primary_provider: &primary_provider,
                replica_provider: &replica_provider,
                policy: &policy,
                memory: &memory,
                work: &accepted.work,
                authority,
                candidate,
                expected: &accepted.expected,
                primary_root: &pdir,
                replica_root: &rdir,
            });
        },
    )
    .unwrap();
}

#[path = "support/genesis_endpoint_host_rejection.rs"]
mod host_rejection;

#[test]
fn real_genesis_dual_remote_quorum_durably_accepts_and_promotes_exact_cache() {
    with_real_accepted_genesis(host_rejection::check);
}

#[path = "support/genesis_endpoint_frozen_recovery.rs"]
mod frozen_recovery;
