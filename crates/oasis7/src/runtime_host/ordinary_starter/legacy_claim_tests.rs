use super::*;
use oasis7_proto::player_auth::sign_gameplay_action_auth_proof;
use oasis7_proto::viewer::GameplayActionRequest;
use oasis7_runtime::{ChainResourceDerivationContext, DomainEvent, RejectReason, WorldEventBody};

fn published_world() -> World {
    let mut world = crate::runtime_host::world::new();
    let pool = initialize(&mut world).unwrap();
    let tick = world.state().time;
    let snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: "ordinary-starter-test",
            chain_id: "ordinary-starter-test",
            genesis_ref: Some("finite-supply-genesis"),
            created_at_height: tick,
            manifest_height: tick,
            commit_block_hash: None,
            tick,
        },
        "test",
        "test",
    );
    let mut world =
        crate::runtime_host::world::from_snapshot(snapshot, world.journal().clone()).unwrap();
    publish(&mut world, &pool).unwrap();
    world
}

#[test]
fn ordinary_starter_actual_legacy_signed_claims_cannot_take_public_supplier() {
    let mut world = published_world();
    world.submit_action(Action::RegisterAgent {
        agent_id: "attacker-agent".into(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    let private = [93_u8; 32];
    let signing = ed25519_dalek::SigningKey::from_bytes(&private);
    let public = hex::encode(signing.verifying_key().to_bytes());
    let before_materials =
        world.state().material_ledgers[&MaterialLedgerId::agent(SUPPLIER_ID)].clone();
    let before_power = world.state().agents[SUPPLIER_ID]
        .state
        .resources
        .get(ResourceKind::Electricity);
    for (index, action_id) in ["claim_starter_oc", "claim_agent"].iter().enumerate() {
        let mut request = GameplayActionRequest {
            action_id: (*action_id).into(),
            target_agent_id: SUPPLIER_ID.into(),
            actor_agent_id: (*action_id == "claim_agent").then(|| "attacker-agent".into()),
            player_id: "attacker-player".into(),
            public_key: Some(public.clone()),
            auth: None,
        };
        request.auth = Some(
            sign_gameplay_action_auth_proof(
                &request,
                index as u64 + 1,
                &public,
                &hex::encode(private),
            )
            .unwrap(),
        );
        // The production legacy authenticator and its real action builder both accept the
        // signed request; the shared Runtime boundary must protect public source ownership.
        let action = crate::world_service::gameplay::authenticated_action(
            &world,
            &serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        let before = world.journal().events.len();
        world.submit_action(action);
        world.step().unwrap();
        assert!(
            world.journal().events[before..]
                .iter()
                .any(|event| matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        reason: RejectReason::RuleDenied { notes }, ..
                    }) if notes.iter().any(|note| note == "ordinary_starter_supplier_reserved")
                )),
            "{action_id}"
        );
        assert!(!world.state().starter_oc_claims.contains_key(SUPPLIER_ID));
        assert!(!world.state().agent_claims.contains_key(SUPPLIER_ID));
        assert!(world.current_agent_owner(SUPPLIER_ID).is_none());
        assert!(world.current_agent_identity(SUPPLIER_ID).is_none());
        assert_eq!(
            world.state().material_ledgers[&MaterialLedgerId::agent(SUPPLIER_ID)],
            before_materials
        );
        assert_eq!(
            world.state().agents[SUPPLIER_ID]
                .state
                .resources
                .get(ResourceKind::Electricity),
            before_power
        );
    }
    let restored =
        crate::runtime_host::world::from_snapshot(world.snapshot(), world.journal().clone())
            .unwrap();
    assert!(restored.state().is_ordinary_starter_supplier(SUPPLIER_ID));
    assert!(restored.current_agent_owner(SUPPLIER_ID).is_none());
}

#[test]
fn ordinary_starter_publication_rejects_preexisting_identity_without_creating_authority() {
    let mut world = crate::runtime_host::world::new();
    let pool = initialize(&mut world).unwrap();
    let tick = world.state().time;
    let snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: "ordinary-starter-test",
            chain_id: "ordinary-starter-test",
            genesis_ref: Some("finite-supply-genesis"),
            created_at_height: tick,
            manifest_height: tick,
            commit_block_hash: None,
            tick,
        },
        "test",
        "test",
    );
    let mut world =
        crate::runtime_host::world::from_snapshot(snapshot, world.journal().clone()).unwrap();
    world
        .install_capability_agent_identity(SUPPLIER_ID, "occupied-owner", 1)
        .unwrap();
    let before = world.snapshot();
    assert!(publish(&mut world, &pool).is_err());
    assert_eq!(world.snapshot(), before);
    assert!(
        world
            .state()
            .world_rules
            .as_ref()
            .is_none_or(|rules| rules.ordinary_starter.offers.is_empty())
    );
}
