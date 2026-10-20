//! Recovery configuration never provisions an authority or rewrites its origin.
use super::*;

#[test]
fn formal_recovery_loader_rejects_initial_replacement_without_writes() {
    let mut fixture = Fixture::new();
    fixture.config.recovery_permission_file = Some(PathBuf::from("permission.json"));
    fixture.config.recovery_issuer_minimum_file = Some(PathBuf::from("issuer-minimum.json"));
    fixture.config.approved_initial_file = Some(PathBuf::from("initial.cbor"));
    fixture.save();
    let before = fs::read(fixture.root.0.join("formal-read-trust.json")).unwrap();
    let error = FormalLiveConfigV1::load(&fixture.config_path)
        .err()
        .unwrap();
    assert!(error.contains("cannot manufacture an initial"), "{error}");
    assert_eq!(
        before,
        fs::read(fixture.root.0.join("formal-read-trust.json")).unwrap()
    );
    assert_eq!(
        fs::read_dir(fixture.root.0.join("writer")).unwrap().count(),
        0
    );
    assert!(!fixture.root.0.join("permission.json").exists());
    assert!(!fixture.root.0.join("initial.cbor").exists());
}

#[test]
fn formal_recovery_loader_rejects_caller_authority_assertions_without_changing_pin() {
    let fixture = Fixture::new();
    let path = fixture.root.0.join("permission.json");
    write_private(&path, br#"{"authority":{"epoch":3},"fencing_proven":true}"#);
    let bytes = fs::read(fixture.root.0.join("formal-read-trust.json")).unwrap();
    let trust: FormalReadTrustConfigV1 = serde_json::from_slice(&bytes).unwrap();
    let error = recovery_config::load_recovery_permission(
        &path,
        &fixture.root.0.join("issuer-minimum.json"),
        None,
        &trust,
        &fixture.anchor,
    )
    .err()
    .unwrap();
    assert!(
        error.contains("unknown field") || error.contains("missing field"),
        "{error}"
    );
    assert_eq!(
        bytes,
        fs::read(fixture.root.0.join("formal-read-trust.json")).unwrap()
    );
    assert_eq!(fixture.anchor.head().record().original_epoch, 1);
    assert_eq!(
        fs::read_dir(fixture.root.0.join("writer")).unwrap().count(),
        0
    );
}

#[test]
fn formal_recovery_loader_requires_independent_issuer_floor_file() {
    let mut fixture = Fixture::new();
    fixture.config.recovery_permission_file = Some(PathBuf::from("permission.json"));
    fixture.save();
    let error = FormalLiveConfigV1::load(&fixture.config_path)
        .err()
        .unwrap();
    assert!(
        error.contains("separately provisioned issuer minimum"),
        "{error}"
    );
    assert_eq!(
        fs::read_dir(fixture.root.0.join("writer")).unwrap().count(),
        0
    );
    assert!(!fixture.root.0.join("issuer-minimum.json").exists());
}

#[test]
fn formal_recovery_issuer_minimum_namespace_is_bound_to_external_origin() {
    let fixture = Fixture::new();
    let authority = fixture.anchor.authority();
    let minimum = recovery_config::FormalRecoveryIssuerMinimumFileV1 {
        schema_version: 1,
        world: authority.definition().world.clone(),
        full_policy_digest: authority.policy().digest().unwrap(),
        recovery_issuer_public_key: authority.policy().recovery_issuer_public_key.clone(),
        issuer_anchor_public_key: authority.policy().issuer_anchor_public_key.clone(),
        anchor: oasis7_distfs::controlled_authority::formal::issuer::IssuerAnchorV1::initial(
            authority.policy(),
        )
        .unwrap(),
    };
    minimum.verify(&fixture.anchor).unwrap();
    for field in 0..4 {
        let mut forged = minimum.clone();
        match field {
            0 => forged.world.genesis_digest = fixtures::h(99),
            1 => forged.full_policy_digest = fixtures::h(99),
            2 => forged.recovery_issuer_public_key = fixtures::h(99),
            _ => forged.issuer_anchor_public_key = fixtures::h(99),
        }
        assert!(
            forged
                .verify(&fixture.anchor)
                .unwrap_err()
                .contains("namespace mismatch")
        );
    }
    assert_eq!(
        fs::read_dir(fixture.root.0.join("writer")).unwrap().count(),
        0
    );
}
