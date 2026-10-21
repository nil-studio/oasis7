use super::*;
use std::cell::Cell;

fn input() -> (Snapshot, Journal) {
    let world = oasis7::runtime_host::world::new();
    (world.snapshot(), world.journal().clone())
}

#[test]
fn successful_restore_reuse_skips_repeated_restore_and_isolates_consumer_mutation() {
    let cache = Mutex::new(RestoreCache::default());
    let (snapshot, journal) = input();
    let key = Key::from_bytes(
        &serde_cbor::to_vec(&snapshot).unwrap(),
        &serde_cbor::to_vec(&journal).unwrap(),
    );
    let calls = Cell::new(0);
    let run = || {
        calls.set(calls.get() + 1);
        oasis7::runtime_host::world::from_snapshot(snapshot.clone(), journal.clone())
    };
    let baseline = restore_with(&cache, key.clone(), run).unwrap();
    let mut returned = restore_with(&cache, key.clone(), run).unwrap();
    returned.submit_action(oasis7_runtime::Action::RegisterAgent {
        agent_id: "caller-private-mutation".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(1, 2, 3),
    });
    returned.step().unwrap();
    let next = restore_with(&cache, key, run).unwrap();
    assert_eq!(
        calls.get(),
        1,
        "warm hit must avoid the actual restore closure"
    );
    assert!(next.snapshot() == baseline.snapshot());
    assert!(next.journal() == baseline.journal());
    assert!(!next.state().agents.contains_key("caller-private-mutation"));
    assert_eq!(next.module_cache_len(), 0);
}

#[test]
fn complete_journal_key_failures_and_oversized_entries_do_not_reuse_restore() {
    let cache = Mutex::new(RestoreCache::default());
    let calls = Cell::new(0);
    let (snapshot, journal) = input();
    let run = || {
        calls.set(calls.get() + 1);
        oasis7::runtime_host::world::from_snapshot(snapshot.clone(), journal.clone())
    };
    let first = Key::from_bytes(b"same-snapshot", b"full-journal-a");
    let different_journal = Key::from_bytes(b"same-snapshot", b"full-journal-b");
    restore_with(&cache, first, run).unwrap();
    restore_with(&cache, different_journal, run).unwrap();
    assert_eq!(
        calls.get(),
        2,
        "complete journal material participates in identity"
    );
    let failed = Key::from_bytes(b"invalid", b"journal");
    for _ in 0..2 {
        assert!(
            restore_with(&cache, failed.clone(), || {
                calls.set(calls.get() + 1);
                Err(oasis7_runtime::WorldError::JournalMismatch)
            })
            .is_err()
        );
    }
    assert_eq!(calls.get(), 4, "failures cannot create a warm entry");
    let (mut invalid_snapshot, invalid_journal) = input();
    invalid_snapshot.journal_len = 1;
    let invalid_bytes = serde_cbor::to_vec(&invalid_snapshot).unwrap();
    let journal_bytes = serde_cbor::to_vec(&invalid_journal).unwrap();
    for _ in 0..2 {
        assert!(
            restore(
                &cache,
                &invalid_bytes,
                &journal_bytes,
                invalid_snapshot.clone(),
                invalid_journal.clone()
            )
            .is_err()
        );
    }
    assert!(
        !cache
            .lock()
            .unwrap()
            .entries
            .iter()
            .any(|entry| entry.key == Key::from_bytes(&invalid_bytes, &journal_bytes))
    );

    let oversized = Key {
        snapshot_len: MAX_ENTRY_INPUT_BYTES + 1,
        ..Key::from_bytes(b"large", b"journal")
    };
    for _ in 0..2 {
        restore_with(&cache, oversized.clone(), run).unwrap();
    }
    assert_eq!(
        calls.get(),
        6,
        "oversized inputs retain original validation behavior"
    );
    let retained = cache.lock().unwrap();
    assert_eq!(retained.entries.len(), 2);
    assert!(retained.retained_input_bytes <= MAX_RETAINED_INPUT_BYTES);
}

#[test]
fn fifo_capacity_and_poison_fallback_preserve_actual_restore() {
    let cache = Mutex::new(RestoreCache::default());
    let calls = Cell::new(0);
    let (snapshot, journal) = input();
    let run = || {
        calls.set(calls.get() + 1);
        oasis7::runtime_host::world::from_snapshot(snapshot.clone(), journal.clone())
    };
    let first = Key::from_bytes(b"first", b"journal");
    restore_with(&cache, first.clone(), run).unwrap();
    for value in [b"second".as_slice(), b"third".as_slice()] {
        restore_with(&cache, Key::from_bytes(value, b"journal"), run).unwrap();
    }
    restore_with(&cache, first, run).unwrap();
    assert_eq!(
        calls.get(),
        4,
        "evicted entry must execute the restore again"
    );
    assert_eq!(cache.lock().unwrap().entries.len(), MAX_ENTRIES);
    let _ = std::panic::catch_unwind(|| {
        let _guard = cache.lock().unwrap();
        panic!("test poison");
    });
    let key = Key::from_bytes(b"after-poison", b"journal");
    restore_with(&cache, key.clone(), run).unwrap();
    restore_with(&cache, key, run).unwrap();
    assert_eq!(
        calls.get(),
        6,
        "poisoned cache must fall back to original restore"
    );
}

#[test]
fn same_complete_journal_with_different_snapshot_bytes_restores_distinct_worlds() {
    let cache = Mutex::new(RestoreCache::default());
    let (first, journal) = input();
    let other_world = oasis7::runtime_host::world::new_with_state(oasis7_runtime::WorldState {
        world_rules: None,
        time: 7,
        ..oasis7_runtime::WorldState::default()
    });
    let second = other_world.snapshot();
    assert!(other_world.journal() == &journal);
    let first_bytes = serde_cbor::to_vec(&first).unwrap();
    let second_bytes = serde_cbor::to_vec(&second).unwrap();
    let journal_bytes = serde_cbor::to_vec(&journal).unwrap();
    assert_eq!(
        restore(
            &cache,
            &first_bytes,
            &journal_bytes,
            first.clone(),
            journal.clone()
        )
        .unwrap()
        .state()
        .time,
        0
    );
    assert_eq!(
        restore(
            &cache,
            &second_bytes,
            &journal_bytes,
            second,
            journal.clone()
        )
        .unwrap()
        .state()
        .time,
        7
    );
    assert_eq!(
        restore(&cache, &first_bytes, &journal_bytes, first, journal)
            .unwrap()
            .state()
            .time,
        0
    );
    assert_eq!(cache.lock().unwrap().entries.len(), 2);
}

pub(super) fn local_context(seed: u8) -> super::super::RestoreContext {
    let release = oasis7_runtime::ReleaseSecurityPolicy::default();
    let public = oasis7::world_service::sign_read_request("test", (), &hex::encode([seed; 32]))
        .unwrap()
        .subject_public_key;
    let local = oasis7_runtime::DevLocalParticipantAuthority::new(
        oasis7_proto::storage_profile::StorageProfile::DevLocal,
        &release,
        public,
    )
    .unwrap();
    super::super::RestoreContext::new(release, Some(local)).unwrap()
}

#[test]
fn actual_restore_cache_never_reuses_release_policy_or_operator_pin() {
    let cache = Mutex::new(RestoreCache::default());
    let (snapshot, journal) = input();
    let snapshot_bytes = serde_cbor::to_vec(&snapshot).unwrap();
    let journal_bytes = serde_cbor::to_vec(&journal).unwrap();
    let local_a = local_context(77);
    let local_b = local_context(78);
    let governed = super::super::RestoreContext::default();
    let hardened = super::super::RestoreContext::new(
        oasis7_runtime::ReleaseSecurityPolicy::production_hardened(),
        None,
    )
    .unwrap();
    let mut narrower_release = local_a.release.clone();
    narrower_release.allow_identity_hash_signature = false;
    let narrower_authority = oasis7_runtime::DevLocalParticipantAuthority::new(
        oasis7_proto::storage_profile::StorageProfile::DevLocal,
        &narrower_release,
        local_a.local.as_ref().unwrap().operator_public_key().into(),
    )
    .unwrap();
    let narrower =
        super::super::RestoreContext::new(narrower_release, Some(narrower_authority)).unwrap();
    for context in [
        &local_a, &local_a, &local_b, &narrower, &governed, &hardened, &local_a,
    ] {
        let restored = restore_with_context(
            &cache,
            &snapshot_bytes,
            &journal_bytes,
            snapshot.clone(),
            journal.clone(),
            context,
        )
        .unwrap();
        assert_eq!(restored.release_security_policy(), &context.release);
        assert_eq!(
            restored.devlocal_participant_authority(),
            context.local.as_ref()
        );
    }
    // A rejection under another effective context cannot become a warm success.
    let wrong_key = Key::from_bytes_with_context(&snapshot_bytes, &journal_bytes, &local_b);
    let unrelated = Mutex::new(RestoreCache::default());
    restore_with_context(
        &unrelated,
        &snapshot_bytes,
        &journal_bytes,
        snapshot,
        journal,
        &local_a,
    )
    .unwrap();
    assert!(restore_with(&unrelated, wrong_key, || Err(WorldError::JournalMismatch)).is_err());
}

#[test]
fn oversized_and_poison_bypass_apply_the_requested_context_before_restore() {
    let context = local_context(77);
    let (snapshot, journal) = input();
    let oversized = vec![0; MAX_ENTRY_INPUT_BYTES + 1];
    let journal_bytes = serde_cbor::to_vec(&journal).unwrap();
    let cache = Mutex::new(RestoreCache::default());
    let restored = restore_with_context(
        &cache,
        &oversized,
        &journal_bytes,
        snapshot.clone(),
        journal.clone(),
        &context,
    )
    .unwrap();
    assert_eq!(
        restored.devlocal_participant_authority(),
        context.local.as_ref()
    );
    assert!(cache.lock().unwrap().entries.is_empty());
    let _ = std::panic::catch_unwind(|| {
        let _guard = cache.lock().unwrap();
        panic!("controlled context bypass poison");
    });
    let restored = restore_with_context(
        &cache,
        b"snapshot",
        &journal_bytes,
        snapshot,
        journal,
        &context,
    )
    .unwrap();
    assert_eq!(
        restored.devlocal_participant_authority(),
        context.local.as_ref()
    );
}

#[test]
fn warm_local_policy_history_remains_rejected_without_matching_operator_context() {
    use oasis7_agent_api::game::{DecimalU64, GameIdentityNamespaceV1};
    use oasis7_runtime::service_admission::{participant::*, participant_policy::*};
    let context = local_context(77);
    let original = oasis7::runtime_host::world::new();
    let snapshot = original.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
            world_id: "local-world",
            chain_id: "local-chain",
            genesis_ref: Some("local-genesis"),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: 0,
        },
        "test-config",
        "test-generation",
    );
    let mut world = context
        .restore(snapshot, original.journal().clone())
        .unwrap();
    world
        .bind_cognition_runtime("local-world", "local-branch", 0, None, "pending", 0)
        .unwrap();
    let issuer_key = hex::encode([71; 32]);
    let policy = ParticipantEntryPolicyV1 {
        api_rev: 1,
        world_id: "local-world".into(),
        genesis_digest: "local-genesis".into(),
        identity_namespace: GameIdentityNamespaceV1 {
            service_id: "identity".into(),
            environment_id: "test".into(),
            instance_epoch: DecimalU64(1),
        },
        issuer_public_key: oasis7::world_service::sign_read_request("test", (), &issuer_key)
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
        branch_id: "local-branch".into(),
        reorg_epoch: 0,
        identity_namespace: policy.identity_namespace.clone(),
        source_snapshot_epoch: 1,
        source_watermark: "test-seq-1".into(),
        source_snapshot_digest: format!("blake3:{}", "a".repeat(64)),
        complete: true,
        previous_registry_digest: world.participant_registry_digest().unwrap(),
        legacy_owner_state_digest: world.participant_legacy_owner_state_digest().unwrap(),
        entries: vec![],
    };
    let authorization = oasis7::world_service::sign_read_request(
        devlocal_policy_signing_domain(1),
        DevLocalParticipantPolicyAuthorizationV1 {
            policy: policy.clone(),
            manifest: oasis7::world_service::sign_read_request(
                PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,
                manifest,
                &issuer_key,
            )
            .unwrap(),
        },
        &hex::encode([77; 32]),
    )
    .unwrap();
    world
        .install_devlocal_participant_policy(policy.clone(), authorization)
        .unwrap();
    let snapshot = world.snapshot();
    let journal = world.journal().clone();
    let snapshot_bytes = serde_cbor::to_vec(&snapshot).unwrap();
    let journal_bytes = serde_cbor::to_vec(&journal).unwrap();
    let cache = Mutex::new(RestoreCache::default());
    let run = |context: &super::super::RestoreContext| {
        restore_with_context(
            &cache,
            &snapshot_bytes,
            &journal_bytes,
            snapshot.clone(),
            journal.clone(),
            context,
        )
    };
    assert_eq!(
        run(&context)
            .unwrap()
            .capability_revocation_state()
            .participants
            .policy,
        Some(policy)
    );
    assert!(run(&super::super::RestoreContext::default()).is_err());
    assert!(run(&local_context(78)).is_err());
    assert!(
        run(&context).is_ok(),
        "failed foreign contexts cannot damage the approved warm entry"
    );
}

#[test]
fn trusted_restore_context_rejects_mixed_release_policy_and_local_authority() {
    let local = local_context(77);
    assert!(
        super::super::RestoreContext::new(
            oasis7_runtime::ReleaseSecurityPolicy::production_hardened(),
            local.local,
        )
        .is_err()
    );
}
