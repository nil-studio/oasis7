//! Shared cryptographic IWS fixture for protocol and actual storage consumers.
//! No deployment, full material closure or durable-domain evidence is implied.
use super as fixture;
use crate::controlled_authority::formal::{incremental::*, *};
use crate::controlled_authority::replicated_protocol::ArtifactRole;
use crate::incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1, RootSetV1};
use crate::incremental_history::{
    CatalogRootV1, GenesisConfigV1, HeaderPreparation, PreparedHistoryV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
pub struct Empty;
impl crate::incremental_state::MerkleNodeReader for Empty {
    type Node = Vec<u8>;
    fn read_node(&self, _: &str, _: usize) -> Result<Option<Vec<u8>>, String> {
        Err("unexpected nonempty fixture page".into())
    }
}
pub struct Setup {
    pub config: GenesisConfigV1,
    pub roots: RootSetV1,
    pub history: ReservedValue<PreparedHistoryV1>,
    pub authority: VerifiedAuthorityV1,
    pub certificate: SignedInitialActivationV1,
    pub proof: FormalQualifiedCommitProofV1,
    pub commit: VerifiedFormalCommitV1,
    pub memory: MemoryBudget,
}
impl Setup {
    pub fn material(&self) -> CommitMaterialRefs<'_> {
        CommitMaterialRefs {
            config: &self.config,
            roots: &self.roots,
            body: &self.history.body,
            history: &self.history.prefix,
        }
    }
}
pub fn setup() -> Setup {
    setup_with_memory(MemoryBudget::new(32 << 20, 4 << 20).unwrap())
}

pub fn setup_with_memory(memory: MemoryBudget) -> Setup {
    setup_with_native(memory, [4; 32])
}

pub fn setup_with_native(memory: MemoryBudget, native: [u8; 32]) -> Setup {
    setup_with_roots(memory, native, None, None)
}
pub fn setup_with_execution_artifacts(
    memory: MemoryBudget,
    native: [u8; 32],
    manifest: [u8; 32],
    wasm: [u8; 32],
    rules: [u8; 32],
    policy: [u8; 32],
) -> Setup {
    setup_with_roots(memory, native, Some([manifest, wasm, rules, policy]), None)
}
pub fn genesis_config(native: [u8; 32]) -> GenesisConfigV1 {
    GenesisConfigV1 {
        world_id: "world".into(),
        format_profile: [1; 32],
        initial_content: [2; 32],
        activation_config: [3; 32],
        execution_environment: native,
    }
}
pub fn setup_with_state(
    memory: MemoryBudget,
    state: [crate::incremental_state::StateRootV1; 6],
) -> Setup {
    setup_with_roots(memory, [4; 32], None, Some(state))
}
/// Explicit fixture input references, not execution-closure evidence. The
/// NativeExecutionEnvironment role comes only from the supplied GenesisConfig.
pub struct FixtureExecutionArtifactRefsV1 {
    pub manifest: [u8; 32],
    pub wasm: [u8; 32],
    pub rules: [u8; 32],
    pub policy: [u8; 32],
}
/// Build the actual activation, initial header and signed decision from a
/// caller-selected new-format config and all six actual initial state roots.
/// Legacy fixture entry points retain their original config and wire values.
pub fn setup_with_config_and_state(
    memory: MemoryBudget,
    config: GenesisConfigV1,
    state: [crate::incremental_state::StateRootV1; 6],
    refs: FixtureExecutionArtifactRefsV1,
) -> Setup {
    setup_with_config_roots(
        memory,
        config,
        Some([refs.manifest, refs.wasm, refs.rules, refs.policy]),
        Some(state),
    )
}
/// The initial activation Input is generated and signed here. Extra Result or
/// Event references must point at actual caller-persisted originals; this
/// fixture never supplies their Core/FactOriginals verification or source Q.
pub fn setup_with_config_state_and_facts(
    memory: MemoryBudget,
    config: GenesisConfigV1,
    state: [crate::incremental_state::StateRootV1; 6],
    refs: FixtureExecutionArtifactRefsV1,
    extra_genesis_facts: &[ExecutionFactRefV1],
) -> Setup {
    setup_with_config_roots_and_facts(
        memory,
        config,
        Some([refs.manifest, refs.wasm, refs.rules, refs.policy]),
        Some(state),
        extra_genesis_facts,
    )
}
#[cfg(test)]
mod explicit_config_tests {
    use super::*;
    fn digest(value: &str) -> [u8; 32] {
        hex::decode(value).unwrap().try_into().unwrap()
    }
    #[test]
    fn explicit_fixture_keeps_legacy_config_and_signed_wire_identical() {
        let old = setup();
        let new = setup_with_config_and_state(
            MemoryBudget::new(32 << 20, 4 << 20).unwrap(),
            old.config.clone(),
            old.roots.state.clone(),
            FixtureExecutionArtifactRefsV1 {
                manifest: digest(&old.certificate.body.runtime_manifest_digest),
                wasm: [47; 32],
                rules: [48; 32],
                policy: digest(&old.certificate.body.execution_policy_digest),
            },
        );
        assert_eq!(new.config, old.config);
        assert_eq!(new.roots, old.roots);
        assert_eq!(
            new.history.body.canonical_bytes().unwrap(),
            old.history.body.canonical_bytes().unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&new.certificate).unwrap(),
            serde_json::to_vec(&old.certificate).unwrap()
        );
        assert_eq!(
            serde_json::to_vec(&new.proof).unwrap(),
            serde_json::to_vec(&old.proof).unwrap()
        );
    }
    #[test]
    fn explicit_fixture_signs_the_actual_config_scope_state_and_artifacts() {
        let config = GenesisConfigV1 {
            world_id: "explicit-new-format-world".into(),
            format_profile: [81; 32],
            initial_content: [82; 32],
            activation_config: [83; 32],
            execution_environment: [84; 32],
        };
        let scope = config.scope().unwrap();
        let state = RootSetV1::empty(scope.world_id.clone(), scope.genesis_config)
            .unwrap()
            .state;
        let setup = setup_with_config_and_state(
            MemoryBudget::new(32 << 20, 4 << 20).unwrap(),
            config.clone(),
            state.clone(),
            FixtureExecutionArtifactRefsV1 {
                manifest: [85; 32],
                wasm: [86; 32],
                rules: [87; 32],
                policy: [88; 32],
            },
        );
        assert_eq!(setup.config, config);
        assert_eq!(setup.roots.state, state);
        assert_eq!(setup.authority.definition().world.world_id, config.world_id);
        assert_eq!(
            setup.authority.definition().world.genesis_digest,
            hex::encode(scope.genesis_config)
        );
        for role in [
            ArtifactRole::NativeExecutionEnvironment,
            ArtifactRole::IncrementalGenesisConfig,
            ArtifactRole::IncrementalRootSet,
        ] {
            let expected = match role {
                ArtifactRole::NativeExecutionEnvironment => config.execution_environment,
                ArtifactRole::IncrementalGenesisConfig => scope.genesis_config,
                _ => setup.roots.digest().unwrap(),
            };
            assert_eq!(
                setup.commit.record().artifact_roots.get(&role),
                Some(&hex::encode(expected))
            );
        }
        assert_eq!(
            setup.certificate.body.runtime_manifest_digest,
            hex::encode([85; 32])
        );
        assert_eq!(
            setup.certificate.body.execution_policy_digest,
            hex::encode([88; 32])
        );
    }
}
fn setup_with_roots(
    memory: MemoryBudget,
    native: [u8; 32],
    refs: Option<[[u8; 32]; 4]>,
    state_roots: Option<[crate::incremental_state::StateRootV1; 6]>,
) -> Setup {
    setup_with_config_roots(memory, genesis_config(native), refs, state_roots)
}
fn setup_with_config_roots(
    memory: MemoryBudget,
    config: GenesisConfigV1,
    refs: Option<[[u8; 32]; 4]>,
    state_roots: Option<[crate::incremental_state::StateRootV1; 6]>,
) -> Setup {
    setup_with_config_roots_and_facts(memory, config, refs, state_roots, &[])
}
fn setup_with_config_roots_and_facts(
    memory: MemoryBudget,
    config: GenesisConfigV1,
    refs: Option<[[u8; 32]; 4]>,
    state_roots: Option<[crate::incremental_state::StateRootV1; 6]>,
    extra_genesis_facts: &[ExecutionFactRefV1],
) -> Setup {
    let (mut certificate, _, legacy, _) = fixture::initial();
    let refs = refs.unwrap_or_else(|| {
        [
            hex::decode(&certificate.body.runtime_manifest_digest)
                .unwrap()
                .try_into()
                .unwrap(),
            [47; 32],
            [48; 32],
            hex::decode(&certificate.body.execution_policy_digest)
                .unwrap()
                .try_into()
                .unwrap(),
        ]
    });
    certificate.body.runtime_manifest_digest = hex::encode(refs[0]);
    certificate.body.execution_policy_digest = hex::encode(refs[3]);
    let scope = config.scope().unwrap();
    let mut roots = RootSetV1::empty(scope.world_id.clone(), scope.genesis_config).unwrap();
    if let Some(state) = state_roots {
        roots.state = state;
    }
    let state = hex::encode(roots.digest().unwrap());
    certificate.body.schema_version = IWS_ACTIVATION_SCHEMA;
    certificate.body.authority.profile_version = IWS_AUTHORITY_PROFILE_VERSION;
    certificate.body.authority.world.world_id = scope.world_id.clone();
    certificate.body.authority.primary.instance.world_id = scope.world_id.clone();
    certificate.body.authority.replica.instance.world_id = scope.world_id.clone();
    certificate.body.authority.world.genesis_digest = hex::encode(scope.genesis_config);
    certificate.body.authority.primary.instance.genesis_digest = hex::encode(scope.genesis_config);
    certificate.body.authority.replica.instance.genesis_digest = hex::encode(scope.genesis_config);
    certificate.body.initial_state_root = state.clone();
    certificate.body.artifact_roots = [
        (
            ArtifactRole::IncrementalGenesisConfig,
            hex::encode(scope.genesis_config),
        ),
        (ArtifactRole::IncrementalRootSet, state.clone()),
        (
            ArtifactRole::ExecutionManifest,
            certificate.body.runtime_manifest_digest.clone(),
        ),
        (ArtifactRole::Wasm, hex::encode(refs[1])),
        (ArtifactRole::Rules, hex::encode(refs[2])),
        (
            ArtifactRole::NativeExecutionEnvironment,
            hex::encode(config.execution_environment),
        ),
    ]
    .into();
    certificate.signature_hex = fixture::sign(
        initial_activation_signing_bytes(&certificate.body).unwrap(),
        9,
    );
    let trust = InitialActivationTrustV1 {
        issuer_public_key: fixture::public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: state.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
    };
    let authority = verify_initial_activation(&certificate, &trust).unwrap();
    let input_id = *blake3::hash(&initial_activation_input_bytes(&certificate).unwrap()).as_bytes();
    let fact_count = extra_genesis_facts
        .len()
        .checked_add(1)
        .filter(|count| *count <= crate::incremental_execution::EXECUTION_MAX_FACTS)
        .expect("fixture GEN fact limit");
    assert!(
        extra_genesis_facts
            .iter()
            .all(|fact| fact.kind != ExecutionFactKindV1::Input)
    );
    let _facts_phase = memory
        .reserve(
            fact_count
                .checked_mul(std::mem::size_of::<ExecutionFactRefV1>())
                .unwrap()
                .checked_add(256)
                .unwrap(),
            oasis7_proto::memory_budget::MemoryClass::Ordinary,
        )
        .unwrap();
    let mut genesis_facts = Vec::with_capacity(fact_count);
    genesis_facts.push(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: input_id,
    });
    genesis_facts.extend_from_slice(extra_genesis_facts);
    let history = HeaderPreparation {
        config: &config,
        catalog: &CatalogRootV1::empty(scope).unwrap(),
        roots: &roots,
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &genesis_facts,
        operation: [51; 32],
        authority: authority_binding(&authority, &memory).unwrap(),
        execution_environment: config.execution_environment,
        checkpoint: true,
    }
    .prepare(&memory)
    .unwrap();
    let mut record = legacy.decision.record;
    record.schema_version = IWS_RECORD_SCHEMA;
    record.world = authority.definition().world.clone();
    record.parent_record_id = record.world.genesis_head().unwrap().record_id;
    record.original_authority_id = authority.authority_id().unwrap();
    record.activation_ref = authority.activation_ref().into();
    record.before_state_root = state.clone();
    record.after_state_root = state;
    record.payload_digest = hex::encode(input_id);
    record.runtime_manifest_digest = certificate.body.runtime_manifest_digest.clone();
    record.execution_policy_digest = certificate.body.execution_policy_digest.clone();
    record.artifact_roots = certificate.body.artifact_roots.clone();
    record
        .artifact_roots
        .insert(ArtifactRole::Input, hex::encode(input_id));
    record.artifact_roots.insert(
        ArtifactRole::IncrementalCommitBody,
        hex::encode(history.body.digest().unwrap()),
    );
    record.artifact_roots.insert(
        ArtifactRole::IncrementalHistoryPrefix,
        hex::encode(history.prefix.digest().unwrap()),
    );
    let proof = fixture::proof(
        record,
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let commit = verify_formal_qualified(&proof, &authority, None, None).unwrap();
    Setup {
        config,
        roots,
        history,
        authority,
        certificate,
        proof,
        commit,
        memory,
    }
}
pub fn frozen(setup: &Setup) -> FormalFrozenCertificateV1 {
    frozen_with_manifest(setup, [62; 32])
}
pub fn frozen_with_manifest(setup: &Setup, manifest: [u8; 32]) -> FormalFrozenCertificateV1 {
    let mut frozen = fixture::frozen(&setup.authority, &setup.commit, None);
    frozen.reservation.body.schema_version = IWS_FROZEN_SCHEMA;
    frozen.reservation.body.runtime_manifest_digest = hex::encode(manifest);
    frozen.reservation.signature_hex =
        fixture::sign(frozen_signing_bytes(&frozen.reservation.body).unwrap(), 4);
    let id = frozen.reservation.body.reservation_id().unwrap();
    for (receipt, key) in [(&mut frozen.primary, 5), (&mut frozen.replica, 6)] {
        receipt.body.reservation_id = id.clone();
        receipt.signature_hex =
            fixture::sign(frozen_receipt_signing_bytes(&receipt.body).unwrap(), key);
    }
    frozen
}

pub type CoreOriginals = ReservedValue<Vec<std::sync::Arc<ReservedValue<Vec<u8>>>>>;

/// Exact current-core originals and derived catalog, for actual source adapters.
/// Native/input execution semantics and complete world closure are not fixtures.
pub fn core_objects(
    s: &Setup,
    history: &PreparedHistoryV1,
) -> (
    CoreOriginals,
    ReservedValue<crate::incremental_history::CatalogDeltaV1>,
) {
    use oasis7_proto::memory_budget::MemoryClass;
    let mut workspace = s.memory.reserve(128 * 1024, MemoryClass::Ordinary).unwrap();
    let mut blobs = Vec::new();
    let mut push = |bytes: Vec<u8>| {
        let charge = workspace.split(bytes.capacity() + 64).unwrap();
        blobs.push(std::sync::Arc::new(charge.retain(bytes)));
    };
    push(s.config.canonical_bytes().unwrap());
    push(s.roots.canonical_bytes().unwrap());
    push(history.body.canonical_bytes().unwrap());
    push(history.prefix.canonical_bytes().unwrap());
    push(history.segment.canonical_bytes().unwrap());
    push(
        history
            .facts
            .canonical_bytes(&s.memory)
            .unwrap()
            .as_slice()
            .to_vec(),
    );
    if let Some(cp) = &history.checkpoint {
        push(cp.canonical_bytes().unwrap());
    }
    push(initial_activation_input_bytes(&s.certificate).unwrap());
    let catalog = history
        .body
        .catalog_before
        .extend(&history.body, &Empty, &s.memory, 4 << 20)
        .unwrap();
    push(catalog.after.canonical_bytes().unwrap());
    let charge = workspace
        .split(
            blobs.capacity() * std::mem::size_of::<std::sync::Arc<ReservedValue<Vec<u8>>>>() + 256,
        )
        .unwrap();
    (charge.retain(blobs), catalog)
}

/// Tiny original byte fixture. These executable bytes are never run.
pub fn native_fixture() -> (
    crate::native_environment::NativeExecutionEnvironmentV1,
    Vec<Vec<u8>>,
) {
    use crate::native_environment::*;
    let raw = vec![b"native-executable".to_vec(), b"native-build".to_vec()];
    let components = raw
        .iter()
        .enumerate()
        .map(|(index, bytes)| {
            let id = blake3::hash(bytes).to_hex().to_string();
            NativeEnvironmentComponentV1 {
                role: if index == 0 {
                    NativeEnvironmentRole::Executable
                } else {
                    NativeEnvironmentRole::BuildMetadata
                },
                name: format!("original-{index}"),
                byte_len: bytes.len() as u64,
                content_digest: id.clone(),
                chunks: vec![NativeEnvironmentChunkV1 {
                    digest: id,
                    byte_len: bytes.len() as u32,
                }],
            }
        })
        .collect();
    (
        NativeExecutionEnvironmentV1 {
            schema_version: 1,
            target: "test".into(),
            os: "test".into(),
            arch: "test".into(),
            components,
        },
        raw,
    )
}
