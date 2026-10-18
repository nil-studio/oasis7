//! Runtime-owned provider fixture bootstrap coverage.

use super::super::*;
use super::pos;
use oasis7_wasm_abi::CapabilityPresenter;

#[test]
fn test_provider_fixture_persists_finalized_authority_and_context() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-0".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register provider fixture agent");
    world
        .bind_cognition_runtime(
            "live-runtime-minimal",
            "provider-context-branch",
            0,
            None,
            "pending",
            0,
        )
        .expect("bind Runtime cognition authority");

    let invocation = world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install Runtime-owned provider fixture");
    assert_eq!(invocation.presenter.presenter_kind, "provider");
    assert_eq!(invocation.audience.world_id, "live-runtime-minimal");
    assert_eq!(invocation.audience.branch_id, "provider-context-branch");
    assert_eq!(invocation.audience.finality_epoch, 0);
    assert_eq!(world.capability_grants_v2().len(), 1);
    assert_eq!(
        world.capability_revocation_state().authority_records.len(),
        1
    );
    assert!(
        world
            .capability_invocation_contexts()
            .values()
            .any(|stored| stored == &invocation)
    );
    let (catalog, projected) = world
        .capability_context_for_agent(
            "agent-0",
            CapabilityPresenter {
                presenter_id: invocation.presenter.presenter_id.clone(),
                presenter_kind: "provider".to_string(),
                session_id: invocation.presenter.session_id.clone(),
                attestation_ref: None,
            },
            "provider-turn-1",
        )
        .expect("project current provider context from durable authority");
    catalog.validate().expect("fixture catalog validates");
    assert_eq!(catalog.audience, projected.audience);
    assert_eq!(catalog.subject, projected.subject);
}
