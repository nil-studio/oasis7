//! Actual writer ordering regression: a held older temp file must not replace newer scope.
use super::*;
use crate::viewer::runtime_live::control_plane::llm_sidecar::RuntimeLlmSidecar;
use crate::viewer::{ViewerLiveDecisionMode, ViewerRuntimeLiveServer};
struct Release(PathBuf);
impl Drop for Release {
    fn drop(&mut self) {
        ViewerRuntimeLiveServer::test_release_lineage_publication_gate(&self.0);
    }
}
fn sidecar(path: PathBuf, branch: &str) -> RuntimeLlmSidecar {
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.provider_lineage_store = Some(path);
    sidecar
        .provider_session_ids
        .insert("agent-a".into(), format!("session-{branch}"));
    sidecar.provider_lineage_binding = Some(crate::simulator::RuntimeBindingV1 {
        world_id: "publication-world".into(),
        branch_id: branch.into(),
        finality_epoch: 0,
        finality_block_hash: None,
        finality_status: "pending".into(),
        base_tick: 1,
        base_world_hash: crate::simulator::h_v1("publication-world", &branch),
        reorg_epoch: 0,
        runtime_manifest_hash: crate::simulator::h_v1("publication-manifest", &1),
    });
    sidecar
}

#[test]
fn lineage_confirmed_idle_outcome_poll_does_not_create_an_unpublishable_dispatch_loop() {
    let root = std::env::temp_dir().join(format!("lineage-idle-outcome-{}", std::process::id()));
    let path = root.join("lineage.json");
    let mut owner = sidecar(path.clone(), "idle-outcome");
    owner.runner = Some(crate::viewer::runtime_live::control_plane::llm_sidecar::RuntimeDecisionRunner::ProviderBacked(
        crate::simulator::AsyncAgentRunner::with_default_capacity(),
    ));
    owner.enable_deferred_publications();
    owner.persist_provider_lineage().unwrap();
    for publication in owner.take_queued_publications() {
        publication.execute().unwrap();
    }
    owner
        .confirm_lineage_publication(owner.publication_ticket())
        .unwrap();
    let original = std::fs::read(&path).unwrap();
    let world = crate::runtime::World::new();
    for _ in 0..3 {
        assert!(
            owner
                .poll_hosted_provider_decision(&world)
                .unwrap()
                .is_none()
        );
        assert!(
            owner.ensure_lineage_published().is_ok(),
            "an empty outcome poll cannot fence the next original dispatch after a successful ACK"
        );
        assert!(owner.take_queued_publications().is_empty());
    }
    assert_eq!(std::fs::read(path).unwrap(), original);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lineage_confirmed_unchanged_wake_observation_stays_published_but_real_change_is_fenced() {
    let root = std::env::temp_dir().join(format!("lineage-idle-wake-{}", std::process::id()));
    let path = root.join("lineage.json");
    let mut owner = sidecar(path.clone(), "idle-wake");
    let world = crate::runtime::World::new();
    owner.provider_service_required = true;
    owner.provider_service_projection =
        Some(crate::world_service::projection::from_world(&world, None).unwrap());
    owner.enable_deferred_publications();
    owner.persist_provider_lineage().unwrap();
    for publication in owner.take_queued_publications() {
        publication.execute().unwrap();
    }
    owner
        .confirm_lineage_publication(owner.publication_ticket())
        .unwrap();
    for _ in 0..3 {
        owner.sync_runtime_wakes(&world).unwrap();
        assert!(
            owner.ensure_lineage_published().is_ok(),
            "reobserving the identical authenticated wake mirror cannot renew a dirty ticket after ACK"
        );
        assert!(owner.take_queued_publications().is_empty());
    }
    // A real lineage mutation still needs its exact publication and ACK.
    owner
        .provider_session_ids
        .insert("agent-b".into(), "new-session".into());
    owner.persist_provider_lineage().unwrap();
    assert!(owner.ensure_lineage_published().is_err());
    for publication in owner.take_queued_publications() {
        publication.execute().unwrap();
    }
    assert!(owner.ensure_lineage_published().is_err());
    owner
        .confirm_lineage_publication(owner.publication_ticket())
        .unwrap();
    assert!(owner.ensure_lineage_published().is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lineage_wake_sync_checkpoint_failure_retries_original_before_new_projection() {
    let root = std::env::temp_dir().join(format!("lineage-wake-sync-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("lineage.json");
    std::fs::create_dir(&path).unwrap();
    let mut owner = sidecar(path.clone(), "sync-wake");
    let world = crate::runtime::World::new();
    let binding = owner.provider_lineage_binding.as_ref().unwrap();
    let wake = crate::runtime::SchedulerWakeV1 {
        schema_version: crate::runtime::SchedulerWakeV1::SCHEMA_VERSION.into(),
        wake_id: "original-wake".into(),
        continuation_id: "original-continuation".into(),
        world_id: binding.world_id.clone(),
        branch_id: binding.branch_id.clone(),
        finality_epoch: binding.finality_epoch,
        finality_block_hash: binding
            .finality_block_hash
            .as_ref()
            .map(ToString::to_string),
        finality_status: binding.finality_status.clone(),
        reorg_epoch: binding.reorg_epoch,
        runtime_manifest_hash: binding.runtime_manifest_hash.to_string(),
        agent_id: "agent-a".into(),
        agent_session_id: "original-session".into(),
        agent_turn_id: "original-turn".into(),
        decision_request_id: "original-request".into(),
        request_digest: "11".repeat(32),
        next_wake_tick: 12,
        eligible_since_tick: 12,
        starvation_deadline_tick: 20,
        initial_priority: 0,
        wake_seq: 1,
        retry_seq: 0,
        status: "pending".into(),
        pending_reason: "capacity_available".into(),
    };
    wake.validate().unwrap();
    let mut projection = crate::world_service::projection::from_world(&world, None).unwrap();
    projection.scheduler_wakes = vec![wake.clone()];
    owner.provider_service_projection = Some(projection);
    owner.provider_service_required = true;
    for _ in 0..2 {
        assert!(owner.sync_runtime_wakes(&world).is_err());
        assert!(owner.hosted_wake_checkpoint_pending);
        assert_eq!(owner.pending_runtime_wakes.get(&wake.wake_id), Some(&wake));
        assert!(owner.runner.is_none());
    }
    std::fs::remove_dir(&path).unwrap();
    owner.sync_runtime_wakes(&world).unwrap();
    assert!(!owner.hosted_wake_checkpoint_pending);
    let checkpoint: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        checkpoint["pending_runtime_wakes"][&wake.wake_id],
        serde_json::to_value(&wake).unwrap()
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn lineage_publication_late_generation_cannot_replace_newer_scope() {
    let directory = std::env::temp_dir().join(format!(
        "lineage-publication-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("lineage.json");
    sidecar(path.clone(), "old-scope")
        .persist_provider_lineage()
        .unwrap();
    let old_bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    ViewerRuntimeLiveServer::test_arm_lineage_encoded_gate(&path);
    let release = Release(path.clone());
    let old_path = path.clone();
    let older =
        std::thread::spawn(move || sidecar(old_path, "old-scope").persist_provider_lineage());
    let temporary =
        ViewerRuntimeLiveServer::test_wait_lineage_publication_gate(&path, Duration::from_secs(3))
            .expect("real writer must reach encoded temp-file boundary");
    assert!(
        !temporary.exists(),
        "older encoded bytes must be held before temp write, excluding shared-temp ENOENT false protection"
    );
    let original: serde_json::Value = serde_json::from_slice(&old_bytes).unwrap();
    assert_eq!(original["runtime_binding"]["branch_id"], "old-scope");
    assert!(!path.exists(), "held capsule is not yet published");
    sidecar(path.clone(), "new-scope")
        .persist_provider_lineage()
        .unwrap();
    let newer = std::fs::read(&path).unwrap();
    let published: serde_json::Value = serde_json::from_slice(&newer).unwrap();
    assert_eq!(published["runtime_binding"]["branch_id"], "new-scope");
    drop(release);
    let result = older.join().unwrap();
    let final_bytes = std::fs::read(&path).unwrap();
    let retained_old = std::fs::read(&temporary).ok();
    ViewerRuntimeLiveServer::test_clear_lineage_publication_gate(&path);
    let _ = std::fs::remove_dir_all(directory);
    assert!(
        final_bytes == newer,
        "late old-generation rename must not overwrite the newer scope"
    );
    assert!(
        result.is_err(),
        "stale publisher must report fenced rather than successful"
    );
    assert!(
        retained_old.as_deref() == Some(old_bytes.as_slice()),
        "fenced original remains recoverable"
    );
}

#[test]
fn lineage_owned_publication_failure_retries_original_before_dispatch_confirmation() {
    use crate::viewer::runtime_live::control_plane::llm_sidecar::lineage_persistence::checkpoint_write::DeferredPublications;
    let root = std::env::temp_dir().join(format!(
        "lineage-retry-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let blocked_parent = root.join("blocked");
    std::fs::write(&blocked_parent, b"not a directory").unwrap();
    let path = blocked_parent.join("lineage.json");
    let sidecar = sidecar(path.clone(), "original-scope");
    let captured = DeferredPublications::begin(&sidecar).unwrap();
    sidecar.persist_provider_lineage().unwrap();
    assert!(
        sidecar.ensure_lineage_published().is_err(),
        "captured is not durable confirmation"
    );
    let batch = captured.finish();
    let ticket = sidecar.publication_ticket();
    let batch = batch.lock().unwrap();
    assert_eq!(batch.len(), 1);
    assert!(batch[0].execute().is_err());
    assert!(sidecar.ensure_lineage_published().is_err());
    std::fs::remove_file(&blocked_parent).unwrap();
    std::fs::create_dir(&blocked_parent).unwrap();
    batch[0].execute().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(original["runtime_binding"]["branch_id"], "original-scope");
    assert_eq!(
        original["provider_session_ids"]["agent-a"],
        "session-original-scope"
    );
    assert!(
        sidecar.ensure_lineage_published().is_err(),
        "publication receipt still needs matching owner ticket commit"
    );
    sidecar.confirm_lineage_publication(ticket).unwrap();
    assert!(sidecar.ensure_lineage_published().is_ok());
    batch[0].execute().unwrap();
    assert!(
        std::fs::read(&path).unwrap() == bytes,
        "confirmed original is not replaced on retry"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lineage_old_ack_cannot_clear_newer_unpublished_batch() {
    use crate::viewer::runtime_live::control_plane::llm_sidecar::lineage_persistence::checkpoint_write::DeferredPublications;
    let root = std::env::temp_dir().join(format!(
        "lineage-ack-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = root.join("lineage.json");
    let mut owner = sidecar(path.clone(), "old-scope");
    let first = DeferredPublications::begin(&owner).unwrap();
    owner.persist_provider_lineage().unwrap();
    let first = first.finish();
    let old_ticket = owner.publication_ticket();
    first.lock().unwrap()[0].execute().unwrap();
    owner.confirm_lineage_publication(old_ticket).unwrap();
    owner
        .provider_session_ids
        .insert("agent-a".into(), "new-session".into());
    let second = DeferredPublications::begin(&owner).unwrap();
    owner.persist_provider_lineage().unwrap();
    let second = second.finish();
    let new_ticket = owner.publication_ticket();
    assert!(new_ticket > old_ticket);
    assert!(owner.confirm_lineage_publication(old_ticket).is_err());
    assert!(
        owner.ensure_lineage_published().is_err(),
        "late old ACK cannot open dispatch of unconfirmed newer original"
    );
    second.lock().unwrap()[0].execute().unwrap();
    assert!(owner.confirm_lineage_publication(old_ticket).is_err());
    owner.confirm_lineage_publication(new_ticket).unwrap();
    assert!(owner.ensure_lineage_published().is_ok());
    let bytes = std::fs::read(&path).unwrap();
    let current: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(current["provider_session_ids"]["agent-a"], "new-session");
    first.lock().unwrap()[0].execute().unwrap();
    assert!(std::fs::read(&path).unwrap() == bytes);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lineage_chain_event_during_real_publication_keeps_new_dirty_ticket() {
    let root = std::env::temp_dir().join(format!(
        "lineage-chain-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = root.join("lineage.json");
    let mut owner = sidecar(path.clone(), "same-scope");
    owner.enable_deferred_publications();
    owner.persist_provider_lineage().unwrap();
    let old_ticket = owner.publication_ticket();
    let mut batch = owner.take_queued_publications();
    ViewerRuntimeLiveServer::test_arm_lineage_encoded_gate(&path);
    let release = Release(path.clone());
    let publication = batch.remove(0);
    let worker = std::thread::spawn(move || publication.execute());
    ViewerRuntimeLiveServer::test_wait_lineage_publication_gate(&path, Duration::from_secs(3))
        .unwrap();
    let runtime_event = crate::runtime::WorldEvent {
        id: 901,
        time: 83,
        caused_by: None,
        body: crate::runtime::WorldEventBody::Domain(
            crate::runtime::DomainEvent::RecipeCompleted {
                committed_recipe_origin: None,
                job_id: 7,
                requester_agent_id: "agent-a".into(),
                factory_id: "factory-smelter".into(),
                recipe_id: "recipe.iron-ingot".into(),
                accepted_batches: 1,
                produce: Vec::new(),
                byproducts: Vec::new(),
                output_ledger: crate::runtime::MaterialLedgerId::world(),
                bottleneck_tags: Vec::new(),
                logistics_route_ids: Vec::new(),
                logistics_path_ids: Vec::new(),
            },
        ),
    };
    let mapped = crate::simulator::WorldEvent {
        id: 901,
        time: 83,
        kind: crate::simulator::WorldEventKind::RuntimeEvent {
            kind: "runtime.economy.recipe_completed".into(),
            domain_kind: None,
        },
        runtime_event: Some(runtime_event.clone()),
    };
    owner.notify_recipe_completion_with_binding(
        &runtime_event,
        mapped,
        owner.provider_lineage_binding.clone(),
    );
    let new_ticket = owner.publication_ticket();
    assert!(new_ticket > old_ticket);
    assert!(owner.confirm_lineage_publication(old_ticket).is_err());
    assert!(owner.ensure_lineage_published().is_err());
    assert_eq!(
        owner.pending_provider_world_events.len(),
        1,
        "unpublished event cannot be delivered or discarded"
    );
    drop(release);
    worker.join().unwrap().unwrap();
    assert!(owner.confirm_lineage_publication(old_ticket).is_err());
    let newer = owner.take_queued_publications();
    assert_eq!(newer.len(), 1);
    newer[0].execute().unwrap();
    let checkpoint: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        checkpoint["pending_provider_world_events"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    owner.confirm_lineage_publication(new_ticket).unwrap();
    assert!(owner.ensure_lineage_published().is_ok());
    ViewerRuntimeLiveServer::test_clear_lineage_publication_gate(&path);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lineage_publication_overflow_retains_first_original_and_never_false_acks() {
    let root = std::env::temp_dir().join(format!(
        "lineage-overflow-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let owner = sidecar(root.join("lineage.json"), "same-scope");
    owner.enable_deferred_publications();
    for _ in 0..32 {
        owner.persist_provider_lineage().unwrap();
    }
    let ticket = owner.publication_ticket();
    assert!(owner.persist_provider_lineage().is_err());
    let failed_id = owner
        .publication
        .failed_original
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .unique_id;
    for _ in 0..100 {
        assert!(owner.persist_provider_lineage().is_err());
    }
    assert_eq!(owner.take_queued_publications().len(), 32);
    assert_eq!(
        owner
            .publication
            .outstanding
            .load(std::sync::atomic::Ordering::Acquire),
        32
    );
    assert_eq!(
        owner
            .publication
            .failed_original
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .unique_id,
        failed_id
    );
    assert!(owner.confirm_lineage_publication(ticket).is_err());
    assert!(owner.ensure_lineage_published().is_err());
}

#[test]
#[cfg(unix)]
fn lineage_deferred_rename_directory_failure_retries_exact_original_after_repair() {
    let root =
        std::env::temp_dir().join(format!("lineage-rename-directory-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("lineage.json");
    let owner = sidecar(path.clone(), "rename-original");
    owner.enable_deferred_publications();
    owner.persist_provider_lineage().unwrap();
    let ticket = owner.publication_ticket();
    let batch = owner.take_queued_publications();
    assert_eq!(batch.len(), 1);
    std::fs::create_dir(&path).unwrap();
    assert!(batch[0].execute().is_err());
    assert!(
        !batch[0].permanently_blocked(),
        "rename to directory is proven unpublished, not an ambiguous publication"
    );
    let temporary = batch[0].temporary.get().unwrap().clone();
    let original = std::fs::read(&temporary).unwrap();
    assert!(owner.ensure_lineage_published().is_err());
    assert_eq!(owner.publication_ticket(), ticket);
    std::fs::remove_dir(&path).unwrap();
    batch[0].execute().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(owner.ensure_lineage_published().is_err());
    owner.confirm_lineage_publication(ticket).unwrap();
    assert!(owner.ensure_lineage_published().is_ok());
    batch[0].execute().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_dir_all(root).unwrap();
}
