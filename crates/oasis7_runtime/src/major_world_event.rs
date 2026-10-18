//! Authoritative, additive projection for player-readable major world events.
//!
//! The projection is deliberately derived from the durable runtime journal
//! instead of being added to [`WorldEvent`].  This keeps existing journal
//! hashes, replay inputs, and snapshot layouts stable while making the
//! authority boundary explicit for consumers such as the Viewer.

use super::events::{CausedBy, DomainEvent};
use super::gameplay_state::{CrisisState, CrisisStatus};
use super::types::{ActionId, WorldEventId, WorldTime};
use super::{WorldEvent, WorldEventBody};
use oasis7_agent_api::world_action::GeoPos;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAJOR_WORLD_EVENT_PROJECTION_SCHEMA_VERSION: &str = "major_world_event/v1";

/// The event classes admitted by the current producer contract.
///
/// Keep this allowlist intentionally narrow.  Adding a category requires a
/// producer-owned authority decision; arbitrary journal payloads must not
/// become attention-worthy events merely because they have display text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MajorWorldEventCategory {
    Crisis,
}

/// Normalized lifecycle for a crisis event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MajorWorldEventLifecycle {
    Active,
    Resolved,
    TimedOut,
}

/// Freshness is supplied by the source read, never inferred from wall-clock
/// age, event text, or the event sequence itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MajorWorldEventFreshness {
    Current,
    LastKnown,
    Stale,
    Unknown,
    Conflict,
    Reconnecting,
    Unavailable,
}

/// Read-stream state controls whether an event list is safe to consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MajorWorldEventStreamState {
    Current,
    Replay,
    Gap,
    Reorg,
    Unavailable,
}

/// Permission is an explicit input to projection.  `current()` intentionally
/// creates an unknown permission and therefore cannot emit a projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MajorWorldEventVisibilityPermission {
    Public,
    Restricted,
    Denied,
    Unknown,
}

/// Stable event identity, including the reorg boundary that scopes its
/// sequence.  `event_seq` is the durable `WorldEvent.id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MajorWorldEventIdentity {
    pub world_id: String,
    pub reorg_epoch: u64,
    pub event_seq: WorldEventId,
}

/// The runtime journal is the only source currently admitted by this
/// projection.  The event kind is retained as provenance, not as a semantic
/// inference input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MajorWorldEventSource {
    pub authority: MajorWorldEventSourceAuthority,
    pub event_kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MajorWorldEventSourceAuthority {
    RuntimeJournal,
}

/// Optional explicit causal lineage.  This is not an Action Receipt and must
/// never be rendered as one by a consumer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum MajorWorldEventCausalReference {
    Action(ActionId),
    Effect { intent_id: String },
}

/// A crisis is currently world-scoped and has no authoritative spatial
/// position.  The logical crisis id is retained for feed/toast continuity;
/// `position` remains `None`, so this can never drive a stage marker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MajorWorldEventAnchor {
    pub scope: MajorWorldEventAnchorScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<GeoPos>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MajorWorldEventAnchorScope {
    World,
}

/// Explicit projection input.  Callers must supply both freshness and
/// visibility; no constructor silently grants either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MajorWorldEventProjectionContext {
    pub world_id: String,
    pub reorg_epoch: u64,
    pub freshness: MajorWorldEventFreshness,
    pub visibility: MajorWorldEventVisibilityPermission,
    pub stream_state: MajorWorldEventStreamState,
}

impl MajorWorldEventProjectionContext {
    /// Current source read with unknown permission.  This is intentionally
    /// fail-closed and is useful for callers that need to complete the
    /// context from an explicit auth decision.
    pub fn current(world_id: impl Into<String>, reorg_epoch: u64) -> Self {
        Self::new(
            world_id,
            reorg_epoch,
            MajorWorldEventFreshness::Current,
            MajorWorldEventVisibilityPermission::Unknown,
            MajorWorldEventStreamState::Current,
        )
    }

    pub fn current_public(world_id: impl Into<String>, reorg_epoch: u64) -> Self {
        Self::new(
            world_id,
            reorg_epoch,
            MajorWorldEventFreshness::Current,
            MajorWorldEventVisibilityPermission::Public,
            MajorWorldEventStreamState::Current,
        )
    }

    pub fn replay_public(world_id: impl Into<String>, reorg_epoch: u64) -> Self {
        Self::new(
            world_id,
            reorg_epoch,
            MajorWorldEventFreshness::LastKnown,
            MajorWorldEventVisibilityPermission::Public,
            MajorWorldEventStreamState::Replay,
        )
    }

    pub fn gap(world_id: impl Into<String>, reorg_epoch: u64) -> Self {
        Self::new(
            world_id,
            reorg_epoch,
            MajorWorldEventFreshness::Unknown,
            MajorWorldEventVisibilityPermission::Unknown,
            MajorWorldEventStreamState::Gap,
        )
    }

    pub fn reorg(world_id: impl Into<String>, reorg_epoch: u64) -> Self {
        Self::new(
            world_id,
            reorg_epoch,
            MajorWorldEventFreshness::Unknown,
            MajorWorldEventVisibilityPermission::Unknown,
            MajorWorldEventStreamState::Reorg,
        )
    }

    pub fn with_freshness(
        world_id: impl Into<String>,
        reorg_epoch: u64,
        freshness: MajorWorldEventFreshness,
    ) -> Self {
        Self::new(
            world_id,
            reorg_epoch,
            freshness,
            MajorWorldEventVisibilityPermission::Unknown,
            match freshness {
                MajorWorldEventFreshness::LastKnown => MajorWorldEventStreamState::Replay,
                _ => MajorWorldEventStreamState::Current,
            },
        )
    }

    pub fn with_permission(
        world_id: impl Into<String>,
        reorg_epoch: u64,
        visibility: MajorWorldEventVisibilityPermission,
    ) -> Self {
        Self::new(
            world_id,
            reorg_epoch,
            MajorWorldEventFreshness::Current,
            visibility,
            MajorWorldEventStreamState::Current,
        )
    }

    pub fn new(
        world_id: impl Into<String>,
        reorg_epoch: u64,
        freshness: MajorWorldEventFreshness,
        visibility: MajorWorldEventVisibilityPermission,
        stream_state: MajorWorldEventStreamState,
    ) -> Self {
        Self {
            world_id: world_id.into(),
            reorg_epoch,
            freshness,
            visibility,
            stream_state,
        }
    }

    fn allows_projection(&self) -> bool {
        !self.world_id.trim().is_empty()
            && matches!(
                self.visibility,
                MajorWorldEventVisibilityPermission::Public
                    | MajorWorldEventVisibilityPermission::Restricted
            )
            && !matches!(
                self.freshness,
                MajorWorldEventFreshness::Unknown
                    | MajorWorldEventFreshness::Conflict
                    | MajorWorldEventFreshness::Reconnecting
                    | MajorWorldEventFreshness::Unavailable
            )
            && matches!(
                self.stream_state,
                MajorWorldEventStreamState::Current | MajorWorldEventStreamState::Replay
            )
    }
}

/// Additive authoritative projection consumed by event presentation layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MajorWorldEventProjection {
    pub schema_version: String,
    pub identity: MajorWorldEventIdentity,
    pub category: MajorWorldEventCategory,
    /// Opaque canonical crisis kind; it is not a category or severity label.
    pub subtype: String,
    /// Original producer severity.  The crisis contract is 1..=5; no
    /// qualitative mapping is performed here.
    pub severity: u32,
    pub lifecycle: MajorWorldEventLifecycle,
    pub source: MajorWorldEventSource,
    pub freshness: MajorWorldEventFreshness,
    pub visibility: MajorWorldEventVisibilityPermission,
    pub logical_time: WorldTime,
    pub causal_reference: Option<MajorWorldEventCausalReference>,
    pub world_anchor: Option<MajorWorldEventAnchor>,
}

/// Project all complete, authoritative crisis events in deterministic journal
/// order.  Equivalent duplicate event ids are deduplicated; a materially
/// conflicting payload for one identity is rejected rather than selecting an
/// arbitrary duplicate.
#[cfg(test)]
pub(crate) fn project_major_world_events(
    events: &[WorldEvent],
    context: &MajorWorldEventProjectionContext,
) -> Vec<MajorWorldEventProjection> {
    if !context.allows_projection() {
        return Vec::new();
    }

    let (unique_events, conflicted_crises) = unique_events_and_conflicted_crises(events);
    let history_index = CrisisHistoryIndex::build(&unique_events);

    unique_events
        .into_iter()
        .filter_map(|event| {
            let crisis_id = crisis_id(event)?;
            if conflicted_crises.contains(crisis_id) {
                return None;
            }
            project_major_world_event(event, &history_index, context)
        })
        .collect()
}

/// Project crisis lifecycle rows only after joining them to the canonical
/// persisted crisis snapshot. The journal supplies identity/order; the
/// snapshot supplies the authoritative kind, severity, lifecycle and times.
pub fn project_major_world_events_with_canonical_state(
    events: &[WorldEvent],
    canonical_crises: &BTreeMap<String, CrisisState>,
    context: &MajorWorldEventProjectionContext,
) -> Vec<MajorWorldEventProjection> {
    project_major_world_events_with_canonical_state_indexed(events, canonical_crises, context).0
}

fn project_major_world_events_with_canonical_state_indexed(
    events: &[WorldEvent],
    canonical_crises: &BTreeMap<String, CrisisState>,
    context: &MajorWorldEventProjectionContext,
) -> (Vec<MajorWorldEventProjection>, CrisisHistoryIndex) {
    if !context.allows_projection() {
        return (Vec::new(), CrisisHistoryIndex::default());
    }

    let (unique_events, conflicted_crises) = unique_events_and_conflicted_crises(events);
    let history_index = CrisisHistoryIndex::build(&unique_events);

    let mut lifecycle_by_crisis: BTreeMap<&str, Vec<&WorldEvent>> = BTreeMap::new();
    for event in &unique_events {
        if let Some(crisis_id) = crisis_id(event) {
            lifecycle_by_crisis
                .entry(crisis_id)
                .or_default()
                .push(*event);
        }
    }

    let valid_authority: BTreeMap<&str, (&CrisisState, WorldEventId)> = lifecycle_by_crisis
        .iter()
        .filter_map(|(crisis_id, lifecycle)| {
            if conflicted_crises.contains(*crisis_id) {
                return None;
            }
            let canonical = canonical_crises.get(*crisis_id)?;
            let latest = validate_canonical_crisis(crisis_id, lifecycle, canonical)?;
            Some((*crisis_id, (canonical, latest)))
        })
        .collect();

    let projections = unique_events
        .into_iter()
        .filter_map(|event| {
            let crisis_id = crisis_id(event)?;
            if conflicted_crises.contains(crisis_id) {
                return None;
            }
            let (canonical, latest_event_id) = valid_authority.get(crisis_id)?;
            let mut projection = project_major_world_event(event, &history_index, context)?;
            projection.subtype = canonical.kind.clone();
            projection.severity = canonical.severity;
            projection.freshness = if context.stream_state == MajorWorldEventStreamState::Replay {
                MajorWorldEventFreshness::LastKnown
            } else if event.id == *latest_event_id {
                context.freshness
            } else {
                MajorWorldEventFreshness::LastKnown
            };
            Some(projection)
        })
        .collect::<Vec<_>>();

    (projections, history_index)
}

fn unique_events_and_conflicted_crises(
    events: &[WorldEvent],
) -> (Vec<&WorldEvent>, BTreeSet<String>) {
    let mut by_id: BTreeMap<WorldEventId, Vec<&WorldEvent>> = BTreeMap::new();
    for event in events {
        by_id.entry(event.id).or_default().push(event);
    }

    let mut conflicted_crises = BTreeSet::new();
    let mut unique_events = Vec::with_capacity(by_id.len());
    for (event_id, candidates) in by_id {
        let Some(first) = candidates.first().copied() else {
            continue;
        };
        if event_id == 0 || candidates.iter().any(|candidate| *candidate != first) {
            for candidate in candidates {
                if let Some(crisis_id) = crisis_id(candidate) {
                    conflicted_crises.insert(crisis_id.to_string());
                }
            }
            continue;
        }
        unique_events.push(first);
    }
    (unique_events, conflicted_crises)
}

#[derive(Default)]
struct CrisisHistoryIndex {
    by_crisis: BTreeMap<String, CrisisHistoryMetadata>,
    #[cfg(test)]
    indexed_event_count: usize,
}

#[derive(Default)]
struct CrisisHistoryMetadata {
    spawn: Option<CrisisSpawnMetadata>,
    multiple_spawns: bool,
}

struct CrisisSpawnMetadata {
    event_id: WorldEventId,
    kind: String,
    severity: u32,
}

impl CrisisHistoryIndex {
    fn build(events: &[&WorldEvent]) -> Self {
        let mut index = Self {
            #[cfg(test)]
            indexed_event_count: events.len(),
            ..Self::default()
        };
        for event in events {
            let WorldEventBody::Domain(DomainEvent::CrisisSpawned {
                crisis_id,
                kind,
                severity,
                ..
            }) = &event.body
            else {
                continue;
            };
            let metadata = index.by_crisis.entry(crisis_id.clone()).or_default();
            if metadata.spawn.is_some() {
                metadata.multiple_spawns = true;
            } else {
                metadata.spawn = Some(CrisisSpawnMetadata {
                    event_id: event.id,
                    kind: kind.clone(),
                    severity: *severity,
                });
            }
        }
        index
    }

    fn metadata(&self, crisis_id: &str) -> Option<&CrisisHistoryMetadata> {
        self.by_crisis.get(crisis_id)
    }

    fn is_consistent(&self, crisis_id: &str) -> bool {
        self.metadata(crisis_id)
            .is_some_and(CrisisHistoryMetadata::is_consistent)
    }

    fn prior_kind(&self, crisis_id: &str, event_id: WorldEventId) -> Option<String> {
        self.metadata(crisis_id)?.prior_kind(event_id)
    }

    fn prior_severity(&self, crisis_id: &str, event_id: WorldEventId) -> Option<u32> {
        self.metadata(crisis_id)?.prior_severity(event_id)
    }
}

impl CrisisHistoryMetadata {
    fn is_consistent(&self) -> bool {
        !self.multiple_spawns
            && self
                .spawn
                .as_ref()
                .is_some_and(|spawn| (1..=5).contains(&spawn.severity))
    }

    fn prior_kind(&self, event_id: WorldEventId) -> Option<String> {
        let spawn = self.spawn.as_ref()?;
        (spawn.event_id < event_id && !spawn.kind.trim().is_empty()).then(|| spawn.kind.clone())
    }

    fn prior_severity(&self, event_id: WorldEventId) -> Option<u32> {
        let spawn = self.spawn.as_ref()?;
        (spawn.event_id < event_id && (1..=5).contains(&spawn.severity)).then_some(spawn.severity)
    }
}

#[cfg(test)]
pub(crate) fn project_major_world_events_with_canonical_state_indexed_for_test(
    events: &[WorldEvent],
    canonical_crises: &BTreeMap<String, CrisisState>,
    context: &MajorWorldEventProjectionContext,
) -> (Vec<MajorWorldEventProjection>, usize) {
    let (projections, history_index) =
        project_major_world_events_with_canonical_state_indexed(events, canonical_crises, context);
    (projections, history_index.indexed_event_count)
}

fn crisis_id(event: &WorldEvent) -> Option<&str> {
    match &event.body {
        WorldEventBody::Domain(
            DomainEvent::CrisisSpawned { crisis_id, .. }
            | DomainEvent::CrisisResolved { crisis_id, .. }
            | DomainEvent::CrisisTimedOut { crisis_id, .. },
        ) => Some(crisis_id),
        _ => None,
    }
}

fn validate_canonical_crisis(
    crisis_id: &str,
    lifecycle: &[&WorldEvent],
    canonical: &CrisisState,
) -> Option<WorldEventId> {
    if crisis_id.trim().is_empty()
        || canonical.crisis_id != crisis_id
        || canonical.kind.trim().is_empty()
        || !(1..=5).contains(&canonical.severity)
    {
        return None;
    }
    let spawns: Vec<_> = lifecycle
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::CrisisSpawned {
                kind,
                severity,
                expires_at,
                ..
            }) => Some((*event, kind, *severity, *expires_at)),
            _ => None,
        })
        .collect();
    let [(spawn, kind, severity, expires_at)] = spawns.as_slice() else {
        return None;
    };
    if *kind != &canonical.kind
        || *severity != canonical.severity
        || spawn.time != canonical.opened_at
        || *expires_at != canonical.expires_at
    {
        return None;
    }
    let resolved: Vec<_> = lifecycle
        .iter()
        .copied()
        .filter(|event| {
            matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::CrisisResolved { .. })
            )
        })
        .collect();
    let timed_out: Vec<_> = lifecycle
        .iter()
        .copied()
        .filter(|event| {
            matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::CrisisTimedOut { .. })
            )
        })
        .collect();
    match canonical.status {
        CrisisStatus::Active
            if resolved.is_empty() && timed_out.is_empty() && canonical.resolved_at.is_none() =>
        {
            Some(spawn.id)
        }
        CrisisStatus::Resolved if resolved.len() == 1 && timed_out.is_empty() => {
            let terminal = resolved[0];
            (terminal.id > spawn.id && canonical.resolved_at == Some(terminal.time))
                .then_some(terminal.id)
        }
        CrisisStatus::TimedOut if timed_out.len() == 1 && resolved.is_empty() => {
            let terminal = timed_out[0];
            (terminal.id > spawn.id && canonical.resolved_at == Some(terminal.time))
                .then_some(terminal.id)
        }
        _ => None,
    }
}

/// Project one event when its complete crisis authority is present in the
/// supplied journal history.
fn project_major_world_event(
    event: &WorldEvent,
    history: &CrisisHistoryIndex,
    context: &MajorWorldEventProjectionContext,
) -> Option<MajorWorldEventProjection> {
    if !context.allows_projection() || event.id == 0 {
        return None;
    }

    let (crisis_id, subtype, severity, lifecycle, event_kind) = match &event.body {
        WorldEventBody::Domain(DomainEvent::CrisisSpawned {
            crisis_id,
            kind,
            severity,
            ..
        }) if !kind.trim().is_empty() && (1..=5).contains(severity) => (
            crisis_id,
            kind.clone(),
            *severity,
            MajorWorldEventLifecycle::Active,
            "crisis_spawned",
        ),
        WorldEventBody::Domain(DomainEvent::CrisisResolved { crisis_id, .. }) => (
            crisis_id,
            history.prior_kind(crisis_id, event.id)?,
            history.prior_severity(crisis_id, event.id)?,
            MajorWorldEventLifecycle::Resolved,
            "crisis_resolved",
        ),
        WorldEventBody::Domain(DomainEvent::CrisisTimedOut { crisis_id, .. }) => (
            crisis_id,
            history.prior_kind(crisis_id, event.id)?,
            history.prior_severity(crisis_id, event.id)?,
            MajorWorldEventLifecycle::TimedOut,
            "crisis_timed_out",
        ),
        _ => return None,
    };
    if crisis_id.trim().is_empty() {
        return None;
    }
    if !history.is_consistent(crisis_id) {
        return None;
    }

    Some(MajorWorldEventProjection {
        schema_version: MAJOR_WORLD_EVENT_PROJECTION_SCHEMA_VERSION.to_string(),
        identity: MajorWorldEventIdentity {
            world_id: context.world_id.clone(),
            reorg_epoch: context.reorg_epoch,
            event_seq: event.id,
        },
        category: MajorWorldEventCategory::Crisis,
        subtype,
        severity,
        lifecycle,
        source: MajorWorldEventSource {
            authority: MajorWorldEventSourceAuthority::RuntimeJournal,
            event_kind: event_kind.to_string(),
        },
        freshness: context.freshness,
        visibility: context.visibility,
        logical_time: event.time,
        causal_reference: event.caused_by.as_ref().map(causal_reference),
        world_anchor: Some(MajorWorldEventAnchor {
            scope: MajorWorldEventAnchorScope::World,
            entity_id: Some(crisis_id.clone()),
            position: None,
        }),
    })
}

fn causal_reference(caused_by: &CausedBy) -> MajorWorldEventCausalReference {
    match caused_by {
        CausedBy::Action(action_id) => MajorWorldEventCausalReference::Action(*action_id),
        CausedBy::Effect { intent_id } => MajorWorldEventCausalReference::Effect {
            intent_id: intent_id.clone(),
        },
    }
}
