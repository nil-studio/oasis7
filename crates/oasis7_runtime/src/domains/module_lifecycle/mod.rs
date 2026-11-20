//! `domains::module_lifecycle` bounded context: module artifact registration/retirement,
//! marketplace order/sale matching, module release completion tail, and change-batch staging.
//!
//! All state mutations in this bounded context stage typed [`SubsystemDelta`]s onto a
//! shared root transaction view and install infallibly through [`RootInstallContext`]
//! after Stage 2 commit confirmation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::state::module_instance_transition::PreparedModuleInstance;
use crate::state::module_marketplace_transition::PreparedModuleMarketplace;
use crate::state::module_release_transition::PreparedModuleRelease;
use crate::world::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedTransition, SubsystemDelta,
    TransitionContext, TransitionReject,
};
use crate::{
    CausedBy, DomainEvent, ModuleProfileChanges, ModuleRegistry, TickConsensusRecord, World,
    WorldError, WorldEvent, WorldEventBody, WorldEventId,
};

#[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
use crate::{ModuleChangeSet, ProposalId};

/// Typed delta for module artifact deployment registration (`module_artifacts` + `module_artifact_bytes`).
#[derive(Debug, Clone)]
pub struct ModuleArtifactRegistrationDelta {
    wasm_hash: String,
    bytes: Arc<[u8]>,
}

impl ModuleArtifactRegistrationDelta {
    pub fn new(wasm_hash: String, bytes: Arc<[u8]>) -> Self {
        Self { wasm_hash, bytes }
    }

    pub fn wasm_hash(&self) -> &str {
        &self.wasm_hash
    }
}

impl SubsystemDelta for ModuleArtifactRegistrationDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        let world = ctx.world_mut();
        world.module_artifacts.insert(self.wasm_hash.clone());
        world
            .module_artifact_bytes
            .insert(self.wasm_hash, self.bytes);
    }
}

/// Typed delta for module artifact retirement (`module_artifacts`, `module_artifact_bytes`, and `module_cache` reset).
#[derive(Debug, Clone)]
pub struct ModuleArtifactRetirementDelta {
    wasm_hash: String,
    max_cached_modules: usize,
}

impl ModuleArtifactRetirementDelta {
    pub fn prepare(world: &World, wasm_hash: String) -> Self {
        Self {
            wasm_hash,
            max_cached_modules: world.module_cache.max_cached_modules(),
        }
    }

    pub fn matches_event(&self, event: &DomainEvent) -> bool {
        matches!(
            event,
            DomainEvent::ModuleArtifactDestroyed { wasm_hash, .. } if wasm_hash == &self.wasm_hash
        )
    }
}

impl SubsystemDelta for ModuleArtifactRetirementDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        let world = ctx.world_mut();
        world.module_artifacts.remove(&self.wasm_hash);
        world.module_artifact_bytes.remove(&self.wasm_hash);
        world.module_cache = oasis7_wasm_abi::ModuleCache::new(self.max_cached_modules);
    }
}

/// Completion specification for governed and already-registered module releases.
#[derive(Debug, Clone)]
pub struct ModuleReleaseCompletion {
    pub request_id: u64,
    pub operator_agent_id: String,
    pub profile_changes: ModuleProfileChanges,
}

impl ModuleReleaseCompletion {
    pub fn events(self, install: &DomainEvent) -> Vec<DomainEvent> {
        let DomainEvent::ModuleInstalled {
            installer_agent_id,
            instance_id,
            module_id,
            module_version,
            proposal_id,
            manifest_hash,
            ..
        } = install
        else {
            unreachable!("release completion requires its prepared install event")
        };
        let mut events = Vec::new();
        let mut products = self.profile_changes.product_profiles;
        products.sort_by(|a, b| a.product_id.cmp(&b.product_id));
        for profile in products {
            events.push(DomainEvent::ProductProfileGoverned {
                operator_agent_id: self.operator_agent_id.clone(),
                proposal_id: *proposal_id,
                profile,
            });
        }
        let mut recipes = self.profile_changes.recipe_profiles;
        recipes.sort_by(|a, b| a.recipe_id.cmp(&b.recipe_id));
        for profile in recipes {
            events.push(DomainEvent::RecipeProfileGoverned {
                operator_agent_id: self.operator_agent_id.clone(),
                proposal_id: *proposal_id,
                profile,
            });
        }
        let mut factories = self.profile_changes.factory_profiles;
        factories.sort_by(|a, b| a.factory_id.cmp(&b.factory_id));
        for profile in factories {
            events.push(DomainEvent::FactoryProfileGoverned {
                operator_agent_id: self.operator_agent_id.clone(),
                proposal_id: *proposal_id,
                profile,
            });
        }
        events.push(DomainEvent::ModuleReleaseApplied {
            request_id: self.request_id,
            operator_agent_id: self.operator_agent_id,
            installer_agent_id: installer_agent_id.clone(),
            instance_id: instance_id.clone(),
            module_id: module_id.clone(),
            module_version: module_version.clone(),
            proposal_id: *proposal_id,
            manifest_hash: manifest_hash.clone(),
        });
        events
    }
}

/// Staged journal cursor and event buffer threaded across nested release tail publication.
#[derive(Debug, Clone)]
pub struct ReleasePublicationJournal {
    pub next_event_id: u64,
    pub next_event_id_era: u64,
    pub events: Vec<WorldEvent>,
    pub evicted: u64,
}

/// Prepared module release tail combining staged state overlay, journal tail, and consensus record.
pub struct PreparedModuleReleaseTail {
    pub state: PreparedModuleRelease,
    pub journal: ReleasePublicationJournal,
    pub consensus_record: TickConsensusRecord,
}

impl SubsystemDelta for PreparedModuleRelease {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        self.install_routed(ctx.state_mut());
    }
}

/// Staged marketplace order + optional immediate matching sale transition.
pub struct StagedModuleMarketplaceOrder {
    pub order: DomainEvent,
}

/// Prepared delta for a module marketplace order (and any immediate matching sale).
pub struct ModuleMarketplaceOrderDelta {
    state: PreparedModuleMarketplace,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    evicted: u64,
}

impl SubsystemDelta for ModuleMarketplaceOrderDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        self.state.install_routed(ctx.state_mut());
        ctx.install_journal_tail(
            self.next_event_id,
            self.next_event_id_era,
            self.journal_events,
            self.evicted,
        );
    }
}

impl StagedTransition for StagedModuleMarketplaceOrder {
    type Delta = ModuleMarketplaceOrderDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let world = view.world();
        assert!(matches!(
            self.order,
            DomainEvent::ModuleArtifactListed { .. } | DomainEvent::ModuleArtifactBidPlaced { .. }
        ));
        let mut state = PreparedModuleMarketplace::new(&world.state, &self.order);
        let mut next_event_id = world.next_event_id;
        let mut next_event_id_era = world.next_event_id_era;
        let mut journal = world.journal.events.clone();
        let mut evicted = 0u64;
        let mut pending = Some(self.order.clone());
        let mut first = true;
        let mut final_record = None;
        while let Some(event) = pending.take() {
            state.apply_event(&world.state, &event, world.state.time)?;
            let (id, next_id, next_era) =
                World::preview_next_event_id(next_event_id, next_event_id_era);
            next_event_id = next_id;
            next_event_id_era = next_era;
            journal.push(WorldEvent {
                id,
                time: world.state.time,
                caused_by: ctx.caused_by.clone(),
                body: WorldEventBody::Domain(event),
            });
            let overflow = journal
                .len()
                .saturating_sub(world.runtime_memory_limits.max_journal_events.max(1));
            if overflow > 0 {
                journal.drain(0..overflow);
            }
            evicted = evicted.saturating_add(overflow as u64);
            let tick_events: Vec<_> = journal
                .iter()
                .filter(|event| event.time == world.state.time)
                .cloned()
                .collect();
            let root = world.state_root_hash_with_module_marketplace_overlay(&state)?;
            let record = world.build_tick_consensus_record_for_prepared_events(
                world.state.time,
                &tick_events,
                root.clone(),
            )?;
            world.validate_tick_consensus_candidate_for_prepared_publication(
                &record,
                &tick_events,
                &root,
            )?;
            final_record = Some(record);
            if first {
                pending = state.matching_sale(&world.state);
                first = false;
            }
        }
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            Some(final_record.expect("marketplace batch includes its order")),
            ModuleMarketplaceOrderDelta {
                state,
                next_event_id,
                next_event_id_era,
                journal_events: journal,
                evicted,
            },
        ))
    }
}

/// Prepared delta for a standalone multi-`ModuleEvent` change batch.
#[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
pub struct ModuleChangeBatchDelta {
    pub(crate) module_registry: ModuleRegistry,
    pub(crate) module_artifacts: BTreeSet<String>,
    pub(crate) module_tick_schedule: BTreeMap<String, u64>,
    pub(crate) cache_invalidations: BTreeSet<String>,
    pub(crate) next_event_id: WorldEventId,
    pub(crate) next_event_id_era: u64,
    pub(crate) journal_events: Vec<WorldEvent>,
    pub(crate) journal_events_evicted: u64,
}

#[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
impl SubsystemDelta for ModuleChangeBatchDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        let world = ctx.world_mut();
        world.module_registry = self.module_registry;
        world.module_artifacts = self.module_artifacts;
        world.module_tick_schedule = self.module_tick_schedule;
        for record_key in self.cache_invalidations {
            let prefix = format!("{record_key}|");
            world
                .prepared_subscription_cache
                .retain(|key, _| !key.starts_with(prefix.as_str()));
        }
        ctx.install_journal_tail(
            self.next_event_id,
            self.next_event_id_era,
            self.journal_events,
            self.journal_events_evicted,
        );
    }
}

/// Prepare a module release completion tail over a borrowed [`World`] and a staged
/// [`PreparedModuleInstance`], preserving same-agent / different-agent sequential composition.
pub(crate) fn prepare_module_release_tail_staged(
    world: &mut World,
    instance: &PreparedModuleInstance,
    install: &DomainEvent,
    completion: ModuleReleaseCompletion,
    mut journal: ReleasePublicationJournal,
    manifest_hash: &str,
    caused_by: Option<CausedBy>,
) -> Result<PreparedModuleReleaseTail, WorldError> {
    // Seed from the fee-debited installer including its install mailbox.
    // The release operator may be that same agent.
    let mut state = PreparedModuleRelease::new(&world.state, instance.routed_agents());
    let mut final_record = None;
    for event in completion.events(install) {
        state.apply_event(&world.state, &event, world.state.time)?;
        let (id, next_id, next_era) =
            World::preview_next_event_id(journal.next_event_id, journal.next_event_id_era);
        journal.next_event_id = next_id;
        journal.next_event_id_era = next_era;
        journal.events.push(WorldEvent {
            id,
            time: world.state.time,
            caused_by: caused_by.clone(),
            body: WorldEventBody::Domain(event),
        });
        let overflow = journal
            .events
            .len()
            .saturating_sub(world.runtime_memory_limits.max_journal_events.max(1));
        if overflow > 0 {
            journal.events.drain(0..overflow);
        }
        journal.evicted = journal.evicted.saturating_add(overflow as u64);
        let tick_events: Vec<_> = journal
            .events
            .iter()
            .filter(|event| event.time == world.state.time)
            .cloned()
            .collect();
        let root = world.state_root_hash_with_module_release_overlay(
            Some(instance),
            &state,
            manifest_hash,
        )?;
        let record = world.build_tick_consensus_record_for_prepared_events(
            world.state.time,
            &tick_events,
            root.clone(),
        )?;
        world.validate_tick_consensus_candidate_for_prepared_publication(
            &record,
            &tick_events,
            &root,
        )?;
        if world.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }
        final_record = Some(record);
    }
    Ok(PreparedModuleReleaseTail {
        state,
        journal,
        consensus_record: final_record.expect("release completion always emits its final status"),
    })
}

/// Prepare a multi-`ModuleEvent` change batch without mutating canonical state.
#[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
pub(crate) fn prepare_module_change_batch_staged(
    world: &mut World,
    proposal_id: ProposalId,
    changes: &ModuleChangeSet,
    actor: &str,
) -> Result<Option<(TickConsensusRecord, ModuleChangeBatchDelta)>, WorldError> {
    let mut module_registry = world.module_registry.clone();
    let mut module_artifacts = world.module_artifacts.clone();
    let mut module_tick_schedule = world.module_tick_schedule.clone();
    let mut cache_invalidations = BTreeSet::new();
    let mut event_bodies = Vec::new();
    crate::world::governance_publication::project_module_changes(
        world.state.time,
        proposal_id,
        changes,
        actor,
        &mut module_registry,
        &mut module_artifacts,
        &mut module_tick_schedule,
        &mut cache_invalidations,
        &mut event_bodies,
    )?;
    if event_bodies.is_empty() {
        return Ok(None);
    }

    let state_root = world.current_state_root_hash()?;
    let mut next_event_id = world.next_event_id;
    let mut next_event_id_era = world.next_event_id_era;
    let mut journal_events = world.journal.events.clone();
    let mut journal_events_evicted = 0u64;
    let mut consensus_record = None;
    for body in event_bodies {
        let (event_id, next_id, next_era) =
            World::preview_next_event_id(next_event_id, next_event_id_era);
        next_event_id = next_id;
        next_event_id_era = next_era;
        journal_events.push(WorldEvent {
            id: event_id,
            time: world.state.time,
            caused_by: None,
            body,
        });
        let overflow = journal_events
            .len()
            .saturating_sub(world.runtime_memory_limits.max_journal_events.max(1));
        if overflow > 0 {
            journal_events.drain(0..overflow);
        }
        journal_events_evicted = journal_events_evicted.saturating_add(overflow as u64);
        let tick_events: Vec<_> = journal_events
            .iter()
            .filter(|event| event.time == world.state.time)
            .cloned()
            .collect();
        let record = world.build_tick_consensus_record_for_prepared_events(
            world.state.time,
            &tick_events,
            state_root.clone(),
        )?;
        world.validate_tick_consensus_candidate_for_prepared_publication(
            &record,
            &tick_events,
            &state_root,
        )?;
        if world.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".into(),
            });
        }
        consensus_record = Some(record);
    }

    let record = consensus_record.expect("non-empty module change batch emits an event");
    Ok(Some((
        record,
        ModuleChangeBatchDelta {
            module_registry,
            module_artifacts,
            module_tick_schedule,
            cache_invalidations,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted,
        },
    )))
}

#[cfg(test)]
mod tests;
