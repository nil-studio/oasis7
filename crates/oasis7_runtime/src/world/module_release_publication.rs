//! Completion tail shared by governed and already-registered module releases,
//! delegated to `crate::domains::module_lifecycle`.
use super::super::state::module_instance_transition::PreparedModuleInstance;
use super::super::{CausedBy, DomainEvent, WorldError};
use super::World;

pub(super) use crate::domains::module_lifecycle::{
    ModuleReleaseCompletion, PreparedModuleReleaseTail, ReleasePublicationJournal,
};

impl World {
    pub(super) fn prepare_module_release_tail(
        &mut self,
        instance: &PreparedModuleInstance,
        install: &DomainEvent,
        completion: ModuleReleaseCompletion,
        journal: ReleasePublicationJournal,
        manifest_hash: &str,
        caused_by: Option<CausedBy>,
    ) -> Result<PreparedModuleReleaseTail, WorldError> {
        crate::domains::module_lifecycle::prepare_module_release_tail_staged(
            self,
            instance,
            install,
            completion,
            journal,
            manifest_hash,
            caused_by,
        )
    }
}
