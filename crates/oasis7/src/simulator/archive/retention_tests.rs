use crate::simulator::WorldKernel;

#[test]
fn restored_snapshot_keeps_external_cut_exactly_through_no_journal_mutations() {
    let mut snapshot = WorldKernel::new().snapshot();
    snapshot.runtime_snapshot = Some(super::tests::runtime_fixture().snapshot());
    snapshot.player_gameplay = Some(serde_json::from_value(serde_json::json!({
        "stage_id":"first_session_loop", "stage_status":"active", "execution_state":"accepted",
        "goal_id":"retained-goal", "goal_kind":"create_first_world_feedback", "goal_title":"Goal",
        "objective":"Original objective", "progress_detail":"Historical progress", "next_step_hint":"Resume"
    })).unwrap());
    snapshot.chain_resource_manifest.world_id = "external-cut".into();
    snapshot.chain_resource_manifest.created_at_block_hash = Some("original-block".into());
    snapshot.chain_resource_manifest.refresh_hashes();
    let json = serde_json::to_vec(&snapshot).unwrap();
    let cbor = serde_cbor::to_vec(&snapshot).unwrap();
    let mut host =
        WorldKernel::from_snapshot(snapshot, WorldKernel::new().journal_snapshot()).unwrap();
    assert_eq!(serde_json::to_vec(&host.snapshot()).unwrap(), json);
    assert_eq!(serde_cbor::to_vec(&host.snapshot()).unwrap(), cbor);
    let explicit = host.snapshot_with_chain_resource_context(
        crate::chain_resource_schema::ChainResourceDerivationContext::simulator_default(
            host.time(),
        ),
    );
    assert_ne!(explicit.chain_resource_manifest.world_id, "external-cut");
    assert_eq!(explicit.runtime_snapshot, host.snapshot().runtime_snapshot);
    assert_eq!(explicit.player_gameplay, host.snapshot().player_gameplay);
    let ttl = host.kernel_snapshot().intel_ttl_ticks;
    host.set_intel_ttl_ticks(ttl + 1);
    assert_eq!(host.journal().len(), 0);
    host.set_intel_ttl_ticks(ttl);
    assert_eq!(serde_json::to_vec(&host.snapshot()).unwrap(), json);
    assert_eq!(serde_cbor::to_vec(&host.snapshot()).unwrap(), cbor);
}

#[test]
fn default_chain_evidence_refreshes_after_restore_and_replay() {
    let origin = WorldKernel::new().snapshot();
    let journal = WorldKernel::new().journal_snapshot();
    let mut original = WorldKernel::from_snapshot(origin.clone(), journal.clone()).unwrap();
    original.submit_action(oasis7_simulator::Action::RegisterLocation {
        location_id: "new-location".into(),
        name: "New location".into(),
        pos: crate::GeoPos::new(0, 0, 0),
        profile: Default::default(),
    });
    original.step().unwrap();
    let default = original.snapshot_with_chain_resource_context(
        crate::chain_resource_schema::ChainResourceDerivationContext::simulator_default(
            original.time(),
        ),
    );
    assert_eq!(original.snapshot(), default);
    let replay = WorldKernel::replay_from_snapshot(origin, original.journal_snapshot()).unwrap();
    assert_eq!(replay.snapshot(), default);
    let restored =
        WorldKernel::from_snapshot(default.clone(), original.journal_snapshot()).unwrap();
    assert_eq!(restored.snapshot(), default);
}

#[test]
fn journal_view_borrows_native_payload_and_survives_sidecar_tail_append() {
    let original = super::tests::runtime_fixture();
    let mut host = WorldKernel::new();
    host.submit_action(oasis7_simulator::Action::RegisterLocation {
        location_id: "origin".into(),
        name: "Origin".into(),
        pos: crate::GeoPos::new(0, 0, 0),
        profile: Default::default(),
    });
    host.step().unwrap();
    host.submit_action(oasis7_simulator::Action::RegisterAgent {
        agent_id: "before-restore".into(),
        location_id: "origin".into(),
    });
    host.step().unwrap();
    let mut journal = host.journal_snapshot();
    journal.events[1].runtime_event = Some(original.journal().events.last().unwrap().clone());
    let mut host = WorldKernel::from_snapshot(host.snapshot(), journal.clone()).unwrap();
    let native: &oasis7_simulator::WorldKernel = &host;
    assert!(std::ptr::eq(
        host.journal().get(0).unwrap().kind,
        &native.journal()[0].kind
    ));
    for i in 0..8 {
        host.submit_action(oasis7_simulator::Action::RegisterAgent {
            agent_id: format!("agent-{i}"),
            location_id: "origin".into(),
        });
        host.step().unwrap();
    }
    assert_eq!(host.journal().get(0).unwrap().to_owned(), journal.events[0]);
    assert_eq!(host.journal().get(1).unwrap().to_owned(), journal.events[1]);
    assert!(host.journal().get(0).unwrap().runtime_event.is_none());
    assert!(host.journal().get(1).unwrap().runtime_event.is_some());
    assert!(
        host.journal()
            .iter()
            .skip(2)
            .all(|event| event.runtime_event.is_none())
    );
    let owned = host.journal_snapshot();
    assert_eq!(
        serde_json::to_vec(&host.journal()).unwrap(),
        serde_json::to_vec(&owned.events).unwrap()
    );
    assert_eq!(
        serde_cbor::to_vec(&host.journal()).unwrap(),
        serde_cbor::to_vec(&owned.events).unwrap()
    );
    assert_eq!(
        host.journal()
            .iter()
            .map(|e| e.to_owned())
            .collect::<Vec<_>>(),
        owned.events
    );
}

#[test]
fn invalid_journal_version_precedes_short_prefix_error_in_restore_and_replay() {
    let mut snapshot = WorldKernel::new().snapshot();
    snapshot.journal_len = 1;
    let mut journal = WorldKernel::new().journal_snapshot();
    journal.version = u32::MAX;
    for result in [
        WorldKernel::from_snapshot(snapshot.clone(), journal.clone()),
        WorldKernel::replay_from_snapshot(snapshot.clone(), journal.clone()),
    ] {
        assert!(
            matches!(result, Err(crate::simulator::PersistError::UnsupportedVersion { kind, version: u32::MAX, .. }) if kind == "journal")
        );
    }
}
