//! Real native-ELF GEN integration setup. Call only from the native fixture
//! binary, whose original worker dispatch can execute this exact captured ELF.
//! C0/remote keys are explicit protocol fixtures, not physical custody evidence.
use super::{formal_iws_genesis::*, incremental_genesis::*};
use crate::{
    iws_genesis::{GenesisEndpointsV1, submit_staged_genesis_bounded},
    iws_host_runtime::{HostGenesisIoWorkV1, IwsHostRuntimeV1},
};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::{
    controlled_authority::{
        fencing::{RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1, directory_fingerprint},
        formal::endpoint::{
            FileFormalEndpoint, FormalArchiveReadWorkV1, FormalEndpointAuthorizationV1,
            IwsCurrentLimitsV1, IwsPageStoreCapabilityV1,
        },
    },
    incremental_page_store::{
        IncrementalPageStore, PageCompression, PageIoBudget, genesis::GenesisIndexWorkV1,
    },
};
use oasis7_formal_protocol::{
    controlled_authority::{
        fencing::EndpointInstanceBindingV1,
        formal::{
            self, AuthorityDefinitionV1, EndpointSignerBindingV1, FormalWorldIdentityV1,
            InitialActivationBodyV1, InitialActivationTrustV1, SignedInitialActivationV1,
            VerifiedAuthorityV1, bounded_context::FormalBoundedContextV1, test_fixtures,
        },
        replicated_protocol::{ArtifactRole, EndpointRole},
    },
    incremental_execution::TransactionLimits,
    incremental_history::GenesisConfigV1,
    incremental_state::StateScanBudget,
    incremental_value::ValueIoBudget,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    AuthorizationRootCacheLimitsV1, NormalHostGenesisAuthorityV1, NormalHostWorkV1,
    ReleaseSecurityPolicy, RuntimeSourceCompiler, WorldRuntimeMemoryLimits,
    incremental_genesis::RuntimeGenesisWorkV1,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_wasm_executor::bounded_worker::WorkerLimitsV1;
use oasis7_world_state::{
    incremental_cognition_history::CognitionHistoryIndexWorkV1,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_cognition_proof_ledger::ProofLedgerWriteWorkV1,
    incremental_cognition_retention_ledger::CognitionRetentionWorkV1,
    incremental_field_member::MemberRecordLimitsV1,
    incremental_json_records::{JsonChangeWorkV1, JsonReadWorkV1, JsonWriteWorkV1},
    incremental_runtime_history_index::RuntimeHistoryIndexWorkV1,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_semantic_key::SemanticKeyBudget,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[path = "../../../tests/support/genesis_endpoint_work.rs"]
mod endpoint_work;
#[path = "incremental_genesis_test_work.rs"]
mod producer_work;

pub(crate) struct GenesisFixturePathsV1 {
    pub source_pages: PathBuf,
    pub primary_pages: PathBuf,
    pub replica_pages: PathBuf,
    pub primary_endpoint: PathBuf,
    pub replica_endpoint: PathBuf,
    /// Unauthenticated original seven-field projection wire. Cold must compare
    /// it with its actual new accepted parent, including after normal commits.
    pub authorization_hint: PathBuf,
}

pub(crate) struct GenuineGenesisHostsV1 {
    pub primary: IwsHostRuntimeV1,
    pub replica: IwsHostRuntimeV1,
    pub memory: MemoryBudget,
    pub archive: FormalArchiveReadWorkV1,
    pub runtime_work: [NormalHostWorkV1; 2],
    pub genesis_io: [HostGenesisIoWorkV1; 2],
    pub transfer: IwsGenesisTransferWorkV1,
    pub producer: RuntimeGenesisWorkV1,
    pub primary_provider: Arc<RemoteMonotonicEndpointProvider>,
    pub replica_provider: Arc<RemoteMonotonicEndpointProvider>,
    pub authority: VerifiedAuthorityV1,
    pub certificate: SignedInitialActivationV1,
    pub trust: InitialActivationTrustV1,
    pub writer: SigningKey,
    pub pinned: TrustedFencingPolicyV1,
    pub release_policy: ReleaseSecurityPolicy,
    pub configuration: WorldRuntimeMemoryLimits,
    pub current_limits: IwsCurrentLimitsV1,
    pub projection_limits: AuthorizationRootCacheLimitsV1,
    pub paths: GenesisFixturePathsV1,
    pub seed_receipt: serde_json::Value,
    pub genesis_record: [u8; 32],
    pub genesis_body: [u8; 32],
    pub transaction: TransactionLimits,
    pub stop: AtomicBool,
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn debug(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

pub(crate) fn worker_limits() -> WorkerLimitsV1 {
    WorkerLimitsV1 {
        address_space_bytes: 512 << 20,
        max_request_bytes: 1 << 20,
        max_output_bytes: 4096,
        wall_time_ms: 10_000,
    }
}
pub(crate) fn transaction_limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 1_000_000,
        max_writes: 100_000,
        max_facts: 100_000,
        max_pending_node_bytes: 16 << 20,
    }
}

/// One finite source-reading allowance for one actual GEN/cold admission.
/// The caller keeps this value across refusals; creating it again is not retry
/// recovery. A fully released cold request has its own independent Common.
pub(crate) fn cognition_startup_work() -> crate::iws_host_runtime::HostCognitionStartupWorkV1 {
    crate::iws_host_runtime::HostCognitionStartupWorkV1 {
        core: endpoint_work::core_budget(),
        facts: endpoint_work::facts(),
        router: endpoint_work::source_work().router,
        fields: endpoint_work::logical(),
        keys: oasis7_world_state::incremental_semantic_key::SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        },
        json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 1_000_000,
            json_nodes_remaining: 1_000_000,
            max_depth: 64,
            max_payload_bytes: 32 << 20,
        },
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
    }
}
fn member_limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 4096,
        payload_bytes: 8 << 20,
        record_bytes: 16 << 20,
    }
}
fn canonical() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 512 << 20,
        max_depth: 64,
        max_map_keys: 4096,
        max_key_bytes: 4096,
    }
}
fn transfer_work() -> IwsGenesisTransferWorkV1 {
    let mut outer_reads = producer_work::physical_io();
    outer_reads.reads_remaining = 26_000_000;
    IwsGenesisTransferWorkV1 {
        objects_physical: std::array::from_fn(|_| producer_work::physical_io()),
        objects_logical: std::array::from_fn(|_| producer_work::logical()),
        page_publish: std::array::from_fn(|_| producer_work::physical_io()),
        outer_reads,
        outer_scan: StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 100_000,
        },
        canonical: canonical(),
        originals_remaining: 100_000,
        max_original_bytes: 8 << 20,
        mailbox_search_steps: 100_000,
    }
}
fn json_read() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_depth: 64,
        max_payload_bytes: 8 << 20,
    }
}
fn json_write() -> JsonWriteWorkV1 {
    JsonWriteWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_json_depth: 64,
        max_pending_node_bytes: 8 << 20,
    }
}
fn source_history_work() -> GenesisSourceHistoryWorkV1 {
    GenesisSourceHistoryWorkV1 {
        physical: std::array::from_fn(|_| producer_work::physical_io()),
        values: producer_work::logical(),
        keys: SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        },
        limits: member_limits(),
        runtime: RuntimeHistoryIndexWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        cognition: CognitionHistoryIndexWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        hashes: CognitionHeadHashWorkV1 {
            input_bytes_remaining: 128 << 20,
            hash_nodes_remaining: 1_000_000,
        },
        proof: ProofLedgerWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            json: json_write(),
        },
    }
}
fn host_io() -> HostGenesisIoWorkV1 {
    HostGenesisIoWorkV1 {
        process_canonical: canonical(),
        state: endpoint_work::physical(),
        values: endpoint_work::physical(),
        members: endpoint_work::physical(),
        originals: endpoint_work::physical(),
        original_values: endpoint_work::logical(),
    }
}

fn endpoint_binding(
    world: &FormalWorldIdentityV1,
    role: EndpointRole,
    key: u8,
    path: &Path,
) -> Result<EndpointSignerBindingV1, String> {
    let directory = directory_fingerprint(path).map_err(error)?;
    // These are explicit endpoint configuration identities derived from the
    // actual test namespace; storage_fingerprint is independently checked by
    // the original protected endpoint and monotonic provider.
    let identity = |domain: &str| -> Result<String, String> {
        let raw = serde_cbor::to_vec(&(domain, role, path.as_os_str().as_encoded_bytes()))
            .map_err(error)?;
        Ok(blake3::hash(&raw).to_hex().to_string())
    };
    Ok(EndpointSignerBindingV1 {
        instance: EndpointInstanceBindingV1 {
            schema_version: 1,
            world_id: world.world_id.clone(),
            chain_id: world.chain_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            authority_epoch: 1,
            endpoint_role: role,
            endpoint_id: format!("genesis-fixture-endpoint-{key}"),
            endpoint_public_key: test_fixtures::public(key),
            instance_id: identity("fixture.instance")?,
            storage_id: identity("fixture.storage")?,
            storage_fingerprint: directory,
        },
        fault_domain_id: format!("genesis-fixture-domain-{key}"),
    })
}

fn approve(
    inputs: &ProvisionalGenesisExecutionInputsV1,
    paths: &GenesisFixturePathsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<
    (
        SignedInitialActivationV1,
        InitialActivationTrustV1,
        VerifiedAuthorityV1,
    ),
    String,
> {
    let source = inputs
        .business()
        .projected()
        .business_source_bounded_v1(common)
        .map_err(error)?;
    let identity = source.identity();
    let world = FormalWorldIdentityV1 {
        world_id: identity.0.into(),
        chain_id: identity.1.into(),
        genesis_digest: identity
            .2
            .ok_or("actual GEN identity has no genesis reference")?
            .into(),
    };
    drop(source);
    let recovery_policy = test_fixtures::policy();
    let definition = AuthorityDefinitionV1 {
        schema_version: 1,
        world: world.clone(),
        profile: formal::FORMAL_PROFILE.into(),
        profile_version: formal::incremental::IWS_AUTHORITY_PROFILE_VERSION,
        epoch: 1,
        writer_public_key: test_fixtures::public(4),
        primary: endpoint_binding(&world, EndpointRole::Primary, 5, &paths.primary_endpoint)?,
        replica: endpoint_binding(&world, EndpointRole::Replica, 6, &paths.replica_endpoint)?,
        recovery_policy_digest: recovery_policy.digest().map_err(error)?,
        predecessor_authority_id: None,
        valid_from_position: 1,
    };
    let business = inputs.business();
    let artifact_roots: BTreeMap<_, _> = [
        (
            ArtifactRole::IncrementalGenesisConfig,
            business.config_original().0.id,
        ),
        (
            ArtifactRole::IncrementalRootSet,
            business.roots_original().0.id,
        ),
        (
            ArtifactRole::ExecutionManifest,
            inputs.manifest_original().0.id,
        ),
        (
            ArtifactRole::NativeExecutionEnvironment,
            inputs.native_original().0.id,
        ),
        (ArtifactRole::Wasm, business.wasm_catalog_original().0.id),
        (ArtifactRole::Rules, inputs.rules_catalog_original().0.id),
    ]
    .into_iter()
    .map(|(role, digest)| (role, hex::encode(digest)))
    .collect();
    let body = InitialActivationBodyV1 {
        schema_version: formal::incremental::IWS_ACTIVATION_SCHEMA,
        authority: definition,
        recovery_policy,
        initial_state_root: hex::encode(business.roots_original().0.id),
        runtime_manifest_digest: hex::encode(inputs.manifest_original().0.id),
        governing_manifest_digest: hex::encode(inputs.governing_manifest()),
        execution_policy_digest: hex::encode(inputs.policy_original().0.id),
        artifact_roots,
    };
    let signing_bytes = formal::initial_activation_signing_bytes(&body).map_err(error)?;
    let certificate = SignedInitialActivationV1 {
        body,
        signature_hex: hex::encode(test_fixtures::key(9).sign(&signing_bytes).to_bytes()),
    };
    let trust = InitialActivationTrustV1 {
        issuer_public_key: test_fixtures::public(9),
        world,
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().map_err(error)?,
    };
    // This original verifier, rather than a fixture Accepted constructor,
    // supplies the independently approved C0 used by every later endpoint.
    let authority = formal::verify_initial_activation(&certificate, &trust).map_err(error)?;
    Ok((certificate, trust, authority))
}

/// Providers remain live throughout `run`, including after every first-phase
/// Host/World/Node/P owner has been dropped and the caller opens a fresh Cold
/// request. No World or provisional/accepted source escapes in the fixture DATA.
pub(crate) fn with_genuine_genesis_hosts<T>(
    root: &Path,
    run: impl FnOnce(GenuineGenesisHostsV1) -> Result<T, String>,
) -> Result<T, String> {
    fs::create_dir_all(root).map_err(error)?;
    let paths = GenesisFixturePathsV1 {
        source_pages: root.join("source-pages"),
        primary_pages: root.join("primary-pages"),
        replica_pages: root.join("replica-pages"),
        primary_endpoint: root.join("primary"),
        replica_endpoint: root.join("replica"),
        authorization_hint: root.join("authorization.cbor"),
    };
    fs::create_dir(&paths.primary_endpoint).map_err(error)?;
    fs::create_dir(&paths.replica_endpoint).map_err(error)?;
    let memory = MemoryBudget::new(2usize << 30, 32 << 20).map_err(error)?;
    let archive = FormalArchiveReadWorkV1::new(1_000_000_000, 16u64 << 30).map_err(error)?;
    let stop = AtomicBool::new(false);
    let _initial_input = memory
        .reserve(32 << 20, MemoryClass::Ordinary)
        .map_err(error)?;
    let captured = CapturedGenesisNativeV1::capture()?;
    let native_id = captured
        .original_digest_bounded(
            &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
        )
        .map_err(error)?;
    // Original fixture configuration bytes, not claimed state/commit digests.
    let initial = serde_cbor::to_vec(&(
        rule_test_support::SENDER,
        rule_test_support::RECEIVER,
        rule_test_support::INITIAL_SENDER_ELECTRICITY,
        rule_test_support::INITIAL_RECEIVER_ELECTRICITY,
        rule_test_support::INITIAL_RECEIVER_DATA,
        rule_test_support::MODULE_ID,
        rule_test_support::MODULE_VERSION,
    ))
    .map_err(error)?;
    let approval_policy = serde_cbor::to_vec(&test_fixtures::policy()).map_err(error)?;
    let config = GenesisConfigV1 {
        world_id: "iws-genuine-host-transfer".into(),
        format_profile:
            oasis7_world_state::incremental_world_format::complete_world_format_profile_v1(),
        initial_content: *blake3::hash(&initial).as_bytes(),
        activation_config: *blake3::hash(&approval_policy).as_bytes(),
        execution_environment: native_id,
    };
    let seed = rule_test_support::governed_transfer_seed(&config);
    let configuration = WorldRuntimeMemoryLimits::default();
    let world = seed.world.with_runtime_memory_limits(configuration.clone());
    let release_policy = world.release_security_policy().clone();
    let source_pages =
        IncrementalPageStore::create(&paths.source_pages, memory.clone(), PageCompression::Raw)
            .map_err(error)?;
    let primary_pages =
        IncrementalPageStore::create(&paths.primary_pages, memory.clone(), PageCompression::Raw)
            .map_err(error)?;
    let replica_pages =
        IncrementalPageStore::create(&paths.replica_pages, memory.clone(), PageCompression::Raw)
            .map_err(error)?;
    let mut transfer = transfer_work();
    let mut producer = producer_work::work();
    let mut values = producer_work::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut retention_read = json_read();
    let mut retention_producer = json_write();
    let mut retention_changed = JsonChangeWorkV1 {
        json: json_read(),
        paths_remaining: 1_000_000,
        max_pending_pairs: 4096,
    };
    let mut retention_processing = BusinessDiffProcessingWorkV1 {
        steps_remaining: 1_000_000,
        bytes_remaining: 64 << 20,
    };
    let inputs = prepare_projected_iws_genesis_inputs_v1(
        ProjectedGenesisInputsRequestV1 {
            world: &world,
            native: &captured,
            source: &source_pages,
            targets: [&primary_pages, &replica_pages],
            config: &config,
        },
        &mut RuntimeGenesisPhysicalContextV1 {
            memory: &memory,
            values: &mut values,
            keys: &mut keys,
            limits: member_limits(),
            cancelled: &stop,
        },
        &mut producer,
        producer_work::physical(),
        source_history_work(),
        &mut transfer,
        &mut CognitionRetentionWorkV1 {
            json: &mut retention_read,
            producer: &mut retention_producer,
            changed: &mut retention_changed,
            processing: &mut retention_processing,
        },
        &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
    )
    .map_err(error)?;
    let mut current_limits = endpoint_work::limits();
    // Initial conservative scan allowance derived once from actual finished P.
    // It is never refilled after a denial or inferred from remaining Common.
    let bootstrap_nodes = inputs
        .business()
        .projected()
        .roots()
        .state
        .iter()
        .try_fold(0u64, |total, root| total.checked_add(root.leaf_count))
        .and_then(|n| n.checked_mul(257))
        .and_then(|n| {
            n.checked_mul(
                inputs
                    .business()
                    .projected()
                    .projected()
                    .genesis()
                    .typed
                    .world_state
                    .native_fields as u64,
            )
        })
        .ok_or("GEN actual Current scan bound")?;
    current_limits.current.nodes_remaining =
        current_limits.current.nodes_remaining.max(bootstrap_nodes);
    let projection_limits = AuthorizationRootCacheLimitsV1 {
        max_wire_bytes: 16 << 20,
        max_depth: 64,
        max_map_keys: 4096,
        max_key_bytes: 4096,
    };
    let projection_primary = world
        .prepare_iws_authorization_projection_data_v1(
            projection_limits,
            &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
            MemoryClass::Ordinary,
        )
        .map_err(error)?;
    let projection_replica = world
        .prepare_iws_authorization_projection_data_v1(
            projection_limits,
            &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
            MemoryClass::Ordinary,
        )
        .map_err(error)?;
    let mut hint = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&paths.authorization_hint)
        .map_err(error)?;
    projection_primary
        .write_hint_for_fixture_v1(
            &mut hint,
            &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
        )
        .map_err(error)?;
    hint.sync_all().map_err(error)?;
    drop(hint);
    let retention = world.normal_journal_retention_v1(&memory).map_err(error)?;
    let (certificate, trust, authority) = approve(
        &inputs,
        &paths,
        &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
    )?;
    let pinned = TrustedFencingPolicyV1 {
        provider_public_key: authority.policy().fencing_provider_public_key.clone(),
        authorized_resource_bindings: authority
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let mut runtime_work = [
        NormalHostWorkV1::new_bounded(
            oasis7_runtime::two_agent_host_work_limits_v1(),
            &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
        )
        .map_err(debug)?,
        NormalHostWorkV1::new_bounded(
            oasis7_runtime::two_agent_host_work_limits_v1(),
            &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
        )
        .map_err(debug)?,
    ];
    let mut genesis_io = [host_io(), host_io()];
    let writer = test_fixtures::key(4);
    let mut output = None;
    FileFormalEndpoint::with_genesis_remote_pair_for_fixture(
        &authority,
        |primary_provider, replica_provider| {
            output = Some((|| {
                let authorization = || FormalEndpointAuthorizationV1 {
                    authority: authority.clone(),
                    original: authority.clone(),
                    recovery: None,
                    parent: None,
                };
                let mut primary = FileFormalEndpoint::open_protected(
                    &paths.primary_endpoint,
                    authorization(),
                    EndpointRole::Primary,
                    primary_provider.clone(),
                    None,
                )
                .map_err(error)?;
                let mut replica = FileFormalEndpoint::open_protected(
                    &paths.replica_endpoint,
                    authorization(),
                    EndpointRole::Replica,
                    replica_provider.clone(),
                    None,
                )
                .map_err(error)?;
                let native = captured.stage(&mut primary, &mut replica)?;
                let package = prepare_iws_genesis_candidate_v1(
                    inputs,
                    GenesisCandidateRequestV1 {
                        authority: &authority,
                        certificate: &certificate,
                        writer: &writer,
                        native,
                        stores: [&source_pages, &primary_pages, &replica_pages],
                    },
                    &mut transfer,
                    &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
                    &stop,
                )
                .map_err(error)?;
                primary
                    .install_iws_page_store(
                        IwsPageStoreCapabilityV1::new(primary_pages, endpoint_work::budgets())
                            .map_err(error)?
                            .with_current_limits(current_limits.clone())
                            .map_err(error)?,
                    )
                    .map_err(error)?;
                replica
                    .install_iws_page_store(
                        IwsPageStoreCapabilityV1::new(replica_pages, endpoint_work::budgets())
                            .map_err(error)?
                            .with_current_limits(current_limits.clone())
                            .map_err(error)?,
                    )
                    .map_err(error)?;
                primary
                    .supply_iws_candidate_work(endpoint_work::source_work())
                    .map_err(error)?;
                replica
                    .supply_iws_candidate_work(endpoint_work::source_work())
                    .map_err(error)?;
                let candidate = package
                    .candidate(
                        &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
                    )
                    .map_err(error)?;
                let primary_context = primary
                    .stage_genesis_source_bounded(
                        candidate,
                        &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
                    )
                    .map_err(error)?;
                let replica_context = replica
                    .stage_genesis_source_bounded(
                        candidate,
                        &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
                    )
                    .map_err(error)?;
                if primary_context.record() != replica_context.record() {
                    return Err("actual dual GEN source context mismatch".into());
                }
                let submitted = submit_staged_genesis_bounded(
                    candidate,
                    &primary_context,
                    GenesisEndpointsV1 {
                        primary: &mut primary,
                        replica: &mut replica,
                        primary_provider: primary_provider.as_ref(),
                        replica_provider: replica_provider.as_ref(),
                        primary_policy: &pinned,
                        replica_policy: &pinned,
                    },
                    &memory,
                    &archive,
                    &mut || stop.load(Ordering::Acquire),
                )
                .map_err(error)?;
                if submitted.primary().head() != submitted.replica().head()
                    || !submitted.uses_budget(&memory)
                {
                    return Err("actual dual GEN accepted head mismatch".into());
                }
                drop(submitted);
                drop(primary_context);
                drop(replica_context);
                let primary_source = primary
                    .into_accepted_genesis_host_source_bounded(
                        primary_provider.as_ref(),
                        &pinned,
                        &memory,
                        &archive,
                        &mut || stop.load(Ordering::Acquire),
                    )
                    .map_err(error)?;
                let replica_source = replica
                    .into_accepted_genesis_host_source_bounded(
                        replica_provider.as_ref(),
                        &pinned,
                        &memory,
                        &archive,
                        &mut || stop.load(Ordering::Acquire),
                    )
                    .map_err(error)?;
                let genesis_record = primary_source.record_id();
                let genesis_body = primary_source.fence().parent_commit;
                if replica_source.record_id() != genesis_record
                    || replica_source.fence() != primary_source.fence()
                {
                    return Err("actual dual GEN Host source mismatch".into());
                }
                let projected = package
                    .into_projected_bounded_v1(
                        &mut archive.bounded_context(&memory, &mut || stop.load(Ordering::Acquire)),
                    )
                    .map_err(error)?;
                let mut primary = IwsHostRuntimeV1::default();
                let mut replica = IwsHostRuntimeV1::default();
                let compiler = RuntimeSourceCompiler::default();
                let (genesis, metadata) = primary
                    .install_accepted_genesis_projected_bounded(
                        primary_source,
                        worker_limits(),
                        projection_primary,
                        projected,
                        NormalHostGenesisAuthorityV1::Governed,
                        retention,
                        &mut genesis_io[0],
                        &mut runtime_work[0],
                        &compiler,
                        transaction_limits(),
                        &memory,
                        &archive,
                        &stop,
                    )
                    .map_err(error)?;
                replica
                    .install_accepted_genesis_replica_from_primary_bounded(
                        &primary,
                        replica_source,
                        worker_limits(),
                        projection_replica,
                        &genesis,
                        NormalHostGenesisAuthorityV1::Governed,
                        &mut genesis_io[1],
                        &mut runtime_work[1],
                        transaction_limits(),
                        &memory,
                        &archive,
                        &stop,
                    )
                    .map_err(error)?;
                drop(metadata);
                drop(genesis);
                drop(world);
                drop(source_pages);
                drop(_initial_input);
                eprintln!(
                    "REAL_GEN_DUAL_RESIDENTS record={} body={} world_clock=3 body_height=0",
                    hex::encode(genesis_record),
                    hex::encode(genesis_body)
                );
                run(GenuineGenesisHostsV1 {
                    primary,
                    replica,
                    memory,
                    archive,
                    runtime_work,
                    genesis_io,
                    transfer,
                    producer,
                    primary_provider,
                    replica_provider,
                    authority: authority.clone(),
                    certificate,
                    trust,
                    writer,
                    pinned,
                    release_policy,
                    configuration,
                    current_limits,
                    projection_limits,
                    paths,
                    seed_receipt: seed.receipt,
                    genesis_record,
                    genesis_body,
                    transaction: transaction_limits(),
                    stop,
                })
            })());
        },
    )
    .map_err(error)?;
    output.ok_or_else(|| "genuine GEN provider callback missing".to_owned())?
}
