//! World-owned capability catalog/context projection tests.

use super::super::*;
use super::capability_grant_v2::*;
use super::capability_grant_v2_fixture::fixture_world_with_provider_invocation;
use oasis7_wasm_abi::CapabilityPresenter;
use serde_json::json;

pub(super) fn budget_account_snapshot(world: &World) -> CapabilityBudgetAccount {
    assert_eq!(
        world.capability_budget_accounts().len(),
        1,
        "fixture has one subject/grant budget account"
    );
    world
        .capability_budget_accounts()
        .values()
        .next()
        .cloned()
        .expect("fixture budget account")
}

#[test]
fn world_emits_subject_bound_capability_context_from_live_grants() {
    let mut world = fixture_world();
    let command_grant = signed_grant(grant_json(json!({
        "grant_nonce": "command-grant-nonce-1"
    })));
    world
        .register_capability_grant_v2(command_grant.clone())
        .expect("register command grant for runtime catalog");
    let presenter = CapabilityPresenter {
        presenter_id: PRESENTER_ID.to_string(),
        presenter_kind: "provider".to_string(),
        session_id: Some("provider-session-1".to_string()),
        attestation_ref: None,
    };

    let (catalog, invocation) = world
        .capability_context_for_agent(SUBJECT_ID, presenter.clone(), "response-nonce-1")
        .expect("derive live capability catalog and invocation context");

    catalog.validate().expect("catalog is valid");
    assert_eq!(
        catalog.snapshot_id,
        catalog.canonical_hash().expect("catalog hash")
    );
    assert_eq!(catalog.world_id, WORLD_ID);
    assert_eq!(catalog.branch_id, BRANCH_ID);
    assert_eq!(catalog.finality_epoch, 4);
    assert_eq!(catalog.logical_tick, world.state().time);
    assert_eq!(catalog.subject, command_grant.subject);
    assert_eq!(catalog.presenter, presenter);
    assert!(
        catalog
            .entries
            .iter()
            .any(|entry| entry.module_id == MODULE_ID
                && entry.module_version == MODULE_VERSION
                && entry.command == "observe"
                && entry.eligible_grant_ids == vec![command_grant.grant_id.clone()])
    );
    assert_eq!(invocation.grant_id, command_grant.grant_id);
    assert_eq!(invocation.subject, catalog.subject);
    assert_eq!(invocation.presenter, catalog.presenter);
    assert_eq!(invocation.audience, catalog.audience);
    assert_eq!(invocation.catalog_snapshot_id, catalog.snapshot_id);
    assert_eq!(invocation.module_id, MODULE_ID);
    assert_eq!(invocation.module_version, MODULE_VERSION);
    assert_eq!(invocation.response_nonce, "response-nonce-1");
}

#[test]
fn world_rejects_capability_context_without_a_live_command_grant() {
    let world = fixture_world();
    let presenter = CapabilityPresenter {
        presenter_id: PRESENTER_ID.to_string(),
        presenter_kind: "provider".to_string(),
        session_id: Some("provider-session-1".to_string()),
        attestation_ref: None,
    };
    let error = world
        .capability_context_for_agent(SUBJECT_ID, presenter, "response-nonce-1")
        .expect_err("effect-only grants must not become command context");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
}

#[test]
fn minimal_provider_fixture_has_finalized_authority_and_persisted_invocation() {
    let (world, grant, catalog, invocation) = fixture_world_with_provider_invocation();
    assert_eq!(world.capability_grants_v2().len(), 2);
    assert_eq!(world.capability_authorization_root().len(), 64);
    assert_eq!(grant.status, "verified");
    assert_eq!(grant.audience.world_id, WORLD_ID);
    assert_eq!(catalog.world_id, WORLD_ID);
    assert!(
        world
            .capability_invocation_contexts()
            .values()
            .any(|stored| stored == &invocation)
    );
    assert_eq!(invocation.presenter.presenter_kind, "provider");
    assert_eq!(
        invocation.presenter.session_id.as_deref(),
        Some("provider-session-fixture")
    );
}
