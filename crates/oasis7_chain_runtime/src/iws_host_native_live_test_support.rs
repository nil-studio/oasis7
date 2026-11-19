//! One production owner and one real Node survive pending and completed
//! stop/start cuts. Test barriers only control scheduling; all execution,
//! original-work ownership and accepted observations use the real consumers.
use super::*;
use crate::iws_host_runtime::node_live::{
    IwsLiveNodeCancellationV1, IwsLiveNodeCognitionV1, IwsLiveNodeColdSettingsV1,
    IwsLiveNodeHookV1, IwsLiveNodeOwnersV1, IwsLiveNodeSettingsV1,
};
use crate::iws_host_runtime::node_checkpoint::{
    IwsNodeCheckpointLimitsV1, load_node_checkpoint_bounded_v1,
};
use oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalReadAnchorV1;
use oasis7_node::{
    NodeConfig, NodeExecutionCommitContext, NodeExecutionCommitOutcome, NodeExecutionCommitResult,
    NodeExecutionGenesisBootstrap, NodeExecutionHook, NodeLocalExecutionBootstrapV1,
    NodeLocalExecutionContinuation, NodeRole, VerifiedNodeLocalContinuationV1,
    VerifiedNodeRestoredContinuationV1,
};
use std::sync::{Mutex, atomic::AtomicU64, mpsc};
use std::time::Duration;
use sha2::{Digest, Sha256};

#[path = "iws_host_native_clean_process_test_support.rs"]
mod clean_process;

struct ProjectionDigest(Sha256);
impl std::io::Write for ProjectionDigest {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

enum Notice {
    Pending,
    CompletionCancelled,
    Completed(NodeExecutionCommitContext),
}
#[derive(Default)]
struct Counts {
    fresh: usize,
    fresh_callbacks: usize,
    originals: Vec<NodeExecutionCommitContext>,
    restored: usize,
    completed: usize,
    cancelled_completions: usize,
}
struct Hook {
    inner: IwsLiveNodeHookV1,
    cancellation: IwsLiveNodeCancellationV1,
    counts: Arc<Mutex<Counts>>,
    allowed_height: Arc<AtomicU64>,
    hold_pending: Arc<AtomicBool>,
    notified_pending: bool,
    notices: mpsc::Sender<Notice>,
    resume: mpsc::Receiver<()>,
}
impl NodeExecutionHook for Hook {
    fn guarded_local_execution(&self) -> bool {
        self.inner.guarded_local_execution()
    }
    fn current_guarded_local_execution_bootstrap(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionBootstrapV1>, String> {
        self.inner.current_guarded_local_execution_bootstrap()
    }
    fn qualified_local_execution_genesis_bootstrap(
        &mut self,
    ) -> Result<Option<NodeExecutionGenesisBootstrap>, String> {
        self.inner.qualified_local_execution_genesis_bootstrap()
    }
    fn prepare_verified_local_continuation(
        &mut self,
        source: &VerifiedNodeLocalContinuationV1<'_>,
    ) -> Result<(), String> {
        // Refuse only fixture scheduling beyond this explicitly requested
        // batch, before calling the production hook or mutating any Host.
        if source.original().context.height > self.allowed_height.load(Ordering::Acquire) {
            return Err("fixture awaits the next submitted batch".into());
        }
        self.inner.prepare_verified_local_continuation(source)?;
        let mut counts = self.counts.lock().map_err(diagnostic)?;
        counts.fresh_callbacks += 1;
        let context = &source.original().context;
        if let Some(previous) = counts
            .originals
            .iter()
            .find(|original| original.height == context.height)
        {
            if previous != context {
                return Err("repeated live decision changed its exact original".into());
            }
        } else {
            counts.originals.push(context.clone());
            counts.fresh += 1;
        }
        Ok(())
    }
    fn restored_verified_local_continuation(
        &mut self,
        source: &VerifiedNodeRestoredContinuationV1<'_>,
    ) -> Result<(), String> {
        self.inner.restored_verified_local_continuation(source)?;
        self.counts.lock().map_err(diagnostic)?.restored += 1;
        Ok(())
    }
    fn pending_local_continuation(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        self.inner.pending_local_continuation()
    }
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        self.inner.on_commit(context)
    }
    fn on_commit_outcome_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        block: Option<&str>,
        root: Option<&str>,
    ) -> Result<NodeExecutionCommitOutcome, String> {
        if self.hold_pending.load(Ordering::Acquire) {
            if !self.notified_pending {
                self.notices.send(Notice::Pending).map_err(diagnostic)?;
                self.notified_pending = true;
            }
            return Ok(NodeExecutionCommitOutcome::Deferred);
        }
        self.inner
            .on_commit_outcome_with_expected(context, block, root)
    }
    fn complete_local_continuation(
        &mut self,
        context: &NodeExecutionCommitContext,
    ) -> Result<(), String> {
        let cancel_once = self
            .counts
            .lock()
            .map_err(diagnostic)?
            .cancelled_completions
            == 0;
        if cancel_once {
            self.cancellation.set_cancelled(true);
            let refused = self.inner.complete_local_continuation(context);
            self.cancellation.set_cancelled(false);
            if refused.is_ok() {
                return Err("cancelled production completion was accepted".into());
            }
            self.counts
                .lock()
                .map_err(diagnostic)?
                .cancelled_completions += 1;
            self.notices
                .send(Notice::CompletionCancelled)
                .map_err(diagnostic)?;
            self.resume
                .recv_timeout(Duration::from_secs(60))
                .map_err(diagnostic)?;
            return refused;
        }
        self.inner.complete_local_continuation(context)?;
        self.counts.lock().map_err(diagnostic)?.completed += 1;
        self.notices
            .send(Notice::Completed(context.clone()))
            .map_err(diagnostic)?;
        self.resume
            .recv_timeout(Duration::from_secs(60))
            .map_err(diagnostic)
    }
}

fn submit(node: &IwsLiveNodeServiceV1, start: u64, amounts: &[i64]) -> Result<(), String> {
    use oasis7::consensus_action_payload::{
        ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
    };
    for (index, amount) in amounts.iter().copied().enumerate() {
        let payload =
            encode_consensus_action_payload(&ConsensusActionPayloadEnvelope::from_runtime_action(
                oasis7_runtime::Action::TransferResource {
                    from_agent_id: "sender".into(),
                    to_agent_id: "receiver".into(),
                    kind: ResourceKind::Electricity,
                    amount,
                },
            ))
            .map_err(diagnostic)?;
        node.submit_consensus_action_payload(start + index as u64, payload)
            .map_err(diagnostic)?;
    }
    Ok(())
}

fn cognition_remaining(io: &HostColdIoWorkV1, work: &HostCognitionStartupWorkV1) -> [u64; 8] {
    [
        io.state.reads_remaining,
        io.values.reads_remaining,
        io.originals.reads_remaining,
        io.original_values.nodes_remaining,
        work.core.objects_remaining,
        work.facts.facts_remaining,
        work.json.cells_remaining,
        work.keys.bytes_remaining,
    ]
}

pub(crate) fn run(root: &Path, cold_recovery: bool) -> Result<Vec<u8>, String> {
    with_genuine_genesis_hosts(root, |fixture| {
        let GenuineGenesisHostsV1 {
            primary,
            replica,
            memory,
            archive,
            runtime_work,
            genesis_io,
            transfer,
            producer,
            primary_provider,
            replica_provider,
            authority,
            certificate,
            trust,
            writer,
            pinned,
            release_policy,
            configuration,
            current_limits,
            projection_limits,
            paths,
            seed_receipt,
            genesis_record,
            genesis_body,
            transaction,
            stop,
        } = fixture;
        let mut observe = HostObservationWorkV1::new_bounded(
            &mut archive.bounded_context(&memory, &mut || false),
        )
        .map_err(diagnostic)?;
        let genesis = observe_current_bounded(
            &primary,
            "sender",
            "receiver",
            transaction,
            &mut observe,
            &memory,
            &archive,
            &stop,
        )
        .map_err(diagnostic)?;
        if genesis.record != genesis_record || genesis.body != genesis_body || genesis.height != 0 {
            return Err("live fixture genuine genesis mismatch".into());
        }
        let initial_anchor = VerifiedFormalReadAnchorV1::from_initial(
            &certificate,
            &trust,
            genesis.qualified_proof(),
        )
        .map_err(diagnostic)?;
        let mut before = balances(&genesis)?;
        let mut hints = BTreeMap::new();
        for event in genesis.events.iter() {
            hints.insert(
                event.original,
                FactLocationV1 {
                    height: genesis.height,
                    ordinal: event.ordinal as u64,
                    original: event.original,
                },
            );
        }
        drop(genesis);
        let mut expected_hints = hints.clone();
        let [mut runtime, replica_runtime] = runtime_work;
        // Separate finite source accounts are supplied once, before the first
        // admission. Every retry/recovery consumes these exact remaining owners.
        let mut cognition_io = HostColdIoWorkV1 {
            process_canonical: endpoint_work::canonical_work(),
            state: endpoint_work::physical(),
            values: endpoint_work::physical(),
            members: endpoint_work::physical(),
            originals: endpoint_work::physical(),
            original_values: endpoint_work::logical(),
        };
        let mut cognition_work = cognition_startup_work();
        let startup = primary
            .prepare_cognition_startup_bounded(
                &mut cognition_io,
                &mut cognition_work,
                &mut runtime,
                transaction,
                &memory,
                &archive,
                &stop,
            )
            .map_err(diagnostic)?;
        let cognition_before = cognition_remaining(&cognition_io, &cognition_work);
        let cold = admit_initial_cold_recovery_work_v1(&current_limits, &memory, &archive)?;
        let mut work = normal_work(runtime, genesis_io, cold);
        if cold_recovery {
            work.fault = Some(super::super::normal_commit::NormalCommitFaultV1::AfterPublication);
        }
        let cold_before = work.cold.remaining_for_fixture_v1();
        let source_before = [
            work.primary_source
                .as_ref()
                .ok_or("primary work")?
                .history_hash_bytes,
            work.replica_source
                .as_ref()
                .ok_or("replica work")?
                .history_hash_bytes,
        ];
        let common_before = [archive.remaining_steps(), archive.remaining_read_bytes()];
        let pool = memory.clone();
        let settings = IwsLiveNodeSettingsV1 {
            world_id: "iws-genuine-host-transfer".into(),
            writer,
            primary_provider,
            replica_provider,
            primary_policy: pinned.clone(),
            replica_policy: pinned,
            compiler: RuntimeSourceCompiler::default(),
            input_limits: CborDataLimitsV1 {
                max_wire_bytes: 1 << 20,
                max_graph_bytes: 256 << 20,
                max_depth: 128,
            },
            action_limits: CborDataLimitsV1 {
                max_wire_bytes: 1 << 20,
                max_graph_bytes: 256 << 20,
                max_depth: 128,
            },
            policy: NormalCommitHeaderLimitsV1 {
                transaction,
                checkpoint: true,
            },
            max_action_bytes: 1 << 20,
            max_grant_payload_bytes: 1 << 20,
            cold: IwsLiveNodeColdSettingsV1 {
                endpoint_paths: [paths.primary_endpoint, paths.replica_endpoint],
                pages_paths: [paths.primary_pages, paths.replica_pages],
                authorization_hint_path: paths.authorization_hint,
                authority,
                process: release_policy,
                configuration,
                worker: worker_limits(),
                current_limits,
                budgets: endpoint_work::budgets(),
                projection_limits,
                mailbox_hints: hints,
                checkpoints: [None, None],
            },
        };
        let counts = Arc::new(Mutex::new(Counts::default()));
        let allowed = Arc::new(AtomicU64::new(2));
        let pending = Arc::new(AtomicBool::new(true));
        let (notices, receive) = mpsc::channel();
        let (resume_sender, acknowledgments) = mpsc::channel();
        let config = NodeConfig::new(
            "iws-continuous-node",
            "iws-genuine-host-transfer",
            NodeRole::Sequencer,
        )
        .map_err(diagnostic)?
        .with_player_id("sender")
        .map_err(diagnostic)?
        .with_tick_interval(Duration::from_millis(20))
        .map_err(diagnostic)?;
        let mut node = IwsLiveNodeServiceV1::new_with_fixture_hook_bounded(
            config,
            IwsLiveNodeOwnersV1 {
                primary,
                replica,
                memory,
                archive,
                work,
            },
            settings,
            IwsLiveNodeCognitionV1 {
                startup,
                io: cognition_io,
                work: cognition_work,
            },
            |inner, cancellation| Hook {
                inner,
                cancellation,
                counts: counts.clone(),
                allowed_height: allowed.clone(),
                hold_pending: pending.clone(),
                notified_pending: false,
                notices,
                resume: acknowledgments,
            },
        )
        .map_err(|failure| failure.cause)?;
        // Declare the sole sender after Node: any early error disconnects the
        // blocked test callback before Node's Drop joins its worker.
        let resume = resume_sender;
        submit(&node, 1, &[3, 2])?;
        node.start().map_err(diagnostic)?;
        let pending_notice = receive.recv_timeout(Duration::from_secs(120));
        node.stop().map_err(diagnostic)?;
        if !matches!(pending_notice, Ok(Notice::Pending))
            || node.snapshot().consensus.last_execution_height != 0
        {
            return Err("live Node pending stop changed accepted boundary".into());
        }
        pending.store(false, Ordering::Release);
        node.start().map_err(diagnostic)?;
        let cancelled_notice = receive
            .recv_timeout(Duration::from_secs(120))
            .map_err(diagnostic)?;
        if !matches!(cancelled_notice, Notice::CompletionCancelled) {
            return Err("missing actual cancelled completion".into());
        }
        // Completion is retried at the beginning of the next engine tick.
        // Queue its successor now, before that tick drains the external queue,
        // so a successful completion retry cannot propose an empty successor.
        submit(&node, 3, &[1])?;
        resume.send(()).map_err(diagnostic)?;
        let mut receipts = Vec::new();
        let mut final_expected = None;
        for (height, amounts) in [(1, &[3, 2][..]), (2, &[1][..]), (3, &[1][..])] {
            let notice = receive
                .recv_timeout(Duration::from_secs(120))
                .map_err(diagnostic)?;
            let Notice::Completed(context) = notice else {
                return Err("unexpected repeated pending notice".into());
            };
            if context.height != height || context.committed_actions.len() != amounts.len() {
                return Err("production Node committed wrong input batch".into());
            }
            node.with_current_bounded(|primary, replica, memory, archive, cancelled| {
                let first = observe_current_bounded(
                    primary,
                    "sender",
                    "receiver",
                    transaction,
                    &mut observe,
                    memory,
                    archive,
                    cancelled,
                )
                .map_err(diagnostic)?;
                let second = observe_current_bounded(
                    replica,
                    "sender",
                    "receiver",
                    transaction,
                    &mut observe,
                    memory,
                    archive,
                    cancelled,
                )
                .map_err(diagnostic)?;
                if summary(&first)? != summary(&second)? {
                    return Err("continuous dual acceptance differs".into());
                }
                let current = primary.current.current().ok_or("live accepted primary")?;
                let built = current
                    .resident
                    .as_ref()
                    .ok_or("live primary resident")?
                    .requires_cognition_startup_bounded_v1(
                        current.source.record_id(),
                        &current.source.fence(),
                        &mut archive
                            .bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
                    )
                    .map_err(diagnostic)?;
                if built != (cold_recovery && height == 1) {
                    return Err("actual Cold/Warm resident succession mismatch".into());
                }
                let mut tick = verify_tick(&first, before, amounts, height, height + 3)?;
                tick["actual_cold_resident"] = built.into();
                receipts.push(tick);
                for event in first.events.iter() {
                    expected_hints.insert(
                        event.original,
                        FactLocationV1 {
                            height: first.height,
                            ordinal: event.ordinal as u64,
                            original: event.original,
                        },
                    );
                }
                before = balances(&first)?;
                if height == 3 {
                    let mut expected = summary(&first)?;
                    expected["qualified_head_proof"] = serde_json::to_value(first.qualified_proof())
                        .map_err(diagnostic)?;
                    final_expected = Some(expected);
                }
                Ok(())
            })?;
            resume.send(()).map_err(diagnostic)?;
            if height >= 2 {
                node.stop().map_err(diagnostic)?;
                let snapshot = node.snapshot();
                if snapshot.consensus.last_execution_height != height
                    || snapshot.consensus.committed_height != height
                {
                    return Err(format!(
                        "continuous Node engine boundary mismatch: {snapshot:?}"
                    ));
                }
                if height == 2 {
                    allowed.store(3, Ordering::Release);
                    submit(&node, 4, &[1])?;
                    node.start().map_err(diagnostic)?;
                }
            }
        }
        // Real clean-stop publication and a new disk read of its DATA. This
        // roundtrip deliberately does not claim a new-process Cold admission.
        let checkpoint_root = root.join("live-clean-checkpoints");
        let checkpoint_limits = IwsNodeCheckpointLimitsV1 {
            max_projection_bytes: projection_limits.max_wire_bytes as u64,
            max_mailbox_entries: expected_hints.len() as u64,
            max_path_bytes: checkpoint_root.as_os_str().as_encoded_bytes().len()
                .checked_add(256).ok_or("live checkpoint path bound")?,
        };
        let checkpoint_record = node.stop_and_checkpoint_bounded(
            &checkpoint_root, checkpoint_limits,
        )?;
        let hints = node.selected_fact_hints_bounded()?;
        node.with_current_bounded(|primary, replica, memory, archive, cancelled| {
            let pool_before = memory.used();
            let loaded = load_node_checkpoint_bounded_v1(
                &checkpoint_root, checkpoint_record, checkpoint_limits,
                &mut archive.bounded_context(memory, &mut || cancelled.load(Ordering::Acquire)),
            ).map_err(diagnostic)?;
            let data = loaded.data();
            if loaded.mailbox_hints() != &expected_hints
                || data.record != checkpoint_record
                || data.height != 3
                || data.authorization_hint_paths[0] == data.authorization_hint_paths[1]
            {
                return Err("live checkpoint DATA lost actual head, paths or historical locators".into());
            }
            for (index, (host, selected)) in [primary, replica].into_iter().zip(hints.iter()).enumerate() {
                let current = host.current.current().ok_or("accepted service locator source")?;
                current.source.with_parent_read_view_bounded(
                    memory, archive, &mut || cancelled.load(Ordering::Acquire),
                    |parent| -> Result<(), IwsHostRuntimeErrorV1> {
                        if data.record != parent.head().verified_record_id()
                            || data.fence != parent.fence()
                            || data.height != parent.core().body().height
                            || data.facts[index].runtime != selected.runtime
                            || data.facts[index].cognition != selected.cognition
                        {
                            return Err(FormalError::Invalid("clean checkpoint differs from actual accepted source".into()).into());
                        }
                        let mut stop = || cancelled.load(Ordering::Acquire);
                        let mut common = archive.bounded_context(memory, &mut stop);
                        let projection = super::super::cold_recovery::read_projection_hint_bounded_v1(
                            &data.authorization_hint_paths[index], projection_limits, &mut common,
                        )?;
                        let mut disk = ProjectionDigest(Sha256::new());
                        let disk_bytes = projection.write_hint_data_bounded_v1(&mut disk, &mut common)
                            .map_err(IwsHostRuntimeErrorV1::ColdProjection)?;
                        let mut actual = ProjectionDigest(Sha256::new());
                        let actual_bytes = current.resident.as_ref()
                            .ok_or_else(|| FormalError::Invalid("live checkpoint accepted resident".into()))?
                            .write_accepted_authorization_hint_bounded_v1(
                                parent.head(), &parent.fence(), &mut actual, &mut common,
                            ).map_err(IwsHostRuntimeErrorV1::NormalSource)?;
                        if disk_bytes == 0 || disk_bytes != actual_bytes
                            || disk.0.finalize() != actual.0.finalize()
                        {
                            return Err(FormalError::Invalid("clean checkpoint projection differs from actual resident".into()).into());
                        }
                        for location in [selected.runtime, selected.cognition] {
                            archive.charge(1, 96, &mut || cancelled.load(Ordering::Acquire))?;
                            let ordinal = usize::try_from(location.ordinal).map_err(|_| FormalError::Invalid("service locator ordinal".into()))?;
                            let fact = parent.core().facts().facts.get(ordinal)
                                .ok_or_else(|| FormalError::Invalid("service locator missing fact".into()))?;
                            if location.height != parent.core().body().height
                                || location.original != fact.id
                                || fact.kind != oasis7_formal_protocol::incremental_execution::ExecutionFactKindV1::Result
                            { return Err(FormalError::Invalid("service locator differs from actual accepted Result".into()).into()); }
                        }
                        Ok(())
                    },
                ).map_err(diagnostic)?;
            }
            drop(loaded);
            if memory.used() != pool_before {
                return Err("live checkpoint DATA or projection retained an unreturned fee".into());
            }
            Ok(())
        })?;
        node.abort().map_err(diagnostic)?;
        let restart = node.start().expect_err("aborted service cannot restart");
        let submission =
            submit(&node, 5, &[1]).expect_err("aborted service cannot accept another input");
        if !node.is_aborted()
            || !restart.to_string().contains("explicitly aborted")
            || !submission.contains("explicitly aborted")
            || node.snapshot().consensus.last_execution_height != 3
        {
            return Err("service abort changed its permanent-stop contract".into());
        }
        let parts = node
            .into_parts_fixture()
            .map_err(|_| "stopped live Node retained active work")?;
        let owners = parts.owners();
        if parts.settings().cold.mailbox_hints != expected_hints {
            return Err(
                "live mailbox hints lost history or changed original FactSet ordinals".into(),
            );
        }
        let source_after = [
            owners
                .work
                .primary_source
                .as_ref()
                .ok_or("returned primary work")?
                .history_hash_bytes,
            owners
                .work
                .replica_source
                .as_ref()
                .ok_or("returned replica work")?
                .history_hash_bytes,
        ];
        if source_after
            .iter()
            .zip(source_before)
            .any(|(after, before)| *after == 0 || *after >= before)
        {
            return Err("continuous original source work was reset or lost".into());
        }
        let common_after = [
            owners.archive.remaining_steps(),
            owners.archive.remaining_read_bytes(),
        ];
        let cold_after = owners.work.cold.remaining_for_fixture_v1();
        let (cognition_io, cognition_work) = parts.cognition_work();
        let cognition_after = cognition_remaining(cognition_io, cognition_work);
        if cognition_after
            .iter()
            .zip(cognition_before)
            .any(|(after, before)| *after > before)
            || (cold_recovery && cognition_after == cognition_before)
            || (!cold_recovery && cognition_after != cognition_before)
        {
            return Err("live Cognition admission was repeated, refilled or skipped".into());
        }
        if cold_after
            .iter()
            .zip(cold_before)
            .any(|(after, before)| *after > before)
            || (cold_recovery && cold_after == cold_before)
            || (!cold_recovery && cold_after != cold_before)
        {
            return Err("live Cold work was reset or incorrectly consumed".into());
        }
        if common_after
            .iter()
            .zip(common_before)
            .any(|(after, before)| *after >= before)
        {
            return Err("continuous Common did not consume its original account".into());
        }
        let final_expected = final_expected.ok_or("clean process accepted observation missing")?;
        let prepared_clean = owners
            .primary
            .current
            .current()
            .ok_or("clean process actual parent missing")?
            .source
            .with_parent_read_view_bounded(
                &owners.memory,
                &owners.archive,
                &mut || false,
                |parent| -> Result<_, FormalError> {
                    clean_process::prepare(clean_process::CleanProcessInputV1 {
                        root,
                        settings: parts.settings(),
                        checkpoint_root: &checkpoint_root,
                        checkpoint_limits,
                        initial_anchor: &initial_anchor,
                        trust: &trust,
                        actual_authority: parent.authority(),
                        actual_head: parent.head(),
                        expected: final_expected,
                    })
                    .map_err(FormalError::Invalid)
                },
            )
            .map_err(diagnostic)?;
        let counts = counts.lock().map_err(diagnostic)?;
        if counts.fresh != 3
            || counts.restored != 1
            || counts.completed != 3
            || counts.cancelled_completions != 1
        {
            return Err("continuous Node lifecycle count mismatch".into());
        }
        let mut receipt = serde_json::json!({"schema":"oasis7.iws.live_node.fixture.v1",
            "node_instances":1,"node_starts":3,"fresh":counts.fresh,"restored":counts.restored,
            "aborted_service_rejects_restart_and_submission":true,
            "clean_checkpoint_roundtrip":{"record":hex::encode(checkpoint_record),"height":3,
                "mailbox_entries":expected_hints.len(),"both_actual_projections_match":true},
            "fresh_callbacks":counts.fresh_callbacks,
            "completed":counts.completed,"cancelled_completions":counts.cancelled_completions,
            "ticks":receipts,"source_work_before":source_before,"source_work_after":source_after,
            "common_before":common_before,"common_after":common_after,"final_sender_electricity":before[0],
            "cold_recovery":cold_recovery,"cold_work_before":cold_before,"cold_work_after":cold_after,
            "cognition_work_before":cognition_before,"cognition_work_after":cognition_after});
        drop(parts);
        drop(observe);
        drop(replica_runtime);
        drop(transfer);
        drop(producer);
        drop(certificate);
        drop(trust);
        drop(initial_anchor);
        drop(seed_receipt);
        let retained = pool.used();
        if retained != 0 {
            return Err(format!(
                "continuous owner release retained {retained} bytes"
            ));
        }
        receipt["pool_used_after_drop"] = retained.into();
        // The actual provider servers still live in the enclosing GEN fixture
        // callback. The new process receives only private files and raw DATA.
        receipt["clean_process"] = prepared_clean.run()?;
        serde_json::to_vec(&receipt).map_err(diagnostic)
    })
}
