use super::*;

#[test]
fn serving_lifecycle_cold_reopens_exact_lease_after_uncertain_cas() {
    for (committed_before_reply_loss, late_reply) in [(false, false), (true, false), (true, true)] {
        let (directory, old, new, mut witness, migration_entry) =
            migrate_fixture("serving-lifecycle-uncertain-cas");
        let mut issuer = open_issuer(&directory, &old, &new, &migration_entry, &mut witness);
        let active_target = target(14);
        let initial = issuer.synchronize_v3(&mut witness).unwrap();
        let grant = management_command(
            &new,
            initial.anchor(),
            active_target.clone(),
            0,
            FullSyncIssuerOperationV3::Issue {
                not_before_unix_ms: 1_000,
                expires_unix_ms: 90_000,
            },
            224,
        );
        issuer.manage(grant, &mut witness).unwrap();
        let state = issuer.synchronize_v3(&mut witness).unwrap();
        let lease = provider_request(&new, state.anchor(), active_target.clone(), 226, 228, 230);
        let minimum = migration_entry.record.next_anchor.clone();
        drop(issuer);
        let mut serving = RecoveringFullSyncIssuerV3::open(
            directory.path(),
            old,
            new.clone(),
            key(7),
            minimum.clone(),
            &mut witness,
        )
        .unwrap();
        let provider_key = public(8);
        let access = FullSyncIssuerHistoryAccessV3 {
            provider_public_key: &provider_key,
            policy_digest: new.policy_digest(),
            target: &active_target,
            target_generation: 1,
        };
        let query = FullSyncReadEvidenceQueryV3 {
            schema_version: 3,
            observation_nonce: hash(91),
            minimum_anchor: minimum.clone(),
            provider_public_key: provider_key.clone(),
            provider_incarnation: lease.body.provider_incarnation.clone(),
            target: active_target.clone(),
            target_generation: 1,
            provider_lease_request: Some(lease),
        };
        let deadline = || Instant::now() + std::time::Duration::from_secs(30);
        let commits = witness.inner.event_commits;
        let attempts = witness.inner.event_attempts.len();
        witness.inner.lose_next_event_reply = committed_before_reply_loss && !late_reply;
        witness.inner.fail_next_event_before_commit = !committed_before_reply_loss;
        let first_deadline = if late_reply {
            Instant::now() + std::time::Duration::from_secs(5)
        } else {
            deadline()
        };
        witness.delay_event_reply_until = late_reply.then_some(first_deadline);
        assert!(
            serving
                .read_evidence_for_provider(&access, query.clone(), first_deadline, &mut witness)
                .is_err()
        );
        if late_reply {
            assert_eq!(witness.inner.event_commits, commits + 1);
        }
        assert!(directory.path().join(PENDING_FILE).is_file());
        let original = witness.inner.event_attempts[attempts].clone();
        let observations = witness.inner.v3_observations.len();

        // A malformed or already expired request cannot trigger reopening
        // the poisoned issuer or mutate its still-unresolved original intent.
        let pending_bytes = fs::read(directory.path().join(PENDING_FILE)).unwrap();
        let mut wrong = query.clone();
        wrong.provider_public_key = public(9);
        assert!(
            serving
                .read_evidence_for_provider(&access, wrong, deadline(), &mut witness)
                .is_err()
        );
        assert!(
            serving
                .read_evidence_for_provider(&access, query.clone(), Instant::now(), &mut witness)
                .is_err()
        );
        assert_eq!(witness.inner.v3_observations.len(), observations);
        assert_eq!(
            fs::read(directory.path().join(PENDING_FILE)).unwrap(),
            pending_bytes
        );

        let mut retry = query.clone();
        retry.observation_nonce = hash(92);
        let evidence = serving
            .read_evidence_for_provider(&access, retry, deadline(), &mut witness)
            .unwrap();
        assert_eq!(evidence.query.body.minimum_anchor, minimum);
        assert_eq!(evidence.query.body.observation_nonce, hash(92));
        let entry = evidence.history.v3_entries.last().unwrap();
        assert_eq!(entry.record, original.1);
        assert_eq!(
            entry.receipt.body.observation,
            FullSyncIssuerReceiptObservationV3::V3(original.0)
        );
        assert_eq!(witness.inner.event_commits, commits + 1);
        assert!(!directory.path().join(PENDING_FILE).exists());

        let mut observation_only = query;
        observation_only.observation_nonce = hash(93);
        observation_only.provider_lease_request = None;
        let current = serving
            .read_evidence_for_provider(&access, observation_only, deadline(), &mut witness)
            .unwrap();
        assert_eq!(current.history, evidence.history);
        assert_eq!(witness.inner.event_commits, commits + 1);
    }
}
