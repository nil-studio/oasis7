use super::*;
use ed25519_dalek::Signer;

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
    // A software-signed receipt fixture exercises only the verified issuer
    // consumer. It cannot stand in for a physically protected cutoff witness.
    let original_lease = inventory.leases.keys().next().unwrap().clone();
    let signed_fence = |kind, lease| {
        let body = FullSyncRevocationFenceBodyV1 {
            schema_version: 1,
            revocation_record_digest: revoke_digest.clone(),
            revocation_anchor: inventory.revocation_anchor.clone(),
            target: inventory.target.clone(),
            revoked_target_generation: inventory.revoked_target_generation,
            original_lease: lease,
            kind,
            protected_floor_revision: verified.anchor().revision,
            protected_earliest_unix_ms: 1_000,
        };
        let mut bytes = b"oasis7.formal.full-sync-revocation.fence.v1\0".to_vec();
        bytes.extend_from_slice(&serde_json::to_vec(&body).unwrap());
        SignedFullSyncRevocationFenceV1 {
            signature_hex: hex::encode(key(4).sign(&bytes).to_bytes()),
            body,
        }
    };
    let mut proven = inventory.clone();
    proven
        .record_protected_fence(
            &verified,
            signed_fence(FullSyncFenceKindV1::TargetGateStopped, None),
        )
        .unwrap();
    let partial = issuer
        .read_revocation_status(&revoke_digest, &proven, &mut witness)
        .unwrap();
    assert!(!partial.effective);
    assert_eq!(partial.pending_lease_count, 1);
    proven
        .record_protected_fence(
            &verified,
            signed_fence(FullSyncFenceKindV1::InstanceCutoff, Some(original_lease)),
        )
        .unwrap();
    let effective = issuer
        .read_revocation_status(&revoke_digest, &proven, &mut witness)
        .unwrap();
    assert!(effective.effective);
    assert_eq!(effective.pending_lease_count, 0);

    issuer = retained_original_fences_are_cold_read_by_the_live_issuer(
        issuer,
        &mut witness,
        &inventory,
        signed_fence(FullSyncFenceKindV1::TargetGateStopped, None),
        signed_fence(
            FullSyncFenceKindV1::InstanceCutoff,
            inventory.leases.keys().next().cloned(),
        ),
    );

    assert!(
        issuer
            .read_revocation_status(&hash(255), &inventory, &mut witness)
            .is_err()
    );

    let mut missing_original = inventory.clone();
    missing_original.leases.clear();
    assert!(missing_original.verify_inventory(&verified).is_err());
    assert!(
        issuer
            .read_revocation_status(&revoke_digest, &missing_original, &mut witness)
            .is_err()
    );
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
    assert!(
        issuer
            .read_revocation_status(&revoke_digest, &inventory, &mut witness)
            .is_err()
    );
    assert!(
        issuer
            .retain_revocation_fence(
                &revoke_digest,
                &serde_json::to_vec(proven.target_gate_fence.as_ref().unwrap()).unwrap(),
                &mut witness,
            )
            .is_err()
    );
}

fn retained_original_fences_are_cold_read_by_the_live_issuer(
    mut issuer: DurableFullSyncIssuerV3,
    witness: &mut SimulatedWitness,
    original: &FullSyncRevocationInventoryV1,
    gate: SignedFullSyncRevocationFenceV1,
    lease: SignedFullSyncRevocationFenceV1,
) -> DurableFullSyncIssuerV3 {
    let directory = issuer.directory.clone();
    let minimum = issuer.minimum_anchor.clone();
    let old_policy = issuer.v2_policy.clone();
    let policy = issuer.policy.clone();
    let reopen = |witness: &mut SimulatedWitness| {
        DurableFullSyncIssuerV3::open_v3(
            &directory,
            old_policy.clone(),
            policy.clone(),
            key(7),
            minimum.clone(),
            witness,
        )
        .unwrap()
    };
    let digest = &original.revocation_record_digest;
    let path = directory.join(retained_revocation::FENCES_FILE);
    let gate_bytes = serde_json::to_vec_pretty(&gate).unwrap();
    let lease_bytes = serde_json::to_vec_pretty(&lease).unwrap();
    let commits = witness.event_commits;
    let mut wrong = lease.clone();
    wrong.body.original_lease.as_mut().unwrap().lease_id = hash(254);
    assert!(
        issuer
            .retain_revocation_fence(digest, &serde_json::to_vec(&wrong).unwrap(), witness)
            .is_err()
    );
    let mut preimage = b"oasis7.formal.full-sync-revocation.fence.v1\0".to_vec();
    preimage.extend_from_slice(&serde_json::to_vec(&wrong.body).unwrap());
    wrong.signature_hex = hex::encode(key(4).sign(&preimage).to_bytes());
    assert!(
        issuer
            .retain_revocation_fence(digest, &serde_json::to_vec(&wrong).unwrap(), witness)
            .is_err()
    );
    assert!(
        issuer
            .retain_revocation_fence(&hash(255), &gate_bytes, witness)
            .is_err()
    );
    assert!(
        issuer
            .retain_revocation_fence(digest, &vec![b' '; 16 * 1024 + 1], witness)
            .is_err()
    );
    assert!(!path.exists());
    let pending = issuer
        .retain_revocation_fence(digest, &gate_bytes, witness)
        .unwrap();
    assert!(!pending.effective);
    assert_eq!(pending.pending_lease_count, 1);
    let partial_bytes = fs::read(&path).unwrap();
    issuer
        .retain_revocation_fence(digest, &gate_bytes, witness)
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), partial_bytes);
    // Even a semantically equal re-encoding cannot replace the retained bytes.
    assert!(
        issuer
            .retain_revocation_fence(digest, &serde_json::to_vec(&gate).unwrap(), witness)
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), partial_bytes);
    // Discard the completed response, drop all issuer state, then retry the
    // same original bytes. This is software reply-loss/cold-reopen coverage.
    let _lost_reply = issuer
        .retain_revocation_fence(digest, &lease_bytes, witness)
        .unwrap();
    let complete_bytes = fs::read(&path).unwrap();
    let retained: serde_json::Value = serde_json::from_slice(&complete_bytes).unwrap();
    assert_eq!(
        retained["receipts"][0].as_str().unwrap().as_bytes(),
        gate_bytes
    );
    assert_eq!(
        retained["receipts"][1].as_str().unwrap().as_bytes(),
        lease_bytes
    );
    drop(issuer);
    issuer = reopen(witness);
    assert!(
        issuer
            .retain_revocation_fence(digest, &lease_bytes, witness)
            .unwrap()
            .effective
    );
    assert_eq!(fs::read(&path).unwrap(), complete_bytes);
    assert!(
        issuer
            .read_revocation_status(digest, original, witness)
            .unwrap()
            .effective
    );
    assert_eq!(witness.event_commits, commits);
    // Local complete receipts cannot bypass an unavailable current witness.
    let history = std::mem::take(&mut witness.new_entries);
    assert!(
        issuer
            .read_revocation_status(digest, original, witness)
            .is_err()
    );
    witness.new_entries = history;
    fs::remove_file(&path).unwrap();
    let absent = issuer
        .read_revocation_status(digest, original, witness)
        .unwrap();
    assert!(!absent.effective);
    assert_eq!(absent.pending_lease_count, 1);
    for damaged in [b"{}".as_slice(), b"not json".as_slice()] {
        atomic_bounded(&directory, &path, damaged, MAX_LEDGER_BYTES).unwrap();
        assert!(
            issuer
                .read_revocation_status(digest, original, witness)
                .is_err()
        );
    }
    let mut altered = retained.clone();
    let mut forged = lease;
    forged.signature_hex = hex::encode([0; 64]);
    altered["receipts"][1] = serde_json::to_string(&forged).unwrap().into();
    let duplicate = serde_json::json!({
        "schema_version": 1,
        "receipts": [retained["receipts"][0], retained["receipts"][0]],
    });
    for damaged in [altered, duplicate] {
        let bytes = serde_json::to_vec(&damaged).unwrap();
        atomic_bounded(&directory, &path, &bytes, MAX_LEDGER_BYTES).unwrap();
        assert!(
            issuer
                .read_revocation_status(digest, original, witness)
                .is_err()
        );
    }
    // Restoring the exact original cache does not create or re-sign a witness
    // event; current signed history and every original fence are rechecked.
    atomic_bounded(&directory, &path, &complete_bytes, MAX_LEDGER_BYTES).unwrap();
    drop(issuer);
    issuer = reopen(witness);
    assert!(
        issuer
            .read_revocation_status(digest, original, witness)
            .unwrap()
            .effective
    );
    assert_eq!(witness.event_commits, commits);
    issuer
}
