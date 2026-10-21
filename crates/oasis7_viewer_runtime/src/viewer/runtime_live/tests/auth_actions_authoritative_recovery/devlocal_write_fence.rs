use super::*;

#[test]
fn committed_fence_readback_preserves_signed_devlocal_history_and_trusted_context() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-session-projection-context-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let (mut server, agent_id, player_id, metadata) =
        persist_session_side_effect_projection(&recovery_dir);
    // This is an already trusted live-world context, never a pin inferred from disk.
    let release = crate::runtime::ReleaseSecurityPolicy::default();
    let operator =
        crate::runtime::service_admission::sign_read_request("test", (), &hex::encode([77; 32]))
            .unwrap()
            .subject_public_key;
    let authority = crate::runtime::DevLocalParticipantAuthority::new(
        oasis7_proto::storage_profile::StorageProfile::DevLocal,
        &release,
        operator,
    )
    .unwrap();
    let source_snapshot = server.world.snapshot();
    let binding_world_id = source_snapshot.cognition["runtime_binding"]["world_id"]
        .as_str()
        .expect("existing viewer world id")
        .to_string();
    let snapshot = server.world.snapshot_with_chain_resource_context(
        crate::runtime::ChainResourceDerivationContext {
            world_id: &binding_world_id,
            chain_id: "viewer-local-chain",
            genesis_ref: Some("viewer-local-genesis"),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: server.world.state().time,
        },
        "viewer-test-config",
        "viewer-test-generation",
    );
    server.world = crate::runtime_host::world::from_snapshot_with_devlocal_participant_authority(
        snapshot,
        server.world.journal().clone(),
        authority.clone(),
    )
    .expect("restore explicitly trusted live context");
    let binding = server
        .world
        .current_cognition_runtime_binding()
        .expect("aligned viewer binding");
    use crate::runtime::service_admission::{
        participant::*, participant_policy::*, sign_read_request,
    };
    use oasis7_agent_api::game::{DecimalU64, GameIdentityNamespaceV1};
    let issuer_key = hex::encode([71; 32]);
    let policy = ParticipantEntryPolicyV1 {
        api_rev: 1,
        world_id: binding.world_id.clone(),
        genesis_digest: "viewer-local-genesis".into(),
        identity_namespace: GameIdentityNamespaceV1 {
            service_id: "identity".into(),
            environment_id: "test".into(),
            instance_epoch: DecimalU64(1),
        },
        issuer_public_key: sign_read_request("test", (), &issuer_key)
            .unwrap()
            .subject_public_key,
        generation: 1,
        max_grant_ticks: 16,
        max_participants: 10,
        registry_complete: true,
        enabled: true,
    };
    let manifest = ParticipantMigrationManifestV1 {
        api_rev: 1,
        policy_digest: policy_digest(&policy).unwrap(),
        world_id: policy.world_id.clone(),
        genesis_digest: policy.genesis_digest.clone(),
        branch_id: binding.branch_id.clone(),
        reorg_epoch: binding.reorg_epoch,
        identity_namespace: policy.identity_namespace.clone(),
        source_snapshot_epoch: 1,
        source_watermark: "viewer-test-seq-1".into(),
        source_snapshot_digest: format!("blake3:{}", "a".repeat(64)),
        complete: true,
        previous_registry_digest: server.world.participant_registry_digest().unwrap(),
        legacy_owner_state_digest: server
            .world
            .participant_legacy_owner_state_digest()
            .unwrap(),
        entries: vec![],
    };
    let signed = sign_read_request(
        devlocal_policy_signing_domain(1),
        DevLocalParticipantPolicyAuthorizationV1 {
            policy: policy.clone(),
            manifest: sign_read_request(
                PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,
                manifest,
                &issuer_key,
            )
            .unwrap(),
        },
        &hex::encode([77; 32]),
    )
    .unwrap();
    server
        .world
        .install_devlocal_participant_policy(policy.clone(), signed)
        .expect("install signed local policy");
    assert_eq!(
        server
            .world
            .capability_revocation_state()
            .participants
            .devlocal_policy_history
            .len(),
        1
    );
    assert!(
        matches!(
            server
                .world
                .commit_authoritative_recovery_generation(&recovery_dir, &metadata)
                .unwrap(),
            crate::runtime::AuthoritativeRecoveryCommitStatus::Committed(_)
        ),
        "publish real signed-policy generation"
    );
    server.authoritative_recovery_write_fence =
        Some(hex::encode(Sha256::digest(metadata.as_slice())));
    server
        .resolve_authoritative_recovery_write_fence()
        .expect("resolve with live authority");
    assert!(server.authoritative_recovery_write_fence.is_none());
    assert_eq!(
        server.world.devlocal_participant_authority(),
        Some(&authority)
    );
    assert_eq!(server.world.release_security_policy(), &release);
    assert_eq!(
        server
            .world
            .capability_revocation_state()
            .participants
            .policy,
        Some(policy)
    );
    assert_eq!(
        server
            .world
            .capability_revocation_state()
            .participants
            .devlocal_policy_history
            .len(),
        1
    );

    assert_session_side_effect_projection(&server, &agent_id, &player_id);
    std::fs::remove_dir_all(recovery_dir).expect("remove recovery fixture");
}
