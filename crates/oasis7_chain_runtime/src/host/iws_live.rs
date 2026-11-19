//! Explicit profile-3 local service consumer. This entry neither provisions
//! GEN nor recovers unresolved cross-process decisions. Every startup requires
//! an independently pinned exact head plus clean checkpoint DATA for that head.
use super::{
    iws_live_config::{self as config, IwsLiveConfigV1, ValidatedIwsLiveConfigV1},
    iws_live_io::{CommandV1, HexIdV1, LocalIoV1},
};
use crate::iws_host_runtime::{
    HostObservationV1, HostObservationWorkV1, IwsLiveNodeColdSettingsV1, IwsLiveNodeServiceV1,
    IwsLiveNodeSettingsV1, NormalColdRecoveryWorkV1, NormalCommitWorkV1,
    node_checkpoint::{IwsNodeCheckpointLimitsV1, load_node_checkpoint_bounded_v1},
    node_start::IwsCleanNodeStartV1,
    observe_current_bounded,
};
use oasis7_distfs::controlled_authority::formal::endpoint::FormalArchiveReadWorkV1;
use oasis7_formal_protocol::incremental_execution::TransactionLimits;
use oasis7_node::{NodeConfig, NodeRole};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use oasis7_runtime::{NormalCommitHeaderLimitsV1, NormalHostWorkV1, RuntimeSourceCompiler};
use serde::{Serialize, ser::SerializeSeq};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn try_run(path: &Path) -> Result<bool, String> {
    let (path, bytes) = config::private_config_bytes(path)?;
    let mode: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("formal live configuration JSON: {e}"))?;
    match mode.get("runtime_mode") {
        None => return Ok(false), // Existing profile-2 schema, unchanged.
        Some(serde_json::Value::String(value)) if value == config::MODE => {}
        _ => return Err("unknown formal live runtime_mode".into()),
    }
    drop(mode);
    let validated = IwsLiveConfigV1::load(&path, &bytes)?;
    drop(bytes);
    let mut running = RunningV1::open(validated)?;
    running.run()?;
    Ok(true)
}

struct RunningV1 {
    // Join/drop the real worker before releasing Node queue admission or the
    // persistent identity lease. No raw hook or alternate execution is exposed.
    service: IwsLiveNodeServiceV1,
    observation: HostObservationWorkV1,
    io: LocalIoV1,
    transaction: TransactionLimits,
    checkpoint_directory: PathBuf,
    checkpoint_limits: IwsNodeCheckpointLimitsV1,
    max_action_bytes: usize,
    _node_queue_fee: MemoryReservation,
    _identity: super::node_keypair_config::RuntimeIdentityLease,
}
impl RunningV1 {
    fn open(validated: ValidatedIwsLiveConfigV1) -> Result<Self, String> {
        let identity = config::acquire_identity(&validated)?;
        let ValidatedIwsLiveConfigV1 {
            config: c,
            record,
            anchor,
            writer,
            providers: [primary_provider, replica_provider],
            policies: [primary_policy, replica_policy],
            process,
            ..
        } = validated;
        let memory = MemoryBudget::new(c.memory_bytes, c.recovery_reserve_bytes)?;
        let archive = FormalArchiveReadWorkV1::new(c.common_steps, c.common_read_bytes)
            .map_err(|e| e.to_string())?;
        // This is an explicit lifetime reservation for the inherited Node
        // queue buffers/copies and bounded snapshot metadata, not another bank.
        let queue_bytes = c
            .node_queue_bytes
            .checked_add(c.node_committed_bytes)
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| {
                c.runtime_memory
                    .max_pending_actions
                    .checked_mul(2)
                    .and_then(|v| v.checked_add(c.node_committed_batches))
                    .and_then(|v| v.checked_mul(4096))
                    .and_then(|v| n.checked_add(v))
            })
            .and_then(|n| n.checked_add(1 << 20))
            .ok_or("Node queue memory overflow")?;
        let queue_fee = memory
            .reserve(queue_bytes, MemoryClass::Ordinary)
            .map_err(|e| e.to_string())?;
        let io = LocalIoV1::new(
            &memory,
            c.max_request_bytes,
            c.max_reply_bytes,
            c.cli_steps,
            c.cli_bytes,
        )?;
        let checkpoint_limits = IwsNodeCheckpointLimitsV1 {
            max_projection_bytes: c.checkpoint_limits.max_projection_bytes,
            max_mailbox_entries: c.checkpoint_limits.max_mailbox_entries,
            max_path_bytes: c.checkpoint_limits.max_path_bytes,
        };
        let current = c.current.build();
        let transaction = c.transaction.build();
        let mut stop = || false;
        let (loaded, initial, normal, observation) = {
            let mut common = archive.bounded_context(&memory, &mut stop);
            let loaded = load_node_checkpoint_bounded_v1(
                &c.checkpoint_directory,
                record,
                checkpoint_limits,
                &mut common,
            )
            .map_err(|e| e.to_string())?;
            let initial = [
                c.initial_cold[0].admit(&current, &mut common)?,
                c.initial_cold[1].admit(&current, &mut common)?,
            ];
            let cold = [
                c.recovery_cold[0].admit(&current, &mut common)?,
                c.recovery_cold[1].admit(&current, &mut common)?,
            ];
            let cold = NormalColdRecoveryWorkV1::new_bounded(cold, &mut common)
                .map_err(|e| e.to_string())?;
            let runtime = NormalHostWorkV1::new_bounded(c.runtime_work.build(), &mut common)
                .map_err(|e| e.to_string())?;
            let normal = NormalCommitWorkV1::new(
                c.input_work.canonical(),
                c.input_work.frozen(),
                c.normal_sources[0].source(),
                c.normal_sources[1].source(),
                c.normal_io[0].normal_io(&c.normal_io[1]),
                runtime,
                cold,
            );
            let observation = c.observation_work.observe(&mut common)?;
            (loaded, initial, normal, observation)
        };
        let world_id = anchor.head().record().world.world_id.clone();
        let mut node = NodeConfig::new(&c.node_id, &world_id, NodeRole::Sequencer)
            .and_then(|n| n.with_player_id(&c.player_id))
            .and_then(|n| n.with_tick_interval(Duration::from_millis(c.node_tick_ms)))
            .map_err(|e| e.to_string())?;
        node.max_pending_consensus_actions = c.runtime_memory.max_pending_actions;
        node.max_engine_pending_consensus_actions = c.runtime_memory.max_pending_actions;
        node.max_consensus_action_payload_bytes = c.max_action_bytes;
        node.max_pending_consensus_action_queue_bytes = c.node_queue_bytes;
        node.max_committed_action_batch_bytes = c.node_committed_bytes;
        node.max_committed_action_batches = c.node_committed_batches;
        // Only the independently paid Service (or an error string) escapes.
        // In particular, a failed constructor's settings/mailbox graph is
        // destroyed while Loaded still retains the DATA admission reservation.
        let mut service = loaded.with_data(|checkpoint, mailbox_hints| {
            let settings = IwsLiveNodeSettingsV1 {
                world_id,
                writer,
                primary_provider,
                replica_provider,
                primary_policy,
                replica_policy,
                compiler: RuntimeSourceCompiler::default(),
                input_limits: c.input.build(),
                action_limits: c.actions.build(),
                policy: NormalCommitHeaderLimitsV1 {
                    transaction,
                    checkpoint: true,
                },
                max_action_bytes: c.max_action_bytes,
                max_grant_payload_bytes: c.max_grant_payload_bytes,
                cold: IwsLiveNodeColdSettingsV1 {
                    endpoint_paths: [
                        c.primary.endpoint_directory.clone(),
                        c.replica.endpoint_directory.clone(),
                    ],
                    pages_paths: [
                        c.primary.pages_directory.clone(),
                        c.replica.pages_directory.clone(),
                    ],
                    authorization_hint_path: checkpoint.authorization_hint_paths[0].clone(),
                    authority: anchor.authority().clone(),
                    process,
                    configuration: c.runtime_memory.clone(),
                    worker: c.worker.build(),
                    current_limits: current,
                    budgets: c.endpoint_budgets.build(),
                    projection_limits: c.projection.build(),
                    mailbox_hints,
                    checkpoints: [None, None],
                },
            };
            IwsCleanNodeStartV1 {
                node,
                settings,
                checkpoint,
                memory,
                archive,
                initial,
                normal,
                cognition_io: c.cognition_work.cold_io(),
                cognition_work: c.cognition_work.cognition(),
            }
            .open()
        })?;
        if c.start_automatically {
            service.start().map_err(|e| e.to_string())?;
        }
        Ok(Self {
            service,
            observation,
            io,
            transaction,
            checkpoint_directory: c.checkpoint_directory,
            checkpoint_limits,
            max_action_bytes: c.max_action_bytes,
            _node_queue_fee: queue_fee,
            _identity: identity,
        })
    }

    fn run(&mut self) -> Result<(), String> {
        let input = std::io::stdin();
        let output = std::io::stdout();
        let mut input = input.lock();
        let mut output = output.lock();
        self.io.encode(&Reply {
            ok: true,
            kind: if self.service.snapshot().running {
                "local_service_started"
            } else {
                "local_service_stopped"
            },
            data: (),
        })?;
        self.io.emit(&mut output)?;
        loop {
            if !self.io.read(&mut input)? {
                let record = self.service.stop_and_checkpoint_bounded(
                    &self.checkpoint_directory,
                    self.checkpoint_limits,
                )?;
                self.io.encode(&Reply {
                    ok: true,
                    kind: "clean_checkpoint",
                    data: HexIdV1(&record),
                })?;
                self.io.emit(&mut output)?;
                return Ok(());
            }
            let action = self
                .io
                .command()
                .and_then(|c| ActionV1::from_command(c, self.max_action_bytes));
            let result = action
                .map_err(CommandFailureV1::Business)
                .and_then(|action| self.execute(action));
            let complete = finish_command_v1(result, &mut self.io)?;
            self.io.emit(&mut output)?;
            if complete {
                return Ok(());
            }
        }
    }
    fn execute(&mut self, action: ActionV1) -> Result<bool, CommandFailureV1> {
        match action {
            ActionV1::Submit(id, payload) => {
                self.service
                    .submit_consensus_action_payload(id, payload)
                    .map_err(|e| e.to_string())?;
                self.io
                    .encode(&Reply {
                        ok: true,
                        kind: "queued_not_accepted",
                        data: id,
                    })
                    .map_err(CommandFailureV1::Reply)?;
            }
            ActionV1::Status => {
                let state = self.service.snapshot();
                #[derive(Serialize)]
                struct Status<'a> {
                    running: bool,
                    committed_height: u64,
                    execution_height: u64,
                    queued: usize,
                    last_error: &'a Option<String>,
                }
                self.io
                    .encode(&Reply {
                        ok: true,
                        kind: "operational_data",
                        data: Status {
                            running: state.running,
                            committed_height: state.consensus.committed_height,
                            execution_height: state.consensus.last_execution_height,
                            queued: state
                                .consensus
                                .pending_consensus_actions
                                .queued_action_count,
                            last_error: &state.last_error,
                        },
                    })
                    .map_err(CommandFailureV1::Reply)?;
            }
            ActionV1::Observe(sender, receiver) => {
                let Self {
                    service,
                    observation,
                    io,
                    transaction,
                    ..
                } = self;
                let mut reply = None;
                service.with_current_bounded(|primary, _, memory, archive, cancelled| {
                    let observed = observe_current_bounded(
                        primary,
                        &sender,
                        &receiver,
                        *transaction,
                        observation,
                        memory,
                        archive,
                        cancelled,
                    )
                    .map_err(|e| e.to_string())?;
                    reply = Some(io.encode(&Reply {
                        ok: true,
                        kind: "accepted_local_data",
                        data: ObservationReplyV1 {
                            value: &observed,
                            sender: &sender,
                            receiver: &receiver,
                        },
                    }));
                    Ok(())
                })?;
                reply
                    .ok_or_else(|| "accepted observation did not produce a reply".to_owned())?
                    .map_err(CommandFailureV1::Reply)?;
            }
            ActionV1::Start => {
                self.service.start().map_err(|e| e.to_string())?;
                self.io
                    .encode(&Reply {
                        ok: true,
                        kind: "same_service_started",
                        data: (),
                    })
                    .map_err(CommandFailureV1::Reply)?;
            }
            ActionV1::Stop => {
                let record = self.service.stop_and_checkpoint_bounded(
                    &self.checkpoint_directory,
                    self.checkpoint_limits,
                )?;
                self.io
                    .encode(&Reply {
                        ok: true,
                        kind: "clean_checkpoint",
                        data: HexIdV1(&record),
                    })
                    .map_err(CommandFailureV1::Reply)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

enum CommandFailureV1 {
    Business(String),
    // A side effect may already have happened. Never turn a transport failure
    // into a business refusal or automatically resume command processing.
    Reply(String),
}
impl From<String> for CommandFailureV1 {
    fn from(value: String) -> Self {
        Self::Business(value)
    }
}
fn finish_command_v1(
    result: Result<bool, CommandFailureV1>,
    io: &mut LocalIoV1,
) -> Result<bool, String> {
    match result {
        Ok(complete) => Ok(complete),
        Err(CommandFailureV1::Reply(error)) => Err(error),
        Err(CommandFailureV1::Business(error)) => {
            let diagnostic: String = error.chars().take(1024).collect();
            io.encode(&Reply {
                ok: false,
                kind: "refused",
                data: &diagnostic,
            })?;
            Ok(false)
        }
    }
}

enum ActionV1 {
    Submit(u64, Vec<u8>),
    Status,
    Observe(String, String),
    Start,
    Stop,
}

#[cfg(test)]
#[path = "iws_live_tests.rs"]
mod tests;
impl ActionV1 {
    fn from_command(c: CommandV1<'_>, max_action: usize) -> Result<Self, String> {
        c.validate()?;
        Ok(match c.command {
            "submit" => {
                let hex = c.payload_hex.ok_or("payload missing")?;
                if hex.len() % 2 != 0 || hex.len() / 2 > max_action {
                    return Err("action byte bound".into());
                }
                let bytes =
                    hex::decode(hex).map_err(|_| "action must be original CBOR encoded as hex")?;
                Self::Submit(c.action_id.ok_or("action id missing")?, bytes)
            }
            "observe" => {
                let valid = |value: Option<&str>| -> Result<String, String> {
                    let value = value.ok_or("Agent identifier missing")?;
                    if value.is_empty()
                        || value.len() > 256
                        || value.trim() != value
                        || value.chars().any(char::is_control)
                    {
                        return Err("Agent identifier bound".into());
                    }
                    Ok(value.to_owned())
                };
                Self::Observe(valid(c.sender)?, valid(c.receiver)?)
            }
            "status" => Self::Status,
            "start" => Self::Start,
            "stop" => Self::Stop,
            _ => return Err("unsupported local command".into()),
        })
    }
}
#[derive(Serialize)]
struct Reply<T> {
    ok: bool,
    kind: &'static str,
    data: T,
}
struct ObservationReplyV1<'a> {
    value: &'a HostObservationV1,
    sender: &'a str,
    receiver: &'a str,
}
impl Serialize for ObservationReplyV1<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Agent<'a> {
            id: &'a str,
            electricity: Option<i64>,
            data: Option<i64>,
        }
        #[derive(Serialize)]
        struct Observation<'a> {
            record: HexIdV1<'a>, body: HexIdV1<'a>, height: u64,
            sender: Agent<'a>, receiver: Agent<'a>,
            qualified_head_proof: &'a oasis7_formal_protocol::controlled_authority::formal::FormalQualifiedCommitProofV1,
            events: Events<'a>,
        }
        struct Events<'a>(&'a HostObservationV1);
        impl Serialize for Events<'_> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                #[derive(Serialize)]
                struct Event<'a> {
                    ordinal: usize,
                    original: HexIdV1<'a>,
                    event: &'a oasis7_runtime::WorldEvent,
                }
                let mut seq = s.serialize_seq(Some(self.0.events.len()))?;
                for event in self.0.events.iter() {
                    seq.serialize_element(&Event {
                        ordinal: event.ordinal,
                        original: HexIdV1(&event.original),
                        event: event.decoded.event(),
                    })?;
                }
                seq.end()
            }
        }
        let value = self.value;
        Observation {
            record: HexIdV1(&value.record),
            body: HexIdV1(&value.body),
            height: value.height,
            sender: Agent {
                id: self.sender,
                electricity: value.sender.electricity,
                data: value.sender.data,
            },
            receiver: Agent {
                id: self.receiver,
                electricity: value.receiver.electricity,
                data: value.receiver.data,
            },
            qualified_head_proof: value.qualified_proof(),
            events: Events(value),
        }
        .serialize(serializer)
    }
}
