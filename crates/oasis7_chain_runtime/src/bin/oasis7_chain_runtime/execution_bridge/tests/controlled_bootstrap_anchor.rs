use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::controlled_authority::{
    LocalRequestIdentity,
    activation::{
        InitialActivationBody, InitialActivationPolicy, SignedInitialActivation,
        initial_activation_signing_bytes, verify_initial_activation,
    },
    replicated_protocol::{
        DurabilityEvidence, EndpointRole, FileEndpoint, FixedTrust, ProtocolOutcome,
        ReplicatedCoordinator,
    },
};
use oasis7_runtime::ChainResourceDerivationContext;
use std::sync::atomic::{AtomicU64, Ordering};
fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn trust() -> FixedTrust {
    FixedTrust {
        world_id: "bootstrap-fixture".into(),
        chain_id: "fixture-chain".into(),
        genesis_digest: "a".repeat(64),
        authority_epoch: 1,
        writer_key: hex::encode(key(1).verifying_key().to_bytes()),
        primary_id: "p".into(),
        primary_key: hex::encode(key(2).verifying_key().to_bytes()),
        replica_id: "r".into(),
        replica_key: hex::encode(key(3).verifying_key().to_bytes()),
    }
}
fn snapshot() -> (Snapshot, Journal) {
    let state = oasis7_runtime::WorldState {
        time: 7,
        ..Default::default()
    };
    // Deliberately distinct simulation tick and execution checkpoint height.
    let world = oasis7::runtime_host::world::new_with_state(state);
    let t = trust();
    let snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: &t.world_id,
            chain_id: &t.chain_id,
            genesis_ref: Some(&t.genesis_digest),
            created_at_height: 0,
            manifest_height: 41,
            commit_block_hash: Some("fixture-block"),
            tick: 7,
        },
        "fixture-config",
        "fixture-generation",
    );
    (snapshot, world.journal().clone())
}
fn package(snapshot: &Snapshot, journal: &Journal, checkpoint_height: u64) -> ClosedRecord {
    let mut objects = BTreeMap::new();
    let mut roots = BTreeMap::new();
    let snapshot_root = insert(&mut objects, to_cbor(snapshot).unwrap(), vec![]);
    let journal_root = insert(&mut objects, to_cbor(journal).unwrap(), vec![]);
    roots.insert(ArtifactRole::Snapshot, snapshot_root.clone());
    roots.insert(ArtifactRole::Journal, journal_root.clone());
    let checkpoint = ExecutionBridgeRecord::new_v3(
        trust().world_id,
        checkpoint_height,
        Some("fixture-node-block".into()),
        Some("fixture-predecessor".into()),
        "fixture-proposer".into(),
        "fixture-action-root".into(),
        blake3_hex(
            &to_cbor(ExecutionHashPayload {
                world_id: &trust().world_id,
                height: checkpoint_height,
                prev_execution_block_hash: "fixture-previous-execution-block",
                execution_state_root: &snapshot_root,
                journal_len: journal.len(),
            })
            .unwrap(),
        ),
        snapshot_root.clone(),
        journal.len(),
        snapshot_root.clone(),
        journal_root,
        None,
        None,
        1000,
    );
    roots.insert(
        ArtifactRole::Result,
        insert(&mut objects, to_cbor(checkpoint).unwrap(), vec![]),
    );
    roots.insert(
        ArtifactRole::NonceIndex,
        insert(
            &mut objects,
            to_cbor(&snapshot.capability_nonce_records).unwrap(),
            vec![],
        ),
    );
    roots.insert(
        ArtifactRole::EffectOutbox,
        insert(
            &mut objects,
            to_cbor((&snapshot.pending_effects, &snapshot.inflight_effects)).unwrap(),
            vec![],
        ),
    );
    roots.insert(
        ArtifactRole::Rules,
        insert(
            &mut objects,
            to_cbor((
                &snapshot.manifest,
                &snapshot.policies,
                ReleaseSecurityPolicy::default(),
            ))
            .unwrap(),
            vec![],
        ),
    );
    let mut wasm_refs = BTreeMap::new();
    for (hash, bytes) in &snapshot.module_artifact_bytes {
        wasm_refs.insert(hash.clone(), insert(&mut objects, bytes.clone(), vec![]));
    }
    roots.insert(
        ArtifactRole::Wasm,
        insert(
            &mut objects,
            to_cbor((
                &snapshot.module_registry,
                &snapshot.module_registry,
                &wasm_refs,
            ))
            .unwrap(),
            wasm_refs.values().cloned().collect(),
        ),
    );
    let t = trust();
    let manifest = ActivationArtifactManifestV1 {
        schema_version: 1,
        scope: SCOPE.into(),
        world_id: t.world_id,
        chain_id: t.chain_id,
        genesis_digest: t.genesis_digest,
        execution_height: 41,
        snapshot_tick: snapshot.state.time,
        previous_execution_block_hash: "fixture-previous-execution-block".into(),
        artifact_roots: roots.clone(),
    };
    let refs = roots.values().cloned().collect();
    roots.insert(
        ArtifactRole::ExecutionManifest,
        insert(&mut objects, to_cbor(manifest).unwrap(), refs),
    );
    ClosedRecord {
        payload_digest: String::new(),
        before_state_root: snapshot_root.clone(),
        after_state_root: snapshot_root,
        roots,
        objects: objects.into_values().collect(),
    }
}
static SERIAL: AtomicU64 = AtomicU64::new(0);
fn seal_full(
    mut record: ClosedRecord,
) -> (
    VerifiedInitialActivation,
    ClosedRecord,
    InitialActivationPolicy,
    DurabilityEvidence,
) {
    // Authorized fixture issuer and local endpoints; no formal-world claim.
    let t = trust();
    let genesis = t.genesis_anchor().unwrap();
    let body = InitialActivationBody {
        schema_version: 1,
        profile: "controlled_single_authority".into(),
        profile_version: 1,
        trust: t.clone(),
        position: 1,
        parent_hash: genesis.decision_hash.clone(),
        activation_height: 41,
        before_state_root: record.before_state_root.clone(),
        after_state_root: record.after_state_root.clone(),
        artifact_roots: record.roots.clone(),
    };
    let signed = SignedInitialActivation {
        signature_hex: hex::encode(
            key(4)
                .sign(&initial_activation_signing_bytes(&body).unwrap())
                .to_bytes(),
        ),
        body,
    };
    let input = object(to_cbor(signed).unwrap(), vec![]);
    record.payload_digest = input.content_hash.clone();
    record
        .roots
        .insert(ArtifactRole::Input, input.content_hash.clone());
    record.objects.push(input);
    record
        .objects
        .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
    let root = std::env::temp_dir().join(format!(
        "oasis7-bootstrap-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let p = FileEndpoint::open(
        root.join("p"),
        t.clone(),
        EndpointRole::Primary,
        key(2),
        &genesis,
    )
    .unwrap();
    let r = FileEndpoint::open(
        root.join("r"),
        t.clone(),
        EndpointRole::Replica,
        key(3),
        &genesis,
    )
    .unwrap();
    let mut coordinator = ReplicatedCoordinator::new(p, r, key(1)).unwrap();
    let request = LocalRequestIdentity {
        verified_subject: "fixture-issuer".into(),
        operation_domain: SCOPE.into(),
        nonce_scope: "initial".into(),
        request_id: "1".into(),
    };
    let ProtocolOutcome::DurabilityQualified(evidence) = coordinator
        .submit(1, &genesis, request, record.clone())
        .unwrap()
    else {
        panic!("qualified fixture")
    };
    let policy = InitialActivationPolicy {
        trust: t,
        issuer_public_key: hex::encode(key(4).verifying_key().to_bytes()),
        initial_state_root: record.before_state_root.clone(),
        execution_manifest_root: record.roots[&ArtifactRole::ExecutionManifest].clone(),
        activation_height: 41,
    };
    let token = verify_initial_activation(
        capture::object(&record, &record.payload_digest).unwrap(),
        &evidence,
        &policy,
        &genesis,
    )
    .unwrap();
    drop(coordinator);
    std::fs::remove_dir_all(root).unwrap();
    (token, record, policy, *evidence)
}
fn seal(record: ClosedRecord) -> (VerifiedInitialActivation, ClosedRecord) {
    let (token, record, _, _) = seal_full(record);
    (token, record)
}
#[test]
#[cfg(unix)]
fn typed_bootstrap_restores_original_checkpoint_with_distinct_tick_and_height() {
    let (s, j) = snapshot();
    let (token, record) = seal(package(&s, &j, 41));
    let verified =
        verify_bootstrap_anchor(&token, &record, &ReleaseSecurityPolicy::default()).unwrap();
    assert_eq!(verified.execution_height(), 41);
    assert_eq!(verified.simulation_tick(), 7);
    assert_eq!(verified.snapshot(), &s);
    assert_eq!(verified.journal(), &j);
    assert_eq!(verified.activation(), &token);
    let mut wrong_policy = ReleaseSecurityPolicy::default();
    wrong_policy.allow_local_finality_signing = !wrong_policy.allow_local_finality_signing;
    assert!(verify_bootstrap_anchor(&token, &record, &wrong_policy).is_err());
    let mut unrelated = record.clone();
    unrelated.after_state_root = "b".repeat(64);
    assert!(
        verify_bootstrap_anchor(&token, &unrelated, &ReleaseSecurityPolicy::default()).is_err()
    );
}
#[test]
#[cfg(unix)]
fn valid_issuer_and_durability_cannot_bless_invalid_runtime_checkpoint() {
    for case in 0..7 {
        let (mut s, j) = snapshot();
        match case {
            0 => s.chain_resource_manifest.genesis_ref = None,
            1 => s.chain_resource_manifest.chain_id = "other".into(),
            2 => s.journal_commitment = "0".repeat(64),
            3 => {
                s.module_artifact_bytes.insert("0".repeat(64), vec![0]);
            }
            4 => s.journal_commitment.clear(),
            5 => s.latest_chain_resource_delta = None,
            _ => s.chain_resource_manifest.manifest_height = 42,
        }
        let (token, record) = seal(package(&s, &j, 41));
        assert!(
            verify_bootstrap_anchor(&token, &record, &ReleaseSecurityPolicy::default()).is_err(),
            "case {case}"
        );
    }
    let (s, j) = snapshot();
    let (token, record) = seal(package(&s, &j, 42));
    assert!(verify_bootstrap_anchor(&token, &record, &ReleaseSecurityPolicy::default()).is_err());
}

fn replace_role(package: &mut ClosedRecord, role: ArtifactRole, bytes: Vec<u8>) {
    let old = package.roots[&role].clone();
    package.objects.retain(|o| o.content_hash != old);
    let o = object(bytes, vec![]);
    let hash = o.content_hash.clone();
    package.objects.push(o);
    package.roots.insert(role, hash.clone());
    if role == ArtifactRole::Snapshot {
        package.before_state_root = hash.clone();
        package.after_state_root = hash;
    }
    let old_manifest = package.roots[&ArtifactRole::ExecutionManifest].clone();
    let mut manifest: ActivationArtifactManifestV1 =
        capture::decode_generic(capture::object(package, &old_manifest).unwrap()).unwrap();
    package.objects.retain(|o| o.content_hash != old_manifest);
    manifest.artifact_roots = package
        .roots
        .iter()
        .filter(|(r, _)| **r != ArtifactRole::ExecutionManifest)
        .map(|(r, h)| (*r, h.clone()))
        .collect();
    let o = object(
        to_cbor(&manifest).unwrap(),
        manifest.artifact_roots.values().cloned().collect(),
    );
    package
        .roots
        .insert(ArtifactRole::ExecutionManifest, o.content_hash.clone());
    package.objects.push(o);
    package
        .objects
        .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
}
#[test]
#[cfg(unix)]
fn opaque_or_oversized_artifacts_and_unbound_objects_fail_closed() {
    let (s, j) = snapshot();
    for malformed in [
        vec![0xff],
        vec![0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
    ] {
        let mut p = package(&s, &j, 41);
        replace_role(&mut p, ArtifactRole::Snapshot, malformed);
        let (t, p) = seal(p);
        assert!(verify_bootstrap_anchor(&t, &p, &ReleaseSecurityPolicy::default()).is_err());
    }
    let mut p = package(&s, &j, 41);
    replace_role(
        &mut p,
        ArtifactRole::NonceIndex,
        to_cbor("opaque-placeholder").unwrap(),
    );
    let (t, p) = seal(p);
    assert!(verify_bootstrap_anchor(&t, &p, &ReleaseSecurityPolicy::default()).is_err());
    let (t, mut p) = seal(package(&s, &j, 41));
    p.objects.push(object(vec![1, 2, 3], vec![]));
    p.objects
        .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
    assert!(verify_bootstrap_anchor(&t, &p, &ReleaseSecurityPolicy::default()).is_err());
    let mut alias = p.clone();
    alias.roots.insert(
        ArtifactRole::Rules,
        alias.roots[&ArtifactRole::NonceIndex].clone(),
    );
    assert!(verify_bootstrap_anchor(&t, &alias, &ReleaseSecurityPolicy::default()).is_err());
}

/// Original bytes are sealed before the subprocess sees any configuration/input.
pub(crate) fn original_cli_fixture() -> (Vec<u8>, Vec<u8>) {
    cli_fixture(false)
}
pub(crate) fn absent_genesis_cli_fixture() -> (Vec<u8>, Vec<u8>) {
    cli_fixture(true)
}
fn cli_fixture(absent_genesis: bool) -> (Vec<u8>, Vec<u8>) {
    let (mut snapshot, journal) = snapshot();
    if absent_genesis {
        snapshot.chain_resource_manifest.genesis_ref = None;
    }
    cli_fixture_with_snapshot(snapshot, journal)
}
pub(crate) fn live_cli_fixture() -> (Vec<u8>, Vec<u8>) {
    let public = oasis7::world_service::sign_read_request("owner", (), &hex::encode([7; 32]))
        .unwrap()
        .subject_public_key;
    let world = super::super::tests::world_service::live_fixture_world(&trust().world_id, &public);
    let t = trust();
    let snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: &t.world_id,
            chain_id: &t.chain_id,
            genesis_ref: Some(&t.genesis_digest),
            created_at_height: 0,
            manifest_height: 41,
            commit_block_hash: Some("fixture-block"),
            tick: world.state().time,
        },
        "fixture-config",
        "fixture-generation",
    );
    cli_fixture_with_snapshot(snapshot, world.journal().clone())
}
fn cli_fixture_with_snapshot(snapshot: Snapshot, journal: Journal) -> (Vec<u8>, Vec<u8>) {
    let (_, _, policy, evidence) = seal_full(package(&snapshot, &journal, 41));
    let config = serde_json::json!({
        "schema_version": 1,
        "issuer_policy": {
            "trust": policy.trust,
            "issuer_public_key": policy.issuer_public_key,
            "initial_state_root": policy.initial_state_root,
            "execution_manifest_root": policy.execution_manifest_root,
            "activation_height": policy.activation_height
        },
        "minimum_head": trust().genesis_anchor().unwrap(),
        "release_security_policy": ReleaseSecurityPolicy::default()
    });
    (
        serde_json::to_vec(&config).unwrap(),
        serde_json::to_vec(&evidence).unwrap(),
    )
}
