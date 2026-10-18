//! Minimal finalized-authority provider capability fixture.

use super::super::*;
use super::capability_grant_v2::*;
use crate::CapabilityInvocationContext;
use oasis7_wasm_abi::{CapabilityCatalogSnapshot, CapabilityGrantV2, CapabilityPresenter};
use serde_json::json;

/// Authority and finality are installed before the command grant; the
/// host-bound invocation is persisted so a strict provider caller can
/// discover a subject/session without fabricating grant fields.
pub(super) fn fixture_world_with_provider_invocation() -> (
    World,
    CapabilityGrantV2,
    CapabilityCatalogSnapshot,
    CapabilityInvocationContext,
) {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "grant_nonce": "provider-fixture-command",
    })));
    world
        .register_capability_grant_v2(grant.clone())
        .expect("register finalized provider command grant");
    let presenter = CapabilityPresenter {
        presenter_id: PRESENTER_ID.to_string(),
        presenter_kind: "provider".to_string(),
        session_id: Some("provider-session-fixture".to_string()),
        attestation_ref: None,
    };
    let (catalog, invocation) = world
        .capability_context_for_agent(SUBJECT_ID, presenter, "provider-fixture-response")
        .expect("derive provider capability context from finalized World");
    world
        .install_capability_invocation_context(invocation.clone())
        .expect("persist provider invocation context");
    (world, grant, catalog, invocation)
}
