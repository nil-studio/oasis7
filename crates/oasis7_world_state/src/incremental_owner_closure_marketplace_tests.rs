use super::*;

#[test]
fn module_marketplace_fields_require_metadata_and_closed_typed_members() {
    let complete = verify(&fixture(true), || false).unwrap();
    assert_eq!(complete.checked_module_marketplace_fields(), 3);

    let empty = verify(
        &fixture_with_issue(true, Some(FixtureIssue::EmptyModuleMarketplaceMaps)),
        || false,
    )
    .unwrap();
    assert_eq!(empty.checked_module_marketplace_fields(), 3);

    for (issue, expected) in [
        (
            FixtureIssue::MissingModuleMarketplaceMetadata,
            "required module marketplace metadata missing or duplicate",
        ),
        (
            FixtureIssue::WrongModuleMarketplaceKey,
            "module marketplace outer semantic key",
        ),
        (
            FixtureIssue::WrongModuleMarketplaceCodec,
            "module marketplace outer record-root profile",
        ),
        (
            FixtureIssue::InvalidModuleMarketplacePayload,
            "module marketplace root binding",
        ),
    ] {
        let result = verify(&fixture_with_issue(true, Some(issue)), || false);
        assert!(
            matches!(result, Err(WorldStateOwnerError::Invalid(actual)) if actual == expected),
            "{issue:?}: {result:?}"
        );
    }

    for (issue, expected) in [
        (
            FixtureIssue::MissingModuleMarketplaceChild,
            "module visual child closure",
        ),
        (
            FixtureIssue::UnknownModuleMarketplaceChild,
            "module marketplace child semantic key",
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
            &fixture_with_issue(true, Some(FixtureIssue::WrongModuleMarketplaceChildCount)),
            || false
        ),
        Err(WorldStateOwnerError::Invalid("state scan leaf count"))
    ));

    for issue in [
        FixtureIssue::InvalidModuleBidParentLink,
        FixtureIssue::SharedModuleBidChild,
        FixtureIssue::ModuleBidChildCycle,
    ] {
        let result = verify(&fixture_with_issue(true, Some(issue)), || false);
        assert!(
            matches!(result, Err(WorldStateOwnerError::Invalid(_))),
            "{issue:?}: {result:?}"
        );
    }
}

#[test]
fn marketplace_owner_closure_uses_recovery_class_when_ordinary_is_full() {
    let fixture = fixture(true);
    fixture
        .state
        .set_read_class(oasis7_proto::memory_budget::MemoryClass::Recovery);

    let ordinary_limit = (96 << 20) - (4 << 20);
    let used_before_pressure = fixture.memory.used();
    assert!(used_before_pressure <= ordinary_limit);
    let pressure = fixture
        .memory
        .reserve(ordinary_limit - used_before_pressure, MemoryClass::Ordinary)
        .unwrap();
    assert_eq!(fixture.memory.used(), ordinary_limit);

    let report = verify_with_class(&fixture, MemoryClass::Recovery, || false).unwrap();
    assert_eq!(report.checked_module_marketplace_fields(), 3);
    assert!(fixture.memory.reserve(1, MemoryClass::Ordinary).is_err());
    drop(report);
    assert_eq!(fixture.memory.used(), ordinary_limit);
    drop(pressure);
    assert_eq!(fixture.memory.used(), used_before_pressure);
}
