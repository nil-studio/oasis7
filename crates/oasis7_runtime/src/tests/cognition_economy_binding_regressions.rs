//! Cognition economy authority-binding and snapshot reconstruction regressions.

use super::super::*;
use serde::Serialize;
use serde_json::Value;

fn economy_digest_for_test<T: Serialize>(domain: &str, payload: &T) -> String {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(domain, payload))
        .expect("economy test payload is canonically encodable");
    format!("blake3:{}", blake3::hash(&bytes))
}

fn refresh_provision_event_digest_for_test(event: &mut Value) {
    let mut value = event.clone();
    value
        .as_object_mut()
        .expect("provisioning event is an object")
        .remove("event_digest");
    event["event_digest"] = Value::String(economy_digest_for_test(
        "oasis7.cognition.economy.provisioning-event.v1",
        &value,
    ));
}

fn refresh_provision_head_digest_for_test(economy: &mut Value) {
    let head_seq = economy["provision_head_seq"]
        .as_u64()
        .expect("provision head sequence");
    let journal = economy["provision_journal"].clone();
    economy["provision_head_digest"] = Value::String(economy_digest_for_test(
        "oasis7.cognition.economy.provisioning-journal.v1",
        &(head_seq, journal),
    ));
}

fn authority_quote_for(
    account_id: &str,
    id: &str,
    amount: u64,
    world_binding: &str,
) -> CognitionLeaseQuoteV1 {
    CognitionLeaseQuoteV1::new(id, "cognition_units", amount).with_authority(
        account_id,
        COGNITION_RESOURCE_VERSION_V1,
        "provider_cognition",
        "agent_turn",
        COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
        "authority-context-a",
        world_binding,
    )
}

fn bound_world_with_identity(agent_id: &str, owner_binding: &str) -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register provider agent");
    world
        .bind_cognition_runtime("provision-world", "main", 0, None, "pending", 0)
        .expect("bind cognition runtime");
    world
        .install_capability_agent_identity(agent_id, owner_binding, 1)
        .expect("install capability identity");
    world
}

#[test]
fn bound_runtime_rejects_provider_cognition_from_legacy_only_balance() {
    let mut world = bound_world_with_identity("agent-a", "owner-a");
    world
        .set_cognition_resource_balance("owner-a", "cognition_units", 7)
        .expect("legacy balance can be restored for a bound snapshot");
    let world_binding = world
        .current_cognition_runtime_binding()
        .expect("current runtime binding")
        .base_world_hash
        .to_string();

    let error = world
        .reserve_cognition_lease(CognitionLeaseRequestV1::new(
            "bound-legacy-only",
            "owner-a",
            "agent-a",
            "session-a",
            "turn-a",
            "request-a",
            "request-digest-a",
            authority_quote_for("owner-a", "bound-legacy-quote", 3, &world_binding),
        ))
        .expect_err("bound provider cognition requires an exact provision");
    assert!(format!("{error:?}").contains("cognition_provisioning_binding_required"));

    let economy = world.cognition_economy().expect("read unchanged economy");
    assert_eq!(
        economy
            .balances
            .get("owner-a")
            .and_then(|resources| resources.get("cognition_units"))
            .map(|balance| balance.available),
        Some(7),
        "legacy balance must not be consumed while Runtime is bound"
    );
    assert!(economy.leases.is_empty());
}

#[test]
fn bound_runtime_rejects_legacy_only_balance_after_generation_and_reorg_rotation() {
    let mut world = bound_world_with_identity("agent-a", "owner-a");
    world = world.with_cognition_scheduler(super::agent_cognition_runtime_hardening::policy(), 1);
    world
        .set_cognition_resource_balance("owner-a", "cognition_units", 7)
        .expect("legacy balance can be restored for a bound snapshot");
    world
        .install_capability_agent_identity("agent-a", "owner-a", 2)
        .expect("rotate capability generation");
    world
        .invalidate_cognition_for_reorg(1)
        .expect("authorize the reorg binding");
    world
        .bind_cognition_runtime("provision-world", "main", 0, None, "pending", 1)
        .expect("bind the new reorg epoch");
    let world_binding = world
        .current_cognition_runtime_binding()
        .expect("current rotated runtime binding")
        .base_world_hash
        .to_string();

    let error = world
        .reserve_cognition_lease(CognitionLeaseRequestV1::new(
            "rotated-bound-legacy-only",
            "owner-a",
            "agent-a",
            "session-b",
            "turn-b",
            "request-b",
            "request-digest-b",
            authority_quote_for("owner-a", "rotated-legacy-quote", 3, &world_binding),
        ))
        .expect_err("generation/reorg rotation must not revive legacy funding");
    assert!(format!("{error:?}").contains("cognition_provisioning_binding_required"));

    let error = world
        .provision_cognition_for_agent("agent-a", "rotated-provision", "authority-b", 2)
        .expect_err("post-rotation provisioning cannot guess a legacy binding");
    assert!(format!("{error:?}").contains("cognition_provisioning_balance_already_initialized"));

    let economy = world
        .cognition_economy()
        .expect("read unchanged rotated economy");
    assert_eq!(
        economy
            .balances
            .get("owner-a")
            .and_then(|resources| resources.get("cognition_units"))
            .map(|balance| balance.available),
        Some(7),
        "legacy balance must remain untouched after rotation"
    );
    assert!(economy.leases.is_empty());
    assert!(economy.provisions.is_empty());
}

#[test]
fn provisioning_versions_balances_across_owner_generations() {
    let mut economy = CognitionEconomyStateV1::new();
    let old_request = CognitionProvisioningRequestV1::new(
        "provision-generation-1",
        "owner-a",
        "owner-a",
        1,
        "world-a",
        "main",
        0,
        7,
        "authority-a",
    );
    let old_binding = old_request.binding_key();
    economy
        .provision(old_request, 1)
        .expect("first generation allowance");

    let old_lease_request = CognitionLeaseRequestV1::new(
        "generation-1-lease",
        "owner-a",
        "agent-a",
        "session-a",
        "turn-a",
        "request-a",
        "request-digest-a",
        CognitionLeaseQuoteV1::new("generation-1-quote", "cognition_units", 4).with_authority(
            "owner-a",
            COGNITION_RESOURCE_VERSION_V1,
            "provider_cognition",
            "agent_turn",
            COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
            "authority-a",
            "world-a",
        ),
    );
    let old_lease = economy
        .reserve_for_binding(old_lease_request, old_binding, 2)
        .expect("old generation reserve");

    let new_request = CognitionProvisioningRequestV1::new(
        "provision-generation-2",
        "owner-a",
        "owner-a",
        2,
        "world-a",
        "main",
        1,
        2,
        "authority-b",
    );
    let new_binding = new_request.binding_key();
    economy
        .provision(new_request, 3)
        .expect("new generation allowance must be independently provisionable");
    assert_eq!(economy.available_balance("owner-a", "cognition_units"), 5);

    let new_lease_request = CognitionLeaseRequestV1::new(
        "generation-2-lease-too-large",
        "owner-a",
        "agent-a",
        "session-b",
        "turn-b",
        "request-b",
        "request-digest-b",
        CognitionLeaseQuoteV1::new("generation-2-quote-too-large", "cognition_units", 3)
            .with_authority(
                "owner-a",
                COGNITION_RESOURCE_VERSION_V1,
                "provider_cognition",
                "agent_turn",
                COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
                "authority-b",
                "world-a",
            ),
    );
    assert_eq!(
        economy
            .reserve_for_binding(new_lease_request, new_binding.clone(), 4)
            .expect_err("new generation must not spend old allowance")
            .code(),
        "cognition_insufficient_balance"
    );

    let new_lease_request = CognitionLeaseRequestV1::new(
        "generation-2-lease",
        "owner-a",
        "agent-a",
        "session-b",
        "turn-b",
        "request-b",
        "request-digest-b",
        CognitionLeaseQuoteV1::new("generation-2-quote", "cognition_units", 2).with_authority(
            "owner-a",
            COGNITION_RESOURCE_VERSION_V1,
            "provider_cognition",
            "agent_turn",
            COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
            "authority-b",
            "world-a",
        ),
    );
    economy
        .reserve_for_binding(new_lease_request, new_binding, 4)
        .expect("new generation may spend its new allowance");
    economy
        .release(&old_lease.lease_id, 5)
        .expect("old lease remains bound to old balance");
    economy.validate().expect("versioned balances remain valid");
    assert_eq!(economy.available_balance("owner-a", "cognition_units"), 7);
}

#[test]
fn world_reserve_uses_current_generation_provisioning_binding() {
    let mut world = bound_world_with_identity("agent-a", "owner-a");
    world
        .provision_cognition_for_agent("agent-a", "provision-generation-1", "authority-a", 7)
        .expect("provision generation one allowance");
    let old_world_binding = world
        .current_cognition_runtime_binding()
        .expect("generation one runtime binding")
        .base_world_hash
        .to_string();
    let old_lease = world
        .reserve_cognition_lease(CognitionLeaseRequestV1::new(
            "world-generation-1-lease",
            "owner-a",
            "agent-a",
            "session-a",
            "turn-a",
            "request-a",
            "request-digest-a",
            authority_quote_for("owner-a", "world-generation-1-quote", 4, &old_world_binding),
        ))
        .expect("generation one lease");

    world
        .install_capability_agent_identity("agent-a", "owner-a", 2)
        .expect("rotate capability generation");
    world
        .provision_cognition_for_agent("agent-a", "provision-generation-2", "authority-a", 2)
        .expect("provision generation two allowance");
    let new_world_binding = world
        .current_cognition_runtime_binding()
        .expect("generation two runtime binding")
        .base_world_hash
        .to_string();

    let oversized = world.reserve_cognition_lease(CognitionLeaseRequestV1::new(
        "world-generation-2-oversized",
        "owner-a",
        "agent-a",
        "session-b",
        "turn-b",
        "request-b",
        "request-digest-b",
        authority_quote_for(
            "owner-a",
            "world-generation-2-oversized-quote",
            3,
            &new_world_binding,
        ),
    ));
    let oversized_error = format!(
        "{:?}",
        oversized.expect_err("old allowance must not fund generation two")
    );
    assert!(oversized_error.contains("cognition_insufficient_balance"));

    let new_lease = world
        .reserve_cognition_lease(CognitionLeaseRequestV1::new(
            "world-generation-2-lease",
            "owner-a",
            "agent-a",
            "session-b",
            "turn-b",
            "request-c",
            "request-digest-c",
            authority_quote_for("owner-a", "world-generation-2-quote", 2, &new_world_binding),
        ))
        .expect("generation two allowance");
    world
        .release_cognition_lease(&old_lease.lease_id)
        .expect("generation one lease remains releasable");
    world
        .release_cognition_lease(&new_lease.lease_id)
        .expect("generation two lease remains releasable");
    assert_eq!(
        world
            .cognition_economy()
            .expect("read generation balances")
            .available_balance("owner-a", "cognition_units"),
        9
    );
}

#[test]
fn from_snapshot_validates_cognition_economy_and_accepts_legacy_absence() {
    let world = World::new();
    let mut invalid_snapshot = world.snapshot();
    let mut economy =
        serde_json::to_value(world.cognition_economy().expect("economy")).expect("encode economy");
    economy["head_digest"] = Value::String("blake3:invalid".to_string());
    invalid_snapshot.cognition = serde_json::json!({ "cognition_economy": economy });
    let error = World::from_snapshot(invalid_snapshot, world.journal().clone())
        .expect_err("direct snapshot reconstruction must validate economy");
    assert!(format!("{error:?}").contains("cognition_economy_head_digest_mismatch"));

    let mut legacy_snapshot = world.snapshot();
    legacy_snapshot.cognition = Value::Null;
    World::from_snapshot(legacy_snapshot, world.journal().clone())
        .expect("legacy snapshots without cognition economy remain valid");
}

#[test]
fn provisioning_snapshot_journal_is_a_bijection_with_provisions() {
    let mut economy = CognitionEconomyStateV1::new();
    let provision_a = CognitionProvisioningRequestV1::new(
        "provision-a",
        "owner-a",
        "owner-a",
        1,
        "world-a",
        "main",
        0,
        7,
        "authority-a",
    );
    let provision_b = CognitionProvisioningRequestV1::new(
        "provision-b",
        "owner-b",
        "owner-b",
        1,
        "world-b",
        "main",
        0,
        5,
        "authority-b",
    );
    economy
        .provision(provision_a, 1)
        .expect("install provision A");
    economy
        .provision(provision_b, 2)
        .expect("install provision B");

    let valid_snapshot = economy.snapshot_json().expect("encode valid economy");
    let mut valid_world_snapshot = World::new().snapshot();
    valid_world_snapshot.cognition = serde_json::json!({
        "cognition_economy": valid_snapshot.clone()
    });
    World::from_snapshot(valid_world_snapshot, World::new().journal().clone())
        .expect("one journal event per provision is valid");

    let mut duplicate_snapshot = valid_snapshot;
    let journal = duplicate_snapshot["provision_journal"]
        .as_array_mut()
        .expect("provision journal array");
    let mut duplicate_event = journal[0].clone();
    duplicate_event["journal_seq"] = Value::from(2_u64);
    duplicate_event["parent_event_digest"] = journal[0]["event_digest"].clone();
    refresh_provision_event_digest_for_test(&mut duplicate_event);
    journal[1] = duplicate_event;
    refresh_provision_head_digest_for_test(&mut duplicate_snapshot);

    let mut invalid_world_snapshot = World::new().snapshot();
    invalid_world_snapshot.cognition = serde_json::json!({
        "cognition_economy": duplicate_snapshot
    });
    let error = World::from_snapshot(invalid_world_snapshot, World::new().journal().clone())
        .expect_err("duplicate A event must not mask missing B event");
    assert!(format!("{error:?}").contains("cognition_provisioning_journal_cardinality_invalid"));
}

#[test]
fn legacy_provision_balance_migrates_before_bound_reserve() {
    let mut economy = CognitionEconomyStateV1::new();
    let provision = CognitionProvisioningRequestV1::new(
        "legacy-provision",
        "owner-a",
        "owner-a",
        1,
        "world-a",
        "main",
        0,
        7,
        "authority-a",
    );
    let binding_key = provision.binding_key();
    economy.provision(provision, 1).expect("install provision");
    let old_lease = economy
        .reserve_for_binding(
            CognitionLeaseRequestV1::new(
                "legacy-lease",
                "owner-a",
                "agent-a",
                "session-a",
                "turn-a",
                "request-a",
                "request-digest-a",
                CognitionLeaseQuoteV1::new("legacy-quote", "cognition_units", 4).with_authority(
                    "owner-a",
                    COGNITION_RESOURCE_VERSION_V1,
                    "provider_cognition",
                    "agent_turn",
                    COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
                    "authority-a",
                    "world-a",
                ),
            ),
            binding_key.clone(),
            2,
        )
        .expect("reserve provision");

    let mut legacy_resources = economy
        .provisioned_balances
        .remove(&binding_key)
        .expect("versioned balance");
    let legacy_balance = legacy_resources
        .remove("cognition_units")
        .expect("versioned resource balance");
    economy
        .balances
        .entry("owner-a".to_string())
        .or_default()
        .insert("cognition_units".to_string(), legacy_balance);
    economy.lease_binding_keys.clear();
    let encoded = economy.snapshot_json().expect("encode legacy projection");
    let mut restored = CognitionEconomyStateV1::from_snapshot_json(encoded)
        .expect("legacy projection remains valid");
    let before_replay = restored.clone();
    let replay = restored
        .reserve_for_binding(
            CognitionLeaseRequestV1::new(
                "legacy-lease",
                "owner-a",
                "agent-a",
                "session-a",
                "turn-a",
                "request-a",
                "request-digest-a",
                CognitionLeaseQuoteV1::new("legacy-quote", "cognition_units", 4).with_authority(
                    "owner-a",
                    COGNITION_RESOURCE_VERSION_V1,
                    "provider_cognition",
                    "agent_turn",
                    COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
                    "authority-a",
                    "world-a",
                ),
            ),
            binding_key.clone(),
            3,
        )
        .expect("replay legacy reserve");
    assert_eq!(replay.lease_id, old_lease.lease_id);
    assert_eq!(restored, before_replay, "exact replay remains a no-op");

    let replayed = restored
        .reserve_for_binding(
            CognitionLeaseRequestV1::new(
                "legacy-followup",
                "owner-a",
                "agent-a",
                "session-a",
                "turn-b",
                "request-b",
                "request-digest-b",
                CognitionLeaseQuoteV1::new("legacy-followup-quote", "cognition_units", 2)
                    .with_authority(
                        "owner-a",
                        COGNITION_RESOURCE_VERSION_V1,
                        "provider_cognition",
                        "agent_turn",
                        COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
                        "authority-a",
                        "world-a",
                    ),
            ),
            binding_key,
            3,
        )
        .expect("migrate legacy balance before reserve");
    restored
        .release(&old_lease.lease_id, 4)
        .expect("migrated old lease remains terminal");
    restored
        .release(&replayed.lease_id, 4)
        .expect("new bound lease remains terminal");
    restored
        .validate()
        .expect("migrated projection remains valid");
    assert_eq!(restored.available_balance("owner-a", "cognition_units"), 7);
}
