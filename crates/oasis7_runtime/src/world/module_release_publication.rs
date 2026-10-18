//! Completion tail shared by governed and already-registered module releases.
use super::super::state::module_instance_transition::PreparedModuleInstance;
use super::super::state::module_release_transition::PreparedModuleRelease;
use super::super::{
    CausedBy, DomainEvent, ModuleProfileChanges, TickConsensusRecord, WorldError, WorldEvent,
    WorldEventBody,
};
use super::World;

pub(super) struct ModuleReleaseCompletion {
    pub(super) request_id: u64,
    pub(super) operator_agent_id: String,
    pub(super) profile_changes: ModuleProfileChanges,
}

pub(super) struct ReleasePublicationJournal {
    pub(super) next_event_id: u64,
    pub(super) next_event_id_era: u64,
    pub(super) events: Vec<WorldEvent>,
    pub(super) evicted: u64,
}

pub(super) struct PreparedModuleReleaseTail {
    pub(super) state: PreparedModuleRelease,
    pub(super) journal: ReleasePublicationJournal,
    pub(super) consensus_record: TickConsensusRecord,
}

impl ModuleReleaseCompletion {
    fn events(self, install: &DomainEvent) -> Vec<DomainEvent> {
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

impl World {
    pub(super) fn prepare_module_release_tail(
        &mut self,
        instance: &PreparedModuleInstance,
        install: &DomainEvent,
        completion: ModuleReleaseCompletion,
        mut journal: ReleasePublicationJournal,
        manifest_hash: &str,
        caused_by: Option<CausedBy>,
    ) -> Result<PreparedModuleReleaseTail, WorldError> {
        // Seed from the fee-debited installer including its install mailbox.
        // The release operator may be that same agent.
        let mut state = PreparedModuleRelease::new(&self.state, instance.routed_agents());
        let mut final_record = None;
        for event in completion.events(install) {
            state.apply_event(&self.state, &event, self.state.time)?;
            let (id, next_id, next_era) =
                Self::preview_next_event_id(journal.next_event_id, journal.next_event_id_era);
            journal.next_event_id = next_id;
            journal.next_event_id_era = next_era;
            journal.events.push(WorldEvent {
                id,
                time: self.state.time,
                caused_by: caused_by.clone(),
                body: WorldEventBody::Domain(event),
            });
            let overflow = journal
                .events
                .len()
                .saturating_sub(self.runtime_memory_limits.max_journal_events.max(1));
            if overflow > 0 {
                journal.events.drain(0..overflow);
            }
            journal.evicted = journal.evicted.saturating_add(overflow as u64);
            let tick_events: Vec<_> = journal
                .events
                .iter()
                .filter(|event| event.time == self.state.time)
                .cloned()
                .collect();
            let root = self.state_root_hash_with_module_release_overlay(
                Some(instance),
                &state,
                manifest_hash,
            )?;
            let record = self.build_tick_consensus_record_for_prepared_events(
                self.state.time,
                &tick_events,
                root.clone(),
            )?;
            self.validate_tick_consensus_candidate_for_prepared_publication(
                &record,
                &tick_events,
                &root,
            )?;
            if self.take_fail_next_append_after_publication_prepare_for_test() {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: "injected append_event failure after publication preparation"
                        .to_string(),
                });
            }
            final_record = Some(record);
        }
        Ok(PreparedModuleReleaseTail {
            state,
            journal,
            consensus_record: final_record
                .expect("release completion always emits its final status"),
        })
    }
}
