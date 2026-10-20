//! End-to-end protected signer handoff through real local endpoint and owner
//! processes. The monotonic oracle and issuer are test simulations, so this
//! proves protocol composition but not external custody or fault domains.
use super::super::*;
use super::{CheckpointParent, HotBody, PrivateRoot, Store};
use ed25519_dalek::SigningKey;
use oasis7_distfs::controlled_authority::{
    formal::{
        coordinator::FormalCoordinator,
        endpoint::{
            FileFormalArchiveSourceV1, SignedFormalArchiveTransferV1,
            VerifiedFormalArchiveTransferV1, verify_formal_archive_transfer,
        },
        fencing::directory_fingerprint,
        issuer::IssuerAnchorV1,
        transport::RemoteFormalEndpoint,
    },
    replicated_protocol::EndpointRole,
};
use oasis7_runtime::{ChainResourceDerivationContext, PolicySet, ReleaseSecurityPolicy};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use super::recovery_test_support;

#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; oracle and issuer are test simulations"]
fn protected_endpoint_children_signer_handoff_stops_exact_owner_and_qualifies_successor() {
    std::thread::Builder::new()
        .name("formal-protected-handoff-fixture".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(run_protected_signer_handoff)
        .expect("spawn protected handoff fixture thread")
        .join()
        .expect("protected handoff fixture thread panicked")
        .unwrap();
}

/// The test harness binary does not understand the production CLI's
/// `op formal-owner-worker` subcommand. The production owner launcher selects
/// this exact ignored test in a child process under cfg(test); it runs the same
/// blocking owner loop as the production entrypoint.
#[test]
#[ignore = "child process entrypoint used only by protected handoff tests"]
fn protected_handoff_owner_worker() {
    let value = |key: &str| {
        std::env::var(key).unwrap_or_else(|_| panic!("missing owner worker variable {key}"))
    };
    let config = PathBuf::from(value("OASIS7_FORMAL_OWNER_TEST_CONFIG"));
    let operation = value("OASIS7_FORMAL_OWNER_TEST_OPERATION");
    let request_digest = value("OASIS7_FORMAL_OWNER_TEST_REQUEST_DIGEST");
    let config_digest = value("OASIS7_FORMAL_OWNER_TEST_CONFIG_DIGEST");
    std::thread::Builder::new()
        .name("formal-protected-owner-worker-fixture".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            crate::formal_live_runtime::run_operation_owner_worker(
                &config,
                &operation,
                &request_digest,
                &config_digest,
            )
        })
        .expect("spawn protected owner worker fixture thread")
        .join()
        .expect("protected owner worker fixture thread panicked")
        .unwrap_or_else(|error| panic!("formal owner worker failed: {error}"));
}

fn run_protected_signer_handoff() -> Result<(), String> {
    let endpoint_binary = std::env::var_os("OASIS7_FORMAL_ENDPOINT_TEST_BIN")
        .ok_or("set OASIS7_FORMAL_ENDPOINT_TEST_BIN to oasis7_durability_endpoint")?;
    let root_guard = PrivateRoot::new()?;
    let root = root_guard.path().to_path_buf();
    let primary_data = root.join("predecessor-primary");
    let replica_data = root.join("predecessor-replica");
    super::private_directory(&primary_data)?;
    super::private_directory(&replica_data)?;

    let release = ReleaseSecurityPolicy::production_hardened();
    let (_policy_material, executable_digest, execution_policy_digest) =
        formal_package::execution_policy_material(&release)?;
    if executable_digest.is_empty() {
        return Err("formal process fixture has no loaded executable digest".into());
    }
    let (mut world, _) = world_with_owner_seed();
    world.set_policy(PolicySet::allow_all());
    world.initialize_empty_formal_history()?;
    let (_seed_world, mut initial_trust, seed_anchor, _) =
        fixtures::actual_world_initial_fixture_with_execution_policy(execution_policy_digest);
    let identity = seed_anchor.authority().definition().world.clone();
    world
        .bind_cognition_runtime(&identity.world_id, "main", 0, None, "pending", 0)
        .map_err(|error| format!("bind handoff World identity: {error:?}"))?;
    let snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: &identity.world_id,
            chain_id: &identity.chain_id,
            genesis_ref: Some(&identity.genesis_digest),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: world.state().time,
        },
        fixtures::h(10),
        fixtures::h(11),
    );
    let world = oasis7_runtime::World::from_snapshot(snapshot, world.journal().clone())
        .map_err(|error| format!("restore handoff genesis World: {error:?}"))?;

    let mut definition = seed_anchor.authority().definition().clone();
    let policy = seed_anchor.authority().policy().clone();
    definition.profile_version = 2;
    definition.primary.instance.storage_fingerprint =
        directory_fingerprint(&primary_data).map_err(|error| error.to_string())?;
    definition.replica.instance.storage_fingerprint =
        directory_fingerprint(&replica_data).map_err(|error| error.to_string())?;
    let state_root = oasis7_runtime::blake3_hex(
        &serde_cbor::to_vec(&world.snapshot()).map_err(|error| error.to_string())?,
    );
    let bootstrap = crate::execution_bridge_real_tests::real_execution_bridge::formal_initial::InitialNodeBootstrapV1 {
        height: world.state().time,
        consensus_block_hash: fixtures::h(81),
        execution_block_hash: fixtures::h(82),
        execution_state_root: state_root,
    };
    let closure = crate::execution_bridge_real_tests::real_execution_bridge::formal_initial::InitialWorldClosure::
        from_genesis_world_with_node_bootstrap(&world, &identity, &release, bootstrap)?;
    let body = closure.activation_body(definition, policy.clone())?;
    initial_trust.world = identity.clone();
    initial_trust.initial_state_root = body.initial_state_root.clone();
    initial_trust.governing_manifest_digest = body.governing_manifest_digest.clone();
    initial_trust.recovery_policy_digest = policy.digest().map_err(|error| error.to_string())?;
    let initial_certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(
            initial_activation_signing_bytes(&body).map_err(|error| error.to_string())?,
            9,
        ),
        body,
    };
    let predecessor_writer_key = fixtures::key(4);
    let (predecessor_authority, initial_candidate) = closure.authorize(
        &initial_certificate,
        &initial_trust,
        &predecessor_writer_key,
    )?;

    let fixture = super::Fixture::new(
        root.clone(),
        primary_data.clone(),
        replica_data.clone(),
        predecessor_authority.clone(),
    );
    let primary_oracle = super::start_oracle(&fixture, EndpointRole::Primary);
    let replica_oracle = super::start_oracle(&fixture, EndpointRole::Replica);
    let oracle_auth = root.join("oracle-authentication-key");
    super::write_private(&oracle_auth, hex::encode([17u8; 32]).as_bytes())?;
    let predecessor_primary_config = root.join("predecessor-primary-endpoint.json");
    let predecessor_replica_config = root.join("predecessor-replica-endpoint.json");
    write_endpoint_config(
        &predecessor_primary_config,
        &primary_data,
        &oracle_auth,
        primary_oracle.address(),
        EndpointRole::Primary,
        &initial_trust,
        &initial_certificate,
    )?;
    write_endpoint_config(
        &predecessor_replica_config,
        &replica_data,
        &oracle_auth,
        replica_oracle.address(),
        EndpointRole::Replica,
        &initial_trust,
        &initial_certificate,
    )?;
    let (_primary_child, predecessor_primary_address) =
        super::start_endpoint(&endpoint_binary, &predecessor_primary_config)?;
    let (_replica_child, predecessor_replica_address) =
        super::start_endpoint(&endpoint_binary, &predecessor_replica_config)?;

    let genesis = identity.genesis_head().map_err(|error| error.to_string())?;
    let make_endpoint =
        |address, role, authority: VerifiedAuthorityV1, key: SigningKey, minimum| {
            RemoteFormalEndpoint::new(
                address,
                Duration::from_secs(15),
                authority,
                role,
                key,
                minimum,
            )
            .map_err(|error| error.to_string())
        };
    let mut coordinator = FormalCoordinator::new(
        make_endpoint(
            predecessor_primary_address,
            EndpointRole::Primary,
            predecessor_authority.clone(),
            predecessor_writer_key.clone(),
            genesis.clone(),
        )?,
        make_endpoint(
            predecessor_replica_address,
            EndpointRole::Replica,
            predecessor_authority.clone(),
            predecessor_writer_key.clone(),
            genesis.clone(),
        )?,
        predecessor_authority.clone(),
        predecessor_authority.clone(),
        None,
        None,
    )
    .map_err(|error| error.to_string())?;
    let initial_proof = coordinator
        .submit(initial_candidate.clone())
        .map_err(|error| format!("qualify exact predecessor initial closure: {error}"))?;
    verify_formal_qualified(&initial_proof, &predecessor_authority, None, None)
        .map_err(|error| error.to_string())?;
    let external_anchor = VerifiedFormalReadAnchorV1::from_initial(
        &initial_certificate,
        &initial_trust,
        &initial_proof,
    )
    .map_err(|error| error.to_string())?;
    let checkpoint = external_anchor.checkpoint();
    let read_trust = FormalReadTrustConfigV1 {
        initial_trust: initial_trust.clone(),
        checkpoint_digest: checkpoint.digest().map_err(|error| error.to_string())?,
        checkpoint,
    };
    let approved_initial = crate::formal_live_config::ApprovedInitialPublicationV1 {
        schema_version: 1,
        candidate: initial_candidate,
        proof: initial_proof,
    };
    let trust_file = root.join("external-formal-read-trust.json");
    super::write_private(
        &trust_file,
        &serde_json::to_vec(&read_trust).map_err(|error| error.to_string())?,
    )?;
    let approved_file = root.join("predecessor-approved-initial.cbor");
    super::write_private(
        &approved_file,
        &serde_cbor::to_vec(&approved_initial).map_err(|error| error.to_string())?,
    )?;
    let node_keypair_directory = root.join("node-keypair");
    let predecessor_writer_directory = root.join("predecessor-writer");
    super::private_directory(&node_keypair_directory)?;
    super::private_directory(&predecessor_writer_directory)?;
    let node_key = fixtures::key(90);
    let node_key_file = node_keypair_directory.join("node-keypair.toml");
    super::write_private(
        &node_key_file,
        format!(
            "[node]\nprivate_key = \"{}\"\npublic_key = \"{}\"\n",
            hex::encode(node_key.to_bytes()),
            hex::encode(node_key.verifying_key().to_bytes()),
        )
        .as_bytes(),
    )?;
    let predecessor_writer_key_file = root.join("predecessor-writer-key");
    super::write_private(
        &predecessor_writer_key_file,
        hex::encode(predecessor_writer_key.to_bytes()).as_bytes(),
    )?;
    let predecessor_status_bind = free_loopback_address()?;
    let predecessor_runtime_config = root.join("predecessor-formal-runtime.json");
    write_runtime_config(
        &predecessor_runtime_config,
        "formal-handoff-test-node",
        &node_keypair_directory,
        &predecessor_writer_key_file,
        &trust_file,
        &predecessor_writer_directory,
        predecessor_primary_address,
        predecessor_replica_address,
        predecessor_status_bind,
        Some(&approved_file),
        None,
        None,
    )?;
    let predecessor_config_digest = sha256_file(&predecessor_runtime_config)?;
    let predecessor_owner_request_digest = blake3::hash(b"protected-handoff-predecessor-owner")
        .to_hex()
        .to_string();
    let mut predecessor_owner_guard = OwnerGuard::new(
        predecessor_runtime_config.clone(),
        "predecessor-owner-operation",
        predecessor_owner_request_digest.clone(),
        predecessor_config_digest.clone(),
    );
    let predecessor_owner = crate::formal_live_runtime::launch_operation_owner(
        &predecessor_runtime_config,
        "predecessor-owner-operation",
        &predecessor_owner_request_digest,
        &predecessor_config_digest,
        Duration::from_secs(60),
    )?;
    if predecessor_owner.state != "ready" {
        return Err(format!(
            "predecessor runtime owner did not become ready: {} {:?}",
            predecessor_owner.state, predecessor_owner.last_error
        ));
    }
    if predecessor_owner.instance.process_id == std::process::id() {
        return Err("predecessor runtime owner was not launched as a separate process".into());
    }
    predecessor_owner_guard.set(predecessor_owner.instance.clone());

    // The seals below come from signed RPC responses by the two actual
    // predecessor endpoint child processes, not from the test issuer helper.
    let mut seal_primary = make_endpoint(
        predecessor_primary_address,
        EndpointRole::Primary,
        predecessor_authority.clone(),
        predecessor_writer_key.clone(),
        genesis.clone(),
    )?;
    let mut seal_replica = make_endpoint(
        predecessor_replica_address,
        EndpointRole::Replica,
        predecessor_authority.clone(),
        predecessor_writer_key.clone(),
        genesis,
    )?;
    let recovery_operation_id = fixtures::h(240);
    let primary_checkpoint = seal_primary
        .checkpoint()
        .map_err(|error| format!("read actual predecessor primary checkpoint: {error}"))?;
    let replica_checkpoint = seal_replica
        .checkpoint()
        .map_err(|error| format!("read actual predecessor replica checkpoint: {error}"))?;
    let primary_seal = seal_primary
        .seal_epoch(recovery_operation_id.clone(), 2, primary_checkpoint)
        .map_err(|error| format!("seal actual predecessor primary: {error}"))?;
    let replica_seal = seal_replica
        .seal_epoch(recovery_operation_id, 2, replica_checkpoint)
        .map_err(|error| format!("seal actual predecessor replica: {error}"))?;
    let primary_archive = seal_primary
        .archive_manifest()
        .map_err(|error| format!("read actual predecessor primary archive: {error}"))?;
    let replica_archive = seal_replica
        .archive_manifest()
        .map_err(|error| format!("read actual predecessor replica archive: {error}"))?;

    let successor_primary_data = root.join("successor-primary");
    let successor_replica_data = root.join("successor-replica");
    super::private_directory(&successor_primary_data)?;
    super::private_directory(&successor_replica_data)?;
    let mut successor_definition = predecessor_authority.definition().clone();
    successor_definition.epoch = 2;
    successor_definition.writer_public_key = fixtures::public(10);
    successor_definition.predecessor_authority_id = Some(
        predecessor_authority
            .authority_id()
            .map_err(|error| error.to_string())?,
    );
    successor_definition.valid_from_position = external_anchor
        .head()
        .record()
        .position
        .checked_add(1)
        .ok_or("successor valid position overflow")?;
    for (binding, directory, key_number) in [
        (
            &mut successor_definition.primary,
            &successor_primary_data,
            11u8,
        ),
        (
            &mut successor_definition.replica,
            &successor_replica_data,
            12u8,
        ),
    ] {
        binding.instance.authority_epoch = 2;
        binding.instance.endpoint_id = format!("successor-endpoint-{key_number}");
        binding.instance.endpoint_public_key = fixtures::public(key_number);
        binding.instance.instance_id = fixtures::h(key_number + 20);
        binding.instance.storage_id = fixtures::h(key_number + 30);
        binding.instance.storage_fingerprint =
            directory_fingerprint(directory).map_err(|error| error.to_string())?;
        binding.fault_domain_id = format!("successor-fault-domain-{key_number}");
    }
    let independent_issuer_floor =
        IssuerAnchorV1::initial(predecessor_authority.policy()).map_err(|e| e.to_string())?;
    let (recovery_permission, verified_recovery) =
        recovery_test_support::permission_for_sealed_endpoints(
            &external_anchor,
            &initial_trust,
            successor_definition,
            RecoveryTailWitnessV1::Empty,
            [primary_seal.clone(), replica_seal.clone()],
            None,
            &fixtures::key(1),
            &fixtures::key(2),
            independent_issuer_floor,
        )?;
    let issuer_minimum = serde_json::json!({
        "schema_version": 1,
        "world": predecessor_authority.definition().world,
        "full_policy_digest": predecessor_authority.policy().digest().map_err(|e| e.to_string())?,
        "recovery_issuer_public_key": predecessor_authority.policy().recovery_issuer_public_key,
        "issuer_anchor_public_key": predecessor_authority.policy().issuer_anchor_public_key,
        "anchor": recovery_permission.minimum_issuer_anchor,
    });
    let recovery_permission_file = root.join("successor-recovery-permission.json");
    super::write_private(
        &recovery_permission_file,
        &serde_json::to_vec(&recovery_permission).map_err(|error| error.to_string())?,
    )?;
    let issuer_minimum_file = root.join("successor-issuer-minimum.json");
    super::write_private(
        &issuer_minimum_file,
        &serde_json::to_vec(&issuer_minimum).map_err(|error| error.to_string())?,
    )?;

    let primary_transfer = verify_formal_archive_transfer(
        &primary_archive,
        &predecessor_authority,
        Arc::new(FileFormalArchiveSourceV1::new(primary_data.clone()).map_err(|e| e.to_string())?),
    )
    .map_err(|error| format!("verify sealed predecessor primary archive: {error}"))?;
    let replica_transfer = verify_formal_archive_transfer(
        &replica_archive,
        &predecessor_authority,
        Arc::new(FileFormalArchiveSourceV1::new(replica_data.clone()).map_err(|e| e.to_string())?),
    )
    .map_err(|error| format!("verify sealed predecessor replica archive: {error}"))?;
    let old_primary_body = Store::verify_public(
        &primary_archive.checkpoint_bytes,
        &predecessor_authority
            .definition()
            .primary
            .instance
            .endpoint_public_key,
    )
    .map_err(|_| "verify actual predecessor primary native checkpoint")?;
    let old_replica_body = Store::verify_public(
        &replica_archive.checkpoint_bytes,
        &predecessor_authority
            .definition()
            .replica
            .instance
            .endpoint_public_key,
    )
    .map_err(|_| "verify actual predecessor replica native checkpoint")?;
    let successor_authority = verified_recovery.recovery().successor().clone();
    let primary_recovery_oracle = super::oracle::Oracle::start_registered(
        successor_authority.clone(),
        EndpointRole::Primary,
        fixtures::key(11),
        policy.authorized_fencing_resource_bindings[0].clone(),
        Some(recovery_hot_body(
            &verified_recovery,
            &primary_transfer,
            &old_primary_body,
            EndpointRole::Primary,
        )?),
    );
    let replica_recovery_oracle = super::oracle::Oracle::start_registered(
        successor_authority.clone(),
        EndpointRole::Replica,
        fixtures::key(12),
        policy.authorized_fencing_resource_bindings[0].clone(),
        Some(recovery_hot_body(
            &verified_recovery,
            &replica_transfer,
            &old_replica_body,
            EndpointRole::Replica,
        )?),
    );
    let successor_primary_config = root.join("successor-primary-endpoint.json");
    let successor_replica_config = root.join("successor-replica-endpoint.json");
    write_recovery_endpoint_config(
        &successor_primary_config,
        &successor_primary_data,
        &oracle_auth,
        primary_recovery_oracle.address,
        EndpointRole::Primary,
        &initial_trust,
        &initial_certificate,
        &recovery_permission,
        &primary_archive,
        &primary_data,
    )?;
    write_recovery_endpoint_config(
        &successor_replica_config,
        &successor_replica_data,
        &oracle_auth,
        replica_recovery_oracle.address,
        EndpointRole::Replica,
        &initial_trust,
        &initial_certificate,
        &recovery_permission,
        &replica_archive,
        &replica_data,
    )?;
    let (_successor_primary_child, successor_primary_address) =
        super::start_endpoint(&endpoint_binary, &successor_primary_config)?;
    let (_successor_replica_child, successor_replica_address) =
        super::start_endpoint(&endpoint_binary, &successor_replica_config)?;

    let successor_writer_key = fixtures::key(10);
    let successor_writer_key_file = root.join("successor-writer-key");
    super::write_private(
        &successor_writer_key_file,
        hex::encode(successor_writer_key.to_bytes()).as_bytes(),
    )?;
    let successor_writer_directory = root.join("successor-writer");
    super::private_directory(&successor_writer_directory)?;
    let successor_status_bind = free_loopback_address()?;
    let successor_runtime_config = root.join("successor-formal-runtime.json");
    write_runtime_config(
        &successor_runtime_config,
        "formal-handoff-test-node",
        &node_keypair_directory,
        &successor_writer_key_file,
        &trust_file,
        &successor_writer_directory,
        successor_primary_address,
        successor_replica_address,
        successor_status_bind,
        None,
        Some(&recovery_permission_file),
        Some(&issuer_minimum_file),
    )?;

    let request_path = root.join("protected-handoff-request.json");
    let store_directory = root.join("operation-store");
    super::private_directory(&store_directory)?;
    let request = serde_json::json!({
        "api_version": "oasis7.local-operation/v1",
        "operation_id": "protected-signer-handoff-test",
        "world_id": identity.world_id,
        "chain_id": identity.chain_id,
        "kind": "protected_signer_migration",
        "config": predecessor_runtime_config,
        "expected_generation": predecessor_owner.instance.authority_epoch,
        "expected_config_digest": predecessor_owner.instance.config_digest,
        "expected_instance": null,
        "authorization": {
            "kind": "protected_signer_transfer",
            "formal_live_config_file": successor_runtime_config,
            "target_signer_public_key": fixtures::public(10),
            "expected_predecessor_instance": predecessor_owner.instance,
        },
    });
    super::write_private(
        &request_path,
        &serde_json::to_vec(&request).map_err(|error| error.to_string())?,
    )?;
    run_operation_command(&[
        "submit".into(),
        "--request".into(),
        request_path.to_string_lossy().into_owned(),
        "--store".into(),
        store_directory.to_string_lossy().into_owned(),
    ])?;
    let admitted = read_operation(&store_directory)?;
    let revision = admitted["revision"]
        .as_u64()
        .ok_or("protected handoff admission omitted revision")?;
    if admitted["state"] != "queued" || admitted["stage"] != "prepared" {
        return Err(format!("unexpected handoff admission record: {admitted}"));
    }
    let successor_config_digest = sha256_file(&successor_runtime_config)?;
    let handoff_request_digest = admitted["request_digest"]
        .as_str()
        .ok_or("admission omitted request digest")?
        .to_owned();
    let mut successor_owner_guard = OwnerGuard::new(
        successor_runtime_config.clone(),
        "protected-signer-handoff-test",
        handoff_request_digest.clone(),
        successor_config_digest.clone(),
    );
    run_operation_command(&[
        "resume".into(),
        "--store".into(),
        store_directory.to_string_lossy().into_owned(),
        "--id".into(),
        "protected-signer-handoff-test".into(),
        "--revision".into(),
        revision.to_string(),
    ])?;
    let completed = read_operation(&store_directory)?;
    if completed["state"] != "succeeded"
        || completed["stage"] != "successor_qualified_readback_verified"
        || completed["formal_handoff"]["predecessor_stopped"] != true
        || completed["formal_handoff"]["active_authority_epoch"] != 2
        || completed["formal_handoff"]["profile_version"] != 2
        || completed["formal_handoff"]["qualified_position"]
            .as_u64()
            .is_none_or(|position| position <= 1)
        || completed["formal_handoff"]["recovery_permission_digest"]
            .as_str()
            .is_none()
        || completed["formal_handoff"]["primary_checkpoint_digest"]
            .as_str()
            .is_none()
        || completed["formal_handoff"]["replica_checkpoint_digest"]
            .as_str()
            .is_none()
    {
        return Err(format!("protected handoff did not qualify: {completed}"));
    }
    if completed["formal_handoff"]["predecessor_instance"]
        != serde_json::to_value(&predecessor_owner.instance).map_err(|e| e.to_string())?
    {
        return Err("handoff receipt lost the exact predecessor process identity".into());
    }

    let successor_owner = crate::formal_live_runtime::query_operation_owner(
        &successor_runtime_config,
        "protected-signer-handoff-test",
        &handoff_request_digest,
        &successor_config_digest,
        None,
    )?
    .ok_or("successor owner receipt absent after successful handoff")?;
    if successor_owner.state != "ready"
        || successor_owner.instance.authority_epoch != 2
        || successor_owner.instance.profile_version != 2
        || successor_owner.instance.node_id != "formal-handoff-test-node"
        || successor_owner.instance.process_id == std::process::id()
    {
        return Err(format!(
            "successor owner identity/readiness differs: {successor_owner:?}"
        ));
    }
    let predecessor_after = crate::formal_live_runtime::query_operation_owner(
        &predecessor_runtime_config,
        &predecessor_owner.instance.operation_id,
        &predecessor_owner.instance.request_digest,
        &predecessor_owner.instance.config_digest,
        Some(&predecessor_owner.instance.incarnation),
    )?
    .ok_or("predecessor owner receipt absent after handoff")?;
    if predecessor_after.state != "stopped"
        || predecessor_after.instance != predecessor_owner.instance
    {
        return Err(format!(
            "predecessor was not exactly stopped: {predecessor_after:?}"
        ));
    }

    successor_owner_guard.set(successor_owner.instance.clone());
    successor_owner_guard.stop_now()?;
    predecessor_owner_guard.clear();
    drop((
        coordinator,
        primary_oracle,
        replica_oracle,
        primary_recovery_oracle,
        replica_recovery_oracle,
    ));
    Ok(())
}

fn recovery_hot_body(
    recovery: &VerifiedRecoveryBootstrapV1,
    transfer: &VerifiedFormalArchiveTransferV1,
    predecessor_body: &HotBody,
    role: EndpointRole,
) -> Result<HotBody, String> {
    let authority = recovery.recovery().successor();
    let instance = match role {
        EndpointRole::Primary => &authority.definition().primary.instance,
        EndpointRole::Replica => &authority.definition().replica.instance,
    };
    let cut = recovery.cut();
    Ok(HotBody {
        version: 1,
        instance_digest: instance.digest().map_err(|error| error.to_string())?,
        authority_id: authority
            .authority_id()
            .map_err(|error| error.to_string())?,
        activation_ref: authority.activation_ref().into(),
        epoch: authority.definition().epoch,
        revision: 0,
        head: cut.head().clone(),
        head_node: None,
        imported_head_node: predecessor_body
            .head_node
            .clone()
            .or_else(|| predecessor_body.imported_head_node.clone()),
        import_digest: Some(transfer.digest().map_err(|error| error.to_string())?),
        base_parent: Some(CheckpointParent {
            record: cut.record().clone(),
            finalizing_authority_id: cut.finalizing_authority_id().into(),
        }),
        index: predecessor_body.index.clone(),
        pending: None,
        decision_head: None,
        seal: None,
        durable_set: None,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "Recovery process fixture keeps independently pinned endpoint, trust, certificate, and archive inputs explicit."
)]
fn write_recovery_endpoint_config(
    path: &Path,
    data_dir: &Path,
    oracle_auth: &Path,
    oracle_address: std::net::SocketAddr,
    role: EndpointRole,
    trust: &InitialActivationTrustV1,
    certificate: &SignedInitialActivationV1,
    permission: &RecoveryPermissionBootstrapV1,
    archive: &SignedFormalArchiveTransferV1,
    archive_directory: &Path,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "schema_version": 2,
        "initial_trust": trust,
        "initial_certificate": certificate,
        "initial_qualified": null,
        "recovery_bootstrap": {
            "predecessor_checkpoint": permission.predecessor_checkpoint,
            "predecessor_checkpoint_digest": permission.predecessor_checkpoint_digest,
            "certificate": permission.certificate,
            "fencing": permission.fencing,
            "tail": permission.tail,
            "issuer_witness": permission.issuer_witness,
            "minimum_issuer_anchor": permission.minimum_issuer_anchor,
            "archive_manifest": archive,
            "archive_directory": archive_directory,
        },
        "role": role,
        "data_dir": data_dir,
        "oracle_authentication_key_file": oracle_auth,
        "oracle_address": oracle_address,
        "listen": "127.0.0.1:0",
        "timeout_ms": 15000,
    }))
    .map_err(|error| error.to_string())?;
    super::write_private(path, &bytes)
}

#[allow(clippy::too_many_arguments)]
fn write_runtime_config(
    path: &Path,
    node_id: &str,
    node_keypair_directory: &Path,
    writer_key_file: &Path,
    trust_file: &Path,
    writer_directory: &Path,
    primary: std::net::SocketAddr,
    replica: std::net::SocketAddr,
    status_bind: std::net::SocketAddr,
    approved_initial: Option<&Path>,
    recovery_permission: Option<&Path>,
    recovery_issuer_minimum: Option<&Path>,
) -> Result<(), String> {
    let value = serde_json::json!({
        "schema_version": 1,
        "node_id": node_id,
        "node_keypair_directory": node_keypair_directory,
        "writer_key_file": writer_key_file,
        "formal_read_trust_file": trust_file,
        "writer_directory": writer_directory,
        "primary": primary,
        "replica": replica,
        "timeout_ms": 15000,
        "status_bind": status_bind,
        "approved_initial_file": approved_initial,
        "recovery_permission_file": recovery_permission,
        "recovery_issuer_minimum_file": recovery_issuer_minimum,
    });
    super::write_private(
        path,
        &serde_json::to_vec(&value).map_err(|e| e.to_string())?,
    )
}

fn run_operation_command(arguments: &[String]) -> Result<(), String> {
    crate::node_operations::run(arguments.iter().map(String::as_str))
}

fn read_operation(store: &Path) -> Result<Value, String> {
    let bytes = fs::read(store.join("operations.json")).map_err(|error| error.to_string())?;
    let data: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    data["records"]
        .as_array()
        .and_then(|records| records.first())
        .cloned()
        .ok_or_else(|| "operation store has no admitted record".into())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

struct OwnerGuard {
    config: PathBuf,
    operation_id: String,
    request_digest: String,
    config_digest: String,
    instance: Option<crate::formal_live_runtime::FormalRuntimeInstanceV1>,
}
impl OwnerGuard {
    fn new(
        config: PathBuf,
        operation_id: impl Into<String>,
        request_digest: impl Into<String>,
        config_digest: impl Into<String>,
    ) -> Self {
        Self {
            config,
            operation_id: operation_id.into(),
            request_digest: request_digest.into(),
            config_digest: config_digest.into(),
            instance: None,
        }
    }
    fn set(&mut self, instance: crate::formal_live_runtime::FormalRuntimeInstanceV1) {
        self.instance = Some(instance);
    }
    fn clear(&mut self) {
        self.instance = None;
    }
    fn stop_now(&mut self) -> Result<(), String> {
        let Some(instance) = self.instance.as_ref() else {
            return Ok(());
        };
        if instance.process_id != std::process::id() {
            crate::formal_live_runtime::stop_operation_owner(
                &self.config,
                instance,
                Duration::from_secs(20),
            )?;
        }
        self.instance = None;
        Ok(())
    }
}
impl Drop for OwnerGuard {
    fn drop(&mut self) {
        let instance = self.instance.clone().or_else(|| {
            crate::formal_live_runtime::query_operation_owner(
                &self.config,
                &self.operation_id,
                &self.request_digest,
                &self.config_digest,
                None,
            )
            .ok()
            .flatten()
            .map(|snapshot| snapshot.instance)
        });
        if let Some(instance) = instance
            && instance.process_id != std::process::id()
            && let Ok(snapshot) = crate::formal_live_runtime::query_operation_owner(
                &self.config,
                &self.operation_id,
                &self.request_digest,
                &self.config_digest,
                Some(&instance.incarnation),
            )
            && snapshot.is_some_and(|snapshot| snapshot.state == "ready")
        {
            let _ = crate::formal_live_runtime::stop_operation_owner(
                &self.config,
                &instance,
                Duration::from_secs(20),
            );
        }
    }
}
