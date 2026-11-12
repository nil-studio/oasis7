//! Explicit queue scheduling against independently restored test-only worlds.
use super::*;

struct ForkDirectory(std::path::PathBuf);
impl Drop for ForkDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct RunningNode(oasis7_node::NodeRuntime);
impl Drop for RunningNode {
    fn drop(&mut self) {
        let _ = self.0.stop();
    }
}

struct ReleaseEmpty(Option<std::sync::mpsc::Sender<()>>);
impl ReleaseEmpty {
    fn release(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}
impl Drop for ReleaseEmpty {
    fn drop(&mut self) {
        self.release();
    }
}

struct InflightEmptyHook {
    driver: Box<NodeRuntimeExecutionDriver>,
    gate: Option<(
        std::sync::mpsc::SyncSender<u64>,
        std::sync::mpsc::Receiver<()>,
    )>,
}
impl oasis7_node::NodeExecutionHook for InflightEmptyHook {
    fn retained_local_commit_time(
        &mut self,
        context: &oasis7_node::NodeExecutionCommitContext,
    ) -> Result<Option<i64>, String> {
        self.driver.retained_local_commit_time(context)
    }
    fn on_commit(
        &mut self,
        context: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        self.on_commit_with_expected(context, None, None)
    }
    fn on_commit_with_expected(
        &mut self,
        context: oasis7_node::NodeExecutionCommitContext,
        expected_execution_block_hash: Option<&str>,
        expected_execution_state_root: Option<&str>,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        if context.committed_actions.is_empty()
            && let Some((entered, release)) = self.gate.take()
        {
            entered
                .send(self.driver.execution_world.state().time)
                .map_err(|_| "empty gate receiver closed")?;
            release
                .recv_timeout(std::time::Duration::from_secs(10))
                .map_err(|_| "empty gate release missing")?;
        }
        self.driver.on_commit_with_expected(
            context,
            expected_execution_block_hash,
            expected_execution_state_root,
        )
    }
}

fn copy_fixture(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        let destination = target.join(entry.file_name());
        assert!(
            !kind.is_symlink(),
            "fixture fork cannot follow external storage"
        );
        if kind.is_dir() {
            copy_fixture(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

pub(super) fn assert_queued_original_across_tick(
    source: &Path,
    world: &WorldIdentity,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) {
    let bytes = correlation::encode_consensus_intent(original).unwrap();
    let original_json = serde_json::to_vec(original).unwrap();
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    for mode in 0..3 {
        let delayed = mode == 1;
        let inflight = mode == 2;
        let dir = temp_dir(if delayed {
            "native-queue-delayed"
        } else {
            "native-queue-immediate"
        });
        assert!(!dir.exists(), "fork directory must be exclusively new");
        let _owned_directory = ForkDirectory(dir.clone());
        std::fs::create_dir_all(&dir).unwrap();
        assert_ne!(source.canonicalize().unwrap(), dir.canonicalize().unwrap());
        copy_fixture(source, &dir);
        let bootstrap =
            super::super::super::super::local_bootstrap::derive_service_execution_bootstrap(
                &dir.join("world"),
                &dir.join("records"),
                &dir.join("store"),
                &world.world_id,
            )
            .unwrap();
        let driver = super::stack::boxed_context_driver_restart(&dir, bootstrap.clone());
        let base_tick = driver.execution_world.state().time;
        assert!(
            !driver.formal_retention_enabled,
            "legacy queue fixture must not begin formal ticks"
        );
        let config = oasis7_node::NodeConfig::new("node-a", &world.world_id, NodeRole::Sequencer)
            .unwrap()
            .with_tick_interval(std::time::Duration::from_millis(20))
            .unwrap();
        let node = oasis7_node::NodeRuntime::new(config);
        if !delayed && !inflight {
            node.submit_consensus_action_payload(907, bytes.clone())
                .unwrap();
        }
        let before_position = driver.execution_world.state().agents["agent-a"].state.pos;
        let boundary =
            super::super::super::super::local_bootstrap::derive_service_execution_bootstrap(
                &dir.join("world"),
                &dir.join("records"),
                &dir.join("store"),
                &world.world_id,
            )
            .unwrap();
        let (entered, observed) = std::sync::mpsc::sync_channel(1);
        let (release, wait) = std::sync::mpsc::channel();
        let hook = InflightEmptyHook {
            driver,
            gate: inflight.then_some((entered, wait)),
        };
        let mut node = node
            .with_local_execution_bootstrap(boundary)
            .with_execution_hook(hook);
        node.start().unwrap();
        let mut node = RunningNode(node);
        // Declared after RunningNode: unblock the hook before stop/join on unwind.
        let mut release = ReleaseEmpty(Some(release));
        let mut before_resources = None;
        let mut before_pending = None;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut batches = Vec::new();
        if inflight {
            let actual_tick = observed
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            let pin = super::super::super::super::world_service_read::pin(
                &dir.join("records"),
                &dir.join("store"),
                world,
                None,
            )
            .unwrap();
            assert_eq!(actual_tick, base_tick);
            assert_eq!(
                pin.world.state().time,
                base_tick,
                "pin is fresh while empty batch is inflight"
            );
            before_resources = Some(serde_json::to_value(&pin.world.state().resources).unwrap());
            before_pending =
                Some(serde_json::to_value(pin.world.snapshot().pending_actions).unwrap());
            node.0
                .submit_consensus_action_payload(907, bytes.clone())
                .unwrap();
            release.release();
        }
        if delayed {
            // The actual consensus worker advances an empty committed batch
            // before admission. This is not a post-enqueue scheduling claim.
            let mut advanced = false;
            while std::time::Instant::now() < deadline {
                if let Ok(pinned) = super::super::super::super::world_service_read::pin(
                    &dir.join("records"),
                    &dir.join("store"),
                    world,
                    None,
                ) && pinned.world.state().time > base_tick
                {
                    before_resources =
                        Some(serde_json::to_value(&pinned.world.state().resources).unwrap());
                    before_pending = Some(
                        serde_json::to_value(pinned.world.snapshot().pending_actions).unwrap(),
                    );
                    advanced = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(
                advanced,
                "real worker must publish an advanced logical tick before admission"
            );
            node.0
                .submit_consensus_action_payload(907, bytes.clone())
                .unwrap();
        }
        while std::time::Instant::now() < deadline {
            batches.extend(node.0.drain_committed_action_batches());
            if batches
                .iter()
                .any(|b| b.actions.iter().any(|a| a.action_id == 907))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        node.0.stop().unwrap();
        drop(node);
        let selected = batches
            .iter()
            .flat_map(|b| &b.actions)
            .filter(|a| a.action_id == 907)
            .collect::<Vec<_>>();
        assert_eq!(
            selected.len(),
            1,
            "real consensus worker must commit the queued original once"
        );
        let boundary =
            super::super::super::super::local_bootstrap::derive_service_execution_bootstrap(
                &dir.join("world"),
                &dir.join("records"),
                &dir.join("store"),
                &world.world_id,
            )
            .unwrap();
        let restored = super::stack::boxed_context_driver_restart(&dir, boundary);
        let canonical: CanonicalIntentResultV1 = serde_json::from_value(
            restored
                .execution_world
                .capability_revocation_state()
                .world_service_results[&key]
                .clone(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_vec(&canonical.request).unwrap(),
            original_json
        );
        if delayed || inflight {
            assert!(
                canonical
                    .rejected
                    .as_deref()
                    .is_some_and(|r| r.contains("stale_base"))
            );
            assert!(canonical.receipt.is_null());
            if delayed {
                assert_eq!(
                    serde_json::to_value(&restored.execution_world.state().resources).unwrap(),
                    before_resources.unwrap()
                );
                assert_eq!(
                    serde_json::to_value(restored.execution_world.snapshot().pending_actions)
                        .unwrap(),
                    before_pending.unwrap()
                );
            }
            assert_eq!(
                restored.execution_world.state().agents["agent-a"].state.pos,
                before_position
            );
        } else {
            assert!(
                canonical.rejected.is_none(),
                "queued fresh original: {:?}",
                canonical.rejected
            );
            assert!(!canonical.receipt.is_null());
            assert_eq!(
                restored.execution_world.state().agents["agent-a"].state.pos,
                oasis7_agent_api::world_action::GeoPos::new(1, 0, 0)
            );
        }
    }
}
