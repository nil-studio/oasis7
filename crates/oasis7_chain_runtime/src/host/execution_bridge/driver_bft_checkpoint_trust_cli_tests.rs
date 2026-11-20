//! Real loader preparation over retained H, not a current-authority test.
use super::*;
use oasis7_game_client::checkpoint_trust::BftCheckpointTrustConfigV1;
use serde::Serialize;

fn canonical<T: Serialize>(value: &T) -> Vec<u8> {
    let value = serde_cbor::value::to_value(value).unwrap();
    let mut bytes = Vec::new();
    let mut serializer = serde_cbor::ser::Serializer::new(&mut bytes);
    serializer.self_describe().unwrap();
    value.serialize(&mut serializer).unwrap();
    bytes
}
fn digest(suffix: &str, value: &serde_json::Value) -> String {
    let label = format!("oasis7.bft.checkpoint-renewal.v1.{suffix}");
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(label.len() as u64).to_be_bytes());
    hasher.update(label.as_bytes());
    hasher.update(&canonical(value));
    hasher.finalize().to_hex().to_string()
}

pub(super) fn check_preparation(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Path,
    node_config: &Path,
    binding: &BftSignerBindingV3,
) {
    let source = successor_source_from_binding_v3(binding).unwrap();
    let pin = binding.digest().unwrap();
    // Independent synthetic deployment pin for this fixture, not evidence of
    // a reconstructed C0-to-H lineage.
    let c0_pin = fixtures::h(41);
    let origin = serde_json::json!({
        "formal_world":h.trust.initial_trust.world,"bft_scope":h.target.scope,
        "original_c0_checkpoint_digest":fixtures::h(41),
        "initial_activation_digest":h.anchor.initial_certificate().body.activation_ref().unwrap(),
        "handoff_head":binding.handoff().handoff_head(),
        "handoff_target_digest":source.target_digest,"handoff_activation_digest":source.activation_digest,
        "handoff_raw_snapshot_root":source.handoff_snapshot_root,
    });
    let security = serde_json::json!({"schema_version":1,"profile":"independent-absolute-bound-v1","origin_digest":digest("origin",&origin),"old_key_constraints_digest":fixtures::h(42),"stake_constraints_digest":fixtures::h(43),"security_not_before_unix_ms":10,"absolute_safe_until_unix_ms":90});
    let clock = serde_json::json!({"schema_version":1,"profile":"bounded-utc-observation-v1","time_source_id":"clock","time_key_id":"clock-key","time_public_key":hex::encode(fixtures::key(119).verifying_key().to_bytes()),"time_key_not_before_unix_ms":10,"time_key_expires_at_unix_ms":100,"max_clock_uncertainty_ms":5,"max_observation_elapsed_ms":5,"elapsed_profile":"protected-elapsed-upper-bound-v1"});
    let policy = serde_json::json!({
        "schema_version":1,"profile":"bft","profile_version":1,"wire_version":3,
        "origin":origin,"policy_id":"test-only-policy","policy_version":1,"authorization_ref":"test-only-decision",
        "renewal_root_public_key":hex::encode(fixtures::key(118).verifying_key().to_bytes()),
        "root_authorization_generation":1,"root_authorized_at_unix_ms":10,"root_not_before_unix_ms":10,"root_expires_at_unix_ms":100,"max_root_revocation_staleness_ms":90,
        "policy_not_before_unix_ms":10,"policy_expires_at_unix_ms":100,"purpose":"bft-checkpoint-renewal-v1",
        "issuer_keys":[{"issuer_id":"issuer","key_id":"issuer-key","public_key":hex::encode(fixtures::key(117).verifying_key().to_bytes()),"issuer_generation":1,"not_before_unix_ms":10,"expires_at_unix_ms":100}],"threshold":1,
        "security_policy":security,"security_assumptions_digest":digest("security-policy",&security),
        "max_checkpoint_validity_ms":50,"max_status_age_ms":5,"minimum_ledger_id":"test-only-ledger","max_head_observation_age_ms":5,
        "clock_profile":clock,"clock_profile_digest":digest("clock-profile",&clock),
        "max_issuer_keys":1,"max_status_bytes":65536,"max_time_evidence_bytes":65536,"max_update_proof_bytes":65536,"max_safety_evidence_bytes":65536,"max_update_decisions":16,"max_decision_archive_bytes":1048576,
    });
    let policy_pin = digest("policy", &policy);
    let value = serde_json::json!({
        "schema_version":1,"formal_read_trust":h.trust,"target":h.target,"consumer_binding_digest":pin,
        "initial_anchor":{"kind":"qualified_handoff_v2","source":source,"checkpoint_sequence":0,"checkpoint_digest":source.origin_digest().unwrap(),"handoff_head":binding.handoff().handoff_head(),"initial_schedule_digest":binding.initial_schedule().digest().unwrap(),"material_closure":{"handoff_material_identity":source.material_identity,"handoff_raw_snapshot_root":source.handoff_snapshot_root}},
        "policy":policy,"policy_digest":policy_pin,"initial_authenticated_at_unix_ms":20,"initial_not_before_unix_ms":20,"initial_expires_at_unix_ms":60,"initial_safety_valid_until_unix_ms":70,
    });
    let config: BftCheckpointTrustConfigV1 = serde_json::from_value(value.clone()).unwrap();
    let bytes = canonical(&config);
    let path = root.join("checkpoint-trust.cbor");
    private_file(&path, &bytes);
    let prepare_with_status = |status: Option<&str>| {
        let mut arguments = vec![
            "bft-header",
            "prepare-checkpoint-trust",
            "--config",
            path.to_str().unwrap(),
            "--node-config",
            node_config.to_str().unwrap(),
            "--policy-pin",
            &policy_pin,
            "--consumer-binding",
            &pin,
            "--original-c0-pin",
            &c0_pin,
        ];
        if let Some(status) = status {
            arguments.extend(["--issuer-status", status]);
        }
        crate::identity_receipt::dispatch(arguments.into_iter()).unwrap()
    };
    let prepare = || prepare_with_status(None);
    prepare().expect("real host consumes SDK preparation and original Node binding");
    let status = serde_json::json!({
        "schema_version":1,
        "origin_digest":digest("origin", &value["policy"]["origin"]),
        "policy_digest":policy_pin,
        "root_authorization_generation":1,
        "status_sequence":2,
        "issued_at_unix_ms":20,
        "not_before_unix_ms":20,
        "expires_at_unix_ms":40,
        "policy_revoked":true,
        "issuer_keys":[{"key_id":"issuer-key","issuer_generation":1,"revoked_at_unix_ms":19}],
    });
    let status_path = root.join("checkpoint-issuer-status.cbor");
    let write_status = |body: serde_json::Value, signing_key| {
        let digest_bytes = hex::decode(digest("issuer-status", &body)).unwrap();
        private_file(
            &status_path,
            &canonical(&serde_json::json!({
                "body":body,
                "signature_hex":fixtures::sign(digest_bytes, signing_key),
            })),
        );
    };
    let with_status = || prepare_with_status(status_path.to_str());
    write_status(status.clone(), 118);
    with_status().expect("revoked statuses are authenticated without opening current or renewal");
    // A root-signed status cannot be backdated before independent authorization,
    // even when its effective window and full revocation set are otherwise valid.
    let mut before_authorization = status.clone();
    before_authorization["issued_at_unix_ms"] = serde_json::json!(9);
    before_authorization["issuer_keys"][0]["revoked_at_unix_ms"] = serde_json::json!(9);
    write_status(before_authorization, 118);
    assert!(with_status().unwrap_err().contains("authorization window"));
    // The independent authorization start itself remains an inclusive boundary.
    let mut at_authorization = status.clone();
    at_authorization["issued_at_unix_ms"] = serde_json::json!(10);
    at_authorization["issuer_keys"][0]["revoked_at_unix_ms"] = serde_json::json!(10);
    write_status(at_authorization, 118);
    with_status().expect("status issued at original authorization start is valid");
    write_status(status.clone(), 117);
    assert!(
        with_status()
            .unwrap_err()
            .contains("root signature rejected")
    );
    for (field, invalid) in [
        ("issuer_keys", serde_json::json!([])),
        ("expires_at_unix_ms", serde_json::json!(101)),
        ("policy_digest", serde_json::json!(fixtures::h(49))),
        ("root_authorization_generation", serde_json::json!(2)),
    ] {
        let mut altered = status.clone();
        altered[field] = invalid;
        write_status(altered, 118);
        assert!(with_status().is_err(), "{field}");
    }
    let prepared =
        BftCheckpointTrustConfigV1::decode_and_prepare(&bytes, &policy_pin, &pin, &c0_pin).unwrap();
    assert_eq!(prepared.config(), &config);
    assert!(
        BftCheckpointTrustConfigV1::decode_and_prepare(&bytes, &fixtures::h(44), &pin, &c0_pin)
            .is_err()
    );
    assert!(
        BftCheckpointTrustConfigV1::decode_and_prepare(
            &bytes,
            &policy_pin,
            &fixtures::h(45),
            &c0_pin
        )
        .is_err()
    );
    assert!(
        BftCheckpointTrustConfigV1::decode_and_prepare(&bytes, &policy_pin, &pin, &fixtures::h(47))
            .is_err()
    );
    let mut foreign_source = value.clone();
    foreign_source["initial_anchor"]["source"]["binding_digest"] =
        serde_json::json!(fixtures::h(48));
    let changed_source: oasis7_formal_protocol::bft::BftSuccessorWindowSourceV1 =
        serde_json::from_value(foreign_source["initial_anchor"]["source"].clone()).unwrap();
    foreign_source["initial_anchor"]["checkpoint_digest"] =
        serde_json::json!(changed_source.origin_digest().unwrap());
    assert!(
        BftCheckpointTrustConfigV1::decode_and_prepare(
            &canonical(&foreign_source),
            &policy_pin,
            &pin,
            &c0_pin
        )
        .err()
        .unwrap()
        .contains("source differs")
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(
        BftCheckpointTrustConfigV1::decode_and_prepare(&trailing, &policy_pin, &pin, &c0_pin)
            .is_err()
    );
    assert!(
        BftCheckpointTrustConfigV1::decode_and_prepare(
            &serde_json::to_vec(&config).unwrap(),
            &policy_pin,
            &pin,
            &c0_pin
        )
        .is_err()
    );
    assert!(serde_json::from_value::<oasis7_game_client::remote_world_service::FormalReadTrustConfigV1>(value.clone()).is_err());
    for (field, replacement) in [
        ("initial_expires_at_unix_ms", serde_json::json!(91)),
        ("schema_version", serde_json::json!(2)),
    ] {
        let mut bad = value.clone();
        bad[field] = replacement;
        assert!(
            BftCheckpointTrustConfigV1::decode_and_prepare(
                &canonical(&bad),
                &policy_pin,
                &pin,
                &c0_pin
            )
            .is_err()
        );
    }
    for (field, replacement) in [
        ("threshold", serde_json::json!(2)),
        ("max_issuer_keys", serde_json::json!(0)),
        ("max_status_bytes", serde_json::json!(2097153)),
        ("root_expires_at_unix_ms", serde_json::json!(101)),
        (
            "renewal_root_public_key",
            serde_json::json!("00".repeat(32)),
        ),
    ] {
        let mut bad = value.clone();
        bad["policy"][field] = replacement;
        let changed_pin = digest("policy", &bad["policy"]);
        bad["policy_digest"] = serde_json::json!(changed_pin);
        assert!(
            BftCheckpointTrustConfigV1::decode_and_prepare(
                &canonical(&bad),
                &changed_pin,
                &pin,
                &c0_pin
            )
            .is_err()
        );
    }
    let mut bad = value.clone();
    bad["initial_anchor"]["initial_schedule_digest"] = serde_json::json!(fixtures::h(46));
    private_file(&path, &canonical(&bad));
    assert!(prepare().unwrap_err().contains("closure/schedule"));
    private_file(&path, &bytes);
    prepare().unwrap();
}
