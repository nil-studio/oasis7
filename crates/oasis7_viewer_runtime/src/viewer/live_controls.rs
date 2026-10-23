use crate::simulator::unsupported_runtime_perf_snapshot;
use crate::simulator::RuntimePerfSnapshot;

impl LiveScript {
    fn new(kernel: &WorldKernel) -> Self {
        let mut agent_ids: Vec<_> = kernel.model().agents.keys().cloned().collect();
        agent_ids.sort();
        let agent_id = agent_ids.first().cloned();

        let mut locations: Vec<_> = kernel.model().locations.keys().cloned().collect();
        locations.sort();

        let target_index = if locations.len() > 1 { 1 } else { 0 };

        Self {
            agent_id,
            locations,
            target_index,
        }
    }

    fn next_action(&mut self, kernel: &WorldKernel) -> Option<Action> {
        let agent_id = self.agent_id.clone()?;
        let model = kernel.model();
        let agent = model.agents.get(&agent_id)?;
        if self.locations.is_empty() {
            return None;
        }

        let current_location_id = agent.location_id.clone();
        let current_location = model.locations.get(&current_location_id)?;

        if self.locations.len() == 1 {
            return Some(single_location_transfer(
                &agent_id,
                &current_location_id,
                agent.resources.get(ResourceKind::Electricity),
                current_location.resources.get(ResourceKind::Electricity),
            ));
        }

        if !self.locations.iter().any(|id| id == &current_location_id) {
            self.locations.push(current_location_id.clone());
            self.locations.sort();
        }

        if self.target_index >= self.locations.len() {
            self.target_index = 0;
        }

        if self.locations[self.target_index] == current_location_id {
            self.target_index = (self.target_index + 1) % self.locations.len();
        }

        let target_id = self.locations[self.target_index].clone();
        let target_location = model.locations.get(&target_id)?;
        let distance_cm = space_distance_cm(agent.pos, target_location.pos);
        let move_cost = kernel.config().movement_cost(distance_cm);
        let agent_power = agent.resources.get(ResourceKind::Electricity);

        if move_cost > 0 && agent_power < move_cost {
            let needed = move_cost - agent_power;
            let available = current_location.resources.get(ResourceKind::Electricity);
            let transfer_amount = if available > 0 {
                needed.min(available).max(1)
            } else {
                1
            };
            return Some(Action::TransferResource {
                from: ResourceOwner::Location {
                    location_id: current_location_id,
                },
                to: ResourceOwner::Agent { agent_id },
                kind: ResourceKind::Electricity,
                amount: transfer_amount,
            });
        }

        Some(Action::MoveAgent {
            agent_id,
            to: target_id,
        })
    }
}

fn single_location_transfer(
    agent_id: &str,
    location_id: &str,
    agent_power: i64,
    location_power: i64,
) -> Action {
    if location_power > 0 {
        return Action::TransferResource {
            from: ResourceOwner::Location {
                location_id: location_id.to_string(),
            },
            to: ResourceOwner::Agent {
                agent_id: agent_id.to_string(),
            },
            kind: ResourceKind::Electricity,
            amount: location_power.min(5),
        };
    }

    if agent_power > 0 {
        return Action::TransferResource {
            from: ResourceOwner::Agent {
                agent_id: agent_id.to_string(),
            },
            to: ResourceOwner::Location {
                location_id: location_id.to_string(),
            },
            kind: ResourceKind::Electricity,
            amount: agent_power.min(5),
        };
    }

    Action::TransferResource {
        from: ResourceOwner::Location {
            location_id: location_id.to_string(),
        },
        to: ResourceOwner::Agent {
            agent_id: agent_id.to_string(),
        },
        kind: ResourceKind::Electricity,
        amount: 1,
    }
}

struct ViewerLiveSession {
    subscribed: HashSet<ViewerStream>,
    event_filters: Option<HashSet<ViewerEventKind>>,
    playing: bool,
    next_play_step_at: Option<Instant>,
    metrics: RunnerMetrics,
}

#[derive(Debug, Clone, Copy)]
struct ViewerLiveRequestOutcome {
    continue_running: bool,
    request_llm_decision: bool,
    deferred_control: Option<ViewerLiveDeferredControl>,
}

#[derive(Debug, Clone, Copy)]
enum ViewerLiveDeferredControl {
    Step {
        count: usize,
        request_id: Option<u64>,
    },
}

#[derive(Debug, Clone, Copy)]
enum CoalescedSignalKind {
    LlmDecisionRequested,
    ConsensusCommitted,
    ConsensusDriveRequested,
    NonConsensusDriveRequested,
}

const LIVE_LOOP_SIGNAL_KIND_COUNT: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveLoopSignalKind {
    Request,
    LlmDecisionRequested,
    ConsensusCommitted,
    ConsensusDriveRequested,
    NonConsensusDriveRequested,
    StepRequested,
}

impl LiveLoopSignalKind {
    const ALL: [Self; LIVE_LOOP_SIGNAL_KIND_COUNT] = [
        Self::Request,
        Self::LlmDecisionRequested,
        Self::ConsensusCommitted,
        Self::ConsensusDriveRequested,
        Self::NonConsensusDriveRequested,
        Self::StepRequested,
    ];

    fn as_index(self) -> usize {
        match self {
            Self::Request => 0,
            Self::LlmDecisionRequested => 1,
            Self::ConsensusCommitted => 2,
            Self::ConsensusDriveRequested => 3,
            Self::NonConsensusDriveRequested => 4,
            Self::StepRequested => 5,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::LlmDecisionRequested => "llm_decision",
            Self::ConsensusCommitted => "consensus_committed",
            Self::ConsensusDriveRequested => "consensus_drive",
            Self::NonConsensusDriveRequested => "non_consensus_drive",
            Self::StepRequested => "step",
        }
    }
}

impl LiveLoopSignal {
    fn kind(&self) -> LiveLoopSignalKind {
        match self {
            LiveLoopSignal::Request(_) => LiveLoopSignalKind::Request,
            LiveLoopSignal::LlmDecisionRequested => LiveLoopSignalKind::LlmDecisionRequested,
            LiveLoopSignal::ConsensusCommitted => LiveLoopSignalKind::ConsensusCommitted,
            LiveLoopSignal::ConsensusDriveRequested => LiveLoopSignalKind::ConsensusDriveRequested,
            LiveLoopSignal::NonConsensusDriveRequested => {
                LiveLoopSignalKind::NonConsensusDriveRequested
            }
            LiveLoopSignal::StepRequested { .. } => LiveLoopSignalKind::StepRequested,
        }
    }
}

struct LiveLoopBackpressure {
    merged_llm_decision_requested: AtomicU64,
    merged_consensus_committed: AtomicU64,
    merged_consensus_drive_requested: AtomicU64,
    merged_non_consensus_drive_requested: AtomicU64,
    dropped_llm_decision_requested: AtomicU64,
    dropped_consensus_committed: AtomicU64,
    dropped_consensus_drive_requested: AtomicU64,
    dropped_non_consensus_drive_requested: AtomicU64,
    enqueued_signals: [AtomicU64; LIVE_LOOP_SIGNAL_KIND_COUNT],
    handled_signals: [AtomicU64; LIVE_LOOP_SIGNAL_KIND_COUNT],
    handled_nanos_total: [AtomicU64; LIVE_LOOP_SIGNAL_KIND_COUNT],
    handled_nanos_max: [AtomicU64; LIVE_LOOP_SIGNAL_KIND_COUNT],
}

#[derive(Debug, Default, Clone, Copy)]
struct LiveLoopBackpressureSnapshot {
    merged_llm_decision_requested: u64,
    merged_consensus_committed: u64,
    merged_consensus_drive_requested: u64,
    merged_non_consensus_drive_requested: u64,
    dropped_llm_decision_requested: u64,
    dropped_consensus_committed: u64,
    dropped_consensus_drive_requested: u64,
    dropped_non_consensus_drive_requested: u64,
    signal_stats: [LiveLoopSignalStatsSnapshot; LIVE_LOOP_SIGNAL_KIND_COUNT],
}

#[derive(Debug, Default, Clone, Copy)]
struct LiveLoopSignalStatsSnapshot {
    enqueued: u64,
    handled: u64,
    avg_handle_us: u64,
    max_handle_us: u64,
}

impl Default for LiveLoopBackpressure {
    fn default() -> Self {
        Self {
            merged_llm_decision_requested: AtomicU64::new(0),
            merged_consensus_committed: AtomicU64::new(0),
            merged_consensus_drive_requested: AtomicU64::new(0),
            merged_non_consensus_drive_requested: AtomicU64::new(0),
            dropped_llm_decision_requested: AtomicU64::new(0),
            dropped_consensus_committed: AtomicU64::new(0),
            dropped_consensus_drive_requested: AtomicU64::new(0),
            dropped_non_consensus_drive_requested: AtomicU64::new(0),
            enqueued_signals: std::array::from_fn(|_| AtomicU64::new(0)),
            handled_signals: std::array::from_fn(|_| AtomicU64::new(0)),
            handled_nanos_total: std::array::from_fn(|_| AtomicU64::new(0)),
            handled_nanos_max: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

impl LiveLoopBackpressureSnapshot {
    fn has_activity(&self) -> bool {
        self.merged_llm_decision_requested > 0
            || self.merged_consensus_committed > 0
            || self.merged_consensus_drive_requested > 0
            || self.merged_non_consensus_drive_requested > 0
            || self.dropped_llm_decision_requested > 0
            || self.dropped_consensus_committed > 0
            || self.dropped_consensus_drive_requested > 0
            || self.dropped_non_consensus_drive_requested > 0
            || self
                .signal_stats
                .iter()
                .any(|stats| stats.enqueued > 0 || stats.handled > 0)
    }

    fn signal_stats(&self, kind: LiveLoopSignalKind) -> LiveLoopSignalStatsSnapshot {
        self.signal_stats[kind.as_index()]
    }
}

impl LiveLoopBackpressure {
    fn record_enqueued(&self, kind: LiveLoopSignalKind) {
        self.enqueued_signals[kind.as_index()].fetch_add(1, Ordering::SeqCst);
    }

    fn record_handled(&self, kind: LiveLoopSignalKind, elapsed: Duration) {
        let index = kind.as_index();
        self.handled_signals[index].fetch_add(1, Ordering::SeqCst);
        let nanos = elapsed.as_nanos().min(u64::MAX as u128) as u64;
        self.handled_nanos_total[index].fetch_add(nanos, Ordering::SeqCst);
        fetch_max_atomic(&self.handled_nanos_max[index], nanos);
    }

    fn record_merged(&self, kind: CoalescedSignalKind) {
        match kind {
            CoalescedSignalKind::LlmDecisionRequested => {
                self.merged_llm_decision_requested
                    .fetch_add(1, Ordering::SeqCst);
            }
            CoalescedSignalKind::ConsensusCommitted => {
                self.merged_consensus_committed
                    .fetch_add(1, Ordering::SeqCst);
            }
            CoalescedSignalKind::ConsensusDriveRequested => {
                self.merged_consensus_drive_requested
                    .fetch_add(1, Ordering::SeqCst);
            }
            CoalescedSignalKind::NonConsensusDriveRequested => {
                self.merged_non_consensus_drive_requested
                    .fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    fn record_dropped(&self, kind: CoalescedSignalKind) {
        match kind {
            CoalescedSignalKind::LlmDecisionRequested => {
                self.dropped_llm_decision_requested
                    .fetch_add(1, Ordering::SeqCst);
            }
            CoalescedSignalKind::ConsensusCommitted => {
                self.dropped_consensus_committed
                    .fetch_add(1, Ordering::SeqCst);
            }
            CoalescedSignalKind::ConsensusDriveRequested => {
                self.dropped_consensus_drive_requested
                    .fetch_add(1, Ordering::SeqCst);
            }
            CoalescedSignalKind::NonConsensusDriveRequested => {
                self.dropped_non_consensus_drive_requested
                    .fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    fn snapshot(&self) -> LiveLoopBackpressureSnapshot {
        LiveLoopBackpressureSnapshot {
            merged_llm_decision_requested: self
                .merged_llm_decision_requested
                .load(Ordering::SeqCst),
            merged_consensus_committed: self.merged_consensus_committed.load(Ordering::SeqCst),
            merged_consensus_drive_requested: self
                .merged_consensus_drive_requested
                .load(Ordering::SeqCst),
            merged_non_consensus_drive_requested: self
                .merged_non_consensus_drive_requested
                .load(Ordering::SeqCst),
            dropped_llm_decision_requested: self
                .dropped_llm_decision_requested
                .load(Ordering::SeqCst),
            dropped_consensus_committed: self.dropped_consensus_committed.load(Ordering::SeqCst),
            dropped_consensus_drive_requested: self
                .dropped_consensus_drive_requested
                .load(Ordering::SeqCst),
            dropped_non_consensus_drive_requested: self
                .dropped_non_consensus_drive_requested
                .load(Ordering::SeqCst),
            signal_stats: std::array::from_fn(|index| {
                let enqueued = self.enqueued_signals[index].load(Ordering::SeqCst);
                let handled = self.handled_signals[index].load(Ordering::SeqCst);
                let total_nanos = self.handled_nanos_total[index].load(Ordering::SeqCst);
                let max_nanos = self.handled_nanos_max[index].load(Ordering::SeqCst);
                LiveLoopSignalStatsSnapshot {
                    enqueued,
                    handled,
                    avg_handle_us: total_nanos.checked_div(handled).unwrap_or(0) / 1_000,
                    max_handle_us: max_nanos / 1_000,
                }
            }),
        }
    }
}

impl CoalescedSignalKind {
    fn signal_kind(self) -> LiveLoopSignalKind {
        match self {
            Self::LlmDecisionRequested => LiveLoopSignalKind::LlmDecisionRequested,
            Self::ConsensusCommitted => LiveLoopSignalKind::ConsensusCommitted,
            Self::ConsensusDriveRequested => LiveLoopSignalKind::ConsensusDriveRequested,
            Self::NonConsensusDriveRequested => LiveLoopSignalKind::NonConsensusDriveRequested,
        }
    }
}

fn fetch_max_atomic(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::SeqCst);
    while value > current {
        match target.compare_exchange(current, value, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

fn format_live_loop_signal_stats(snapshot: &LiveLoopBackpressureSnapshot) -> String {
    let mut parts = Vec::new();
    for kind in LiveLoopSignalKind::ALL {
        let stats = snapshot.signal_stats(kind);
        if stats.enqueued == 0 && stats.handled == 0 {
            continue;
        }
        parts.push(format!(
            "{}={{in:{}, handled:{}, avg_us:{}, max_us:{}}}",
            kind.as_str(),
            stats.enqueued,
            stats.handled,
            stats.avg_handle_us,
            stats.max_handle_us
        ));
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

fn enqueue_coalesced_signal(
    tx: &mpsc::SyncSender<LiveLoopSignal>,
    signal: LiveLoopSignal,
    queued: &Arc<AtomicBool>,
    kind: CoalescedSignalKind,
    backpressure: &LiveLoopBackpressure,
) {
    if queued.swap(true, Ordering::SeqCst) {
        backpressure.record_merged(kind);
        return;
    }
    match tx.try_send(signal) {
        Ok(()) => {
            backpressure.record_enqueued(kind.signal_kind());
        }
        Err(mpsc::TrySendError::Full(_)) => {
            queued.store(false, Ordering::SeqCst);
            backpressure.record_dropped(kind);
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            queued.store(false, Ordering::SeqCst);
        }
    }
}

impl ViewerLiveSession {
    fn new() -> Self {
        Self {
            subscribed: HashSet::new(),
            event_filters: None,
            playing: false,
            next_play_step_at: None,
            metrics: RunnerMetrics::default(),
        }
    }

    fn handle_request(
        &mut self,
        request: ViewerRequest,
        writer: &mut BufWriter<TcpStream>,
        world: &mut LiveWorld,
        world_id: &str,
    ) -> Result<ViewerLiveRequestOutcome, ViewerLiveServerError> {
        let mut request_llm_decision = false;
        let mut deferred_control = None;
        match request {
            ViewerRequest::SafeAlternative { .. }
            | ViewerRequest::RequestCanonicalAgentOwnerReadContext { .. }
            | ViewerRequest::CanonicalAgentOwnerRead { .. } => {
                send_response(writer, &ViewerResponse::Error {message: "canonical owner read requires configured WorldService runtime".into()})?;
            }
            ViewerRequest::Hello { .. } | ViewerRequest::HelloV2 { .. } => {
                let response = ViewerResponse::HelloAck {
                    server: "oasis7".to_string(),
                    version: VIEWER_PROTOCOL_VERSION,
                    min_version: 1,
                    max_version: VIEWER_PROTOCOL_VERSION,
                    capabilities: Vec::new(),
                    world_id: world_id.to_string(),
                    control_profile: ViewerControlProfile::Live,
                    authority_epoch: None,
                };
                send_response(writer, &response)?;
            }
            ViewerRequest::Subscribe {
                streams,
                event_kinds,
            } => {
                self.subscribed = streams.into_iter().collect();
                self.event_filters = if event_kinds.is_empty() {
                    None
                } else {
                    Some(event_kinds.into_iter().collect())
                };
            }
            ViewerRequest::RequestSnapshot => {
                if self.subscribed.is_empty() || self.subscribed.contains(&ViewerStream::Snapshot) {
                    send_response(
                        writer,
                        &ViewerResponse::Snapshot {
                            snapshot: world.snapshot(),
                        },
                    )?;
                }
                if self.subscribed.contains(&ViewerStream::Metrics) {
                    self.update_metrics(world.metrics());
                    send_response(
                        writer,
                        &ViewerResponse::Metrics {
                            time: Some(world.kernel().time()),
                            metrics: self.metrics.clone(),
                        },
                    )?;
                }
            }
            ViewerRequest::RequestScopeSnapshot { request_generation } => {
                if request_generation.is_empty() || request_generation.len() > 64 {
                    send_response(
                        writer,
                        &ViewerResponse::Error {
                            message: "invalid_scope_snapshot_generation".into(),
                        },
                    )?;
                } else {
                    send_response(
                        writer,
                        &ViewerResponse::ScopeSnapshot {
                            request_generation,
                            snapshot: world.snapshot(),
                        },
                    )?;
                }
            }
            ViewerRequest::RequestWorldView { query } => {
                let view = crate::viewer::world_view::build(
                    &world.snapshot(),
                    world_id,
                    0,
                    "public",
                    query,
                );
                send_response(writer, &ViewerResponse::WorldView { view })?;
            }
            ViewerRequest::RequestWorldFeed { .. } => {
                send_response(
                    writer,
                    &ViewerResponse::WorldFeed {
                        feed: crate::viewer::protocol::WorldFeedEnvelope {
                            schema_version:
                                crate::viewer::protocol::WORLD_FEED_SCHEMA_VERSION.to_string(),
                            world_id: world_id.to_string(),
                            reorg_epoch: 0,
                            cursor: String::new(),
                            events: Vec::new(),
                            status: crate::viewer::protocol::WorldFeedStatus::Unavailable,
                            gap_reason: None,
                            unavailable_reason: Some(
                                crate::viewer::protocol::WorldFeedUnavailableReason::SourceUnavailable,
                            ),
                            snapshot_reload_required: false,
                        },
                    },
                )?;
            }
            ViewerRequest::PlaybackControl { mode, request_id } => {
                self.apply_control_mode(
                    ViewerControl::from(mode),
                    request_id,
                    &mut request_llm_decision,
                    &mut deferred_control,
                );
            }
            ViewerRequest::LiveControl { mode, request_id } => {
                self.apply_control_mode(
                    ViewerControl::from(mode),
                    request_id,
                    &mut request_llm_decision,
                    &mut deferred_control,
                );
            }
            ViewerRequest::PromptControl { command } => {
                let (result, wake_llm) = match *command {
                    PromptControlCommand::Preview { request } => {
                        (world.prompt_control_preview(request), false)
                    }
                    PromptControlCommand::Apply { request } => {
                        (world.prompt_control_apply(request), true)
                    }
                    PromptControlCommand::Rollback { request } => {
                        (world.prompt_control_rollback(request), true)
                    }
                };
                match result {
                    Ok(ack) => {
                        if wake_llm {
                            request_llm_decision = true;
                        }
                        send_response(writer, &ViewerResponse::PromptControlAck { ack })?;
                    }
                    Err(error) => {
                        send_response(writer, &ViewerResponse::PromptControlError { error })?;
                    }
                }
            }
            ViewerRequest::AgentChat { request } => match world.agent_chat(request) {
                Ok(ack) => {
                    request_llm_decision = true;
                    send_response(writer, &ViewerResponse::AgentChatAck { ack })?;
                }
                Err(error) => {
                    send_response(writer, &ViewerResponse::AgentChatError { error })?;
                }
            },
            ViewerRequest::GameplayAction { request } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "gameplay_action is only available in runtime live mode"
                                .to_string(),
                            action_id: Some(request.action_id),
                            target_agent_id: Some(request.target_agent_id),
                        },
                    },
                )?;
            }
            ViewerRequest::CollectData { command: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "collect_data is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("collect_data".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteRefineCompound { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "quote_refine_compound is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_refine_compound".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteScheduleRecipe { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "quote_schedule_recipe is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_schedule_recipe".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteProductValidation { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message:
                                "quote_validate_product is only available in runtime live mode"
                                    .to_string(),
                            action_id: Some("quote_validate_product".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuotePowerSurvival { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "quote_power_survival is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_power_survival".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuotePowerSale { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "quote_power_sale is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_power_sale".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteDeclareSocialEdge { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "quote_declare_social_edge is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_declare_social_edge".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuotePublishSocialFact { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "quote_publish_social_fact is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_publish_social_fact".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteAdjudicateSocialFact { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message:
                                "quote_adjudicate_social_fact is only available in runtime live mode"
                                    .to_string(),
                            action_id: Some("quote_adjudicate_social_fact".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteRevokeSocialFact { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message:
                                "quote_revoke_social_fact is only available in runtime live mode"
                                    .to_string(),
                            action_id: Some("quote_revoke_social_fact".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteSocialContact { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "quote_social_contact is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_social_contact".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteDeclareWar { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "quote_declare_war is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_declare_war".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteGovernanceVote { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "quote_governance_vote is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_governance_vote".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::PreviewFragmentReplenishment { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message:
                                "preview_fragment_replenishment is only available in runtime live mode"
                                    .to_string(),
                            action_id: Some("preview_fragment_replenishment".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteMarketDecision { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "market cost preview is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_market_decision".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::QuoteTransferMaterial { request: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: crate::viewer::GameplayActionError {
                            code: "unsupported_in_live_server".to_string(),
                            message: "quote_transfer_material is only available in runtime live mode"
                                .to_string(),
                            action_id: Some("quote_transfer_material".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::Control { mode, request_id } => {
                // Legacy compatibility: map mixed control channel into live semantics.
                self.apply_control_mode(
                    mode,
                    request_id,
                    &mut request_llm_decision,
                    &mut deferred_control,
                );
            }
            ViewerRequest::AuthoritativeChallenge { command: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::AuthoritativeChallengeError {
                        error: crate::viewer::AuthoritativeChallengeError {
                            code: "unsupported_in_live_server".to_string(),
                            message:
                                "authoritative_challenge is only available in runtime live mode"
                                    .to_string(),
                            challenge_id: None,
                            batch_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::AuthoritativeRecovery { command: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::AuthoritativeRecoveryError {
                        error: crate::viewer::AuthoritativeRecoveryError {
                            code: "unsupported_in_live_server".to_string(),
                            message:
                                "authoritative_recovery is only available in runtime live mode"
                                    .to_string(),
                            batch_id: None,
                            player_id: None,
                            session_pubkey: None,
                            revoke_reason: None,
                            revoked_by: None,
                        },
                    },
                )?;
            }
        }
        Ok(ViewerLiveRequestOutcome {
            continue_running: true,
            request_llm_decision,
            deferred_control,
        })
    }

    fn apply_control_mode(
        &mut self,
        mode: ViewerControl,
        request_id: Option<u64>,
        request_llm_decision: &mut bool,
        deferred_control: &mut Option<ViewerLiveDeferredControl>,
    ) {
        match mode {
            ViewerControl::Pause => {
                self.playing = false;
                self.next_play_step_at = None;
            }
            ViewerControl::Play => {
                self.playing = true;
                self.next_play_step_at = None;
                *request_llm_decision = true;
            }
            ViewerControl::Step { count } => {
                self.playing = false;
                self.next_play_step_at = None;
                *deferred_control = Some(ViewerLiveDeferredControl::Step {
                    count: count.max(1),
                    request_id,
                });
            }
            ViewerControl::Seek { tick } => {
                self.playing = false;
                self.next_play_step_at = None;
                // P2P live mode is monotonic and does not support rewind/seek semantics.
                crate::observability::emit_stderr_or_event(
                    tracing::Level::INFO,
                    format!("viewer live: ignore seek control in live mode (target_tick={tick})")
                        .as_str(),
                    "viewer live ignored seek control in live mode",
                );
            }
        }
    }

    fn throttle_play_drive(&mut self) {
        if !self.playing {
            return;
        }
        if let Some(next_step_at) = self.next_play_step_at {
            let now = Instant::now();
            if now < next_step_at {
                thread::sleep(next_step_at - now);
            }
        }
    }

    fn schedule_next_play_drive(&mut self, interval: Duration, advanced_ticks: u64) {
        if !self.playing {
            self.next_play_step_at = None;
            return;
        }
        let scaled_ticks = advanced_ticks.clamp(1, 16) as u32;
        let delay = interval
            .checked_mul(scaled_ticks)
            .unwrap_or_else(|| Duration::from_secs(10));
        self.next_play_step_at = Some(Instant::now() + delay);
    }

    fn should_emit_event(&self) -> bool {
        self.playing && self.subscribed.contains(&ViewerStream::Events)
    }

    fn event_allowed(&self, event: &crate::simulator::WorldEvent) -> bool {
        match &self.event_filters {
            Some(filters) => filters
                .iter()
                .any(|filter| viewer_event_kind_matches(filter, &event.kind)),
            None => true,
        }
    }

    fn update_metrics(&mut self, metrics: RunnerMetrics) {
        self.metrics = metrics;
    }

    fn emit_metrics(&self, writer: &mut BufWriter<TcpStream>) -> Result<(), ViewerLiveServerError> {
        if self.subscribed.contains(&ViewerStream::Metrics) {
            send_response(
                writer,
                &ViewerResponse::Metrics {
                    time: Some(self.metrics.total_ticks),
                    metrics: self.metrics.clone(),
                },
            )?;
        }
        Ok(())
    }
}

fn metrics_from_kernel(kernel: &WorldKernel) -> RunnerMetrics {
    let total_ticks = kernel.time();
    let total_actions = kernel.journal().len() as u64;
    let actions_per_tick = if total_ticks > 0 {
        total_actions as f64 / total_ticks as f64
    } else {
        0.0
    };
    RunnerMetrics {
        total_ticks,
        total_agents: kernel.model().agents.len(),
        agents_active: kernel.model().agents.len(),
        agents_quota_exhausted: 0,
        total_actions,
        total_decisions: 0,
        actions_per_tick,
        decisions_per_tick: 0.0,
        success_rate: 0.0,
        runtime_perf: unsupported_live_control_runtime_perf_snapshot(),
    }
}

fn unsupported_live_control_runtime_perf_snapshot() -> RuntimePerfSnapshot {
    unsupported_runtime_perf_snapshot()
}

fn read_requests(
    stream: TcpStream,
    tx: mpsc::SyncSender<LiveLoopSignal>,
    loop_running: Arc<AtomicBool>,
    backpressure: Arc<LiveLoopBackpressure>,
) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(request) = oasis7_viewer_codec::request_from_json(trimmed) {
                    if tx.send(LiveLoopSignal::Request(request)).is_err() {
                        loop_running.store(false, Ordering::SeqCst);
                        break;
                    }
                    backpressure.record_enqueued(LiveLoopSignalKind::Request);
                }
            }
            Err(_) => {
                loop_running.store(false, Ordering::SeqCst);
                break;
            }
        }
    }
    loop_running.store(false, Ordering::SeqCst);
}

fn emit_consensus_commit_signals(
    tx: mpsc::SyncSender<LiveLoopSignal>,
    loop_running: Arc<AtomicBool>,
    committed_batches: NodeCommittedActionBatchesHandle,
    signal_queued: Arc<AtomicBool>,
    backpressure: Arc<LiveLoopBackpressure>,
) {
    let wait_timeout = Duration::from_millis(50);
    while loop_running.load(Ordering::SeqCst) {
        if !committed_batches.wait_for_batches(wait_timeout) {
            continue;
        }
        if !loop_running.load(Ordering::SeqCst) {
            break;
        }
        enqueue_coalesced_signal(
            &tx,
            LiveLoopSignal::ConsensusCommitted,
            &signal_queued,
            CoalescedSignalKind::ConsensusCommitted,
            backpressure.as_ref(),
        );
    }
}

fn send_response(
    writer: &mut BufWriter<TcpStream>,
    response: &ViewerResponse,
) -> Result<(), ViewerLiveServerError> {
    let payload = oasis7_viewer_codec::response_to_json(response)
        .map_err(|err| ViewerLiveServerError::Serde(err.to_string()))?;
    writer.write_all(payload.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn is_disconnect_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
    )
}

#[cfg(test)]
#[path = "live/tests.rs"]
mod tests;
