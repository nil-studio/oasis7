//! Real signed profile-v2 archives consumed through the production exact resolver.
use super::*;
use crate::execution_bridge_real_tests::real_execution_bridge::governance_authorization_tests as governance;
use crate::formal_live_service::FormalLiveHistoryResolver;
use crate::formal_read_fixtures as fixtures;
use oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeReader;
use oasis7_distfs::controlled_authority::formal::verify_record_material;
use oasis7_runtime::{Action, GovernanceValidatorAdmissionStatus};
use serde::Serialize;
use std::{fs, os::unix::fs::PermissionsExt, sync::Arc};

struct EmptyNodes;
impl BusinessIndexNodeReader for EmptyNodes {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}
static NEXT_ARCHIVE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct Archive {
    root: PathBuf,
    resolver: NativeHistoryResolver,
}
impl Archive {
    fn new(origin: &VerifiedFormalReadAnchorV1) -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-governance-history-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_ARCHIVE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("published")).unwrap();
        for directory in [&root, &root.join("published")] {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let resolver = NativeHistoryResolver {
            addresses: ["127.0.0.1:1".parse().unwrap(); 2],
            timeout: Duration::from_secs(10),
            authentication: fixtures::key(4),
            external_origin: origin.clone(),
            trusted: origin.clone(),
            permission: None,
            active: origin.authority().clone(),
            permissions: vec![],
            release: ReleaseSecurityPolicy::default(),
            business_reader: Arc::new(EmptyNodes),
            writer_directory: root.clone(),
            origin_history_directory: None,
            activation_markers:
                super::super::durable_set_activation::DurableSetActivationMarkerStoreV1::new(
                    root.clone(),
                ),
            prewarm_scope: Arc::new(()),
            governance_verified: std::sync::Mutex::new(None),
        };
        Self { root, resolver }
    }
    fn save(&self, candidate: &FormalCandidateMaterialV1, proof: &FormalQualifiedCommitProofV1) {
        #[derive(Serialize)]
        struct Published<'a> {
            schema_version: u32,
            candidate: &'a FormalCandidateMaterialV1,
            proof: &'a FormalQualifiedCommitProofV1,
        }
        let path = self
            .root
            .join("published")
            .join(format!("{:020}.cbor", proof.decision.record.position));
        fs::write(
            &path,
            serde_cbor::to_vec(&Published {
                schema_version: 1,
                candidate,
                proof,
            })
            .unwrap(),
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn restore(
        &self,
        target: &VerifiedFormalReadAnchorV1,
    ) -> Result<FormalReadPublication, FormalReadError> {
        self.resolver
            .publication_from_exact_replay(&FormalReadAnchorRefV1 {
                position: target.head().record().position,
                record_id: target.head().head().record_id,
                active_authority_id: target.authority().authority_id().unwrap(),
            })
    }
}
impl Drop for Archive {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            assert_eq!(
                error.kind(),
                std::io::ErrorKind::NotFound,
                "archive cleanup: {error}"
            );
        }
    }
}

#[test]
fn exact_resolver_restores_signed_governance_across_distinct_qualified_parents() {
    let (mut world, mut anchor, mut candidate, mut proof) = governance::history_origin(1);
    let archive = Archive::new(&anchor);
    archive.save(&candidate, &proof);
    let commands = [
        governance::action("history"),
        Action::ApproveGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "history".into(),
        },
    ];
    let mut parents = Vec::new();
    for (index, command) in commands.into_iter().enumerate() {
        let before = world.clone();
        let authorization = governance::signed(&world, &command, index as u64 + 1);
        parents.push((
            authorization.parent_position,
            authorization.parent_record_id.clone(),
        ));
        governance::begin(&mut world);
        world.submit_governance_action(command, authorization);
        governance::execute(&mut world).unwrap();
        let (next, next_anchor, next_candidate, next_proof) =
            governance::history_successor(&before, &anchor, world, &candidate, &proof);
        archive.save(&next_candidate, &next_proof);
        candidate = next_candidate;
        proof = next_proof;
        world = next;
        anchor = next_anchor;
    }
    assert_eq!(parents[0].0 + 1, parents[1].0);
    assert_ne!(parents[0].1, parents[1].1);
    // All public resolver entrypoints must consume the same authenticated lineage.
    let requested = FormalReadAnchorRefV1 {
        position: anchor.head().record().position,
        record_id: anchor.head().head().record_id,
        active_authority_id: anchor.authority().authority_id().unwrap(),
    };
    let commit = fixtures::commit_ref(&anchor, &fixtures::projection(&world));
    let origin = archive.resolver.external_origin.clone();
    for publication in [
        archive.resolver.origin_publication(&requested).unwrap(),
        archive.resolver.fixed_publication(&commit).unwrap(),
        archive
            .resolver
            .forward_publication(&origin, &anchor, 2)
            .unwrap(),
    ] {
        assert_eq!(publication.anchor().head().head(), anchor.head().head());
        assert_eq!(
            serde_cbor::to_vec(&publication.world().snapshot()).unwrap(),
            serde_cbor::to_vec(&world.snapshot()).unwrap()
        );
    }
    archive.resolver.verify_governance_history(&anchor).unwrap();
    let material = archive.resolver.full_sync_material(&anchor).unwrap();
    verify_record_material(anchor.head().record(), &material).unwrap();
    let publication = archive.restore(&anchor).unwrap();
    let mut restored = publication.world().clone();
    assert_eq!(
        serde_cbor::to_vec(&restored.snapshot()).unwrap(),
        serde_cbor::to_vec(&world.snapshot()).unwrap()
    );
    assert_eq!(
        restored
            .state()
            .governance_authorization_nonces
            .get("controller"),
        Some(&2)
    );
    assert_eq!(
        restored.governance_validator_admissions()["history"].status,
        GovernanceValidatorAdmissionStatus::ApprovedCandidate
    );
    let command = Action::ActivateGovernanceValidatorAdmission {
        controller_account_id: "controller".into(),
        candidate_id: "history".into(),
        activation_epoch: 100,
    };
    let authorization = governance::signed(&restored, &command, 3);
    assert_eq!(
        authorization.parent_record_id,
        anchor.head().head().record_id
    );
    governance::begin(&mut restored);
    restored.submit_governance_action(command, authorization);
    governance::execute(&mut restored).unwrap();
    assert_eq!(
        restored
            .state()
            .governance_authorization_nonces
            .get("controller"),
        Some(&3)
    );
}

#[test]
fn exact_resolver_rejects_requalified_silent_controller_policy_replacement() {
    let (before, origin, candidate, proof) = governance::history_origin(1);
    let archive = Archive::new(&origin);
    archive.save(&candidate, &proof);
    let mut after = before.clone();
    governance::begin(&mut after);
    let mut registry = after
        .governance_main_token_controller_registry()
        .unwrap()
        .clone();
    let policy = registry
        .controller_signer_policies
        .get_mut("controller")
        .unwrap();
    policy
        .allowed_public_keys
        .insert(hex::encode(fixtures::key(42).verifying_key().as_bytes()));
    policy.threshold = 2;
    after
        .set_governance_main_token_controller_registry(registry)
        .unwrap();
    governance::execute(&mut after).unwrap();
    let (_, target, candidate, proof) =
        governance::history_successor(&before, &origin, after.clone(), &candidate, &proof);
    // This is an entirely re-signed, hash-consistent candidate, not corrupt bytes.
    verify_record_material(&proof.decision.record, &candidate.material).unwrap();
    verify_formal_qualified(&proof, origin.authority(), Some(origin.head()), None).unwrap();
    // The old per-cut checks accept this authentic Snapshot and Tick chain.
    // Rejection below therefore exercises cross-commit governance semantics.
    let independently_restored = archive
        .resolver
        .world_for(&target, &candidate.material)
        .unwrap();
    FormalReadPublication::new(target.clone(), independently_restored).unwrap();
    archive.save(&candidate, &proof);
    let error = archive
        .restore(&target)
        .err()
        .expect("silent policy replacement must fail before publication");
    assert!(
        error.to_string().contains("governance"),
        "unexpected rejection layer: {error}"
    );
    assert!(
        archive
            .resolver
            .verify_governance_history(&target)
            .unwrap_err()
            .to_string()
            .contains("governance")
    );
    let requested = FormalReadAnchorRefV1 {
        position: target.head().record().position,
        record_id: target.head().head().record_id,
        active_authority_id: target.authority().authority_id().unwrap(),
    };
    let commit = fixtures::commit_ref(&target, &fixtures::projection(&after));
    for rejected in [
        archive.resolver.origin_publication(&requested).err(),
        archive.resolver.fixed_publication(&commit).err(),
        archive
            .resolver
            .forward_publication(&origin, &target, 1)
            .err(),
    ] {
        assert!(
            rejected
                .expect("no resolver entry may publish unauthenticated governance")
                .to_string()
                .contains("governance")
        );
    }
    assert!(
        archive
            .resolver
            .full_sync_material(&target)
            .unwrap_err()
            .to_string()
            .contains("governance")
    );
}

#[test]
fn exact_resolver_never_publishes_target_with_missing_or_corrupt_intermediate_closure() {
    let (mut world, mut anchor, mut candidate, mut proof) = governance::history_origin(1);
    let archive = Archive::new(&anchor);
    archive.save(&candidate, &proof);
    for nonce in 1..=2 {
        let before = world.clone();
        let command = governance::action(&format!("closure-{nonce}"));
        let authorization = governance::signed(&world, &command, nonce);
        governance::begin(&mut world);
        world.submit_governance_action(command, authorization);
        governance::execute(&mut world).unwrap();
        (world, anchor, candidate, proof) =
            governance::history_successor(&before, &anchor, world, &candidate, &proof);
        archive.save(&candidate, &proof);
    }
    let intermediate = archive
        .root
        .join("published")
        .join(format!("{:020}.cbor", anchor.head().record().position - 1));
    let saved = fs::read(&intermediate).unwrap();
    fs::remove_file(&intermediate).unwrap();
    assert!(
        archive.restore(&anchor).is_err(),
        "missing C1 must not expose C2"
    );
    fs::write(&intermediate, b"corrupt publication closure").unwrap();
    fs::set_permissions(&intermediate, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        archive.restore(&anchor).is_err(),
        "corrupt C1 must not expose C2"
    );
    fs::write(&intermediate, saved).unwrap();
    assert!(
        archive.restore(&anchor).is_ok(),
        "complete repaired fixture must recover exact target"
    );
}

use crate::formal_live_runtime::tests::protected_process_tests::oracle_support;
use oasis7_distfs::controlled_authority::formal::{
    InitialActivationTrustV1, SignedInitialActivationV1, coordinator::FormalCoordinator,
};
type RemoteCoordinator = FormalCoordinator<RemoteFormalEndpoint, RemoteFormalEndpoint>;
struct ProtectedEndpoints {
    _children: [oracle_support::EndpointChild; 2],
    _oracles: [oracle_support::OracleHandle; 2],
}
fn protected_endpoints(
    archive: &mut Archive,
    certificate: &SignedInitialActivationV1,
    trust: &InitialActivationTrustV1,
    candidate: &FormalCandidateMaterialV1,
) -> (
    ProtectedEndpoints,
    RemoteCoordinator,
    FormalQualifiedCommitProofV1,
) {
    let binary = std::env::var_os("OASIS7_FORMAL_ENDPOINT_TEST_BIN")
        .expect("set OASIS7_FORMAL_ENDPOINT_TEST_BIN");
    let pdir = archive.root.join("primary");
    let rdir = archive.root.join("replica");
    if !pdir.exists() {
        oracle_support::private_directory(&pdir).unwrap();
    }
    if !rdir.exists() {
        oracle_support::private_directory(&rdir).unwrap();
    }
    let authority = archive.resolver.active.clone();
    let fixture = oracle_support::Fixture::new(
        archive.root.clone(),
        pdir.clone(),
        rdir.clone(),
        authority.clone(),
    );
    let oracles = [
        oracle_support::start_oracle(
            &fixture,
            oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Primary,
        ),
        oracle_support::start_oracle(
            &fixture,
            oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Replica,
        ),
    ];
    let auth = archive.root.join("oracle-auth");
    oracle_support::write_private(&auth, hex::encode([17u8; 32]).as_bytes()).unwrap();
    let mut children = Vec::new();
    let mut addresses = Vec::new();
    for (index, role) in [
        oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Primary,
        oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Replica,
    ]
    .into_iter()
    .enumerate()
    {
        let config = archive.root.join(format!("endpoint-{index}.json"));
        oracle_support::write_private(
            &config,
            &serde_json::to_vec(&serde_json::json!({
                "schema_version":2, "initial_trust":trust, "initial_certificate":certificate,
                "initial_qualified":null, "recovery_bootstrap":null, "role":role,
                "data_dir":if index == 0 { &pdir } else { &rdir },
                "oracle_authentication_key_file":auth, "oracle_address":oracles[index].address(),
                "listen":"127.0.0.1:0", "timeout_ms":5000
            }))
            .unwrap(),
        )
        .unwrap();
        let (child, address) = oracle_support::start_endpoint(&binary, &config).unwrap();
        children.push(child);
        addresses.push(address);
    }
    archive.resolver.addresses = [addresses[0], addresses[1]];
    let endpoint = |index, role| {
        RemoteFormalEndpoint::new(
            addresses[index],
            Duration::from_secs(10),
            authority.clone(),
            role,
            fixtures::key(4),
            authority.definition().world.genesis_head().unwrap(),
        )
        .unwrap()
    };
    let mut coordinator = FormalCoordinator::new(
        endpoint(
            0,
            oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Primary,
        ),
        endpoint(
            1,
            oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Replica,
        ),
        authority.clone(),
        authority,
        None,
        None,
    )
    .unwrap();
    let proof = coordinator.submit(candidate.clone()).unwrap();
    archive.save(candidate, &proof);
    (
        ProtectedEndpoints {
            _children: children.try_into().ok().unwrap(),
            _oracles: oracles,
        },
        coordinator,
        proof,
    )
}
fn qualify_native(
    coordinator: &mut RemoteCoordinator,
    candidate: &FormalCandidateMaterialV1,
) -> FormalQualifiedCommitProofV1 {
    let frozen = candidate.frozen.as_ref().unwrap();
    let executable = coordinator
        .freeze(
            frozen.reservation.clone(),
            candidate.frozen_material.clone().unwrap(),
        )
        .unwrap();
    assert_eq!(executable.certificate(), frozen);
    coordinator.submit(candidate.clone()).unwrap()
}
#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; protected oracle simulates custody"]
fn governance_history_missing_middle_publication_restores_from_authenticated_native_pair() {
    let (_, seed_anchor, _, _, _, _) = governance::history_origin_with_trust(1);
    let mut archive = Archive::new(&seed_anchor);
    let pdir = archive.root.join("primary");
    let rdir = archive.root.join("replica");
    oracle_support::private_directory(&pdir).unwrap();
    oracle_support::private_directory(&rdir).unwrap();
    let (mut world, mut anchor, mut candidate, _, certificate, trust) =
        governance::history_origin_for_endpoints(1, Some([&pdir, &rdir]));
    archive.resolver.external_origin = anchor.clone();
    archive.resolver.trusted = anchor.clone();
    archive.resolver.active = anchor.authority().clone();
    let (_endpoints, mut coordinator, mut proof) =
        protected_endpoints(&mut archive, &certificate, &trust, &candidate);
    let mut snapshots = Vec::new();
    for (index, command) in [
        governance::action("native-history"),
        Action::ApproveGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "native-history".into(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        let before = world.clone();
        let authorization = governance::signed(&world, &command, index as u64 + 1);
        governance::begin(&mut world);
        world.submit_governance_action(command, authorization);
        governance::execute(&mut world).unwrap();
        (world, anchor, candidate, _) =
            governance::history_successor(&before, &anchor, world, &candidate, &proof);
        proof = qualify_native(&mut coordinator, &candidate);
        archive.save(&candidate, &proof);
        snapshots.push(world.snapshot());
    }
    let missing = archive
        .root
        .join("published")
        .join(format!("{:020}.cbor", anchor.head().record().position - 1));
    // Available authenticated endpoints must not conceal invalid present local material.
    let position = anchor.head().record().position - 1;
    let (valid_middle, middle_proof) =
        crate::execution_bridge::formal_writer::read_published_candidate_at(
            &archive.root,
            position,
        )
        .unwrap()
        .unwrap();
    let mut invalid_middle = valid_middle.clone();
    invalid_middle.material.objects[0].bytes.push(0);
    archive.save(&invalid_middle, &middle_proof);
    let rejected = archive
        .restore(&anchor)
        .err()
        .expect("present corrupt closure must not fall back to healthy native replicas");
    assert!(
        rejected.to_string().contains("local publication closure"),
        "unexpected rejection: {rejected}"
    );
    archive.save(&valid_middle, &middle_proof);
    fs::remove_file(&missing).unwrap();
    assert!(!missing.exists());
    let target_file = archive
        .root
        .join("published")
        .join(format!("{:020}.cbor", anchor.head().record().position));
    fs::remove_file(&target_file).unwrap();
    let commit = fixtures::commit_ref(&anchor, &fixtures::projection(&world));
    let fixed = archive.resolver.fixed_publication(&commit).unwrap();
    assert_eq!(fixed.world().snapshot(), snapshots[1]);
    let page = archive
        .resolver
        .forward_publication(&archive.resolver.external_origin, &anchor, 1)
        .unwrap();
    assert_eq!(page.world().snapshot(), snapshots[0]);
    // A present invalid target must still fail even though both native copies are healthy.
    let mut invalid_target = candidate.clone();
    invalid_target.material.objects[0].bytes.push(0);
    archive.save(&invalid_target, &proof);
    let rejected = archive
        .resolver
        .fixed_publication(&commit)
        .err()
        .expect("invalid present target may not fall back");
    assert!(
        rejected.to_string().contains("local publication closure"),
        "unexpected rejection: {rejected}"
    );
    archive.save(&candidate, &proof);
    let publication = archive.restore(&anchor).unwrap();
    let mut restored = publication.world().clone();
    assert_eq!(
        serde_cbor::to_vec(&restored.snapshot()).unwrap(),
        serde_cbor::to_vec(&world.snapshot()).unwrap()
    );
    assert_eq!(
        restored
            .state()
            .governance_authorization_nonces
            .get("controller"),
        Some(&2)
    );
    let command = Action::ActivateGovernanceValidatorAdmission {
        controller_account_id: "controller".into(),
        candidate_id: "native-history".into(),
        activation_epoch: 100,
    };
    let authorization = governance::signed(&restored, &command, 3);
    governance::begin(&mut restored);
    restored.submit_governance_action(command, authorization);
    governance::execute(&mut restored).unwrap();
    assert_eq!(
        restored
            .state()
            .governance_authorization_nonces
            .get("controller"),
        Some(&3)
    );
}
fn runtime_config(
    archive: &Archive,
    certificate: &SignedInitialActivationV1,
    trust: &InitialActivationTrustV1,
    initial: &FormalCandidateMaterialV1,
    initial_proof: &FormalQualifiedCommitProofV1,
) -> PathBuf {
    let writer_key = archive.root.join("writer-key");
    oracle_support::write_private(
        &writer_key,
        hex::encode(fixtures::key(4).to_bytes()).as_bytes(),
    )
    .unwrap();
    let node_dir = archive.root.join("node-keypair");
    oracle_support::private_directory(&node_dir).unwrap();
    let node_key = fixtures::key(90);
    oracle_support::write_private(
        &node_dir.join("node-keypair.toml"),
        format!(
            "[node]\nprivate_key = \"{}\"\npublic_key = \"{}\"\n",
            hex::encode(node_key.to_bytes()),
            hex::encode(node_key.verifying_key().to_bytes())
        )
        .as_bytes(),
    )
    .unwrap();
    let pin = oasis7::world_service::client::FormalReadTrustConfigV1 {
        initial_trust: trust.clone(),
        checkpoint_digest: archive
            .resolver
            .external_origin
            .checkpoint()
            .digest()
            .unwrap(),
        checkpoint: archive.resolver.external_origin.checkpoint(),
    };
    assert_eq!(&pin.checkpoint.initial_certificate, certificate);
    let pin_file = archive.root.join("trust.json");
    oracle_support::write_private(&pin_file, &serde_json::to_vec(&pin).unwrap()).unwrap();
    let initial_file = archive.root.join("approved-initial.cbor");
    let approved = crate::formal_live_config::ApprovedInitialPublicationV1 {
        schema_version: 1,
        candidate: initial.clone(),
        proof: initial_proof.clone(),
    };
    oracle_support::write_private(&initial_file, &serde_cbor::to_vec(&approved).unwrap()).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let status_bind = listener.local_addr().unwrap();
    drop(listener);
    let path = archive.root.join("runtime.json");
    oracle_support::write_private(&path, &serde_json::to_vec(&serde_json::json!({
        "schema_version":1, "node_id":"node-a", "node_keypair_directory":node_dir,
        "writer_key_file":writer_key, "formal_read_trust_file":pin_file, "writer_directory":archive.root,
        "primary":archive.resolver.addresses[0], "replica":archive.resolver.addresses[1],
        "timeout_ms":30000, "status_bind":status_bind, "approved_initial_file":initial_file,
    })).unwrap()).unwrap();
    path
}
#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; protected oracle simulates custody"]
fn governance_history_runtime_open_rejects_native_qualified_silent_policy_replacement() {
    let (_, origin, _, _, _, _) = governance::history_origin_with_trust(1);
    let mut archive = Archive::new(&origin);
    let pdir = archive.root.join("primary");
    let rdir = archive.root.join("replica");
    oracle_support::private_directory(&pdir).unwrap();
    oracle_support::private_directory(&rdir).unwrap();
    let (before, origin, initial, _, certificate, trust) =
        governance::history_origin_for_endpoints_with_policy(
            1,
            Some([&pdir, &rdir]),
            &ReleaseSecurityPolicy::production_hardened(),
        );
    archive.resolver.external_origin = origin.clone();
    archive.resolver.trusted = origin.clone();
    archive.resolver.active = origin.authority().clone();
    let (_endpoints, mut coordinator, initial_proof) =
        protected_endpoints(&mut archive, &certificate, &trust, &initial);
    let path = runtime_config(&archive, &certificate, &trust, &initial, &initial_proof);
    let mut after = before.clone();
    governance::begin(&mut after);
    let mut registry = after
        .governance_main_token_controller_registry()
        .unwrap()
        .clone();
    registry
        .controller_signer_policies
        .get_mut("controller")
        .unwrap()
        .threshold = 2;
    after
        .set_governance_main_token_controller_registry(registry)
        .unwrap();
    governance::execute(&mut after).unwrap();
    let (_, target, candidate, _) =
        governance::history_successor(&before, &origin, after, &initial, &initial_proof);
    let proof = qualify_native(&mut coordinator, &candidate);
    archive.save(&candidate, &proof);
    archive
        .resolver
        .world_for(&target, &candidate.material)
        .unwrap();
    let error = std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            crate::formal_live_runtime::FormalLiveRuntime::open_with_native_history(&path)
                .err()
                .expect("runtime must reject before owner startup")
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(
        error.to_string().contains("verify restored formal governance history: HistoryUnavailable: governance history authentication:"),
        "unexpected startup rejection: {error}"
    );
}

#[test]
fn malformed_present_publication_is_internal_in_the_native_read_chain() {
    let (_, origin, _, _) = governance::history_origin(1);
    let archive = Archive::new(&origin);
    let path = archive
        .root
        .join("published")
        .join(format!("{:020}.cbor", origin.head().record().position));
    fs::write(&path, b"grant node not ready HistoryUnavailable").unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    // The real private publication reader fails before any endpoint fallback;
    // diagnostic words in its malformed bytes confer no history/grant status.
    assert!(matches!(
        archive.resolver.full_sync_material(&origin),
        Err(FormalReadError::Internal(_))
    ));
    assert!(matches!(
        archive.resolver.origin_publication(&FormalReadAnchorRefV1 {
            position: origin.head().record().position,
            record_id: origin.head().head().record_id,
            active_authority_id: origin.authority().authority_id().unwrap(),
        }),
        Err(FormalReadError::Internal(_))
    ));
}

#[test]
fn governance_budget_callback_deadline_remains_history_unavailable() {
    let (world, origin, candidate, proof) = governance::history_origin(1);
    let archive = Archive::new(&origin);
    archive.save(&candidate, &proof);
    let before_world = world.clone();
    let mut world = world;
    governance::begin(&mut world);
    governance::execute(&mut world).unwrap();
    let (_, anchor, candidate, proof) =
        governance::history_successor(&before_world, &origin, world, &candidate, &proof);
    let deadline = Instant::now() + Duration::from_secs(10);
    let before = archive
        .resolver
        .governance_projection_at(&origin, None, None, deadline)
        .unwrap();
    let next = archive
        .resolver
        .governance_projection_at(&anchor, Some(&proof), Some(&candidate), deadline)
        .unwrap();
    let mut calls = 0;
    let rejected = verify_governance_successor(&before, &next, || {
        calls += 1;
        if calls > 1 {
            remaining(Instant::now())
        } else {
            Ok(())
        }
    });
    assert!(
        calls > 1,
        "the real verifier must reach a repeated budget callback"
    );
    assert!(
        matches!(rejected, Err(FormalReadError::HistoryUnavailable(_))),
        "{rejected:?}"
    );
    assert!(matches!(
        verify_governance_successor(&before, &before, || Ok(())),
        Err(FormalReadError::Integrity(_))
    ));
}
