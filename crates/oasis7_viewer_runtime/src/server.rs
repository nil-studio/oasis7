use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use tracing::Level;

use oasis7::observability::emit_stderr_or_event;
#[cfg(test)]
use oasis7::simulator::RuntimePerfHealth;
use oasis7::simulator::unsupported_runtime_perf_snapshot;
use oasis7::simulator::{
    RunnerMetrics, RuntimePerfSnapshot, WorldEvent, WorldJournal, WorldSnapshot,
};
use oasis7::viewer::{
    AgentChatError, AuthoritativeChallengeError, AuthoritativeRecoveryError, ControlCompletionAck,
    ControlCompletionStatus, PlaybackControl, PromptControlError, VIEWER_PROTOCOL_VERSION,
    ViewerControlProfile, ViewerEventKind, ViewerRequest, ViewerResponse, ViewerStream,
    viewer_event_kind_matches,
};
use oasis7_simulator::{PersistError, WorldTime};

#[derive(Debug, Clone)]
pub struct ViewerServerConfig {
    pub bind_addr: String,
    pub snapshot_path: PathBuf,
    pub journal_path: PathBuf,
    pub world_id: String,
}

impl ViewerServerConfig {
    pub fn from_dir(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        let snapshot_path = dir.join("snapshot.json");
        let journal_path = dir.join("journal.json");
        let world_id = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("world")
            .to_string();
        Self {
            bind_addr: "127.0.0.1:5010".to_string(),
            snapshot_path,
            journal_path,
            world_id,
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
}

#[derive(Debug)]
pub enum ViewerServerError {
    Io(String),
    Serde(String),
    Persist(String),
}

impl From<io::Error> for ViewerServerError {
    fn from(err: io::Error) -> Self {
        ViewerServerError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for ViewerServerError {
    fn from(err: serde_json::Error) -> Self {
        ViewerServerError::Serde(err.to_string())
    }
}

impl From<PersistError> for ViewerServerError {
    fn from(err: PersistError) -> Self {
        ViewerServerError::Persist(format!("{err:?}"))
    }
}

pub struct ViewerServer {
    config: ViewerServerConfig,
    snapshot: WorldSnapshot,
    journal: WorldJournal,
}

impl ViewerServer {
    pub fn load(config: ViewerServerConfig) -> Result<Self, ViewerServerError> {
        let snapshot = WorldSnapshot::load_json(&config.snapshot_path)?;
        let journal = WorldJournal::load_json(&config.journal_path)?;
        Ok(Self {
            config,
            snapshot,
            journal,
        })
    }

    pub fn run(&self) -> Result<(), ViewerServerError> {
        let listener = TcpListener::bind(&self.config.bind_addr)?;
        for incoming in listener.incoming() {
            let stream = incoming?;
            if let Err(err) = self.serve_stream(stream) {
                let stderr_message = format!("viewer server error: {err:?}");
                emit_stderr_or_event(
                    Level::WARN,
                    stderr_message.as_str(),
                    "viewer server session failed",
                );
            }
        }
        Ok(())
    }

    pub fn run_once(&self) -> Result<(), ViewerServerError> {
        let listener = TcpListener::bind(&self.config.bind_addr)?;
        let (stream, _) = listener.accept()?;
        self.serve_stream(stream)?;
        Ok(())
    }

    fn serve_stream(&self, stream: TcpStream) -> Result<(), ViewerServerError> {
        stream.set_nodelay(true)?;
        let reader_stream = stream.try_clone()?;
        let mut writer = BufWriter::new(stream);
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || read_requests(reader_stream, tx));

        let mut session = ViewerSession::new(&self.journal.events);

        while let Ok(command) = rx.recv() {
            if !session.handle_request(
                command,
                &mut writer,
                &self.snapshot,
                &self.config.world_id,
            )? {
                break;
            }
        }
        Ok(())
    }
}

struct ViewerSession<'a> {
    events: &'a [WorldEvent],
    subscribed: HashSet<ViewerStream>,
    event_filters: Option<HashSet<ViewerEventKind>>,
    cursor: usize,
    metrics: RunnerMetrics,
    last_event_seq: u64,
}

impl<'a> ViewerSession<'a> {
    fn new(events: &'a [WorldEvent]) -> Self {
        Self {
            events,
            subscribed: HashSet::new(),
            event_filters: None,
            cursor: 0,
            metrics: RunnerMetrics::default(),
            last_event_seq: 0,
        }
    }

    fn handle_request(
        &mut self,
        request: ViewerRequest,
        writer: &mut BufWriter<TcpStream>,
        snapshot: &WorldSnapshot,
        world_id: &str,
    ) -> Result<bool, ViewerServerError> {
        match request {
            ViewerRequest::RequestCanonicalAgentOwnerReadContext { .. }
            | ViewerRequest::CanonicalAgentOwnerRead { .. } => {
                send_response(
                    writer,
                    &ViewerResponse::Error {
                        message: "canonical owner read requires configured WorldService runtime"
                            .into(),
                    },
                )?;
            }
            ViewerRequest::Hello { .. } | ViewerRequest::HelloV2 { .. } => {
                let response = ViewerResponse::HelloAck {
                    server: "oasis7".to_string(),
                    version: VIEWER_PROTOCOL_VERSION,
                    min_version: 1,
                    max_version: VIEWER_PROTOCOL_VERSION,
                    capabilities: Vec::new(),
                    world_id: world_id.to_string(),
                    control_profile: ViewerControlProfile::Playback,
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
                            snapshot: snapshot.clone(),
                        },
                    )?;
                }
                if self.subscribed.contains(&ViewerStream::Metrics) {
                    self.metrics = metrics_from_snapshot(snapshot);
                    send_response(
                        writer,
                        &ViewerResponse::Metrics {
                            time: Some(snapshot.time),
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
                            snapshot: snapshot.clone(),
                        },
                    )?;
                }
            }
            ViewerRequest::RequestWorldView { query } => {
                let view = oasis7::viewer::build_playback_world_view(
                    snapshot, world_id, 0, "public", query,
                );
                send_response(writer, &ViewerResponse::WorldView { view })?;
            }
            ViewerRequest::RequestWorldFeed { .. } => {
                send_response(
                    writer,
                    &ViewerResponse::WorldFeed {
                        feed: oasis7::viewer::WorldFeedEnvelope {
                            schema_version: oasis7::viewer::WORLD_FEED_SCHEMA_VERSION.to_string(),
                            world_id: world_id.to_string(),
                            reorg_epoch: 0,
                            cursor: String::new(),
                            events: Vec::new(),
                            status: oasis7::viewer::WorldFeedStatus::Unavailable,
                            gap_reason: None,
                            unavailable_reason: Some(
                                oasis7::viewer::WorldFeedUnavailableReason::SourceUnavailable,
                            ),
                            snapshot_reload_required: false,
                        },
                    },
                )?;
            }
            ViewerRequest::PlaybackControl { mode, request_id } => {
                self.handle_playback_control(mode, request_id, writer)?
            }
            ViewerRequest::LiveControl {
                mode,
                request_id: _request_id,
            } => {
                send_response(
                    writer,
                    &ViewerResponse::Error {
                        message: format!(
                            "live_control is not supported in offline playback server (mode={mode:?})"
                        ),
                    },
                )?;
            }
            ViewerRequest::Control { mode, request_id } => {
                // Legacy compatibility: map mixed control channel into playback semantics.
                self.handle_playback_control(PlaybackControl::from(mode), request_id, writer)?
            }
            ViewerRequest::PromptControl { .. } => {
                send_response(
                    writer,
                    &ViewerResponse::PromptControlError {
                        error: PromptControlError {
                            code: "unsupported_in_offline_server".to_string(),
                            message: "prompt_control is only available in live mode".to_string(),
                            agent_id: None,
                            current_version: None,
                            ..PromptControlError::default_legacy()
                        },
                    },
                )?;
            }
            ViewerRequest::AgentChat { request } => {
                send_response(
                    writer,
                    &ViewerResponse::AgentChatError {
                        error: AgentChatError {
                            code: "unsupported_in_offline_server".to_string(),
                            message: "agent_chat is only available in live mode".to_string(),
                            agent_id: Some(request.agent_id),
                        },
                    },
                )?;
            }
            ViewerRequest::GameplayAction { request } => {
                send_response(
                    writer,
                    &ViewerResponse::GameplayActionError {
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
                            message:
                                "quote_declare_social_edge is only available in runtime live mode"
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
                            message:
                                "quote_publish_social_fact is only available in runtime live mode"
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
                            message: "preview_fragment_replenishment is only available in runtime live mode"
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: oasis7::viewer::GameplayActionError {
                            code: "unsupported_in_offline_server".to_string(),
                            message:
                                "quote_transfer_material is only available in runtime live mode"
                                    .to_string(),
                            action_id: Some("quote_transfer_material".to_string()),
                            target_agent_id: None,
                        },
                    },
                )?;
            }
            ViewerRequest::AuthoritativeChallenge { command: _ } => {
                send_response(
                    writer,
                    &ViewerResponse::AuthoritativeChallengeError {
                        error: AuthoritativeChallengeError {
                            code: "unsupported_in_offline_server".to_string(),
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
                        error: AuthoritativeRecoveryError {
                            code: "unsupported_in_offline_server".to_string(),
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
        Ok(true)
    }

    fn emit_playback_events(
        &mut self,
        writer: &mut BufWriter<TcpStream>,
    ) -> Result<(), ViewerServerError> {
        if !self.subscribed.contains(&ViewerStream::Events) {
            return Ok(());
        }
        while self.emit_next_event(writer)? {}
        Ok(())
    }

    fn handle_playback_control(
        &mut self,
        mode: PlaybackControl,
        request_id: Option<u64>,
        writer: &mut BufWriter<TcpStream>,
    ) -> Result<(), ViewerServerError> {
        match mode {
            PlaybackControl::Pause => {}
            PlaybackControl::Play => self.emit_playback_events(writer)?,
            PlaybackControl::Step { count } => {
                let baseline_logical_time = self.metrics.total_ticks;
                let baseline_event_seq = self.last_event_seq;
                let steps = count.max(1);
                for _ in 0..steps {
                    if !self.emit_next_event(writer)? {
                        break;
                    }
                }
                if let Some(request_id) = request_id {
                    let delta_logical_time = self
                        .metrics
                        .total_ticks
                        .saturating_sub(baseline_logical_time);
                    let delta_event_seq = self.last_event_seq.saturating_sub(baseline_event_seq);
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
            }
            PlaybackControl::Seek { tick } => {
                self.cursor = seek_to_tick(self.events, tick);
                self.update_metrics_time(tick);
                self.emit_metrics(writer)?;
            }
        }
        Ok(())
    }

    fn emit_next_event(
        &mut self,
        writer: &mut BufWriter<TcpStream>,
    ) -> Result<bool, ViewerServerError> {
        let Some(event) = self.next_event() else {
            return Ok(false);
        };
        let time = event.time;
        self.last_event_seq = event.id;
        send_response(writer, &ViewerResponse::Event { event })?;
        self.update_metrics_time(time);
        self.emit_metrics(writer)?;
        Ok(true)
    }

    fn next_event(&mut self) -> Option<WorldEvent> {
        while self.cursor < self.events.len() {
            let event = self.events.get(self.cursor).cloned();
            self.cursor = self.cursor.saturating_add(1);
            if let Some(event) = event
                && self.event_allowed(&event)
            {
                return Some(event);
            }
        }
        None
    }

    fn event_allowed(&self, event: &WorldEvent) -> bool {
        match &self.event_filters {
            Some(filters) => filters
                .iter()
                .any(|filter| viewer_event_kind_matches(filter, &event.kind)),
            None => true,
        }
    }

    fn update_metrics_time(&mut self, time: WorldTime) {
        self.metrics.total_ticks = time;
    }

    fn emit_metrics(&self, writer: &mut BufWriter<TcpStream>) -> Result<(), ViewerServerError> {
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

fn seek_to_tick(events: &[WorldEvent], tick: WorldTime) -> usize {
    events
        .iter()
        .position(|event| event.time >= tick)
        .unwrap_or(events.len())
}

fn read_requests(stream: TcpStream, tx: mpsc::Sender<ViewerRequest>) {
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
                match serde_json::from_str::<ViewerRequest>(trimmed) {
                    Ok(request) => {
                        if tx.send(request).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        // Ignore malformed requests for now.
                    }
                }
            }
            Err(_) => break,
        }
    }
}

fn send_response(
    writer: &mut BufWriter<TcpStream>,
    response: &ViewerResponse,
) -> Result<(), ViewerServerError> {
    serde_json::to_writer(&mut *writer, response)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn metrics_from_snapshot(snapshot: &WorldSnapshot) -> RunnerMetrics {
    RunnerMetrics {
        total_ticks: snapshot.time,
        total_agents: snapshot.model.agents.len(),
        agents_active: snapshot.model.agents.len(),
        agents_quota_exhausted: 0,
        total_actions: 0,
        total_decisions: 0,
        actions_per_tick: 0.0,
        decisions_per_tick: 0.0,
        success_rate: 0.0,
        runtime_perf: unsupported_viewer_runtime_perf_snapshot(),
    }
}

fn unsupported_viewer_runtime_perf_snapshot() -> RuntimePerfSnapshot {
    unsupported_runtime_perf_snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7::simulator::WorldKernel;
    use oasis7_simulator::{RejectReason, WorldEventKind};
    use std::io::{BufRead, BufReader, BufWriter};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};

    fn make_event(id: u64, time: WorldTime) -> WorldEvent {
        WorldEvent {
            id,
            time,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount { amount: 1 },
            },
            runtime_event: None,
        }
    }

    fn test_writer_pair() -> (BufWriter<TcpStream>, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let addr = listener.local_addr().expect("listener local addr");
        let client = TcpStream::connect(addr).expect("connect test client");
        let (server, _) = listener.accept().expect("accept test peer");
        (BufWriter::new(server), client)
    }

    fn read_control_completion_ack(
        peer: &TcpStream,
        timeout: Duration,
    ) -> Option<oasis7::viewer::ControlCompletionAck> {
        let stream = peer.try_clone().expect("clone test peer");
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .expect("set read timeout");
        let mut reader = BufReader::new(stream);
        let start = Instant::now();
        let mut line = String::new();
        while start.elapsed() < timeout {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => continue,
                Ok(_) => {}
                Err(err) => {
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) {
                        continue;
                    }
                    panic!("read response line failed: {err}");
                }
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(response) = serde_json::from_str::<oasis7::viewer::ViewerResponse>(trimmed)
            else {
                continue;
            };
            if let oasis7::viewer::ViewerResponse::ControlCompletionAck { ack } = response {
                return Some(ack);
            }
        }
        None
    }

    #[test]
    fn seek_to_tick_finds_first_event_at_or_after_time() {
        let events = vec![make_event(1, 10), make_event(2, 20), make_event(3, 30)];
        assert_eq!(seek_to_tick(&events, 0), 0);
        assert_eq!(seek_to_tick(&events, 10), 0);
        assert_eq!(seek_to_tick(&events, 15), 1);
        assert_eq!(seek_to_tick(&events, 25), 2);
        assert_eq!(seek_to_tick(&events, 35), 3);
    }

    #[test]
    fn snapshot_metrics_mark_runtime_perf_unknown_when_unavailable() {
        let perf = unsupported_viewer_runtime_perf_snapshot();
        assert_eq!(perf.health, RuntimePerfHealth::Unknown);
        assert_eq!(perf.tick.samples_total, 0);
        assert_eq!(perf.tick.samples_window, 0);
    }

    #[test]
    fn playback_step_emits_completion_ack_advanced_when_event_emitted() {
        let events = vec![make_event(1, 10)];
        let mut session = ViewerSession::new(&events);
        let (mut writer, peer) = test_writer_pair();

        session
            .handle_playback_control(PlaybackControl::Step { count: 1 }, Some(3001), &mut writer)
            .expect("step control");

        let ack = read_control_completion_ack(&peer, Duration::from_secs(1))
            .expect("control completion ack should be emitted");
        assert_eq!(ack.request_id, 3001);
        assert_eq!(ack.status, ControlCompletionStatus::Advanced);
        assert!(ack.delta_logical_time > 0 || ack.delta_event_seq > 0);
    }

    #[test]
    fn playback_step_emits_completion_ack_timeout_when_no_event_emitted() {
        let events = vec![];
        let mut session = ViewerSession::new(&events);
        let (mut writer, peer) = test_writer_pair();

        session
            .handle_playback_control(PlaybackControl::Step { count: 1 }, Some(3002), &mut writer)
            .expect("step control");

        let ack = read_control_completion_ack(&peer, Duration::from_secs(1))
            .expect("control completion ack should be emitted");
        assert_eq!(ack.request_id, 3002);
        assert_eq!(ack.status, ControlCompletionStatus::TimeoutNoProgress);
        assert_eq!(ack.delta_logical_time, 0);
        assert_eq!(ack.delta_event_seq, 0);
    }

    #[test]
    fn offline_server_rejects_market_quote_decision_with_stable_code() {
        let events = vec![];
        let mut session = ViewerSession::new(&events);
        let (mut writer, peer) = test_writer_pair();
        session
            .handle_request(
                ViewerRequest::QuoteMarketDecision {
                    request: oasis7::viewer::MarketQuoteDecisionRequest {
                        consume: vec![],
                        player_id: "player".to_string(),
                        public_key: None,
                        auth: None,
                    },
                },
                &mut writer,
                &WorldKernel::new().snapshot(),
                "test-world",
            )
            .expect("unsupported request is a response");
        writer.flush().expect("flush response");
        let mut line = String::new();
        BufReader::new(peer)
            .read_line(&mut line)
            .expect("read response");
        let response: oasis7::viewer::ViewerResponse =
            serde_json::from_str(line.trim()).expect("decode response");
        assert!(
            matches!(response, oasis7::viewer::ViewerResponse::GameplayActionError { error } if error.code == "unsupported_in_offline_server" && error.action_id.as_deref() == Some("quote_market_decision"))
        );
    }
}
