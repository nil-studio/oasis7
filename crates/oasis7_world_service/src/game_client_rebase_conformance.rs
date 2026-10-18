//! Mechanical extraction of bounded cursor rebase regression.
use super::*;

#[test]
fn formal_cursor_rebase_across_33_cuts_requires_exact_ancestry_and_cache_cas() {
    let (mut world, trust, origin, _) = actual_world_initial_fixture();
    let mut current_anchor = origin.clone();
    let mut active = FormalReadVerifier::new(&trust, origin.clone()).unwrap();
    let stale_cas = active.replay_cursor().unwrap();
    let mut steps = Vec::new();
    let mut last = None;
    let mut previous_anchor = origin.clone();
    let mut previous_world = world.clone();
    for index in 1..=33 {
        previous_anchor = current_anchor.clone();
        previous_world = world.clone();
        let (next_world, next_anchor, proof) =
            actual_world_successor_fixture(&current_anchor, world);
        let old_anchor = active.anchor_ref().unwrap();
        let (mut request, mut reply) = response(
            &next_world,
            &next_anchor,
            FormalReadHistoryV1 {
                schema_version: 1,
                steps: vec![FormalReadStepV1::Commit(Box::new(proof.clone()))],
            },
        );
        request.formal_anchor = Some(old_anchor.clone());
        request.observation_nonce = Some(format!("{index:064x}"));
        request.min_commit = last.clone();
        reply.formal_proof.as_mut().unwrap().anchor = old_anchor;
        active.verify_view(&reply, &request, &public(7)).unwrap();
        last = Some(reply.version.commit.clone());
        steps.push(FormalReadStepV1::Commit(Box::new(proof)));
        world = next_world;
        current_anchor = next_anchor;
    }
    assert!(
        active
            .export_cursor()
            .unwrap_err()
            .contains("budget exhausted")
    );
    let minimum = last.unwrap();
    let expected = active.replay_cursor().unwrap();
    let make_fresh = |world: &oasis7_runtime::World,
                      anchor: &VerifiedFormalReadAnchorV1,
                      steps: Vec<FormalReadStepV1>| {
        let mut fresh = FormalReadVerifier::new(&trust, origin.clone()).unwrap();
        let (mut request, mut reply) = response(
            world,
            anchor,
            FormalReadHistoryV1 {
                schema_version: 1,
                steps,
            },
        );
        request.formal_anchor = Some(fresh.anchor_ref().unwrap());
        request.observation_nonce = Some("fe".repeat(32));
        reply.formal_proof.as_mut().unwrap().anchor = request.formal_anchor.clone().unwrap();
        fresh.verify_view(&reply, &request, &public(7)).unwrap();
        fresh
    };
    let fresh = make_fresh(&world, &current_anchor, steps.clone());
    let before = active.replay_cursor().unwrap();
    assert!(
        active
            .adopt_rebased_cursor(fresh.clone(), &stale_cas, &minimum)
            .unwrap_err()
            .contains("CAS")
    );
    assert_eq!(active.replay_cursor().unwrap(), before);
    let older = make_fresh(&previous_world, &previous_anchor, steps[..32].to_vec());
    assert!(
        active
            .adopt_rebased_cursor(older, &expected, &minimum)
            .is_err()
    );
    assert_eq!(active.replay_cursor().unwrap(), before);
    // A fully signed sibling is not an ancestor, even at the same height.
    let mut fork_world = previous_world.clone();
    fork_world.step().unwrap();
    let (fork_world, fork_anchor, fork_proof) =
        actual_world_successor_fixture(&previous_anchor, fork_world);
    let mut fork_steps = steps[..32].to_vec();
    fork_steps.push(FormalReadStepV1::Commit(Box::new(fork_proof)));
    let fork = make_fresh(&fork_world, &fork_anchor, fork_steps);
    assert!(
        active
            .adopt_rebased_cursor(fork, &expected, &minimum)
            .is_err()
    );
    assert_eq!(active.replay_cursor().unwrap(), before);
    active
        .adopt_rebased_cursor(fresh, &expected, &minimum)
        .unwrap();
    let bundle = active.export_cursor().unwrap();
    assert_eq!(bundle.frames.len(), 1);
    assert_eq!(bundle.commit, minimum);
    FormalReadVerifier::restore_cursor(
        &trust,
        origin.clone(),
        &origin.checkpoint().digest().unwrap(),
        &bundle,
        &public(7),
    )
    .unwrap();
    // The inherited exact binding remains usable for the follow-up read.
    let (mut request, reply) = response(
        &world,
        &current_anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    request.min_commit = Some(minimum);
    request.observation_nonce = Some("fd".repeat(32));
    active.verify_view(&reply, &request, &public(7)).unwrap();
    assert_eq!(active.export_cursor().unwrap().commit, reply.version.commit);
    // The independent network read may select a newer publication. It must
    // retain the old exact minimum, and only its own new cut may be exported.
    let minimum = reply.version.commit.clone();
    let (new_world, new_anchor, new_proof) = actual_world_successor_fixture(&current_anchor, world);
    let mut newer_steps = steps;
    newer_steps.push(FormalReadStepV1::Commit(Box::new(new_proof)));
    let newer = make_fresh(&new_world, &new_anchor, newer_steps);
    let expected = active.replay_cursor().unwrap();
    active
        .adopt_rebased_cursor(newer, &expected, &minimum)
        .unwrap();
    let (mut request, reply) = response(
        &new_world,
        &new_anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    request.min_commit = Some(minimum);
    request.observation_nonce = Some("fc".repeat(32));
    active.verify_view(&reply, &request, &public(7)).unwrap();
    assert_eq!(active.export_cursor().unwrap().commit, reply.version.commit);
    assert_eq!(reply.version.commit.position, 35);
}
