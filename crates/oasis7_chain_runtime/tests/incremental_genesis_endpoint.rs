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
fn physical() -> PageIoBudget {
    PageIoBudget {
        // Physical reads include frame/recipe verification and repeated
        // Current qualification, not only the one-time page publication.
        reads_remaining: 128_000_000,
        read_bytes_remaining: 4 << 30,
        writes_remaining: 1_000_000,
        write_bytes_remaining: 512 << 20,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        write_bytes_remaining: 512 << 20,
    }
}
fn core_budget() -> CoreBudget {
    CoreBudget {
        objects_remaining: 128,
        original_bytes_remaining: 16 << 20,
        new_pages_remaining: 4096,
        merkle_reads_remaining: 100000,
    }
}
fn facts() -> FactBudget {
    FactBudget {
        facts_remaining: 128,
        original_bytes_remaining: 16 << 20,
        max_fact_bytes: 4 << 20,
    }
}
fn work() -> RuntimeHistoryIndexWorkV1 {
    RuntimeHistoryIndexWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 1 << 20,
    }
}
fn canonical_work() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 100000,
        bytes_remaining: 16 << 20,
        max_depth: 64,
        max_map_keys: 1024,
        max_key_bytes: 1024,
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

/// One actual physical GEN is shared by both endpoint verifiers. No accepted
/// setter, staged flag, local-key receipt or profile-2 persistence is used.
#[test]
fn real_genesis_dual_remote_quorum_durably_accepts_and_promotes_exact_cache() {
    use oasis7_distfs::controlled_authority::fencing::TrustedFencingPolicyV1;
    use oasis7_distfs::controlled_authority::formal::endpoint::{
        FormalArchiveReadWorkV1, FormalQueryOutcomeV1, FormalRequestStateV1,
    };
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
            let authorization = || FormalEndpointAuthorizationV1 {
                authority: authority.clone(),
                original: authority.clone(),
                recovery: None,
                parent: None,
            };
            let mut primary = FileFormalEndpoint::open_protected(
                &pdir,
                authorization(),
                EndpointRole::Primary,
                primary_provider.clone(),
                None,
            )
            .unwrap();
            let mut replica = FileFormalEndpoint::open_protected(
                &rdir,
                authorization(),
                EndpointRole::Replica,
                replica_provider.clone(),
                None,
            )
            .unwrap();
            primary
                .install_iws_page_store(
                    IwsPageStoreCapabilityV1::new(
                        primary_pages.take().unwrap(),
                        endpoint_material::budgets(),
                    )
                    .unwrap()
                    .with_current_limits(limits.clone())
                    .unwrap(),
                )
                .unwrap();
            replica
                .install_iws_page_store(
                    IwsPageStoreCapabilityV1::new(
                        replica_pages.take().unwrap(),
                        endpoint_material::budgets(),
                    )
                    .unwrap()
                    .with_current_limits(limits.clone())
                    .unwrap(),
                )
                .unwrap();
            let request = &candidate.candidate.record.request_digest;
            primary
                .supply_iws_candidate_work(endpoint_material::source_work())
                .unwrap();
            replica
                .supply_iws_candidate_work(endpoint_material::source_work())
                .unwrap();
            // The production source API succeeds directly. No expected error
            // from old guards is used as coordinator control flow.
            let context = primary.stage_genesis_source(candidate, || false).unwrap();
            let replica_context = replica.stage_genesis_source(candidate, || false).unwrap();
            assert_eq!(context.record(), replica_context.record());
            assert!(matches!(
                primary.query(request).unwrap(),
                FormalQueryOutcomeV1::NotRecorded
            ));
            assert!(matches!(
                replica.query(request).unwrap(),
                FormalQueryOutcomeV1::NotRecorded
            ));
            use oasis7_chain_runtime::iws_genesis::{
                GenesisEndpointsV1, GenesisSubmissionStageV1, submit_staged_genesis_bounded,
            };
            let work = FormalArchiveReadWorkV1::new(1_000_000_000, 16u64 << 30).unwrap();
            let cancelled = submit_staged_genesis_bounded(
                candidate,
                &context,
                GenesisEndpointsV1 {
                    primary: &mut primary,
                    replica: &mut replica,
                    primary_provider: primary_provider.as_ref(),
                    replica_provider: replica_provider.as_ref(),
                    primary_policy: &policy,
                    replica_policy: &policy,
                },
                &memory,
                &work,
                &mut || true,
            )
            .err()
            .unwrap();
            assert_eq!(cancelled.stage(), GenesisSubmissionStageV1::Admission);
            assert!(matches!(
                primary.query(request).unwrap(),
                FormalQueryOutcomeV1::NotRecorded
            ));
            assert!(matches!(
                replica.query(request).unwrap(),
                FormalQueryOutcomeV1::NotRecorded
            ));
            let result = submit_staged_genesis_bounded(
                candidate,
                &context,
                GenesisEndpointsV1 {
                    primary: &mut primary,
                    replica: &mut replica,
                    primary_provider: primary_provider.as_ref(),
                    replica_provider: replica_provider.as_ref(),
                    primary_policy: &policy,
                    replica_policy: &policy,
                },
                &memory,
                &work,
                &mut || false,
            )
            .unwrap();
            let expected =
                formal::verify_formal_qualified(result.proof(), authority, None, None).unwrap();
            assert_eq!(result.primary().head(), &expected.head());
            assert_eq!(result.replica().head(), &expected.head());
            assert!(result.uses_budget(&memory));
            for endpoint in [&mut primary, &mut replica] {
                let FormalQueryOutcomeV1::Recorded(state) = endpoint.query(request).unwrap() else {
                    panic!("durable Qualified missing")
                };
                let FormalRequestStateV1::Qualified {
                    proof: installed, ..
                } = state.as_ref()
                else {
                    panic!("durable entry not Qualified")
                };
                assert_eq!(installed, result.proof());
            }
            assert_eq!(
                primary.accepted_genesis_record_id().unwrap(),
                Some(expected.verified_record_id())
            );
            assert_eq!(
                replica.accepted_genesis_record_id().unwrap(),
                Some(expected.verified_record_id())
            );
            let before_primary = std::fs::read(pdir.join("formal-checkpoint-v1.cbor")).unwrap();
            let before_replica = std::fs::read(rdir.join("formal-checkpoint-v1.cbor")).unwrap();
            // Accepted exact retry reads retained full proofs before prepare;
            // no source-work reset, re-signing or fresh staged lease is needed.
            let retry = submit_staged_genesis_bounded(
                candidate,
                &context,
                GenesisEndpointsV1 {
                    primary: &mut primary,
                    replica: &mut replica,
                    primary_provider: primary_provider.as_ref(),
                    replica_provider: replica_provider.as_ref(),
                    primary_policy: &policy,
                    replica_policy: &policy,
                },
                &memory,
                &work,
                &mut || false,
            )
            .unwrap();
            assert_eq!(retry.proof(), result.proof());
            assert_eq!(retry.primary().head(), result.primary().head());
            assert_eq!(retry.replica().head(), result.replica().head());
            assert_eq!(
                std::fs::read(pdir.join("formal-checkpoint-v1.cbor")).unwrap(),
                before_primary
            );
            assert_eq!(
                std::fs::read(rdir.join("formal-checkpoint-v1.cbor")).unwrap(),
                before_replica
            );
        },
    )
    .unwrap();
}
