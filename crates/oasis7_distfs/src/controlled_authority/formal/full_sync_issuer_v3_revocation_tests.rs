use super::*;

#[test]
fn real_witness_history_revocation_inventory_rejects_forks_and_reissued_targets() {
    let temp = TempDir::new("revocation-witness-inventory");
    let old = old_policy();
    let new = new_policy(&old);
    let v2_minimum = v2_genesis(&old);
    let requested_migration = migration(&old, &new, v2_minimum.clone(), 38);
    let mut witness = SimulatedWitness::new(old.clone(), new.clone(), vec![]);
    let migrated = DurableFullSyncIssuerV3::migrate_from_v2(
        migration_config(
            temp.path(),
            old.clone(),
            new.clone(),
            key(3),
            key(7),
            v2_minimum,
        ),
        requested_migration,
        &mut witness,
    )
    .unwrap();
    let mut issuer = DurableFullSyncIssuerV3::open_v3(
        temp.path(),
        old,
        new.clone(),
        key(7),
        migrated.record.next_anchor.clone(),
        &mut witness,
    )
    .unwrap();
    let pinned = issuer.synchronize_v3(&mut witness).unwrap();
    let target = target(10);
    let issue = SignedFullSyncIssuerCommandV3::sign(
        FullSyncIssuerCommandBodyV3 {
            schema_version: 3,
            world: new.body().world.clone(),
            authority_epoch: new.body().authority_epoch,
            scope_digest: new.scope_digest().into(),
            policy_digest: new.policy_digest().into(),
            signer_generation: new.body().signer_generation,
            target: target.clone(),
            operation: FullSyncIssuerOperationV3::Issue {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 90_000,
            },
            expected_head: pinned.anchor().clone(),
            expected_target_generation: 0,
            request_id: hash(40),
            command_nonce: hash(41),
        },
        &key(6),
    )
    .unwrap();
    issuer.manage(issue, &mut witness).unwrap();
    let issued_state = issuer.synchronize_v3(&mut witness).unwrap();
    let lease = SignedFullSyncProviderLeaseRequestV3::sign(
        FullSyncProviderLeaseRequestBodyV3 {
            schema_version: 3,
            world: new.body().world.clone(),
            authority_epoch: new.body().authority_epoch,
            scope_digest: new.scope_digest().into(),
            policy_digest: new.policy_digest().into(),
            signer_generation: new.body().signer_generation,
            provider_public_key: public(8),
            provider_incarnation: hash(42),
            lease_id: hash(43),
            request_id: hash(44),
            request_nonce: hash(45),
            expected_head: issued_state.anchor().clone(),
            target: target.clone(),
            target_generation: 1,
        },
        &key(8),
    )
    .unwrap();
    issuer.issue_lease(lease, &mut witness).unwrap();
    let leased_state = issuer.synchronize_v3(&mut witness).unwrap();
    let revoke = SignedFullSyncIssuerCommandV3::sign(
        FullSyncIssuerCommandBodyV3 {
            schema_version: 3,
            world: new.body().world.clone(),
            authority_epoch: new.body().authority_epoch,
            scope_digest: new.scope_digest().into(),
            policy_digest: new.policy_digest().into(),
            signer_generation: new.body().signer_generation,
            target: target.clone(),
            operation: FullSyncIssuerOperationV3::Revoke,
            expected_head: leased_state.anchor().clone(),
            expected_target_generation: 1,
            request_id: hash(46),
            command_nonce: hash(47),
        },
        &key(6),
    )
    .unwrap();
    let committed_revoke = issuer.manage(revoke, &mut witness).unwrap();
    let verified = issuer.synchronize_v3(&mut witness).unwrap();
    let revoke_digest = committed_revoke.record.digest().unwrap();
    let inventory =
        FullSyncRevocationInventoryV1::from_verified_history(&verified, &revoke_digest).unwrap();
    assert_eq!(inventory.leases.len(), 1);
    assert_eq!(inventory.revoked_target_generation, 2);
    assert_eq!(inventory.summary().pending_lease_count, 1);
    assert!(!inventory.summary_verified(&verified).unwrap().effective);

    // Exercise the actual serving issuer's fresh witness/history sync, not
    // the cached inventory's status helper or a locally asserted ACK.
    let pending = issuer
        .read_revocation_status(&revoke_digest, &inventory, &mut witness)
        .unwrap();
    assert_eq!(pending.original_lease_count, 1);
    assert_eq!(pending.pending_lease_count, 1);
    assert!(!pending.effective);
    assert!(issuer
        .read_revocation_status(&hash(255), &inventory, &mut witness)
        .is_err());

    let mut missing_original = inventory.clone();
    missing_original.leases.clear();
    assert!(missing_original.verify_inventory(&verified).is_err());
    assert!(issuer
        .read_revocation_status(&revoke_digest, &missing_original, &mut witness)
        .is_err());
    let mut forged_phase = inventory.clone();
    forged_phase.phase = FullSyncRevocationPhaseV1::SignedExpiryObserved;
    assert!(forged_phase.verify_inventory(&verified).is_err());
    let floor = FullSyncV3RequiredFloorV1::from_verified_history(&verified).unwrap();
    floor.require_v3(&verified).unwrap();
    let mut bad_floor = floor;
    bad_floor.migration_record_digest = hash(48);
    assert!(bad_floor.require_v3(&verified).is_err());

    let reactivate = SignedFullSyncIssuerCommandV3::sign(
        FullSyncIssuerCommandBodyV3 {
            schema_version: 3,
            world: new.body().world.clone(),
            authority_epoch: new.body().authority_epoch,
            scope_digest: new.scope_digest().into(),
            policy_digest: new.policy_digest().into(),
            signer_generation: new.body().signer_generation,
            target,
            operation: FullSyncIssuerOperationV3::Issue {
                not_before_unix_ms: 2_000,
                expires_unix_ms: 90_000,
            },
            expected_head: verified.anchor().clone(),
            expected_target_generation: 2,
            request_id: hash(49),
            command_nonce: hash(50),
        },
        &key(6),
    )
    .unwrap();
    issuer.manage(reactivate, &mut witness).unwrap();
    let reissued = issuer.synchronize_v3(&mut witness).unwrap();
    assert!(
        FullSyncRevocationInventoryV1::from_verified_history(&reissued, &revoke_digest,).is_err()
    );
    assert!(issuer
        .read_revocation_status(&revoke_digest, &inventory, &mut witness)
        .is_err());
}
