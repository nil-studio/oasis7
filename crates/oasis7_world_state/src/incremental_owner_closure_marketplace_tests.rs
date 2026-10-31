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
}
