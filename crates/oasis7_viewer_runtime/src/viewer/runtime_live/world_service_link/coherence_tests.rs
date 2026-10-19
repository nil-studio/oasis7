//! Relation tests seeded by the real signed CollectData interleaving capture.
//! Typed mutations protect comparison gates; these are not signature proofs.
use super::*;
fn fixture() -> (EventCursor, EventCursor) {
    let baseline = serde_json::from_str(r#"{"commit":{"binding":{"authority_generation":0,"branch_id":"main","finality_ref":"blake3:b76a3dee41809f3f86a7fe2f58218c5a7a6f83e3dcfc96beb3fd99abc498cb3a","governing_manifest_ref":"blake3:6bb35bcd3204a154856806164c90b28d106523e8de7ea03a289ef2209bf2acf2","permission_generation":0,"provider_world_id":"w1","reorg_generation":0},"execution_block_hash":"01d4f10b7de412c7d5c9c32ffc7c4a0cfc0417f187d029843ddb26d20d6000b1","position":3,"state_root_ref":"7c52d4ca3c2af16933f7dc1b2833b7fdec6f666fd8b8074606a6dcb1f47b9431","world":{"genesis_digest":"fixture-genesis-v1","world_id":"w1"}},"era":0,"scope_id":"agent:agent-a","sequence":2,"stream_id":"domain-events:w1"}"#).unwrap();
    let page = serde_json::from_str(r#"{"commit":{"binding":{"authority_generation":0,"branch_id":"main","finality_ref":"blake3:b76a3dee41809f3f86a7fe2f58218c5a7a6f83e3dcfc96beb3fd99abc498cb3a","governing_manifest_ref":"blake3:6bb35bcd3204a154856806164c90b28d106523e8de7ea03a289ef2209bf2acf2","permission_generation":0,"provider_world_id":"w1","reorg_generation":0},"execution_block_hash":"8e050df042160094eb824e57e88f9d20614030e6d665011e143f3d2c89ec6553","position":4,"state_root_ref":"7c914d6c60e782f7379674c37709075bb251b79a957e6d3aad0627ed86a56553","world":{"genesis_digest":"fixture-genesis-v1","world_id":"w1"}},"era":0,"scope_id":"agent:agent-a","sequence":14,"stream_id":"domain-events:w1"}"#).unwrap();
    (baseline, page)
}
#[test]
fn newer_commit_accepts_partial_equal_and_newer_visible_pages() {
    let (base, page) = fixture();
    for sequence in [page.sequence - 1, page.sequence, page.sequence + 1] {
        let mut target = base.clone();
        target.sequence = sequence;
        assert_eq!(
            coherence_refresh_eligible(&base, &base, &target, &page, false),
            Ok(true)
        );
        let mut fresh = page.clone();
        fresh.sequence = sequence.max(page.sequence);
        assert_eq!(
            coherence_refresh_target_valid(&base, &base, &target, &page, &fresh, &fresh.commit),
            Ok(true)
        );
    }
}
#[test]
fn every_world_and_binding_axis_fails_closed() {
    let (base, page) = fixture();
    for axis in 0..9 {
        let mut drift = page.clone();
        match axis {
            0 => drift.commit.world.world_id.push_str("-foreign"),
            1 => drift.commit.world.genesis_digest.push_str("-foreign"),
            2 => drift.commit.binding.provider_world_id.push_str("-foreign"),
            3 => drift.commit.binding.branch_id.push_str("-foreign"),
            4 => drift.commit.binding.finality_ref.push_str("-foreign"),
            5 => drift.commit.binding.reorg_generation += 1,
            6 => drift
                .commit
                .binding
                .governing_manifest_ref
                .push_str("-foreign"),
            7 => drift.commit.binding.authority_generation += 1,
            _ => drift.commit.binding.permission_generation += 1,
        }
        assert!(
            coherence_refresh_eligible(&base, &base, &base, &drift, false).is_err(),
            "axis {axis}"
        );
        assert!(
            coherence_refresh_target_valid(&base, &base, &base, &page, &drift, &drift.commit)
                .is_err(),
            "fresh axis {axis}"
        );
    }
}
#[test]
fn history_identity_drift_at_each_cursor_is_rejected() {
    let (base, page) = fixture();
    for location in 0..4 {
        for axis in 0..3 {
            let mut cursors = [base.clone(), base.clone(), base.clone(), page.clone()];
            match axis {
                0 => cursors[location].stream_id.push_str("-foreign"),
                1 => cursors[location].scope_id.push_str("-foreign"),
                _ => cursors[location].era += 1,
            }
            assert_eq!(
                coherence_refresh_eligible(
                    &cursors[0],
                    &cursors[1],
                    &cursors[2],
                    &cursors[3],
                    false
                ),
                Ok(false)
            );
        }
    }
    for axis in 0..3 {
        let mut fresh = page.clone();
        match axis {
            0 => fresh.stream_id.push_str("-foreign"),
            1 => fresh.scope_id.push_str("-foreign"),
            _ => fresh.era += 1,
        }
        assert_eq!(
            coherence_refresh_target_valid(&base, &base, &base, &page, &fresh, &fresh.commit),
            Ok(false)
        );
    }
}
#[test]
fn actual_request_reversals_and_conflicting_refs_fail_closed() {
    let (base, page) = fixture();
    let mut requested = base.clone();
    requested.sequence -= 1;
    assert_eq!(
        coherence_refresh_eligible(&base, &requested, &base, &page, false),
        Ok(false)
    );
    requested = page.clone();
    assert_eq!(
        coherence_refresh_eligible(&base, &requested, &base, &page, false),
        Ok(false)
    );
    let mut backwards = page.clone();
    backwards.sequence = base.sequence - 1;
    assert_eq!(
        coherence_refresh_eligible(&base, &base, &base, &backwards, false),
        Ok(false)
    );
    backwards = page.clone();
    backwards.commit.position = base.commit.position - 1;
    assert_eq!(
        coherence_refresh_eligible(&base, &base, &base, &backwards, false),
        Ok(false)
    );
    for state_root in [false, true] {
        let mut conflict = base.clone();
        if state_root {
            conflict.commit.state_root_ref.push_str("-conflict");
        } else {
            conflict.commit.execution_block_hash.push_str("-conflict");
        }
        assert!(coherence_refresh_eligible(&base, &base, &base, &conflict, false).is_err());
    }
}
#[test]
fn refreshed_target_cannot_be_behind_or_internally_inconsistent() {
    let (base, page) = fixture();
    assert_eq!(
        coherence_refresh_target_valid(&base, &base, &base, &page, &base, &base.commit),
        Ok(false)
    );
    let mut behind = page.clone();
    behind.commit = base.commit.clone();
    assert_eq!(
        coherence_refresh_target_valid(&base, &base, &base, &page, &behind, &behind.commit),
        Ok(false)
    );
    let mut target = page.clone();
    target.sequence += 1;
    assert_eq!(
        coherence_refresh_target_valid(&base, &base, &target, &page, &page, &page.commit),
        Ok(false)
    );
    let mut projection = page.commit.clone();
    projection.state_root_ref.push_str("-mismatch");
    assert_eq!(
        coherence_refresh_target_valid(&base, &base, &base, &page, &page, &projection),
        Ok(false)
    );
}
#[test]
fn exhausted_whole_sync_refresh_budget_rejects_second_advance() {
    let (base, page) = fixture();
    assert_eq!(
        coherence_refresh_eligible(&base, &base, &base, &page, false),
        Ok(true)
    );
    assert_eq!(
        coherence_refresh_eligible(&base, &base, &base, &page, true),
        Ok(false)
    );
    let mut next = page.clone();
    next.commit.position += 1;
    next.sequence += 1;
    assert_eq!(
        coherence_refresh_eligible(&base, &page, &page, &next, true),
        Ok(false)
    );
}
