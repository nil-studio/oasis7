//! Real signed Node commits, hot-window compaction and isolated driver replay.
use super::super::super::preview_world_history::{
    PublicationBudget, ReplayBudget, create_scratch, reconstruct, rehydrate,
};
use super::*;

fn budget(heights: u64) -> ReplayBudget {
    ReplayBudget {
        max_heights: heights,
        max_total_bytes: 64 * 1024 * 1024,
        deadline: Instant::now() + Duration::from_secs(60),
        diagnostic_sink: None,
    }
}

fn publication_budget() -> PublicationBudget {
    PublicationBudget {
        max_store_bytes: 512 * 1024 * 1024,
        max_new_bytes: 32 * 1024 * 1024,
        max_store_objects: 4096,
        max_history_cuts: 4,
        before_pin: None,
    }
}

#[test]
fn preview_history_reconstructs_compacted_original_without_changing_source() {
    let directory = Directory::new();
    let options = directory.options();
    let receipt = initialize(&options).unwrap();
    let initial = receipt.published_commit["position"].as_u64().unwrap();
    let target = initial + 1;
    let bootstrap = bridge::derive_service_execution_bootstrap(
        &options.root.join("world"),
        &options.root.join("records"),
        &options.root.join("store"),
        &options.world_id,
    )
    .unwrap();
    let driver = bridge::NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        options.root.join("state.json"),
        options.root.join("world"),
        options.root.join("records"),
        options.root.join("store"),
        &StorageProfileConfig::default(),
        bootstrap.clone(),
    )
    .unwrap();
    // Signing belongs only to this genuine fresh-world fixture. The history
    // reader itself has no key path or signing API.
    let (key, _) =
        super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &options.key_dir,
        )
        .unwrap();
    let config = preview_node_config(&options, &key, &options.root.join("replication")).unwrap();
    let mut node = NodeRuntime::new(config)
        .with_execution_hook(driver)
        .with_local_execution_bootstrap(bootstrap);
    // A well-formed signed Scheduler request with no Agent authority must be
    // canonically rejected. It still exercises the real driver's identity
    // sidecar read and retains the original correlated business result.
    use oasis7::world_service::*;
    let initial_view = bridge::world_service_read::pin(
        &options.root.join("records"),
        &options.root.join("store"),
        &receipt.world,
        None,
    )
    .unwrap();
    let payload = WorldServicePayloadV1::Scheduler(
        sign_read_request(
            "scheduler",
            SchedulerIntentV1 {
                task_prefix_binding_revision: 0,
                agent_id: "history-unregistered-agent".into(),
                request_id: "history-rejected-settle".into(),
                delegation_generation: 0,
                captured_base_binding: initial_view.world.current_cognition_base_binding().unwrap(),
                operation: SchedulerOperationV1::SettleLease {
                    lease_id: "history-missing-lease".into(),
                    consumed_amount: 1,
                },
            },
            &hex::encode([44; 32]),
        )
        .unwrap(),
    );
    let request = SubmitIntentRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: derive_correlation(receipt.world.clone(), &payload).unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    };
    let result_key = correlation::key_digest(&request.correlation.key).unwrap();
    node.submit_consensus_action_payload(
        9001,
        correlation::encode_consensus_intent(&request).unwrap(),
    )
    .unwrap();
    node.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut expected = None;
    while node.snapshot().consensus.committed_height < target + 33 && Instant::now() < deadline {
        if expected.is_none()
            && options
                .root
                .join("records")
                .join(format!("{target:020}.json"))
                .exists()
        {
            let original = bridge::world_service_read::pin_at_height_with_context(
                &options.root.join("records"),
                &options.root.join("store"),
                &receipt.world,
                target,
                &bridge::world_service_read::RestoreContext::default(),
            )
            .unwrap();
            let result = original
                .world
                .capability_revocation_state()
                .world_service_results
                .get(&result_key)
                .expect("first real block must contain the submitted original")
                .clone();
            assert!(result["rejected"].is_string());
            expected = Some((original.commit, original.world.state().time, result));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    node.stop().unwrap();
    assert!(node.snapshot().consensus.committed_height >= target + 33);
    let target_path = options
        .root
        .join("records")
        .join(format!("{target:020}.json"));
    let original_header = fs::read(&target_path).unwrap();
    let compacted: Value = serde_json::from_slice(&original_header).unwrap();
    assert!(compacted["snapshot_ref"].is_null());
    assert!(
        bridge::world_service_read::pin_retained_at_height(
            &options.root.join("records"),
            &options.root.join("store"),
            &receipt.world,
            target,
        )
        .unwrap()
        .is_none()
    );
    let current = fs::read(options.root.join("state.json")).unwrap();
    let original_receipt = fs::read(options.root.join("initialization-receipt.json")).unwrap();
    // Exercise genuine material loss too, not just cleared header fields.
    let (expected, expected_tick, expected_result) = expected.unwrap();
    let _ = fs::remove_file(
        options
            .root
            .join("store/blobs")
            .join(format!("{}.blob", expected.state_root_ref)),
    );
    let restored = reconstruct(
        &options.root,
        &directory.0.join("scratch"),
        &options.world_id,
        &options.node_id,
        &receipt.service_public_key,
        target,
        budget(1),
        None,
    )
    .unwrap();
    assert_eq!(restored.commit, expected);
    assert_eq!(restored.world.state().time, expected_tick);
    assert!(
        restored
            .world
            .capability_revocation_state()
            .world_service_results
            .get(&result_key)
            == Some(&expected_result),
        "reconstructed original rejection must be exact"
    );
    assert!(fs::read(&target_path).unwrap() == original_header);
    assert!(fs::read(options.root.join("state.json")).unwrap() == current);
    assert!(
        fs::read(options.root.join("initialization-receipt.json")).unwrap() == original_receipt
    );

    // Source identity loss/substitution must never be repaired or replaced by
    // replay. These are fixture faults; each invocation leaves its input alone.
    let identity_path = options.root.join("world/world-service-identity.json");
    let original_identity = fs::read(&identity_path).unwrap();
    fs::remove_file(&identity_path).unwrap();
    assert!(
        reconstruct(
            &options.root,
            &directory.0.join("missing-identity"),
            &options.world_id,
            &options.node_id,
            &receipt.service_public_key,
            target,
            budget(1),
            None
        )
        .is_err()
    );
    assert!(!identity_path.exists());
    fs::write(&identity_path, &original_identity).unwrap();
    let mut wrong_identity: Value = serde_json::from_slice(&original_identity).unwrap();
    wrong_identity["genesis_digest"] = json!("blake3:".to_owned() + &"00".repeat(32));
    let wrong_identity_bytes = serde_json::to_vec(&wrong_identity).unwrap();
    fs::write(&identity_path, &wrong_identity_bytes).unwrap();
    assert!(
        reconstruct(
            &options.root,
            &directory.0.join("wrong-identity"),
            &options.world_id,
            &options.node_id,
            &receipt.service_public_key,
            target,
            budget(1),
            None
        )
        .is_err()
    );
    assert!(fs::read(&identity_path).unwrap() == wrong_identity_bytes);
    fs::write(&identity_path, &original_identity).unwrap();

    let source = oasis7_runtime::LocalCasStore::new(options.root.join("store"));
    let pin_path = bridge::historical_material::shard(&source, target);
    let mut no_new = publication_budget();
    no_new.max_new_bytes = 0;
    assert!(
        rehydrate(
            &options.root,
            &directory.0.join("publication-quota"),
            &options.world_id,
            &options.node_id,
            &receipt.service_public_key,
            target,
            budget(1),
            no_new,
            None
        )
        .is_err()
    );
    assert!(!pin_path.exists());
    let mut partial = publication_budget();
    partial.before_pin = Some(|| Err("injected publication stop before pin".into()));
    assert!(
        rehydrate(
            &options.root,
            &directory.0.join("publication-partial"),
            &options.world_id,
            &options.node_id,
            &receipt.service_public_key,
            target,
            budget(1),
            partial,
            None
        )
        .is_err()
    );
    assert!(!pin_path.exists());
    assert!(
        bridge::world_service_read::pin_retained_at_height(
            &options.root.join("records"),
            &options.root.join("store"),
            &receipt.world,
            target
        )
        .unwrap()
        .is_none()
    );
    assert!(
        !pin_path.exists(),
        "read must not heal a partial publication"
    );
    let published = rehydrate(
        &options.root,
        &directory.0.join("publication-complete"),
        &options.world_id,
        &options.node_id,
        &receipt.service_public_key,
        target,
        budget(1),
        publication_budget(),
        None,
    )
    .unwrap();
    assert_eq!(published, expected);
    let historical = bridge::world_service_read::pin_retained_at_height(
        &options.root.join("records"),
        &options.root.join("store"),
        &receipt.world,
        target,
    )
    .unwrap()
    .unwrap();
    assert_eq!(historical.commit, expected);
    assert!(
        historical
            .world
            .capability_revocation_state()
            .world_service_results
            .get(&result_key)
            == Some(&expected_result)
    );
    assert!(
        bridge::historical_material::protected_heights(&options.root.join("records"), &source)
            .unwrap()
            .contains(&target)
    );
    source.prune_orphan_blobs().unwrap();
    assert!(
        bridge::world_service_read::pin_retained_at_height(
            &options.root.join("records"),
            &options.root.join("store"),
            &receipt.world,
            target
        )
        .unwrap()
        .is_some()
    );
    let restored_blob = bridge::historical_material::cas(
        &oasis7_runtime::LocalCasStore::new(directory.0.join("scratch/store")),
        &expected.state_root_ref,
    )
    .unwrap()
    .unwrap();
    fs::remove_file(
        source
            .blobs_dir()
            .join(format!("{}.blob", expected.state_root_ref)),
    )
    .unwrap();
    assert!(
        bridge::world_service_read::pin_retained_at_height(
            &options.root.join("records"),
            &options.root.join("store"),
            &receipt.world,
            target
        )
        .unwrap()
        .is_none(),
        "a cached successful read cannot hide subsequently missing CAS"
    );
    oasis7_runtime::BlobStore::put(&source, &expected.state_root_ref, &restored_blob).unwrap();
    let original_pins = fs::read(&pin_path).unwrap();
    fs::remove_file(&pin_path).unwrap();
    assert!(
        bridge::world_service_read::pin_retained_at_height(
            &options.root.join("records"),
            &options.root.join("store"),
            &receipt.world,
            target
        )
        .unwrap()
        .is_none()
    );
    assert!(
        !pin_path.exists(),
        "query must never recreate a missing pin"
    );
    fs::write(&pin_path, &original_pins).unwrap();
    fs::write(&pin_path, br#"{"pins":[]}"#).unwrap();
    assert!(
        bridge::world_service_read::pin_retained_at_height(
            &options.root.join("records"),
            &options.root.join("store"),
            &receipt.world,
            target
        )
        .is_err()
    );
    fs::write(&pin_path, &original_pins).unwrap();
    let mut conflicting: bridge::ExecutionBridgeRecord =
        serde_json::from_slice(&original_header).unwrap();
    conflicting.snapshot_ref = Some("00".repeat(32));
    assert!(bridge::historical_material::hydrate(&conflicting, &source).is_err());
    assert!(fs::read(&target_path).unwrap() == original_header);
    assert!(fs::read(options.root.join("state.json")).unwrap() == current);
    assert!(
        fs::read(options.root.join("initialization-receipt.json")).unwrap() == original_receipt
    );
    {
        let _live_owner = super::super::super::world_writer_lock::acquire_live_world_writer_lock(
            &options.root.join("world"),
        )
        .unwrap();
        let stopped_scratch = directory.0.join("writer-still-live");
        assert!(
            rehydrate(
                &options.root,
                &stopped_scratch,
                &options.world_id,
                &options.node_id,
                &receipt.service_public_key,
                target,
                budget(1),
                publication_budget(),
                None
            )
            .is_err()
        );
        assert!(!stopped_scratch.exists());
    }

    // An explicit local capability still cannot change this public profile's
    // hardened restore policy. It is not inferred from a receipt or snapshot.
    let local = oasis7_runtime::DevLocalParticipantAuthority::new(
        oasis7_proto::storage_profile::StorageProfile::DevLocal,
        &oasis7_runtime::ReleaseSecurityPolicy::default(),
        receipt.service_public_key.clone(),
    )
    .unwrap();
    let policy_scratch = directory.0.join("wrong-restore-policy");
    assert!(
        reconstruct(
            &options.root,
            &policy_scratch,
            &options.world_id,
            &options.node_id,
            &receipt.service_public_key,
            target,
            budget(1),
            Some(local)
        )
        .is_err()
    );
    assert!(!policy_scratch.exists());

    // Corrupt a real signed envelope; rejection must precede scratch creation.
    let envelope_path = options
        .root
        .join("replication/replication_commit_messages")
        .join(format!("{target:020}.json"));
    let envelope_bytes = fs::read(&envelope_path).unwrap();
    let mut envelope: Value = serde_json::from_slice(&envelope_bytes).unwrap();
    envelope["signature_hex"] = json!("00".repeat(64));
    fs::write(&envelope_path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    let rejected_scratch = directory.0.join("bad-signature");
    assert!(
        reconstruct(
            &options.root,
            &rejected_scratch,
            &options.world_id,
            &options.node_id,
            &receipt.service_public_key,
            target,
            budget(1),
            None,
        )
        .is_err()
    );
    assert!(!rejected_scratch.exists());
    fs::write(envelope_path, envelope_bytes).unwrap();
    assert!(
        reconstruct(
            &options.root,
            &directory.0.join("bounded"),
            &options.world_id,
            &options.node_id,
            &receipt.service_public_key,
            target,
            budget(0),
            None,
        )
        .is_err()
    );
    assert!(!directory.0.join("bounded").exists());
}

#[test]
#[ignore = "Explicit source/pins/height required; never searches user directories or sends requests."]
fn preview_history_explicit_preserved_world_diagnostic() {
    let required =
        |name| std::env::var(name).expect("explicit preview history diagnostic input missing");
    let root = PathBuf::from(required("OASIS7_PREVIEW_HISTORY_ROOT"));
    let scratch = PathBuf::from(required("OASIS7_PREVIEW_HISTORY_SCRATCH"));
    let world = required("OASIS7_PREVIEW_HISTORY_WORLD");
    let node = required("OASIS7_PREVIEW_HISTORY_NODE");
    let pin = required("OASIS7_PREVIEW_HISTORY_SERVICE_PIN");
    let target: u64 = required("OASIS7_PREVIEW_HISTORY_HEIGHT").parse().unwrap();
    let heights: u64 = required("OASIS7_PREVIEW_HISTORY_MAX_HEIGHTS")
        .parse()
        .unwrap();
    let local = std::env::var("OASIS7_PREVIEW_HISTORY_OPERATOR_PIN")
        .ok()
        .map(|operator| {
            oasis7_runtime::DevLocalParticipantAuthority::new(
                oasis7_proto::storage_profile::StorageProfile::DevLocal,
                &oasis7_runtime::ReleaseSecurityPolicy::default(),
                operator,
            )
            .expect("explicit diagnostic local authority invalid")
        });
    let mut diagnostic_budget = budget(heights);
    #[cfg(unix)]
    {
        diagnostic_budget.diagnostic_sink = Some(record_private_driver_error);
    }
    let outcome = reconstruct(
        &root,
        &scratch,
        &world,
        &node,
        &pin,
        target,
        diagnostic_budget,
        local.clone(),
    );
    if outcome
        .as_ref()
        .err()
        .is_some_and(|error| error == "archived World restore rejected: distributed_validation")
    {
        #[cfg(unix)]
        capture_private_restore_reason(&root, local);
    }
    // Never let a raw Runtime error enter the test runner's panic output.
    assert!(
        outcome.is_ok(),
        "preview history diagnostic rejected; inspect explicit private diagnostic if supplied"
    );
    let restored = outcome.ok().unwrap();
    assert_eq!(restored.commit.position, target);
    assert_eq!(restored.commit.world.world_id, world);
    // Numeric progress only. No World, action, goal, key or receipt contents.
    eprintln!("preview_history_verified_height={target}");
}

#[cfg(unix)]
#[test]
fn preview_history_scratch_rejects_source_and_ancestor_alias_without_writes() {
    use std::os::unix::fs::symlink;
    let directory = Directory::new();
    let source = directory.0.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("original"), b"unchanged").unwrap();
    let alias = directory.0.join("ancestor-alias");
    symlink(&source, &alias).unwrap();
    for scratch in [source.join("scratch"), alias.join("scratch")] {
        assert!(create_scratch(&source, &scratch).is_err());
        assert!(!scratch.exists());
    }
    // Even when the source spelling has an aliased ancestor, comparison uses
    // the physical canonical source, not its supplied lexical prefix.
    assert!(create_scratch(&alias, &source.join("scratch")).is_err());
    assert!(!source.join("scratch").exists());
    assert_eq!(fs::read(source.join("original")).unwrap(), b"unchanged");
    assert_eq!(fs::read_dir(&source).unwrap().count(), 1);
}

#[cfg(unix)]
fn capture_private_restore_reason(
    root: &Path,
    local: Option<oasis7_runtime::DevLocalParticipantAuthority>,
) {
    let receipt: Value =
        serde_json::from_slice(&fs::read(root.join("initialization-receipt.json")).unwrap())
            .unwrap();
    let policy = if receipt
        .get("provider")
        .is_some_and(|value| !value.is_null())
    {
        oasis7_runtime::ReleaseSecurityPolicy::default()
    } else {
        oasis7_runtime::ReleaseSecurityPolicy::production_hardened()
    };
    let context = bridge::world_service_read::RestoreContext::new(policy, local).unwrap();
    // This is reached only after reconstruct authenticated the original archive
    // and returned the specific fixed restore class. No raw error is printed.
    let archive = root.join("genesis/initial-publication");
    let snapshot = oasis7_world_state::codec::snapshot_from_cbor(
        &fs::read(archive.join("snapshot.cbor")).unwrap(),
    )
    .unwrap();
    let journal = oasis7_world_state::codec::journal_from_cbor(
        &fs::read(archive.join("journal.cbor")).unwrap(),
    )
    .unwrap();
    let reason = match context.restore(snapshot, journal) {
        Err(oasis7_runtime::WorldError::DistributedValidationFailed { reason }) => reason,
        _ => panic!("private diagnostic restore class changed"),
    };
    let written = write_private_diagnostic(root, &reason).is_ok();
    eprintln!("preview_history_private_restore_diagnostic_written={written}");
}

#[cfg(unix)]
fn record_private_driver_error(root: &Path, stage: &'static str, height: u64, error: &str) {
    let stage = match stage {
        "deterministic_execution" => "deterministic_execution",
        "proof_effect_equality" => "proof_effect_equality",
        _ => "unclassified",
    };
    let written = write_private_diagnostic(root, error).is_ok();
    eprintln!(
        "preview_history_failure_stage={stage} failed_height={height} private_diagnostic_written={written}"
    );
}

#[cfg(unix)]
fn write_private_diagnostic(root: &Path, reason: &str) -> Result<(), ()> {
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    let destination =
        PathBuf::from(std::env::var_os("OASIS7_PREVIEW_HISTORY_PRIVATE_DIAGNOSTIC").ok_or(())?);
    let parent = destination.parent().ok_or(())?;
    let metadata = fs::symlink_metadata(parent).map_err(|_| ())?;
    // SAFETY: geteuid has no arguments and no memory side effects.
    let owner = unsafe { libc::geteuid() };
    if !destination.is_absolute()
        || !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o777 != 0o700
        || metadata.uid() != owner
    {
        return Err(());
    }
    let canonical = parent.canonicalize().map_err(|_| ())?;
    if canonical != parent || canonical.starts_with(root.canonicalize().map_err(|_| ())?) {
        return Err(());
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(destination)
        .map_err(|_| ())?;
    output
        .write_all(&reason.as_bytes()[..reason.len().min(16 * 1024)])
        .map_err(|_| ())
}

#[cfg(unix)]
#[test]
fn preview_history_pin_namespace_symlinks_refuse_reads_and_publication() {
    use std::os::unix::fs::symlink;
    for alias_kind in ["pin-parent", "pin-scope", "blobs"] {
        let directory = Directory::new();
        let root = directory.0.join("source");
        fs::create_dir(&root).unwrap();
        let storage = root.join("store");
        fs::create_dir(&storage).unwrap();
        fs::create_dir(storage.join("blobs")).unwrap();
        fs::write(storage.join("blobs/source-canary"), b"source unchanged").unwrap();
        let external = directory.0.join("external");
        fs::create_dir(&external).unwrap();
        let scope = bridge::historical_material::SCOPE;
        let external_scope = if alias_kind == "pin-parent" {
            fs::create_dir(external.join(scope)).unwrap();
            symlink(&external, storage.join("pin_scopes")).unwrap();
            external.join(scope)
        } else if alias_kind == "pin-scope" {
            fs::create_dir(storage.join("pin_scopes")).unwrap();
            symlink(&external, storage.join("pin_scopes").join(scope)).unwrap();
            external.clone()
        } else {
            // The pin suffix is deliberately absent: blob admission must run
            // before any early "no pin" return in reader/GC/publication.
            fs::rename(storage.join("blobs"), external.join("moved-blobs")).unwrap();
            symlink(external.join("moved-blobs"), storage.join("blobs")).unwrap();
            external.clone()
        };
        let external_pin = external_scope.join(format!("{:020}.json", 17));
        fs::write(&external_pin, b"not a pin: must never be parsed").unwrap();
        fs::create_dir(root.join("records")).unwrap();
        let header = serde_json::to_vec(&json!({"schema_version":3,"world_id":"namespace-world",
            "height":17,"node_block_hash":null,"execution_block_hash":"11".repeat(32),
            "execution_state_root":"22".repeat(32),"journal_len":0,"timestamp_ms":0}))
        .unwrap();
        let record_path = root.join("records").join(format!("{:020}.json", 17));
        fs::write(&record_path, &header).unwrap();
        let identity = oasis7::world_service::WorldIdentity {
            world_id: "namespace-world".into(),
            genesis_digest: "namespace-genesis".into(),
        };
        let source = oasis7_runtime::LocalCasStore::new(&storage);
        let expected_error = if alias_kind == "blobs" {
            "historical blob namespace carrier invalid"
        } else {
            "historical pin namespace carrier invalid"
        };
        let error = bridge::world_service_read::pin_retained_at_height(
            &root.join("records"),
            &storage,
            &identity,
            17,
        )
        .err()
        .expect("reader must reject the namespace before parsing external bytes");
        assert_eq!(error, expected_error);
        assert_eq!(
            bridge::historical_material::protected_heights(&root.join("records"), &source)
                .err()
                .unwrap(),
            expected_error
        );
        let scratch = directory.0.join("scratch");
        let error = rehydrate(
            &root,
            &scratch,
            "namespace-world",
            "node",
            &"33".repeat(32),
            17,
            budget(13),
            publication_budget(),
            None,
        )
        .err()
        .unwrap();
        assert_eq!(error, expected_error);
        assert!(!scratch.exists());
        assert!(
            !root.join("world").exists(),
            "rejection precedes writer-lock or replay setup"
        );
        assert_eq!(fs::read_dir(storage.join("blobs")).unwrap().count(), 1);
        assert!(fs::read(storage.join("blobs/source-canary")).unwrap() == b"source unchanged");
        assert!(fs::read(&record_path).unwrap() == header);
        assert!(fs::read(&external_pin).unwrap() == b"not a pin: must never be parsed");
        assert_eq!(
            fs::read_dir(&external_scope).unwrap().count(),
            if alias_kind == "blobs" { 2 } else { 1 }
        );
    }
}
