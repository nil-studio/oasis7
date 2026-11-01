use super::*;

#[test]
fn complete_registered_scalar_set_and_map_families_remain_incomplete_world_owners() {
    let fixture = fixture(true);
    let report = verify(&fixture, || false).unwrap();
    assert_eq!(
        report.root_set_digest(),
        root_set(&fixture).digest().unwrap()
    );
    assert_eq!(report.checked_u64_fields(), 7);
    assert_eq!(report.checked_id_set_fields(), 5);
    assert_eq!(report.checked_number_map_fields(), 11);
    assert_eq!(report.checked_text_byte_fields(), 4);
    assert_eq!(report.checked_installed_target_fields(), 1);
    assert_eq!(report.checked_world_balance_fields(), 2);
    assert_eq!(report.checked_module_instance_fields(), 8);
    assert_eq!(report.checked_module_marketplace_fields(), 3);
    assert!(report.unsupported_fields().contains(&"resources"));
    assert!(
        !report
            .unsupported_fields()
            .contains(&"installed_module_targets")
    );
    assert!(!report.unsupported_fields().contains(&"main_token_balances"));
    assert!(report.runtime_registry_missing());
    assert!(report.cognition_registry_missing());
    assert!(!report.unsupported_fields().is_empty());
}

#[test]
fn owner_coverage_digest_binds_the_complete_root_set_identity() {
    let fixture = fixture(true);
    let root_set = root_set(&fixture);
    let report =
        verify_with_root_set(&fixture, &root_set, MemoryClass::Ordinary, || false).unwrap();
    let digest = root_set.digest().unwrap();
    assert_eq!(report.root_set_digest(), digest);

    let mut variants = Vec::new();
    let mut changed = root_set.clone();
    changed.world_id.push_str("-other");
    variants.push(changed);
    let mut changed = root_set.clone();
    changed.genesis_config[0] ^= 1;
    variants.push(changed);
    let mut changed = root_set.clone();
    changed.height += 1;
    variants.push(changed);
    let mut changed = root_set.clone();
    changed.simulation_tick += 1;
    variants.push(changed);
    let mut changed = root_set.clone();
    changed.state[0] = StateRootV1::empty(StateNamespaceV1::Business).unwrap();
    variants.push(changed);

    for changed in variants {
        assert_ne!(changed.digest().unwrap(), report.root_set_digest());
    }

    let mut invalid = root_set;
    invalid.state[0].namespace = StateNamespaceV1::Allocator;
    assert!(matches!(
        verify_with_root_set(&fixture, &invalid, MemoryClass::Ordinary, || false),
        Err(WorldStateOwnerError::RootSet(_))
    ));
}

#[test]
fn installed_targets_and_world_balances_require_metadata_and_typed_members() {
    let complete = verify(&fixture(true), || false).unwrap();
    assert_eq!(complete.checked_installed_target_fields(), 1);
    assert_eq!(complete.checked_world_balance_fields(), 2);

    let empty = verify(
        &fixture_with_issue(true, Some(FixtureIssue::EmptyTargetAndBalanceMaps)),
        || false,
    )
    .unwrap();
    assert_eq!(empty.checked_installed_target_fields(), 1);
    assert_eq!(empty.checked_world_balance_fields(), 2);

    for (issue, expected) in [
        (
            FixtureIssue::MissingInstalledTargetMetadata,
            "required installed-target metadata missing or duplicate",
        ),
        (
            FixtureIssue::WrongInstalledTargetKey,
            "installed target semantic key profile",
        ),
        (
            FixtureIssue::WrongInstalledTargetCodec,
            "installed target payload profile",
        ),
        (
            FixtureIssue::MissingWorldBalanceMetadata,
            "required world-balance metadata missing or duplicate",
        ),
        (
            FixtureIssue::WrongWorldBalanceKey,
            "world balance semantic key profile",
        ),
    ] {
        let result = verify(&fixture_with_issue(true, Some(issue)), || false);
        assert!(
            matches!(result, Err(WorldStateOwnerError::Invalid(actual)) if actual == expected),
            "{issue:?}: {result:?}"
        );
    }

    assert!(matches!(
        verify(
            &fixture_with_issue(true, Some(FixtureIssue::WrongWorldBalanceCodec)),
            || false
        ),
        Err(WorldStateOwnerError::Value(ValueTreeError::Invalid(_)))
    ));
}

#[test]
fn module_instance_owner_requires_eight_metadata_and_same_identity_siblings() {
    let fixture = fixture(true);
    let report = verify(&fixture, || false).unwrap();
    assert_eq!(report.checked_module_instance_fields(), 8);

    let empty = verify(
        &fixture_with_issue(true, Some(FixtureIssue::EmptyModuleInstances)),
        || false,
    )
    .unwrap();
    assert_eq!(empty.checked_module_instance_fields(), 8);

    for (issue, expected) in [
        (
            FixtureIssue::MissingModuleInstanceMetadata,
            "required module instance metadata missing or duplicate",
        ),
        (
            FixtureIssue::MissingModuleInstanceSibling,
            "module instance sibling field missing",
        ),
        (
            FixtureIssue::WrongModuleInstanceSemanticKey,
            "module instance sibling field missing",
        ),
        (
            FixtureIssue::WrongModuleInstanceCodec,
            "module instance member payload profile",
        ),
    ] {
        let result = verify(&fixture_with_issue(true, Some(issue)), || false);
        assert!(
            matches!(result, Err(WorldStateOwnerError::Invalid(actual)) if actual == expected),
            "{issue:?}: {result:?}"
        );
    }
}

#[test]
fn text_and_byte_owners_require_registered_metadata_and_exact_member_profiles() {
    let fixture = fixture(true);
    let report = verify(&fixture, || false).unwrap();
    assert_eq!(report.checked_text_byte_fields(), 4);
    assert!(report.unsupported_fields().contains(&"resources"));

    for (issue, expected) in [
        (
            FixtureIssue::WrongTextKey,
            "typed owner semantic key profile",
        ),
        (
            FixtureIssue::InvalidTextPayload,
            "typed owner text payload UTF-8",
        ),
        (
            FixtureIssue::WrongTextCodec,
            "typed owner text/bytes payload profile",
        ),
        (
            FixtureIssue::MissingTextMetadata,
            "required text/bytes metadata missing or duplicate",
        ),
    ] {
        let result = verify(&fixture_with_issue(true, Some(issue)), || false);
        assert!(
            matches!(result, Err(WorldStateOwnerError::Invalid(actual)) if actual == expected),
            "{issue:?}: {result:?}"
        );
    }
    assert!(matches!(
        verify(
            &fixture_with_issue(true, Some(FixtureIssue::WrongBytesCodec)),
            || false
        ),
        Err(WorldStateOwnerError::Value(ValueTreeError::Invalid(
            "member key/payload kind"
        )))
    ));
}

#[test]
fn owner_semantic_key_probe_handles_utf8_boundaries_and_rejects_wrong_tokens() {
    let ascii_prefix_len = 64 * 1024 - (SEMANTIC_KEY_DOMAIN.len() + 9) - 1;
    let text = format!("{}é-tail", "a".repeat(ascii_prefix_len));
    let mut encoded = Vec::new();
    crate::incremental_semantic_key::write_semantic_key_v1(text.as_str(), &mut encoded).unwrap();
    let mut probe = OwnerSemanticKeyProbe::new(OwnerKeyKind::Text, encoded.len() as u64);
    for byte in &encoded {
        probe.write_all(std::slice::from_ref(byte)).unwrap();
    }
    assert!(probe.finish(), "split multi-byte UTF-8 key should validate");

    let mut wrong_token = OwnerSemanticKeyProbe::new(OwnerKeyKind::Action, 128);
    assert!(wrong_token.write_all(&encoded).is_err());
    assert!(!wrong_token.finish());

    let mut malformed = SEMANTIC_KEY_DOMAIN.to_vec();
    malformed.push(1);
    malformed.extend_from_slice(&1u64.to_be_bytes());
    malformed.push(0xff);
    let mut invalid_utf8 = OwnerSemanticKeyProbe::new(OwnerKeyKind::Text, 128);
    assert!(invalid_utf8.write_all(&malformed).is_err());
    assert!(!invalid_utf8.finish());
}

#[test]
fn missing_required_u64_entry_fails_closed_even_when_all_metadata_is_present() {
    let fixture = fixture(false);
    assert!(matches!(
        verify(&fixture, || false),
        Err(WorldStateOwnerError::Invalid(
            "required scalar-u64 metadata/entry missing or duplicate"
        ))
    ));
}

#[test]
fn full_owner_scan_observes_cancellation() {
    let fixture = fixture(true);
    assert!(matches!(
        verify(&fixture, || true),
        Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled))
    ));
}

#[test]
fn text_byte_owner_scan_observes_late_cancellation() {
    let fixture = fixture(true);
    let calls = std::cell::Cell::new(0usize);
    assert!(matches!(
        verify(&fixture, || {
            let next = calls.get() + 1;
            calls.set(next);
            next == 64
        }),
        Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled))
    ));
    assert_eq!(calls.get(), 64);
}
