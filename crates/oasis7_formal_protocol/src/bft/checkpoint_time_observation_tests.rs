//! Signed protocol fixtures only: no protected clock or production authority.
use super::*;
use crate::bft::checkpoint_issuer_status::{
    AuthenticatedBftCheckpointIssuerStatusV1, BftCheckpointIssuerRevocationV1,
    BftCheckpointIssuerStatusBodyV1,
};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;

fn h(byte: u8) -> String {
    hex::encode([byte; 32])
}
fn key(byte: u8) -> SigningKey {
    SigningKey::from_bytes(&[byte; 32])
}
fn public(byte: u8) -> String {
    hex::encode(key(byte).verifying_key().to_bytes())
}
fn digest(suffix: &str, value: &impl Serialize) -> String {
    domain_digest(
        &format!("oasis7.bft.checkpoint-renewal.v1.{suffix}"),
        &canonical_bytes(value, 65_536, "fixture").unwrap(),
    )
}
fn signed(suffix: &str, body: &impl Serialize, seed: u8) -> Vec<u8> {
    let message = hex::decode(digest(suffix, body)).unwrap();
    canonical_bytes(
        &json!({"body": body, "signature_hex": hex::encode(key(seed).sign(&message).to_bytes())}),
        65_536,
        "fixture",
    )
    .unwrap()
}
fn policy() -> BftCheckpointTrustPolicyV1 {
    let origin = json!({
        "formal_world":{"world_id":"w","chain_id":"c","genesis_digest":h(1)},
        "bft_scope":{"world_id":"w","chain_id":"c","genesis_hash":h(2)},
        "original_c0_checkpoint_digest":h(3),"initial_activation_digest":h(4),
        "handoff_head":{"position":7,"record_id":h(5)},
        "handoff_target_digest":h(6),"handoff_activation_digest":h(7),
        "handoff_raw_snapshot_root":h(8),
    });
    let security = json!({
        "schema_version":1,"profile":"independent-absolute-bound-v1",
        "origin_digest":digest("origin",&origin),
        "old_key_constraints_digest":blake3::hash(b"old").to_hex().to_string(),
        "stake_constraints_digest":blake3::hash(b"stake").to_hex().to_string(),
        "security_not_before_unix_ms":10,"absolute_safe_until_unix_ms":90,
    });
    let clock = json!({
        "schema_version":1,"profile":"bounded-utc-observation-v1",
        "time_source_id":"clock","time_key_id":"clock-key","time_public_key":public(119),
        "time_key_not_before_unix_ms":10,"time_key_expires_at_unix_ms":100,
        "max_clock_uncertainty_ms":5,"max_observation_elapsed_ms":5,
        "elapsed_profile":"protected-elapsed-upper-bound-v1",
    });
    serde_json::from_value(json!({
        "schema_version":1,"profile":"bft","profile_version":1,"wire_version":3,
        "origin":origin,"policy_id":"fixture","policy_version":1,"authorization_ref":"fixture",
        "renewal_root_public_key":public(118),"root_authorization_generation":1,
        "root_authorized_at_unix_ms":10,"root_not_before_unix_ms":10,
        "root_expires_at_unix_ms":100,"max_root_revocation_staleness_ms":90,
        "policy_not_before_unix_ms":10,"policy_expires_at_unix_ms":100,
        "purpose":"bft-checkpoint-renewal-v1",
        "issuer_keys":[{"issuer_id":"issuer","key_id":"issuer-key","public_key":public(117),
            "issuer_generation":1,"not_before_unix_ms":10,"expires_at_unix_ms":100}],
        "threshold":1,"security_policy":security,
        "security_assumptions_digest":digest("security-policy",&security),
        "max_checkpoint_validity_ms":50,"max_status_age_ms":5,
        "minimum_ledger_id":"fixture","max_head_observation_age_ms":5,
        "clock_profile":clock,"clock_profile_digest":digest("clock-profile",&clock),
        "max_issuer_keys":1,"max_status_bytes":65536,"max_time_evidence_bytes":65536,
        "max_update_proof_bytes":65536,"max_safety_evidence_bytes":65536,
        "max_update_decisions":16,"max_decision_archive_bytes":1048576,
    }))
    .unwrap()
}
fn body(policy: &BftCheckpointTrustPolicyV1) -> BftCheckpointTimeObservationBodyV1 {
    BftCheckpointTimeObservationBodyV1 {
        schema_version: 1,
        origin_digest: policy.origin.digest().unwrap(),
        policy_digest: policy.digest().unwrap(),
        clock_profile_digest: policy.clock_profile_digest.clone(),
        consumer_binding_digest: h(51),
        observation_nonce: h(52),
        clock_epoch: h(53),
        time_generation: 7,
        utc_earliest_unix_ms: 40,
        utc_latest_unix_ms: 42,
        time_source_id: "clock".into(),
        time_key_id: "clock-key".into(),
    }
}
fn challenge() -> BftCheckpointTimeChallengeV1 {
    BftCheckpointTimeChallengeV1 {
        consumer_binding_digest: h(51),
        observation_nonce: h(52),
        clock_epoch: h(53),
    }
}
fn status(policy: &BftCheckpointTrustPolicyV1) -> BftCheckpointIssuerStatusBodyV1 {
    BftCheckpointIssuerStatusBodyV1 {
        schema_version: 1,
        origin_digest: policy.origin.digest().unwrap(),
        policy_digest: policy.digest().unwrap(),
        root_authorization_generation: 1,
        status_sequence: 2,
        issued_at_unix_ms: 40,
        not_before_unix_ms: 40,
        expires_at_unix_ms: 46,
        policy_revoked: false,
        issuer_keys: vec![BftCheckpointIssuerRevocationV1 {
            key_id: "issuer-key".into(),
            issuer_generation: 1,
            revoked_at_unix_ms: None,
        }],
    }
}

#[test]
fn signed_clock_exact_retry_and_refusals() {
    let policy = policy();
    let pin = policy.digest().unwrap();
    let original = body(&policy);
    let bytes = signed("time-observation", &original, 119);
    let read = |bytes: &[u8]| {
        AuthenticatedBftCheckpointTimeObservationV1::decode(bytes, &policy, &pin, &challenge())
    };
    let observed = read(&bytes).unwrap();
    assert_eq!(observed.body(), &original);
    assert!(observed.candidate_interval(None).is_err());
    let mut sample = BftCheckpointTimeContinuityV1 {
        challenge: challenge(),
        elapsed_upper_ms: 3,
        utc_lower_floor_unix_ms: 41,
        previous_observation: Some(BftCheckpointPreviousObservationV1 {
            time_generation: 7,
            clock_epoch: h(53),
            observation_nonce: h(52),
            observation_digest: observed.digest().to_owned(),
        }),
    };
    let interval = observed.candidate_interval(Some(&sample)).unwrap();
    assert_eq!(
        (interval.earliest_unix_ms(), interval.latest_unix_ms()),
        (41, 45)
    );
    assert!(interval.within_window(41, 46));
    assert!(!interval.within_window(41, 45));
    sample.elapsed_upper_ms = 4;
    assert!(observed.candidate_interval(Some(&sample)).is_err());
    sample.elapsed_upper_ms = 3;
    sample.utc_lower_floor_unix_ms = 46;
    assert!(observed.candidate_interval(Some(&sample)).is_err());
    sample.utc_lower_floor_unix_ms = 41;
    let previous = sample.previous_observation.as_mut().unwrap();
    previous.time_generation = 8;
    assert!(observed.candidate_interval(Some(&sample)).is_err());
    sample
        .previous_observation
        .as_mut()
        .unwrap()
        .time_generation = 7;
    sample
        .previous_observation
        .as_mut()
        .unwrap()
        .observation_digest = h(99);
    assert!(observed.candidate_interval(Some(&sample)).is_err());
    sample
        .previous_observation
        .as_mut()
        .unwrap()
        .observation_digest = observed.digest().to_owned();
    assert_eq!(
        observed.candidate_interval(Some(&sample)).unwrap(),
        interval
    );
    for (field, value) in [
        ("schema_version", json!(2)),
        ("origin_digest", json!(h(99))),
        ("policy_digest", json!(h(99))),
        ("clock_profile_digest", json!(h(99))),
        ("consumer_binding_digest", json!(h(99))),
        ("observation_nonce", json!(h(99))),
        ("clock_epoch", json!(h(99))),
        ("time_source_id", json!("other")),
        ("time_key_id", json!("other")),
        ("utc_earliest_unix_ms", json!(43)),
        ("utc_latest_unix_ms", json!(46)),
        ("utc_latest_unix_ms", json!(100)),
    ] {
        let mut changed = serde_json::to_value(&original).unwrap();
        changed[field] = value;
        assert!(
            read(&signed("time-observation", &changed, 119)).is_err(),
            "{field}"
        );
    }
    assert!(read(&signed("time-observation", &original, 118)).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(read(&trailing).is_err());
    assert!(read(&vec![0; 65_537]).is_err());
    assert_eq!(read(&bytes).unwrap().digest(), observed.digest());
}

#[test]
fn issuer_status_elapsed_and_irreversible_revocation() {
    let policy = policy();
    let pin = policy.digest().unwrap();
    let observation = AuthenticatedBftCheckpointTimeObservationV1::decode(
        &signed("time-observation", &body(&policy), 119),
        &policy,
        &pin,
        &challenge(),
    )
    .unwrap();
    let read = |body: &BftCheckpointIssuerStatusBodyV1| {
        AuthenticatedBftCheckpointIssuerStatusV1::decode(
            &signed("issuer-status", body, 118),
            &policy,
            &pin,
        )
        .unwrap()
    };
    let original = status(&policy);
    let previous = read(&original);
    let sample = BftCheckpointTimeContinuityV1 {
        challenge: challenge(),
        elapsed_upper_ms: 3,
        utc_lower_floor_unix_ms: 41,
        previous_observation: None,
    };
    previous.check_predecessor(&read(&original)).unwrap();
    previous
        .check_candidate_window(&policy, &observation, Some(&sample))
        .unwrap();
    assert!(
        previous
            .check_candidate_window(&policy, &observation, None)
            .is_err()
    );
    for (issued, expiry) in [(40, 45), (39, 46)] {
        let mut altered = original.clone();
        altered.issued_at_unix_ms = issued;
        altered.expires_at_unix_ms = expiry;
        let changed = read(&altered);
        changed
            .check_signed_observation_window(&policy, &observation)
            .unwrap();
        assert!(
            changed
                .check_candidate_window(&policy, &observation, Some(&sample))
                .is_err()
        );
        assert!(changed.check_predecessor(&previous).is_err());
    }
    let mut successor = original.clone();
    successor.status_sequence = 3;
    successor.policy_revoked = true;
    successor.issuer_keys[0].revoked_at_unix_ms = Some(40);
    let revoked = read(&successor);
    revoked.check_predecessor(&previous).unwrap();
    assert!(
        revoked
            .check_signed_observation_window(&policy, &observation)
            .is_err()
    );
    assert!(previous.check_predecessor(&revoked).is_err());
    successor.status_sequence = 4;
    successor.policy_revoked = false;
    assert!(read(&successor).check_predecessor(&revoked).is_err());
    successor.policy_revoked = true;
    successor.issuer_keys[0].revoked_at_unix_ms = None;
    assert!(read(&successor).check_predecessor(&revoked).is_err());
    successor.issuer_keys[0].revoked_at_unix_ms = Some(39);
    assert!(read(&successor).check_predecessor(&revoked).is_err());
    read(&original).check_predecessor(&previous).unwrap();
    read(&original)
        .check_candidate_window(&policy, &observation, Some(&sample))
        .unwrap();
}

#[test]
fn exact_safety_originals_and_capacity() {
    let policy = policy();
    let pin = policy.digest().unwrap();
    let check = |pin: &str, old: &[u8], stake: &[u8]| {
        policy.check_safety_constraint_originals(pin, old, stake)
    };
    check(&pin, b"old", b"stake").unwrap();
    for (old, stake) in [
        (b"".as_slice(), b"stake".as_slice()),
        (b"bad".as_slice(), b"stake".as_slice()),
        (b"old".as_slice(), b"bad".as_slice()),
        (b"old".as_slice(), vec![1; 65_536].as_slice()),
    ] {
        assert!(check(&pin, old, stake).is_err());
    }
    assert!(check(&h(99), b"old", b"stake").is_err());
    check(&pin, b"old", b"stake").unwrap();
}
