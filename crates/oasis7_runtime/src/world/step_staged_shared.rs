//! Staged shared tick control-state deltas and root assembler for one-root tick execution.
//!
//! This Dark slice stages shared tick control state (`queue`, `schedule`, `publication`,
//! and `consensus`) together with `NextTickClock` into a single `StagedRootTransaction`
//! without modifying any active `World::step*` entrypoint or removing the four outer
//! `World::clone()` guards yet.

use super::step_staged_clock::{NextTickClock, TickClockDelta};
use super::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedRootTransaction,
    StagedTransition, SubsystemDelta, TransitionContext, TransitionReject,
};
use super::{World, WorldError};
use crate::{
    ActionEnvelope, ActionId, CausedBy, RuntimeCommittedTickContext, TickConsensusRecord,
    TickConsensusSubmissionRole, WorldEvent, WorldEventBody, WorldEventId, WorldTime,
};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, VecDeque};

/// Staged pending-action queue dequeue delta installed only in Stage 3.
#[derive(Debug, Clone, Default)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "shared queue delta is staged until tick cutover")
)]
pub(crate) struct PendingActionQueueDelta {
    expected_front_ids: Vec<ActionId>,
}

impl SubsystemDelta for PendingActionQueueDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        let queue = &mut ctx.world_mut().pending_actions;
        for expected_id in self.expected_front_ids {
            let popped = queue.pop_front();
            debug_assert_eq!(popped.as_ref().map(|env| env.id), Some(expected_id));
        }
    }
}

/// Non-mutating Stage 1 cursor over `World.pending_actions`.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "staged queue cursor is dormant until cutover")
)]
pub(crate) struct StagedPendingActionQueue<'a> {
    base: &'a VecDeque<ActionEnvelope>,
    cursor: usize,
    popped_ids: Vec<ActionId>,
}

impl<'a> StagedPendingActionQueue<'a> {
    pub(crate) fn new(base: &'a VecDeque<ActionEnvelope>) -> Self {
        Self {
            base,
            cursor: 0,
            popped_ids: Vec::new(),
        }
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "dormant until adapter/cutover wiring")
    )]
    pub(crate) fn pop_next(&mut self) -> Option<&'a ActionEnvelope> {
        let envelope = self.base.get(self.cursor)?;
        self.cursor += 1;
        self.popped_ids.push(envelope.id);
        Some(envelope)
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "dormant until adapter/cutover wiring")
    )]
    pub(crate) fn remaining(&self) -> usize {
        self.base.len().saturating_sub(self.cursor)
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "dormant until adapter/cutover wiring")
    )]
    pub(crate) fn drained_count(&self) -> usize {
        self.cursor
    }

    pub(crate) fn into_delta(&self) -> PendingActionQueueDelta {
        PendingActionQueueDelta {
            expected_front_ids: self.popped_ids.clone(),
        }
    }
}

/// Staged cognition scheduler & module tick schedule delta installed only in Stage 3.
#[derive(Debug, Clone, Default)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "shared schedule delta is staged but not wired until five-domain tick cutover"
    )
)]
pub(crate) struct CognitionScheduleDelta {
    next_cognition: Option<JsonValue>,
    next_scheduler_cursor: Option<Option<String>>,
    module_tick_schedule_updates: BTreeMap<String, Option<u64>>,
}

impl CognitionScheduleDelta {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "staged schedule prepare is dormant until adapter/cutover wiring"
        )
    )]
    pub(crate) fn prepare(
        _view: &BorrowedWorldView<'_>,
        next_cognition: Option<JsonValue>,
        next_scheduler_cursor: Option<Option<String>>,
        module_tick_schedule_updates: BTreeMap<String, Option<u64>>,
    ) -> Result<Self, TransitionReject> {
        if let Some(cognition) = next_cognition.as_ref() {
            let Some(obj) = cognition.as_object() else {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "staged cognition state must be a JSON object".to_string(),
                }
                .into());
            };
            if let Some(scheduler_state) = obj.get("scheduler_state")
                && !scheduler_state.is_null()
                && !scheduler_state.is_object()
            {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "staged cognition scheduler_state must be a JSON object or null"
                        .to_string(),
                }
                .into());
            }
        }
        Ok(Self {
            next_cognition,
            next_scheduler_cursor,
            module_tick_schedule_updates,
        })
    }
}

impl SubsystemDelta for CognitionScheduleDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        let world = ctx.world_mut();
        if let Some(cognition) = self.next_cognition {
            world.cognition = cognition;
        }
        if let Some(cursor) = self.next_scheduler_cursor {
            world.scheduler_cursor = cursor;
        }
        for (instance_id, next_tick) in self.module_tick_schedule_updates {
            match next_tick {
                Some(wake) => {
                    world.module_tick_schedule.insert(instance_id, wake);
                }
                None => {
                    world.module_tick_schedule.remove(&instance_id);
                }
            }
        }
    }
}

/// Staged journal + event-id allocator delta installed only in Stage 3.
#[derive(Debug, Clone)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "shared publication delta is staged but not wired until five-domain tick cutover"
    )
)]
pub(crate) struct PublicationJournalDelta {
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
}

impl SubsystemDelta for PublicationJournalDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        ctx.install_journal_tail(
            self.next_event_id,
            self.next_event_id_era,
            self.journal_events,
            self.journal_events_evicted,
        );
    }
}

/// Non-mutating Stage 1 journal & event-id allocator overlay.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "staged publication journal is dormant until adapter/cutover wiring"
    )
)]
pub(crate) struct StagedPublicationJournal {
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    max_journal_events: usize,
    journal_events_evicted: u64,
}

impl StagedPublicationJournal {
    pub(crate) fn from_view(view: &BorrowedWorldView<'_>) -> Self {
        let world = view.world();
        Self {
            next_event_id: world.next_event_id,
            next_event_id_era: world.next_event_id_era,
            journal_events: world.journal.events.clone(),
            max_journal_events: world.runtime_memory_limits.max_journal_events.max(1),
            journal_events_evicted: 0,
        }
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "staged event allocation is dormant until adapter/cutover wiring"
        )
    )]
    pub(crate) fn allocate_and_append(
        &mut self,
        tick: WorldTime,
        body: WorldEventBody,
        caused_by: Option<CausedBy>,
    ) -> WorldEventId {
        let (event_id, next_event_id, next_event_id_era) =
            World::preview_next_event_id(self.next_event_id, self.next_event_id_era);
        self.next_event_id = next_event_id;
        self.next_event_id_era = next_event_id_era;
        self.journal_events.push(WorldEvent {
            id: event_id,
            time: tick,
            caused_by,
            body,
        });
        let overflow = self
            .journal_events
            .len()
            .saturating_sub(self.max_journal_events);
        if overflow > 0 {
            self.journal_events.drain(0..overflow);
            self.journal_events_evicted =
                self.journal_events_evicted.saturating_add(overflow as u64);
        }
        event_id
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "staged tick_events projection is dormant until adapter/cutover wiring"
        )
    )]
    pub(crate) fn tick_events(&self, tick: WorldTime) -> Vec<WorldEvent> {
        self.journal_events
            .iter()
            .filter(|event| event.time == tick)
            .cloned()
            .collect()
    }

    pub(crate) fn into_delta(self) -> PublicationJournalDelta {
        PublicationJournalDelta {
            next_event_id: self.next_event_id,
            next_event_id_era: self.next_event_id_era,
            journal_events: self.journal_events,
            journal_events_evicted: self.journal_events_evicted,
        }
    }
}

/// Staged tick consensus record delta validated in Stage 1 and installed in Stage 3.
#[derive(Debug, Clone)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "shared consensus delta is staged but not wired until five-domain tick cutover"
    )
)]
pub(crate) struct TickConsensusDelta {
    record: TickConsensusRecord,
    formal_execution_tick: Option<(RuntimeCommittedTickContext, bool)>,
}

impl TickConsensusDelta {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "staged consensus prepare is dormant until adapter/cutover wiring"
        )
    )]
    pub(crate) fn prepare(
        view: &BorrowedWorldView<'_>,
        tick: WorldTime,
        tick_events: &[WorldEvent],
        state_root: String,
        committed_context: Option<&RuntimeCommittedTickContext>,
        formal_execution_tick: Option<(RuntimeCommittedTickContext, bool)>,
    ) -> Result<Self, TransitionReject> {
        let world = view.world();
        let source_node_id = world
            .validate_tick_consensus_source_node(world.tick_consensus_authority_source.as_str())?;
        let record = world.build_tick_consensus_record_from_events(
            tick,
            source_node_id.as_str(),
            TickConsensusSubmissionRole::Authority,
            committed_context,
            tick_events,
            state_root,
        )?;
        world.validate_formal_tick_immutability(&record)?;
        Ok(Self {
            record,
            formal_execution_tick,
        })
    }
}

impl SubsystemDelta for TickConsensusDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        let world = ctx.world_mut();
        if let Some(index) = world
            .tick_consensus_records
            .iter()
            .position(|existing| existing.block.header.tick == self.record.block.header.tick)
        {
            world.tick_consensus_records[index] = self.record;
        } else {
            world.tick_consensus_records.push(self.record);
        }
        if let Some(formal_tick) = self.formal_execution_tick {
            world.formal_execution_tick = Some(formal_tick);
        }
    }
}

/// Combined root delta for shared tick control state (`clock`, `queue`, `publication`,
/// `schedule`, `consensus`).
#[derive(Debug, Clone, Default)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "combined shared control delta is dormant until five-domain tick cutover"
    )
)]
pub(crate) struct TickSharedControlDelta {
    clock: Option<TickClockDelta>,
    queue: Option<PendingActionQueueDelta>,
    publication: Option<PublicationJournalDelta>,
    schedule: Option<CognitionScheduleDelta>,
    consensus: Option<TickConsensusDelta>,
}

impl SubsystemDelta for TickSharedControlDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        self.clock.apply_infallible(ctx);
        self.queue.apply_infallible(ctx);
        self.publication.apply_infallible(ctx);
        self.schedule.apply_infallible(ctx);
        self.consensus.apply_infallible(ctx);
    }
}

/// Root assembler for shared tick control state over a single `StagedRootTransaction`.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "shared tick assembler is dormant until five-domain tick cutover"
    )
)]
pub(crate) struct StagedTickSharedAssembler<'a> {
    tx: StagedRootTransaction<'a, TickSharedControlDelta>,
    queue: StagedPendingActionQueue<'a>,
    publication: StagedPublicationJournal,
    clock: Option<TickClockDelta>,
    schedule: Option<CognitionScheduleDelta>,
    consensus: Option<TickConsensusDelta>,
}

impl<'a> StagedTickSharedAssembler<'a> {
    pub(crate) fn begin(world: &'a World, ctx: TransitionContext) -> Result<Self, WorldError> {
        let tx = StagedRootTransaction::begin(world, ctx)?;
        let queue = StagedPendingActionQueue::new(&tx.view().world().pending_actions);
        let publication = StagedPublicationJournal::from_view(tx.view());
        Ok(Self {
            tx,
            queue,
            publication,
            clock: None,
            schedule: None,
            consensus: None,
        })
    }

    pub(crate) fn view(&self) -> &BorrowedWorldView<'a> {
        self.tx.view()
    }

    pub(crate) fn context(&self) -> &TransitionContext {
        self.tx.context()
    }

    pub(crate) fn stage_clock(&mut self) -> Result<(), TransitionReject> {
        let prepared = NextTickClock.prepare(self.tx.view(), self.tx.context())?;
        self.clock = Some(*prepared.delta());
        Ok(())
    }

    pub(crate) fn queue_mut(&mut self) -> &mut StagedPendingActionQueue<'a> {
        &mut self.queue
    }

    pub(crate) fn publication_mut(&mut self) -> &mut StagedPublicationJournal {
        &mut self.publication
    }

    pub(crate) fn stage_schedule(
        &mut self,
        next_cognition: Option<JsonValue>,
        next_scheduler_cursor: Option<Option<String>>,
        module_tick_schedule_updates: BTreeMap<String, Option<u64>>,
    ) -> Result<(), TransitionReject> {
        let delta = CognitionScheduleDelta::prepare(
            self.tx.view(),
            next_cognition,
            next_scheduler_cursor,
            module_tick_schedule_updates,
        )?;
        self.schedule = Some(delta);
        Ok(())
    }

    pub(crate) fn stage_consensus(
        &mut self,
        state_root: String,
        committed_context: Option<&RuntimeCommittedTickContext>,
        formal_execution_tick: Option<(RuntimeCommittedTickContext, bool)>,
    ) -> Result<(), TransitionReject> {
        let tick = self.tx.context().tick;
        let tick_events = self.publication.tick_events(tick);
        let delta = TickConsensusDelta::prepare(
            self.tx.view(),
            tick,
            tick_events.as_slice(),
            state_root,
            committed_context,
            formal_execution_tick,
        )?;
        self.consensus = Some(delta);
        Ok(())
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "nested savepoint helper is dormant until adapter/cutover wiring"
        )
    )]
    pub(crate) fn stage_nested_delta(
        &mut self,
        op: impl FnOnce(
            &mut StagedRootTransaction<'a, TickSharedControlDelta>,
        ) -> Result<(), TransitionReject>,
    ) -> Result<(), TransitionReject> {
        self.tx.stage_nested(op)
    }

    pub(crate) fn finish_prepare(mut self) -> PreparedStagedCommit<Vec<TickSharedControlDelta>> {
        let queue_delta = (self.queue.drained_count() > 0).then(|| self.queue.into_delta());
        let publication_delta = Some(self.publication.into_delta());
        self.tx.stage_delta(TickSharedControlDelta {
            clock: self.clock,
            queue: queue_delta,
            publication: publication_delta,
            schedule: self.schedule,
            consensus: self.consensus,
        });
        self.tx.finish_prepare()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::transition::DurableCommitConfirmation;
    use crate::{Action, DomainEvent, RejectReason, WorldEventBody};
    use oasis7_agent_api::world_action::GeoPos;
    use serde_json::json;

    fn sample_action(agent_id: &str) -> Action {
        Action::RegisterAgent {
            agent_id: agent_id.to_string(),
            pos: GeoPos {
                x_cm: 0,
                y_cm: 0,
                z_cm: 0,
            },
        }
    }

    fn rejected_event(action_id: ActionId, note: &str) -> WorldEventBody {
        WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![note.to_string()],
            },
        })
    }

    #[test]
    fn shared_assembler_stages_without_mutation_and_installs_atomically_on_confirmed() {
        let mut world = World::new();
        let a1 = world.submit_action(sample_action("agent-1"));
        let a2 = world.submit_action(sample_action("agent-2"));
        let next_tick = world.state().time.saturating_add(1);
        let original_next_event_id = world.next_event_id;
        let original_consensus_len = world.tick_consensus_records.len();

        let prepared = {
            let mut assembler =
                StagedTickSharedAssembler::begin(&world, TransitionContext::new(next_tick, None))
                    .expect("begin shared assembler");
            assembler.stage_clock().expect("stage clock");

            let first = assembler.queue_mut().pop_next().expect("first action");
            assert_eq!(first.id, a1);
            let second = assembler.queue_mut().pop_next().expect("second action");
            assert_eq!(second.id, a2);
            assert_eq!(assembler.queue_mut().remaining(), 0);

            assembler.publication_mut().allocate_and_append(
                next_tick,
                rejected_event(a1, "staged-1"),
                Some(CausedBy::Action(a1)),
            );
            assembler.publication_mut().allocate_and_append(
                next_tick,
                rejected_event(a2, "staged-2"),
                Some(CausedBy::Action(a2)),
            );

            let mut schedule_updates = BTreeMap::new();
            schedule_updates.insert("mod.alpha".to_string(), Some(next_tick + 5));
            assembler
                .stage_schedule(
                    Some(json!({
                        "scheduler_state": { "cursor_seq": 1 },
                        "events": []
                    })),
                    Some(Some("cursor-1".to_string())),
                    schedule_updates,
                )
                .expect("stage schedule");

            let state_root = assembler
                .view()
                .world()
                .current_state_root_hash()
                .expect("state root");
            assembler
                .stage_consensus(state_root, None, None)
                .expect("stage consensus");
            assembler.finish_prepare()
        };

        // Stage 1 must not mutate canonical World.
        assert_eq!(world.state().time, 0);
        assert_eq!(world.pending_actions.len(), 2);
        assert_eq!(world.journal.events.len(), 0);
        assert_eq!(world.next_event_id, original_next_event_id);
        assert_eq!(world.scheduler_cursor, None);
        assert!(world.module_tick_schedule.is_empty());
        assert_eq!(world.tick_consensus_records.len(), original_consensus_len);

        prepared
            .install_confirmed(&mut world, DurableCommitConfirmation::Confirmed)
            .expect("install confirmed shared tick delta");

        assert_eq!(world.state().time, next_tick);
        assert_eq!(world.pending_actions.len(), 0);
        assert_eq!(world.journal.events.len(), 2);
        assert_eq!(world.next_event_id, original_next_event_id + 2);
        assert_eq!(world.scheduler_cursor.as_deref(), Some("cursor-1"));
        assert_eq!(
            world.module_tick_schedule.get("mod.alpha").copied(),
            Some(next_tick + 5)
        );
        assert_eq!(
            world.tick_consensus_records.len(),
            original_consensus_len + 1
        );
        assert_eq!(
            world
                .tick_consensus_records
                .last()
                .map(|record| record.block.header.tick),
            Some(next_tick)
        );
    }

    #[test]
    fn shared_assembler_stage1_failure_and_nested_savepoint_leave_world_untouched() {
        let mut world = World::new();
        let next_tick = world.state().time.saturating_add(1);

        // 1. Invalid cognition schedule JSON rejected in Stage 1.
        {
            let mut assembler =
                StagedTickSharedAssembler::begin(&world, TransitionContext::new(next_tick, None))
                    .expect("begin");
            let err = assembler
                .stage_schedule(Some(json!("not-an-object")), None, BTreeMap::new())
                .expect_err("non-object cognition must fail Stage 1");
            assert!(matches!(
                err,
                TransitionReject::World(WorldError::DistributedValidationFailed { .. })
            ));
        }

        // 2. Untrusted consensus authority source rejected in Stage 1.
        world.tick_consensus_authority_source = "untrusted.node".to_string();
        {
            let mut assembler =
                StagedTickSharedAssembler::begin(&world, TransitionContext::new(next_tick, None))
                    .expect("begin");
            let state_root = world.current_state_root_hash().expect("state root");
            let err = assembler
                .stage_consensus(state_root, None, None)
                .expect_err("untrusted authority must fail Stage 1");
            assert!(matches!(
                err,
                TransitionReject::World(WorldError::DistributedValidationFailed { .. })
            ));
        }

        // 3. Nested savepoint rollback discards failed nested delta while preserving prior staged work.
        let mut assembler =
            StagedTickSharedAssembler::begin(&world, TransitionContext::new(next_tick, None))
                .expect("begin");
        assembler.stage_clock().expect("stage clock");
        let nested_err = assembler
            .stage_nested_delta(|tx| {
                tx.stage_delta(TickSharedControlDelta::default());
                Err(TransitionReject::World(
                    WorldError::DistributedValidationFailed {
                        reason: "injected Stage 1 nested failure".to_string(),
                    },
                ))
            })
            .expect_err("nested failure must propagate");
        assert!(matches!(
            nested_err,
            TransitionReject::World(WorldError::DistributedValidationFailed { .. })
        ));
        let prepared = assembler.finish_prepare();
        assert_eq!(prepared.delta().len(), 1);
        assert_eq!(world.state().time, 0);
    }

    #[test]
    fn shared_assembler_stage2_status_unknown_fences_without_mutation() {
        let mut world = World::new();
        let a1 = world.submit_action(sample_action("agent-fenced"));
        let next_tick = world.state().time.saturating_add(1);
        let snapshot_before = world.snapshot();

        let prepared = {
            let mut assembler =
                StagedTickSharedAssembler::begin(&world, TransitionContext::new(next_tick, None))
                    .expect("begin");
            assembler.stage_clock().expect("stage clock");
            let popped = assembler.queue_mut().pop_next().expect("queued action");
            assert_eq!(popped.id, a1);
            assembler.publication_mut().allocate_and_append(
                next_tick,
                rejected_event(a1, "fenced"),
                Some(CausedBy::Action(a1)),
            );
            let state_root = world.current_state_root_hash().expect("state root");
            assembler
                .stage_consensus(state_root, None, None)
                .expect("stage consensus");
            assembler.finish_prepare()
        };

        let result = prepared.install_confirmed(
            &mut world,
            DurableCommitConfirmation::StatusUnknown {
                reason: "durable persistence outcome unknown".to_string(),
            },
        );
        assert!(matches!(
            result,
            Err(TransitionReject::StatusUnknownFenced { .. })
        ));
        assert_eq!(world.snapshot(), snapshot_before);
        assert_eq!(world.state().time, 0);
        assert_eq!(world.pending_actions.len(), 1);
        assert!(world.journal.events.is_empty());
        assert!(world.tick_consensus_records.is_empty());
    }

    #[test]
    fn shared_assembler_stage2_rejects_stale_base_head_across_shared_subsystems() {
        let mut world = World::new();
        let a1 = world.submit_action(sample_action("agent-stale"));
        let next_tick = world.state().time.saturating_add(1);

        let prepared = {
            let mut assembler =
                StagedTickSharedAssembler::begin(&world, TransitionContext::new(next_tick, None))
                    .expect("begin");
            assembler.stage_clock().expect("stage clock");
            let _ = assembler.queue_mut().pop_next();
            assembler.publication_mut().allocate_and_append(
                next_tick,
                rejected_event(a1, "stale-check"),
                Some(CausedBy::Action(a1)),
            );
            assembler.finish_prepare()
        };

        // Mutate shared queue after Stage 1 prepare.
        let a2 = world.submit_action(sample_action("agent-concurrent"));
        let result = prepared.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed);
        assert!(matches!(result, Err(TransitionReject::BaseHeadStale)));
        assert_eq!(world.state().time, 0);
        assert_eq!(world.pending_actions.len(), 2);
        assert_eq!(world.pending_actions.back().map(|env| env.id), Some(a2));
        assert!(world.journal.events.is_empty());
    }

    #[test]
    fn shared_assembler_journal_eviction_tracks_backpressure_stats_in_stage3() {
        let mut world = World::new();
        world.runtime_memory_limits.max_journal_events = 2;
        let next_tick = world.state().time.saturating_add(1);

        let prepared = {
            let mut assembler =
                StagedTickSharedAssembler::begin(&world, TransitionContext::new(next_tick, None))
                    .expect("begin");
            assembler.stage_clock().expect("stage clock");
            for idx in 1..=3 {
                assembler.publication_mut().allocate_and_append(
                    next_tick,
                    rejected_event(idx, "evict-test"),
                    None,
                );
            }
            assembler.finish_prepare()
        };

        assert_eq!(world.runtime_backpressure_stats.journal_events_evicted, 0);
        prepared
            .install_confirmed(&mut world, DurableCommitConfirmation::Confirmed)
            .expect("install confirmed");
        assert_eq!(world.journal.events.len(), 2);
        assert_eq!(world.runtime_backpressure_stats.journal_events_evicted, 1);
    }
}
