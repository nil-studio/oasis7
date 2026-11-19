//! Current owner readers against manufactured-agent World oracles and a
//! malformed same-transaction manifest child replacement.
use super::*;
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateDeltaV1, StateNamespaceV1, StateWriteV1},
};
use oasis7_world_state::{
    incremental_field_member::{MemberPayloadRefV1, write_member_record_v1},
    incremental_runtime_artifact_resources::{
        RuntimeArtifactFieldV1, RuntimeArtifactRecordV1, inspect_runtime_artifact_record_root_v1,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    world_rules::AgentIndustrialPhaseV1,
};

fn manufactured_owner_world() -> (World, String) {
    let mut world = producing_world();
    let delivered = super::factory_order_directory::directory_order(
        "delivered-owner",
        AgentIndustrialPhaseV1::Delivered,
    );
    let manufactured = delivered.target_agent_id.clone();
    let digest = "ab".repeat(32);

    let now = world.state.time;
    world.state.agents.insert(
        manufactured.clone(),
        crate::AgentCell::new(
            oasis7_world_state::AgentState::new(
                manufactured.clone(),
                oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
            ),
            now,
        ),
    );
    world.state.agent_claims.insert(
        manufactured.clone(),
        serde_json::from_value(serde_json::json!({
            "control_source": {
                "acquisition_sequence": 1,
                "root_agent_id": "owner",
                "player_id": "player",
                "root_claimed_at": 0,
                "acquired_event_digest": digest.clone()
            },
            "target_agent_id": manufactured.clone(),
            "claim_owner_id": "owner"
        }))
        .unwrap(),
    );
    world
        .state
        .agent_claim_acquisition_sequences
        .insert(manufactured.clone(), 1);
    world.state.starter_oc_claims.insert(
        manufactured.clone(),
        oasis7_world_state::StarterOcClaimState {
            agent_id: manufactured.clone(),
            player_id: "player".into(),
            public_key: Some("manufactured-key".into()),
            ..Default::default()
        },
    );
    world
        .state
        .factories
        .get_mut("assembler")
        .unwrap()
        .builder_agent_id = manufactured.clone();
    world
        .state
        .factory_site_authorities
        .get_mut("site")
        .unwrap()
        .owner_agent_id = manufactured.clone();
    let order = world
        .state
        .world_rules
        .as_mut()
        .unwrap()
        .agent_orders
        .get_mut("order")
        .unwrap();
    order.owner_id = manufactured.clone();
    order.quote.owner_id = manufactured.clone();
    order.quote.owner_source_id = format!("manufactured_claim:{digest}");
    order.quote.delivery_location_id = "location".into();
    order.quote.quote_digest.clear();
    order.quote.quote_digest = crate::util::hash_json(&order.quote).unwrap();
    order.validate().unwrap();
    world
        .state
        .world_rules
        .as_mut()
        .unwrap()
        .agent_orders
        .insert(delivered.order_id.clone(), delivered);
    // Declare the actual initial identity before GEN. The administrative
    // install helper publishes an event and therefore is not a fresh source.
    world.capability_revocation_state.agent_identities.insert(
        "owner".into(),
        crate::capability_authorization::CapabilityAgentIdentity {
            owner_binding: "player".into(),
            generation: 1,
        },
    );
    assert!(world.journal.events.is_empty());
    (world, manufactured)
}

fn typed_work() -> oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1 {
    super::typed()
}

#[test]
fn current_manufactured_owner_matches_world_oracles_and_missing_required_option_value_aborts() {
    let (world, manufactured) = manufactured_owner_world();
    assert!(world.chain_resource_manifest.genesis_ref.is_none());
    let oracle_order = world.state.world_rules.as_ref().unwrap().agent_orders["order"].clone();
    assert_eq!(
        world
            .state
            .resolved_agent_owner(&manufactured)
            .unwrap()
            .primary_agent_id,
        "owner"
    );
    assert_eq!(
        world
            .current_agent_identity(&manufactured)
            .unwrap()
            .owner_binding,
        "player"
    );
    assert_eq!(oracle_order.owner_id, manufactured);
    current_equals_independent_world(world.clone(), false);
    // A second genuine fresh source covers the original Some wire. Both
    // quotes must use that actual World binding before their digest is sealed.
    let mut world = world;
    world.chain_resource_manifest.world_id = "manufactured-world".into();
    world.chain_resource_manifest.chain_id = "manufactured-chain".into();
    world.chain_resource_manifest.genesis_ref = Some("actual-genesis".into());
    let binding = world.world_rule_binding();
    assert!(binding.is_some());
    for order in world
        .state
        .world_rules
        .as_mut()
        .unwrap()
        .agent_orders
        .values_mut()
    {
        order.quote.world_binding = binding.clone();
        order.quote.quote_digest.clear();
        order.quote.quote_digest = crate::util::hash_json(&order.quote).unwrap();
        order.validate().unwrap();
    }
    current_equals_independent_world(world.clone(), false);

    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut read_io = genesis::io();
    let mut read_keys = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut read_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut scan = genesis::scan();
    let mut order_typed = typed_work();
    let proof =
        qualify_world_rule_agent_orders_v1(&fixture.roots, &mut read, &mut scan, &mut order_typed)
            .unwrap();
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [93; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 4096,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut overlay_io = genesis::io();
    let mut overlay_keys = genesis::keys();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut overlay_io,
        },
        key_work: &mut overlay_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let guard = overlay.transaction.guard();
    let order_source = bind_current_agent_orders_v1(
        &proof,
        &fixture.roots,
        &reader,
        &mut overlay,
        &mut scan,
        &mut order_typed,
        &guard,
        &live,
    )
    .unwrap();
    let mut directory_work = FactoryOrderDirectoryWorkV1 {
        ids:
            oasis7_world_state::incremental_world_rule_composite_family::AgentOrderDirectoryWorkV1 {
                max_ids: 32,
                max_id_bytes: 256,
                comparison_bytes_remaining: 16 << 20,
            },
        pending_entries_remaining: 10000,
        merge_comparison_bytes_remaining: 16 << 20,
        max_index_text_bytes: 256,
    };
    let guard = overlay.transaction.guard();
    let accepted = build_accepted_factory_order_directory_v1(
        &fixture.runtime,
        &fixture.head,
        &order_source,
        &reader,
        &mut overlay,
        &mut scan,
        &mut order_typed,
        &mut directory_work,
        &guard,
        &live,
    )
    .unwrap();
    // Replace the real Current outer manifest pointer with a child root that
    // preserves `genesis_ref.present=true` but removes its required Text cell.
    let mut artifact_io = genesis::io();
    let mut artifact_keys = genesis::keys();
    let child = {
        let mut artifact_read =
            oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                root: fixture.roots.state_root(StateNamespaceV1::Artifacts),
                state: &reader,
                values: ValueReadContext {
                    source: &reader,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut artifact_io,
                },
                key_work: &mut artifact_keys,
                semantic_bytes: 4096,
                record_bytes: 16 << 20,
                take_state_error: || reader.cas.error(),
                cancelled: || false,
            };
        inspect_runtime_artifact_record_root_v1(
            RuntimeArtifactFieldV1::ChainResourceManifest,
            &mut artifact_read,
        )
        .unwrap()
        .unwrap()
    };
    let option_semantic = oasis7_world_state::incremental_semantic_key::hash_semantic_key_v1(
        &("genesis_ref", "value"),
        &memory,
        MemoryClass::Ordinary,
        &mut artifact_keys,
        &mut || false,
    )
    .unwrap();
    let missing_key = WorldFieldKeyV1::new(
        RuntimeArtifactRecordV1::ChainResourceManifest.descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(option_semantic.digest),
    )
    .unwrap();
    let _point_workspace = memory.reserve(8 << 10, MemoryClass::Ordinary).unwrap();
    let before = child
        .root()
        .get(missing_key.as_bytes(), &reader)
        .unwrap()
        .unwrap();
    let malformed_delta = StateDeltaV1::build(
        child.root().clone(),
        vec![StateWriteV1 {
            key: missing_key.as_bytes().to_vec(),
            before: Some(before),
            after: None,
        }],
        &reader,
        16 << 20,
    )
    .unwrap();
    let mut cas = fixture.cas.clone();
    for node in &malformed_delta.new_nodes {
        cas.put_node(
            *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
            memory
                .reserve(node.bytes.len(), MemoryClass::Ordinary)
                .unwrap()
                .retain(node.bytes.clone()),
        )
        .unwrap();
    }
    let outer_semantic = oasis7_world_state::incremental_semantic_key::hash_semantic_key_v1(
        &(),
        &memory,
        MemoryClass::Ordinary,
        &mut artifact_keys,
        &mut || false,
    )
    .unwrap();
    let reference = RecordRootRefV1::new(
        RuntimeArtifactFieldV1::ChainResourceManifest.schema(),
        outer_semantic.digest,
        malformed_delta.after,
    )
    .unwrap();
    let mut pointer_io = genesis::io();
    let mut pointer_keys = genesis::keys();
    let mut pointer_writer = genesis::member(
        &mut cas,
        &memory,
        &mut pointer_io,
        &mut pointer_keys,
        || false,
    );
    let pointer = write_member_record_v1(
        RuntimeArtifactFieldV1::ChainResourceManifest.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::RecordRoot(&reference),
        &mut pointer_writer,
    )
    .unwrap();
    overlay
        .transaction
        .write(
            StateNamespaceV1::Artifacts,
            pointer.field_key.as_bytes(),
            Some(pointer.value_root),
        )
        .unwrap();
    let malformed_guard = overlay.transaction.guard();
    let directory = open_current_factory_order_directory_v1(
        &accepted,
        &order_source,
        &reader,
        &mut overlay,
        &mut scan,
        &mut order_typed,
        &mut directory_work,
        &malformed_guard,
        &live,
    )
    .unwrap();
    let native = fixture.current.native();
    let runtime = fixture.current.runtime();
    let sources = IndustrialEligibilitySourcesV1 {
        roots: &fixture.roots,
        orders: &directory,
        agents: fixture.current.raw().agents(),
        native: native.native(),
        factories: native.factory(),
        sites: native.factory_authority(),
        anchors: native.agent_authority(),
        revocation: runtime.revocation(),
        artifacts: runtime.resources(),
    };
    let mut work = IndustrialEligibilityWorkV1 {
        read: IndustrialEligibilityReadWorkV1 {
            cells_remaining: 100000,
            comparison_bytes_remaining: 32 << 20,
            max_text_bytes: 4096,
        },
        typed: typed_work(),
        scan: genesis::scan(),
        revocation: typed_work(),
        pending_entries_remaining: 100000,
    };
    let malformed_guard = overlay.transaction.guard();
    let result = current_producing_order_blocker_v1(
        &sources,
        "order",
        2,
        &reader,
        &mut overlay,
        &mut read,
        &mut work,
        &malformed_guard,
        &live,
    );
    assert!(
        result.is_err(),
        "a missing required option cell must fail closed"
    );
    assert!(matches!(
        overlay.transaction.validate_guard(&malformed_guard, &live),
        Err(ExecutionError::Aborted)
    ));
}
