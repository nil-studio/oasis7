//! Typed publication for governed manifest and module-lifecycle proposals.
//!
//! Proposal application validates against an immutable [`World`] base and
//! stages only the projections that the existing lifecycle reducers can touch.
//! The prepared install is the sole canonical publication seam; in particular,
//! this path must not turn a full `World` clone into the production transaction.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

use super::super::util::hash_json;
use super::super::{
    CausedBy, DomainEvent, GovernanceEvent, GovernanceFinalityCertificate, Manifest,
    ManifestUpdate, ModuleChangeSet, ModuleEvent, ModuleEventKind, ModuleRecord, ModuleRegistry,
    ModuleSubscriptionStage, Proposal, ProposalId, ProposalStatus, TickConsensusRecord, WorldError,
    WorldEvent, WorldEventBody, WorldEventId,
};
use super::World;

#[derive(Serialize)]
struct StateRootProjection<'a, T: ?Sized> {
    state: &'a T,
    manifest_hash: &'a str,
    policy_hash: &'a str,
}

/// All state published by one governed proposal application.
///
/// The projections are owned so preparation can run every fallible reducer,
/// journal, backpressure, and consensus check without mutating the canonical
/// world.  Process-local subscription cache entries are invalidated by stable
/// registry keys at install; no cache value participates in a commitment.
pub(super) struct PreparedGovernanceProposalApply {
    applied_hash: String,
    manifest: Manifest,
    module_registry: ModuleRegistry,
    module_artifacts: BTreeSet<String>,
    module_tick_schedule: BTreeMap<String, u64>,
    cache_invalidations: BTreeSet<String>,
    proposal_id: ProposalId,
    proposal: Proposal,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
    consensus_record: TickConsensusRecord,
}

impl World {
    pub(super) fn prepare_proposal_with_finality(
        &self,
        proposal_id: ProposalId,
        finality_certificate: &GovernanceFinalityCertificate,
    ) -> Result<PreparedGovernanceProposalApply, WorldError> {
        // Keep this order aligned with the former inner reducer: callers rely
        // on the first validation error when multiple inputs are invalid.
        let proposal = self
            .proposals
            .get(&proposal_id)
            .ok_or(WorldError::ProposalNotFound { proposal_id })?;
        let (manifest, actor, approved_manifest_hash) = match &proposal.status {
            ProposalStatus::Approved { manifest_hash, .. } => (
                proposal.manifest.clone(),
                proposal.author.clone(),
                manifest_hash.clone(),
            ),
            other => {
                return Err(WorldError::ProposalInvalidState {
                    proposal_id,
                    expected: "approved".to_string(),
                    found: other.label(),
                });
            }
        };
        if self.is_governance_emergency_brake_active() {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "governance apply blocked by emergency brake until_tick={}",
                    self.governance_emergency_brake_until_tick
                        .unwrap_or(self.state.time)
                ),
            });
        }
        if let Some(not_before_tick) = proposal.not_before_tick
            && self.state.time < not_before_tick
        {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "proposal_id={} timelock pending current_tick={} not_before_tick={}",
                    proposal_id, self.state.time, not_before_tick
                ),
            });
        }
        if let Some(activate_epoch) = proposal.activate_epoch {
            let current_epoch = self.current_governance_epoch();
            if current_epoch < activate_epoch {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "proposal_id={} activation epoch pending current_epoch={} activate_epoch={}",
                        proposal_id, current_epoch, activate_epoch
                    ),
                });
            }
        }

        let module_changes = manifest.module_changes()?;
        if let Some(changes) = &module_changes {
            self.validate_module_changes(changes)?;
        }
        let applied_manifest = if module_changes.is_some() {
            manifest.without_module_changes()?
        } else {
            manifest.clone()
        };
        let proposal_manifest_hash = hash_json(&manifest)?;
        if proposal_manifest_hash != approved_manifest_hash {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "approved manifest hash drift".to_string(),
            });
        }
        let applied_hash = hash_json(&applied_manifest)?;
        let finality_epoch_id = self.current_governance_epoch();
        self.validate_governance_finality_certificate(
            proposal_id,
            approved_manifest_hash.as_str(),
            finality_epoch_id,
            finality_certificate,
        )?;

        let mut module_registry = self.module_registry.clone();
        let mut module_artifacts = self.module_artifacts.clone();
        let mut module_tick_schedule = self.module_tick_schedule.clone();
        let mut cache_invalidations = BTreeSet::new();
        let mut event_bodies = Vec::new();
        if let Some(changes) = module_changes.as_ref() {
            project_module_changes(
                self.state.time,
                proposal_id,
                changes,
                actor.as_str(),
                &mut module_registry,
                &mut module_artifacts,
                &mut module_tick_schedule,
                &mut cache_invalidations,
                &mut event_bodies,
            )?;
        }

        event_bodies.push(WorldEventBody::ManifestUpdated(ManifestUpdate {
            manifest: applied_manifest.clone(),
            manifest_hash: applied_hash.clone(),
        }));
        event_bodies.push(WorldEventBody::Governance(GovernanceEvent::Applied {
            proposal_id,
            manifest_hash: Some(applied_hash.clone()),
            consensus_height: Some(finality_certificate.consensus_height),
            threshold: Some(finality_certificate.effective_min_unique_signers()),
            signer_node_ids: finality_certificate.signatures.keys().cloned().collect(),
        }));

        let mut next_event_id = self.next_event_id;
        let mut next_event_id_era = self.next_event_id_era;
        let mut events = Vec::with_capacity(event_bodies.len());
        for body in event_bodies {
            let (event_id, next_id, next_era) =
                Self::preview_next_event_id(next_event_id, next_event_id_era);
            events.push(WorldEvent {
                id: event_id,
                time: self.state.time,
                caused_by: None,
                body,
            });
            next_event_id = next_id;
            next_event_id_era = next_era;
        }

        let mut journal_events = self.journal.events.clone();
        journal_events.extend(events);
        let max_len = self.runtime_memory_limits.max_journal_events.max(1);
        let overflow = journal_events.len().saturating_sub(max_len);
        if overflow > 0 {
            journal_events.drain(0..overflow);
        }
        let tick_events: Vec<WorldEvent> = journal_events
            .iter()
            .filter(|event| event.time == self.state.time)
            .cloned()
            .collect();
        let state_root = state_root_hash_with_manifest(self, &applied_manifest)?;
        let consensus_record = self.build_tick_consensus_record_for_prepared_events(
            self.state.time,
            tick_events.as_slice(),
            state_root.clone(),
        )?;
        self.validate_tick_consensus_candidate_for_prepared_publication(
            &consensus_record,
            tick_events.as_slice(),
            state_root.as_str(),
        )?;

        let mut proposal = proposal.clone();
        proposal.status = ProposalStatus::Applied {
            manifest_hash: applied_hash.clone(),
        };
        Ok(PreparedGovernanceProposalApply {
            applied_hash,
            manifest: applied_manifest,
            module_registry,
            module_artifacts,
            module_tick_schedule,
            cache_invalidations,
            proposal_id,
            proposal,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted: overflow as u64,
            consensus_record,
        })
    }
}

impl PreparedGovernanceProposalApply {
    pub(super) fn applied_hash(&self) -> &str {
        &self.applied_hash
    }

    /// Extend the already validated governance projection with its instance
    /// tail. All tail errors propagate to the action caller; no governance
    /// sidecar, cache invalidation, or publication is installed on error.
    pub(super) fn publish_lifecycle_tail(
        self,
        world: &mut World,
        event: DomainEvent,
        caused_by: Option<CausedBy>,
    ) -> Result<(), WorldError> {
        self.publish_lifecycle_tail_with_release(world, event, caused_by, None)
    }

    pub(super) fn publish_lifecycle_tail_with_release(
        mut self,
        world: &mut World,
        event: DomainEvent,
        caused_by: Option<CausedBy>,
        completion: Option<super::module_release_publication::ModuleReleaseCompletion>,
    ) -> Result<(), WorldError> {
        let (proposal_id, manifest_hash) = match &event {
            DomainEvent::ModuleInstalled {
                proposal_id,
                manifest_hash,
                ..
            }
            | DomainEvent::ModuleUpgraded {
                proposal_id,
                manifest_hash,
                ..
            }
            | DomainEvent::ModuleRollbackApplied {
                proposal_id,
                manifest_hash,
                ..
            } => (*proposal_id, manifest_hash),
            _ => {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: "governance tail requires a module lifecycle event".to_string(),
                });
            }
        };
        if proposal_id != self.proposal_id || manifest_hash != &self.applied_hash {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "module lifecycle tail does not match prepared governance proposal"
                    .to_string(),
            });
        }
        let instance = world.state.prepare_module_instance_event_with_registry(
            &event,
            world.state.time,
            Some(&world.module_registry),
        )?;
        let schedule = World::prepare_module_instance_schedule_with_registry(
            &self.module_registry,
            &event,
            world.state.time,
        )?;
        if let Some((key, next)) = schedule {
            match next {
                Some(tick) => {
                    self.module_tick_schedule.insert(key, tick);
                }
                None => {
                    self.module_tick_schedule.remove(&key);
                }
            }
        }

        let (id, next_id, next_era) =
            World::preview_next_event_id(self.next_event_id, self.next_event_id_era);
        self.next_event_id = next_id;
        self.next_event_id_era = next_era;
        self.journal_events.push(WorldEvent {
            id,
            time: world.state.time,
            caused_by: caused_by.clone(),
            body: WorldEventBody::Domain(event.clone()),
        });
        let overflow = self
            .journal_events
            .len()
            .saturating_sub(world.runtime_memory_limits.max_journal_events.max(1));
        if overflow > 0 {
            self.journal_events.drain(0..overflow);
        }
        self.journal_events_evicted = self.journal_events_evicted.saturating_add(overflow as u64);
        let tick_events: Vec<_> = self
            .journal_events
            .iter()
            .filter(|event| event.time == world.state.time)
            .cloned()
            .collect();
        let state_root = world.state_root_hash_with_module_instance_and_manifest_hash(
            &instance,
            &self.applied_hash,
        )?;
        let consensus_record = world.build_tick_consensus_record_for_prepared_events(
            world.state.time,
            &tick_events,
            state_root.clone(),
        )?;
        world.validate_tick_consensus_candidate_for_prepared_publication(
            &consensus_record,
            &tick_events,
            &state_root,
        )?;
        self.consensus_record = consensus_record;
        if world.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }

        let release = if let Some(completion) = completion {
            let tail = world.prepare_module_release_tail(
                &instance,
                &event,
                completion,
                super::module_release_publication::ReleasePublicationJournal {
                    next_event_id: self.next_event_id,
                    next_event_id_era: self.next_event_id_era,
                    events: std::mem::take(&mut self.journal_events),
                    evicted: self.journal_events_evicted,
                },
                &self.applied_hash,
                caused_by,
            )?;
            self.next_event_id = tail.journal.next_event_id;
            self.next_event_id_era = tail.journal.next_event_id_era;
            self.journal_events = tail.journal.events;
            self.journal_events_evicted = tail.journal.evicted;
            self.consensus_record = tail.consensus_record;
            Some(tail.state)
        } else {
            None
        };
        self.install(world)?;
        instance.install_infallible(&mut world.state);
        if let Some(release) = release {
            release.install_routed(&mut world.state);
        } else {
            world.state.route_domain_event(&event);
        }
        Ok(())
    }

    pub(super) fn install(self, world: &mut World) -> Result<String, WorldError> {
        let Self {
            applied_hash,
            manifest,
            module_registry,
            module_artifacts,
            module_tick_schedule,
            cache_invalidations,
            proposal_id,
            proposal,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted,
            consensus_record,
        } = self;

        world.install_prepared_tick_consensus_record(consensus_record)?;
        world.manifest = manifest;
        world.module_registry = module_registry;
        world.module_artifacts = module_artifacts;
        world.module_tick_schedule = module_tick_schedule;
        world.proposals.insert(proposal_id, proposal);
        world.next_event_id = next_event_id;
        world.next_event_id_era = next_event_id_era;
        world.journal.events = journal_events;
        world.runtime_backpressure_stats.journal_events_evicted = world
            .runtime_backpressure_stats
            .journal_events_evicted
            .saturating_add(journal_events_evicted);

        for record_key in cache_invalidations {
            let prefix = format!("{record_key}|");
            world
                .prepared_subscription_cache
                .retain(|key, _| !key.starts_with(prefix.as_str()));
        }
        Ok(applied_hash)
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Module projection receives independent mutable stores to preserve deterministic update ordering."
)]
pub(super) fn project_module_changes(
    time: u64,
    proposal_id: ProposalId,
    changes: &ModuleChangeSet,
    actor: &str,
    module_registry: &mut ModuleRegistry,
    module_artifacts: &mut BTreeSet<String>,
    module_tick_schedule: &mut BTreeMap<String, u64>,
    cache_invalidations: &mut BTreeSet<String>,
    event_bodies: &mut Vec<WorldEventBody>,
) -> Result<(), WorldError> {
    let mut registers = changes.register.clone();
    registers.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    for module in registers {
        let key = ModuleRegistry::record_key(&module.module_id, &module.version);
        let event = ModuleEvent {
            proposal_id,
            kind: ModuleEventKind::RegisterModule {
                module: module.clone(),
                registered_by: actor.to_string(),
            },
        };
        project_module_event(
            time,
            &event,
            module_registry,
            module_artifacts,
            module_tick_schedule,
            cache_invalidations,
        )?;
        event_bodies.push(WorldEventBody::ModuleEvent(event));
        debug_assert!(module_registry.records.contains_key(&key));
    }

    let mut upgrades = changes.upgrade.clone();
    upgrades.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    for upgrade in upgrades {
        let event = ModuleEvent {
            proposal_id,
            kind: ModuleEventKind::UpgradeModule {
                module_id: upgrade.module_id,
                from_version: upgrade.from_version,
                to_version: upgrade.to_version,
                wasm_hash: upgrade.manifest.wasm_hash.clone(),
                manifest: upgrade.manifest,
                upgraded_by: actor.to_string(),
            },
        };
        project_module_event(
            time,
            &event,
            module_registry,
            module_artifacts,
            module_tick_schedule,
            cache_invalidations,
        )?;
        event_bodies.push(WorldEventBody::ModuleEvent(event));
    }

    let mut activations = changes.activate.clone();
    activations.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    for activation in activations {
        let event = ModuleEvent {
            proposal_id,
            kind: ModuleEventKind::ActivateModule {
                module_id: activation.module_id,
                version: activation.version,
                activated_by: actor.to_string(),
            },
        };
        project_module_event(
            time,
            &event,
            module_registry,
            module_artifacts,
            module_tick_schedule,
            cache_invalidations,
        )?;
        event_bodies.push(WorldEventBody::ModuleEvent(event));
    }

    let mut deactivations = changes.deactivate.clone();
    deactivations.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    for deactivation in deactivations {
        let event = ModuleEvent {
            proposal_id,
            kind: ModuleEventKind::DeactivateModule {
                module_id: deactivation.module_id,
                reason: deactivation.reason,
                deactivated_by: actor.to_string(),
            },
        };
        project_module_event(
            time,
            &event,
            module_registry,
            module_artifacts,
            module_tick_schedule,
            cache_invalidations,
        )?;
        event_bodies.push(WorldEventBody::ModuleEvent(event));
    }

    Ok(())
}

pub(super) fn project_module_event(
    time: u64,
    event: &ModuleEvent,
    module_registry: &mut ModuleRegistry,
    module_artifacts: &mut BTreeSet<String>,
    module_tick_schedule: &mut BTreeMap<String, u64>,
    cache_invalidations: &mut BTreeSet<String>,
) -> Result<(), WorldError> {
    match &event.kind {
        ModuleEventKind::RegisterModule {
            module,
            registered_by,
        } => {
            let key = ModuleRegistry::record_key(&module.module_id, &module.version);
            cache_invalidations.insert(key);
            module_registry.records.insert(
                ModuleRegistry::record_key(&module.module_id, &module.version),
                ModuleRecord {
                    manifest: module.clone(),
                    registered_at: time,
                    registered_by: registered_by.clone(),
                    audit_event_id: None,
                },
            );
            module_artifacts.insert(module.wasm_hash.clone());
        }
        ModuleEventKind::UpgradeModule {
            module_id,
            to_version,
            manifest,
            upgraded_by,
            ..
        } => {
            let key = ModuleRegistry::record_key(module_id, to_version);
            cache_invalidations.insert(key.clone());
            module_registry.records.insert(
                key,
                ModuleRecord {
                    manifest: manifest.clone(),
                    registered_at: time,
                    registered_by: upgraded_by.clone(),
                    audit_event_id: None,
                },
            );
            module_artifacts.insert(manifest.wasm_hash.clone());
        }
        ModuleEventKind::ActivateModule {
            module_id, version, ..
        } => {
            module_registry
                .active
                .insert(module_id.clone(), version.clone());
            let key = ModuleRegistry::record_key(module_id, version);
            let record = module_registry.records.get(&key).ok_or_else(|| {
                WorldError::ModuleChangeInvalid {
                    reason: format!("module record missing {key}"),
                }
            })?;
            if record
                .manifest
                .subscriptions
                .iter()
                .any(|subscription| subscription.resolved_stage() == ModuleSubscriptionStage::Tick)
            {
                module_tick_schedule.insert(module_id.clone(), time);
            } else {
                module_tick_schedule.remove(module_id);
            }
        }
        ModuleEventKind::DeactivateModule { module_id, .. } => {
            module_registry.active.remove(module_id);
            module_tick_schedule.remove(module_id);
        }
    }
    Ok(())
}

pub(super) fn state_root_hash_with_manifest(
    world: &World,
    manifest: &Manifest,
) -> Result<String, WorldError> {
    let manifest_hash = hash_json(manifest)?;
    let policy_hash = hash_json(&world.policies)?;
    let projection = StateRootProjection {
        state: &world.state,
        manifest_hash: manifest_hash.as_str(),
        policy_hash: policy_hash.as_str(),
    };
    hash_json(&projection)
}
