//! Fixed OutcomeRecorded original sink after the real authorization-root CAS.
//! No raw event, replacement root or caller permission flag is accepted.
use super::*;
use crate::world::incremental_authorization_root_current::{RootPublishedOutcomeV1, WarmError};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
};
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[derive(Debug)]
pub(in crate::world) enum AuthorizationOutcomePublishErrorV1 {
    Projection(WarmError),
    Body(BodyMutationError),
}
impl From<WarmError> for AuthorizationOutcomePublishErrorV1 {
    fn from(e: WarmError) -> Self {
        Self::Projection(e)
    }
}
impl From<BodyMutationError> for AuthorizationOutcomePublishErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<oasis7_formal_protocol::incremental_execution::ExecutionError>
    for AuthorizationOutcomePublishErrorV1
{
    fn from(e: oasis7_formal_protocol::incremental_execution::ExecutionError) -> Self {
        Self::Body(e.into())
    }
}
impl From<FormalError> for AuthorizationOutcomePublishErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Body(BodyMutationError::Original(Box::new(e)))
    }
}
impl From<ValueTreeError> for AuthorizationOutcomePublishErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Body(e.into())
    }
}

/// Owns the genuine source proof and exact event bytes through downstream
/// routing. The constructor is only the fixed, counted original sink below.
pub(in crate::world) struct PublishedAuthorizationOutcomeEventV1 {
    event: WorldEvent,
    _source: RootPublishedOutcomeV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _fee: ReservedValue<()>,
}
impl PublishedAuthorizationOutcomeEventV1 {
    pub(in crate::world) fn event(&self) -> &WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), AuthorizationOutcomePublishErrorV1> {
        common.consume_steps(1)?;
        tx.validate_guard(&self.guard, live)
            .map_err(BodyMutationError::from)?;
        if self.core != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(
                BodyMutationError::Invalid("SafeAlternative published original binding").into(),
            );
        }
        Ok(())
    }
    pub(in crate::world) fn persisted<'a, M: MerkleNodeReader>(
        &'a self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<PersistedWorldEventV1<'a>, AuthorizationOutcomePublishErrorV1> {
        self.validate(tx, common, live)?;
        Ok(PersistedWorldEventV1 {
            owner: PersistedEventOwnerV1::Module {
                event: &self.event,
                identity: tx.guard(),
            },
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn publish_safe_alternative_outcome_event_v1<M, V, W, C, F>(
    source: RootPublishedOutcomeV1,
    now: u64,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    tx: &mut ExecutionTransaction<'_, M>,
    events: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<PublishedAuthorizationOutcomeEventV1, AuthorizationOutcomePublishErrorV1>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = tx.contribute(|tx| {
        source.validate(tx, common, live)?;
        if !events.mutation.memory.same_pool(common.memory()) {
            return Err(BodyMutationError::Invalid("SafeAlternative original sink pool").into());
        }
        let crate::CapabilityAuthorizationEvent::SafeAlternativeOutcomeRecorded {
            key,
            action_id,
            event_id,
        } = source.authorization_event(tx, common, live)?
        else {
            return Err(
                BodyMutationError::Invalid("SafeAlternative fixed outcome original").into(),
            );
        };
        let bytes = std::mem::size_of::<PublishedAuthorizationOutcomeEventV1>()
            .checked_add(key.len())
            .ok_or(ValueTreeError::ResourceLimited(
                "SafeAlternative original owner layout",
            ))?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        common.charge(4, key.len() as u64)?;
        let mut owned_key = String::with_capacity(key.len());
        if owned_key.capacity() > key.len() {
            return Err(
                ValueTreeError::ResourceLimited("SafeAlternative original key capacity").into(),
            );
        }
        owned_key.push_str(key);
        let mut event = WorldEvent {
            id: 0,
            time: now,
            caused_by: None,
            body: WorldEventBody::CapabilityAuthorization(
                crate::CapabilityAuthorizationEvent::SafeAlternativeOutcomeRecorded {
                    key: owned_key,
                    action_id: *action_id,
                    event_id: *event_id,
                },
            ),
        };
        let (_, meter, stop) = common.parts();
        let child = meter.begin_child()?;
        let visits = child.remaining_steps() / 2;
        let bytes = child.remaining_read_bytes() / 3;
        events.mutation.io.nodes_remaining = events.mutation.io.nodes_remaining.min(visits);
        events.original_work.visits_remaining = events.original_work.visits_remaining.min(visits);
        events.mutation.io.read_bytes_remaining =
            events.mutation.io.read_bytes_remaining.min(bytes);
        events.mutation.io.write_bytes_remaining =
            events.mutation.io.write_bytes_remaining.min(bytes);
        events.original_work.bytes_remaining = events.original_work.bytes_remaining.min(bytes);
        let before_steps = [
            events.mutation.io.nodes_remaining,
            events.original_work.visits_remaining,
        ];
        let before_bytes = [
            events.mutation.io.read_bytes_remaining,
            events.mutation.io.write_bytes_remaining,
            events.original_work.bytes_remaining,
        ];
        let persisted = {
            let mut scoped = ModuleOutputEventContextV1 {
                values: events.values,
                world_events: events.world_events,
                mutation: BodyMutationContext {
                    sink: &mut *events.mutation.sink,
                    memory: events.mutation.memory,
                    io: &mut *events.mutation.io,
                    cancelled: || (events.mutation.cancelled)() || stop(),
                },
                persist_original: &mut events.persist_original,
                max_event_bytes: events.max_event_bytes,
                original_work: CanonicalCborWorkV1 {
                    visits_remaining: events.original_work.visits_remaining,
                    bytes_remaining: events.original_work.bytes_remaining,
                    max_depth: events.original_work.max_depth,
                    max_map_keys: events.original_work.max_map_keys,
                    max_key_bytes: events.original_work.max_key_bytes,
                },
            };
            let result = (|| {
                let seq = World::read_iws_allocator(
                    super::super::incremental_allocators::RuntimeAllocatorV1::Event,
                    tx,
                    scoped.values,
                    &mut scoped.mutation,
                )?;
                let (id, _, _) = World::preview_next_event_id(seq.next, seq.era);
                event.id = id;
                persist_counted_event_original_v1(now, &event, tx, &mut scoped).map(|_| ())
            })();
            events.original_work = scoped.original_work;
            result
        };
        let after_steps = [
            events.mutation.io.nodes_remaining,
            events.original_work.visits_remaining,
        ];
        let after_bytes = [
            events.mutation.io.read_bytes_remaining,
            events.mutation.io.write_bytes_remaining,
            events.original_work.bytes_remaining,
        ];
        let debit = |before: &[u64], after: &[u64]| {
            before
                .iter()
                .zip(after)
                .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
        };
        let Some(steps) = debit(&before_steps, &after_steps) else {
            return Err(
                BodyMutationError::Invalid("SafeAlternative original steps accounting").into(),
            );
        };
        let Some(bytes) = debit(&before_bytes, &after_bytes) else {
            return Err(
                BodyMutationError::Invalid("SafeAlternative original bytes accounting").into(),
            );
        };
        let paid = child.consume(steps, bytes);
        child.finish();
        persisted?;
        paid?;
        common.consume_steps(0)?;
        let guard = tx.guard();
        tx.validate_guard(&guard, live)
            .map_err(BodyMutationError::from)?;
        Ok((event, guard, fee.retain(())))
    });
    match result {
        Ok((event, guard, fee)) => Ok(PublishedAuthorizationOutcomeEventV1 {
            event,
            _source: source,
            guard,
            core: common.work().identity(),
            _fee: fee,
        }),
        Err(error) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> = tx
                .contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            Err(error)
        }
    }
}
