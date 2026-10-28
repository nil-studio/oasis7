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
    let (mut certificate, _, legacy, _) = fixture::initial();
    let config = GenesisConfigV1 {
        world_id: "world".into(),
        format_profile: [1; 32],
        initial_content: [2; 32],
        activation_config: [3; 32],
        execution_environment: [4; 32],
    };
    let scope = config.scope().unwrap();
    let roots = RootSetV1::empty(scope.world_id.clone(), scope.genesis_config).unwrap();
    let state = hex::encode(roots.digest().unwrap());
    certificate.body.schema_version = IWS_ACTIVATION_SCHEMA;
    certificate.body.authority.profile_version = IWS_AUTHORITY_PROFILE_VERSION;
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
        (ArtifactRole::Wasm, fixture::h(47)),
        (ArtifactRole::Rules, fixture::h(48)),
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
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let history = HeaderPreparation {
        config: &config,
        catalog: &CatalogRootV1::empty(scope).unwrap(),
        roots: &roots,
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &[ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Input,
            id: input_id,
        }],
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
    let mut frozen = fixture::frozen(&setup.authority, &setup.commit, None);
    frozen.reservation.body.schema_version = IWS_FROZEN_SCHEMA;
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
