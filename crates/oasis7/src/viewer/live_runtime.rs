use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::observability::emit_stderr_or_event;
use oasis7_node::{NodeCommittedActionBatchesHandle, NodeRuntime};
use tracing::Level;

use crate::geometry::space_distance_cm;
use crate::simulator::{
    ActionResult, AgentDecision, AgentDecisionTrace, AgentRunner, LlmAgentBehavior,
    LlmAgentBuildError, LlmAgentConfig, OpenAiChatCompletionClient, RunnerMetrics, WorldEvent,
    WorldKernel, WorldSnapshot, initialize_kernel,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_simulator::{
    Action, ActionSubmitter, AgentPromptProfile, PromptUpdateOperation, ResourceOwner, WorldConfig,
    WorldEventKind, WorldInitConfig, WorldInitError, WorldScenario,
};

#[path = "live/consensus_bridge.rs"]
mod consensus_bridge;
use consensus_bridge::*;

use super::auth::{
    PromptControlAuthIntent, verify_agent_chat_auth_proof, verify_prompt_control_apply_auth_proof,
    verify_prompt_control_rollback_auth_proof,
};
use super::protocol::{
    AgentChatAck, AgentChatError, AgentChatRequest, ControlCompletionAck, ControlCompletionStatus,
    PromptControlAck, PromptControlApplyRequest, PromptControlCommand, PromptControlError,
    PromptControlOperation, PromptControlRollbackRequest, VIEWER_PROTOCOL_VERSION, ViewerControl,
    ViewerControlProfile, ViewerEventKind, ViewerRequest, ViewerResponse, ViewerStream,
    viewer_event_kind_matches,
};
#[path = "live/live_auth_prompt.rs"]
mod live_auth_prompt;
#[path = "live/live_helpers.rs"]
mod live_helpers;
use live_helpers::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerLiveDecisionMode {
    Script,
    Llm,
}

#[derive(Debug, Clone)]
pub struct ViewerLiveServerConfig {
    pub bind_addr: String,
    pub scenario: WorldScenario,
    pub world_id: String,
    pub decision_mode: ViewerLiveDecisionMode,
    pub play_step_interval: Duration,
    pub consensus_gate_max_tick: Option<Arc<AtomicU64>>,
    pub consensus_runtime: Option<Arc<Mutex<NodeRuntime>>>,
}

impl ViewerLiveServerConfig {
    pub fn new(scenario: WorldScenario) -> Self {
        Self {
            bind_addr: "127.0.0.1:5010".to_string(),
            world_id: format!("live-{}", scenario.as_str()),
            scenario,
            decision_mode: ViewerLiveDecisionMode::Script,
            play_step_interval: Duration::from_millis(200),
            consensus_gate_max_tick: None,
            consensus_runtime: None,
        }
    }

    pub fn with_bind_addr(mut self, addr: impl Into<String>) -> Self {
        self.bind_addr = addr.into();
        self
    }

    pub fn with_world_id(mut self, world_id: impl Into<String>) -> Self {
        self.world_id = world_id.into();
        self
    }

    pub fn with_decision_mode(mut self, mode: ViewerLiveDecisionMode) -> Self {
        self.decision_mode = mode;
        self
    }

    pub fn with_play_step_interval(mut self, interval: Duration) -> Self {
        self.play_step_interval = interval.max(Duration::from_millis(20));
        self
    }

    pub fn with_llm_mode(mut self, enabled: bool) -> Self {
        self.decision_mode = if enabled {
            ViewerLiveDecisionMode::Llm
        } else {
            ViewerLiveDecisionMode::Script
        };
        self
    }

    pub fn with_consensus_gate_max_tick(mut self, max_tick: Arc<AtomicU64>) -> Self {
        self.consensus_gate_max_tick = Some(max_tick);
        self
    }

    pub fn with_consensus_runtime(mut self, runtime: Arc<Mutex<NodeRuntime>>) -> Self {
        self.consensus_runtime = Some(runtime);
        self
    }
}

#[derive(Debug)]
pub enum ViewerLiveServerError {
    Io(io::Error),
    Serde(String),
    Init(WorldInitError),
    LlmBuild(LlmAgentBuildError),
    Node(String),
}

impl From<io::Error> for ViewerLiveServerError {
    fn from(err: io::Error) -> Self {
        ViewerLiveServerError::Io(err)
    }
}

impl From<WorldInitError> for ViewerLiveServerError {
    fn from(err: WorldInitError) -> Self {
        ViewerLiveServerError::Init(err)
    }
}

impl From<LlmAgentBuildError> for ViewerLiveServerError {
    fn from(err: LlmAgentBuildError) -> Self {
        ViewerLiveServerError::LlmBuild(err)
    }
}

impl ViewerLiveServerError {
    fn is_disconnect(&self) -> bool {
        match self {
            ViewerLiveServerError::Io(err) => is_disconnect_error(err),
            _ => false,
        }
    }
}

pub struct ViewerLiveServer {
    config: ViewerLiveServerConfig,
    world: LiveWorld,
}

#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "Viewer request transport keeps the protocol request inline for direct dispatch"
)]
enum LiveLoopSignal {
    Request(ViewerRequest),
    LlmDecisionRequested,
    ConsensusCommitted,
    ConsensusDriveRequested,
    NonConsensusDriveRequested,
    StepRequested {
        count: usize,
        request_id: Option<u64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveLoopIterationAction {
    Continue,
    Stop,
}

fn normalize_live_loop_action_result(
    result: Result<LiveLoopIterationAction, ViewerLiveServerError>,
) -> Result<LiveLoopIterationAction, ViewerLiveServerError> {
    match result {
        Err(err) if err.is_disconnect() => Ok(LiveLoopIterationAction::Stop),
        result => result,
    }
}

const LIVE_LOOP_QUEUE_CAPACITY: usize = 256;
const STEP_REQUEST_CONSENSUS_RETRY_MAX_ATTEMPTS: usize = 100;
const STEP_REQUEST_CONSENSUS_RETRY_SLEEP: Duration = Duration::from_millis(20);

impl ViewerLiveServer {
    pub fn new(config: ViewerLiveServerConfig) -> Result<Self, ViewerLiveServerError> {
        let init = WorldInitConfig::from_scenario(config.scenario, &WorldConfig::default());
        let world = if let Some(max_tick) = config.consensus_gate_max_tick.clone() {
            LiveWorld::new_with_consensus_gate(
                WorldConfig::default(),
                init,
                config.decision_mode,
                Some(max_tick),
                config.consensus_runtime.clone(),
            )?
        } else {
            LiveWorld::new_with_consensus_gate(
                WorldConfig::default(),
                init,
                config.decision_mode,
                None,
                config.consensus_runtime.clone(),
            )?
        };
        Ok(Self { config, world })
    }

    pub fn run(&mut self) -> Result<(), ViewerLiveServerError> {
        let listener = TcpListener::bind(&self.config.bind_addr)?;
        for incoming in listener.incoming() {
            let stream = incoming?;
            if let Err(err) = self.serve_stream(stream) {
                let stderr_message = format!("viewer live server error: {err:?}");
                emit_stderr_or_event(
                    Level::WARN,
                    stderr_message.as_str(),
                    "viewer live server session failed",
                );
            }
        }
        Ok(())
    }

    pub fn run_once(&mut self) -> Result<(), ViewerLiveServerError> {
        let listener = TcpListener::bind(&self.config.bind_addr)?;
        let (stream, _) = listener.accept()?;
        self.serve_stream(stream)?;
        Ok(())
    }

    fn serve_stream(&mut self, stream: TcpStream) -> Result<(), ViewerLiveServerError> {
        stream.set_nodelay(true)?;
        let reader_stream = stream.try_clone()?;
        let mut writer = BufWriter::new(stream);
        let (tx, rx) = mpsc::sync_channel::<LiveLoopSignal>(LIVE_LOOP_QUEUE_CAPACITY);
        let loop_running = Arc::new(AtomicBool::new(true));
        let backpressure = Arc::new(LiveLoopBackpressure::default());
        let llm_signal_queued = Arc::new(AtomicBool::new(false));
        let consensus_signal_queued = Arc::new(AtomicBool::new(false));
        let consensus_drive_signal_queued = Arc::new(AtomicBool::new(false));
        let non_consensus_drive_signal_queued = Arc::new(AtomicBool::new(false));
        let request_loop_running = Arc::clone(&loop_running);
        let loop_tx = tx.clone();
        let request_tx = tx.clone();
        let consensus_batches_handle = self.world.consensus_batches_handle()?;
        let request_backpressure = Arc::clone(&backpressure);

        thread::spawn(move || {
            read_requests(
                reader_stream,
                request_tx,
                request_loop_running,
                request_backpressure,
            )
        });
        if let Some(committed_batches) = consensus_batches_handle {
            let consensus_tx = tx.clone();
            let consensus_loop_running = Arc::clone(&loop_running);
            let queued_flag = Arc::clone(&consensus_signal_queued);
            let consensus_backpressure = Arc::clone(&backpressure);
            thread::spawn(move || {
                emit_consensus_commit_signals(
                    consensus_tx,
                    consensus_loop_running,
                    committed_batches,
                    queued_flag,
                    consensus_backpressure,
                )
            });
        }
        drop(tx);

        let mut session = ViewerLiveSession::new();
        let result = loop {
            let signal = match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(signal) => signal,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if !loop_running.load(Ordering::SeqCst) {
                        break Ok(());
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break Ok(()),
            };
            let signal_kind = signal.kind();
            let signal_started_at = Instant::now();
            let action_result: Result<LiveLoopIterationAction, ViewerLiveServerError> =
                (|| match signal {
                    LiveLoopSignal::Request(command) => {
                        let was_emitting = session.should_emit_event();
                        let outcome = session.handle_request(
                            command,
                            &mut writer,
                            &mut self.world,
                            &self.config.world_id,
                        )?;
                        if outcome.request_llm_decision {
                            enqueue_coalesced_signal(
                                &loop_tx,
                                LiveLoopSignal::LlmDecisionRequested,
                                &llm_signal_queued,
                                CoalescedSignalKind::LlmDecisionRequested,
                                backpressure.as_ref(),
                            );
                        }
                        if let Some(control) = outcome.deferred_control {
                            let ViewerLiveDeferredControl::Step { count, request_id } = control;
                            if loop_tx
                                .send(LiveLoopSignal::StepRequested { count, request_id })
                                .is_ok()
                            {
                                backpressure.record_enqueued(LiveLoopSignalKind::StepRequested);
                            }
                        }
                        let now_emitting = session.should_emit_event();
                        if self.world.uses_consensus_bridge()
                            && now_emitting
                            && (!was_emitting || outcome.request_llm_decision)
                        {
                            enqueue_coalesced_signal(
                                &loop_tx,
                                LiveLoopSignal::ConsensusDriveRequested,
                                &consensus_drive_signal_queued,
                                CoalescedSignalKind::ConsensusDriveRequested,
                                backpressure.as_ref(),
                            );
                        } else if self.world.uses_non_consensus_event_drive()
                            && now_emitting
                            && (!was_emitting || outcome.request_llm_decision)
                        {
                            enqueue_coalesced_signal(
                                &loop_tx,
                                LiveLoopSignal::NonConsensusDriveRequested,
                                &non_consensus_drive_signal_queued,
                                CoalescedSignalKind::NonConsensusDriveRequested,
                                backpressure.as_ref(),
                            );
                        }
                        if outcome.continue_running {
                            Ok(LiveLoopIterationAction::Continue)
                        } else {
                            Ok(LiveLoopIterationAction::Stop)
                        }
                    }
                    LiveLoopSignal::LlmDecisionRequested => {
                        llm_signal_queued.store(false, Ordering::SeqCst);
                        self.world.request_llm_decision();
                        if self.world.uses_consensus_bridge() && session.should_emit_event() {
                            enqueue_coalesced_signal(
                                &loop_tx,
                                LiveLoopSignal::ConsensusDriveRequested,
                                &consensus_drive_signal_queued,
                                CoalescedSignalKind::ConsensusDriveRequested,
                                backpressure.as_ref(),
                            );
                        } else if self.world.uses_non_consensus_event_drive()
                            && session.should_emit_event()
                        {
                            enqueue_coalesced_signal(
                                &loop_tx,
                                LiveLoopSignal::NonConsensusDriveRequested,
                                &non_consensus_drive_signal_queued,
                                CoalescedSignalKind::NonConsensusDriveRequested,
                                backpressure.as_ref(),
                            );
                        }
                        Ok(LiveLoopIterationAction::Continue)
                    }
                    LiveLoopSignal::ConsensusCommitted => {
                        self.handle_consensus_committed(&mut session, &mut writer)?;
                        consensus_signal_queued.store(false, Ordering::SeqCst);
                        Ok(LiveLoopIterationAction::Continue)
                    }
                    LiveLoopSignal::ConsensusDriveRequested => {
                        consensus_drive_signal_queued.store(false, Ordering::SeqCst);
                        self.handle_consensus_drive_requested(&mut session, &mut writer)?;
                        Ok(LiveLoopIterationAction::Continue)
                    }
                    LiveLoopSignal::NonConsensusDriveRequested => {
                        non_consensus_drive_signal_queued.store(false, Ordering::SeqCst);
                        let should_requeue =
                            self.handle_non_consensus_drive_requested(&mut session, &mut writer)?;
                        if should_requeue {
                            enqueue_coalesced_signal(
                                &loop_tx,
                                LiveLoopSignal::NonConsensusDriveRequested,
                                &non_consensus_drive_signal_queued,
                                CoalescedSignalKind::NonConsensusDriveRequested,
                                backpressure.as_ref(),
                            );
                        }
                        Ok(LiveLoopIterationAction::Continue)
                    }
                    LiveLoopSignal::StepRequested { count, request_id } => {
                        self.handle_step_request(&mut session, &mut writer, count, request_id)?;
                        Ok(LiveLoopIterationAction::Continue)
                    }
                })();
            backpressure.record_handled(signal_kind, signal_started_at.elapsed());
            match normalize_live_loop_action_result(action_result) {
                Ok(LiveLoopIterationAction::Continue) => {}
                Ok(LiveLoopIterationAction::Stop) => break Ok(()),
                Err(err) => break Err(err),
            }
        };
        loop_running.store(false, Ordering::SeqCst);
        drop(loop_tx);
        let backpressure_snapshot = backpressure.snapshot();
        if backpressure_snapshot.has_activity() {
            emit_stderr_or_event(
                Level::INFO,
                format!(
                    "viewer live backpressure merged={{llm_decision:{}, consensus_committed:{}, consensus_drive:{}, non_consensus_drive:{}}} dropped={{llm_decision:{}, consensus_committed:{}, consensus_drive:{}, non_consensus_drive:{}}} signals=[{}]",
                    backpressure_snapshot.merged_llm_decision_requested,
                    backpressure_snapshot.merged_consensus_committed,
                    backpressure_snapshot.merged_consensus_drive_requested,
                    backpressure_snapshot.merged_non_consensus_drive_requested,
                    backpressure_snapshot.dropped_llm_decision_requested,
                    backpressure_snapshot.dropped_consensus_committed,
                    backpressure_snapshot.dropped_consensus_drive_requested,
                    backpressure_snapshot.dropped_non_consensus_drive_requested,
                    format_live_loop_signal_stats(&backpressure_snapshot),
                )
                .as_str(),
                "viewer live backpressure summary",
            );
        }
        result
    }

    fn handle_consensus_committed(
        &mut self,
        session: &mut ViewerLiveSession,
        writer: &mut BufWriter<TcpStream>,
    ) -> Result<(), ViewerLiveServerError> {
        if !self.world.uses_consensus_bridge() {
            return Ok(());
        }
        if !self.world.can_step_for_consensus() {
            return Ok(());
        }

        if !session.playing {
            // Keep committed state synchronized while paused, but avoid pushing
            // per-step visual updates to the viewer to prevent HUD flicker.
            loop {
                let step = self.world.step()?;
                if step.event.is_none() {
                    break;
                }
            }
            return Ok(());
        }

        session.throttle_play_drive();
        let baseline_tick = self.world.logical_time();
        let baseline_event_seq = self.world.latest_event_seq();
        let step = self.world.step()?;
        let advanced_ticks = self.world.logical_time().saturating_sub(baseline_tick);
        let advanced_events = self
            .world
            .latest_event_seq()
            .saturating_sub(baseline_event_seq);
        session.schedule_next_play_drive(
            self.config.play_step_interval,
            advanced_ticks.max(advanced_events),
        );
        self.emit_step_outcome(session, writer, step, false)
    }

    fn handle_consensus_drive_requested(
        &mut self,
        session: &mut ViewerLiveSession,
        writer: &mut BufWriter<TcpStream>,
    ) -> Result<(), ViewerLiveServerError> {
        if !session.should_emit_event() || !self.world.uses_consensus_bridge() {
            return Ok(());
        }
        if !self.world.can_step_for_consensus() {
            return Ok(());
        }
        session.throttle_play_drive();
        let baseline_tick = self.world.logical_time();
        let baseline_event_seq = self.world.latest_event_seq();
        let step = self.world.step()?;
        let advanced_ticks = self.world.logical_time().saturating_sub(baseline_tick);
        let advanced_events = self
            .world
            .latest_event_seq()
            .saturating_sub(baseline_event_seq);
        session.schedule_next_play_drive(
            self.config.play_step_interval,
            advanced_ticks.max(advanced_events),
        );
        self.emit_step_outcome(session, writer, step, false)
    }

    fn handle_non_consensus_drive_requested(
        &mut self,
        session: &mut ViewerLiveSession,
        writer: &mut BufWriter<TcpStream>,
    ) -> Result<bool, ViewerLiveServerError> {
        if !session.should_emit_event() || !self.world.uses_non_consensus_event_drive() {
            return Ok(false);
        }
        if !self.world.should_step_on_event_drive() {
            return Ok(false);
        }
        session.throttle_play_drive();
        let baseline_tick = self.world.logical_time();
        let baseline_event_seq = self.world.latest_event_seq();
        let step = self.world.step()?;
        let advanced_ticks = self.world.logical_time().saturating_sub(baseline_tick);
        let advanced_events = self
            .world
            .latest_event_seq()
            .saturating_sub(baseline_event_seq);
        session.schedule_next_play_drive(
            self.config.play_step_interval,
            advanced_ticks.max(advanced_events),
        );
        let should_requeue = self.world.should_requeue_non_consensus_drive(&step);
        self.emit_step_outcome(session, writer, step, false)?;
        Ok(should_requeue)
    }

    fn handle_step_request(
        &mut self,
        session: &mut ViewerLiveSession,
        writer: &mut BufWriter<TcpStream>,
        count: usize,
        request_id: Option<u64>,
    ) -> Result<(), ViewerLiveServerError> {
        session.playing = false;
        let steps = count.max(1);
        let baseline_logical_time = self.world.logical_time();
        let baseline_event_seq = self.world.latest_event_seq();
        for _ in 0..steps {
            self.world.request_llm_decision();
            let mut step = self.world.step()?;
            if self.world.uses_consensus_bridge()
                && step.event.is_none()
                && step.decision_trace.is_none()
            {
                for _ in 0..STEP_REQUEST_CONSENSUS_RETRY_MAX_ATTEMPTS {
                    thread::sleep(STEP_REQUEST_CONSENSUS_RETRY_SLEEP);
                    let retry = self.world.step()?;
                    let has_progress = retry.event.is_some() || retry.decision_trace.is_some();
                    step = retry;
                    if has_progress {
                        break;
                    }
                }
            }
            self.emit_step_outcome(session, writer, step, true)?;
        }
        if let Some(request_id) = request_id {
            let delta_logical_time = self
                .world
                .logical_time()
                .saturating_sub(baseline_logical_time);
            let delta_event_seq = self
                .world
                .latest_event_seq()
                .saturating_sub(baseline_event_seq);
            let status = if delta_logical_time > 0 || delta_event_seq > 0 {
                ControlCompletionStatus::Advanced
            } else {
                ControlCompletionStatus::TimeoutNoProgress
            };
            send_response(
                writer,
                &ViewerResponse::ControlCompletionAck {
                    ack: ControlCompletionAck {
                        request_id,
                        status,
                        delta_logical_time,
                        delta_event_seq,
                        error_code: None,
                        error_message: None,
                    },
                },
            )?;
        }
        Ok(())
    }

    fn emit_step_outcome(
        &mut self,
        session: &mut ViewerLiveSession,
        writer: &mut BufWriter<TcpStream>,
        step: LiveStepResult,
        emit_idle_metrics: bool,
    ) -> Result<(), ViewerLiveServerError> {
        let has_progress = step.event.is_some() || step.decision_trace.is_some();
        if let Some(trace) = step.decision_trace
            && session.subscribed.contains(&ViewerStream::Events)
        {
            send_response(writer, &ViewerResponse::DecisionTrace { trace })?;
        }

        if let Some(event) = step.event {
            if session.event_allowed(&event) && session.subscribed.contains(&ViewerStream::Events) {
                send_response(writer, &ViewerResponse::Event { event })?;
            }
            if session.subscribed.contains(&ViewerStream::Snapshot) {
                send_response(
                    writer,
                    &ViewerResponse::Snapshot {
                        snapshot: self.world.snapshot(),
                    },
                )?;
            }
        }

        if emit_idle_metrics || has_progress {
            session.update_metrics(self.world.metrics());
            session.emit_metrics(writer)?;
        }
        Ok(())
    }
}

struct LiveWorld {
    #[cfg(test)]
    config: WorldConfig,
    #[cfg(test)]
    init: WorldInitConfig,
    kernel: WorldKernel,
    #[cfg(test)]
    decision_mode: ViewerLiveDecisionMode,
    driver: LiveDriver,
    llm_decision_mailbox: u64,
    consensus_gate_max_tick: Option<Arc<AtomicU64>>,
    consensus_bridge: Option<LiveConsensusBridge>,
}

#[expect(
    clippy::large_enum_variant,
    reason = "Viewer live driver keeps the script and built-in runner variants in one dispatch enum"
)]
enum LiveDriver {
    Script(LiveScript),
    Llm(AgentRunner<LlmAgentBehavior<OpenAiChatCompletionClient>>),
}

struct LiveStepResult {
    event: Option<WorldEvent>,
    decision_trace: Option<AgentDecisionTrace>,
}

impl LiveWorld {
    #[cfg(test)]
    fn new(
        config: WorldConfig,
        init: WorldInitConfig,
        decision_mode: ViewerLiveDecisionMode,
    ) -> Result<Self, ViewerLiveServerError> {
        Self::new_with_consensus_gate(config, init, decision_mode, None, None)
    }

    fn new_with_consensus_gate(
        config: WorldConfig,
        init: WorldInitConfig,
        decision_mode: ViewerLiveDecisionMode,
        consensus_gate_max_tick: Option<Arc<AtomicU64>>,
        consensus_runtime: Option<Arc<Mutex<NodeRuntime>>>,
    ) -> Result<Self, ViewerLiveServerError> {
        #[cfg(test)]
        let reset_config = config.clone();
        #[cfg(test)]
        let reset_init = init.clone();
        let (kernel, _) = initialize_kernel(config.clone(), init.clone())?;
        let driver = build_driver(&kernel, decision_mode)?;
        let llm_decision_mailbox = if matches!(&driver, LiveDriver::Llm(_)) {
            1
        } else {
            0
        };
        Ok(Self {
            #[cfg(test)]
            config: reset_config,
            #[cfg(test)]
            init: reset_init,
            kernel,
            #[cfg(test)]
            decision_mode,
            driver,
            llm_decision_mailbox,
            consensus_gate_max_tick,
            consensus_bridge: consensus_runtime.map(LiveConsensusBridge::new),
        })
    }

    fn kernel(&self) -> &WorldKernel {
        &self.kernel
    }

    fn metrics(&self) -> RunnerMetrics {
        match &self.driver {
            LiveDriver::Script(_) => metrics_from_kernel(&self.kernel),
            LiveDriver::Llm(runner) => runner.metrics_with_kernel(&self.kernel),
        }
    }

    fn snapshot(&self) -> WorldSnapshot {
        self.kernel.snapshot()
    }

    fn logical_time(&self) -> u64 {
        self.kernel.time()
    }

    fn latest_event_seq(&self) -> u64 {
        self.kernel
            .journal()
            .last()
            .map(|event| event.id)
            .unwrap_or(0)
    }

    fn uses_consensus_bridge(&self) -> bool {
        self.consensus_bridge.is_some()
    }

    fn uses_non_consensus_event_drive(&self) -> bool {
        !self.uses_consensus_bridge()
    }

    fn consensus_batches_handle(
        &self,
    ) -> Result<Option<NodeCommittedActionBatchesHandle>, ViewerLiveServerError> {
        self.consensus_bridge
            .as_ref()
            .map(LiveConsensusBridge::committed_batches_handle)
            .transpose()
    }

    fn can_step_for_consensus(&self) -> bool {
        if self.consensus_bridge.is_some() {
            return true;
        }
        let Some(max_tick) = self.consensus_gate_max_tick.as_ref() else {
            return true;
        };
        self.kernel.time() < max_tick.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    fn reset(&mut self) -> Result<(), ViewerLiveServerError> {
        let (kernel, _) = initialize_kernel(self.config.clone(), self.init.clone())?;
        self.kernel = kernel;
        self.driver = build_driver(&self.kernel, self.decision_mode)?;
        self.llm_decision_mailbox = if matches!(&self.driver, LiveDriver::Llm(_)) {
            1
        } else {
            0
        };
        if let Some(bridge) = self.consensus_bridge.as_mut() {
            bridge.reset_pending();
        }
        Ok(())
    }

    fn step(&mut self) -> Result<LiveStepResult, ViewerLiveServerError> {
        if self.consensus_bridge.is_some() {
            return self.step_via_consensus();
        }
        match &mut self.driver {
            LiveDriver::Script(script) => {
                if let Some(action) = script.next_action(&self.kernel) {
                    self.kernel.submit_action(action);
                }
                Ok(LiveStepResult {
                    event: self.kernel.step(),
                    decision_trace: None,
                })
            }
            LiveDriver::Llm(runner) => {
                if self.llm_decision_mailbox == 0 {
                    return Ok(LiveStepResult {
                        event: None,
                        decision_trace: None,
                    });
                }
                self.llm_decision_mailbox = self.llm_decision_mailbox.saturating_sub(1);
                let tick_result = runner.tick(&mut self.kernel);
                sync_llm_runner_long_term_memory(&mut self.kernel, runner);
                let mut event = None;
                let mut decision_trace = None;
                if let Some(result) = tick_result {
                    event = result.action_result.map(|action| action.event);
                    decision_trace = result.decision_trace;
                }
                if event.is_some() {
                    self.llm_decision_mailbox = self.llm_decision_mailbox.saturating_add(1);
                }
                Ok(LiveStepResult {
                    event,
                    decision_trace,
                })
            }
        }
    }

    fn request_llm_decision(&mut self) {
        if matches!(&self.driver, LiveDriver::Llm(_)) {
            self.llm_decision_mailbox = self.llm_decision_mailbox.saturating_add(1);
        }
    }

    fn llm_mailbox_has_pending(&self) -> bool {
        self.llm_decision_mailbox > 0
    }

    fn should_step_on_event_drive(&self) -> bool {
        if self.consensus_bridge.is_some() {
            return true;
        }
        match &self.driver {
            LiveDriver::Script(_) => true,
            LiveDriver::Llm(_) => self.llm_mailbox_has_pending(),
        }
    }

    fn should_requeue_non_consensus_drive(&self, step: &LiveStepResult) -> bool {
        if self.consensus_bridge.is_some() {
            return false;
        }
        match &self.driver {
            LiveDriver::Script(_) => step.event.is_some(),
            LiveDriver::Llm(_) => self.llm_mailbox_has_pending(),
        }
    }
}

fn map_auth_verify_error_code(message: &str) -> &'static str {
    if message.contains("nonce") {
        return "auth_nonce_invalid";
    }
    if message.contains("signature") || message.contains("awviewauth:v1") {
        return "auth_signature_invalid";
    }
    if message.contains("player_id") || message.contains("public_key") {
        return "auth_claim_mismatch";
    }
    if message.contains("required") || message.contains("empty") {
        return "auth_claim_invalid";
    }
    "auth_invalid"
}

#[derive(Debug, Clone)]
struct LiveScript {
    agent_id: Option<String>,
    locations: Vec<String>,
    target_index: usize,
}
