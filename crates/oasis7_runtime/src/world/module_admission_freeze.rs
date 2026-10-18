use super::super::{ActionId, CausedBy, DomainEvent, RejectReason, WorldEventBody};
use super::{World, WorldError};
use crate::state::ModuleAdmissionFreeze;
use oasis7_wasm_abi::ModuleManifest;

impl World {
    pub(super) fn ensure_module_admission_allowed(
        &self,
        manifest: &ModuleManifest,
    ) -> Result<(), WorldError> {
        let key =
            ModuleAdmissionFreeze::key(&manifest.module_id, &manifest.version, &manifest.wasm_hash);
        if let Some(marker) = self.state.module_admission_freezes.get(&key) {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module admission frozen: {} scope={} rollback_proposal_id={}",
                    marker.reason, key, marker.rollback_proposal_id,
                ),
            });
        }
        Ok(())
    }

    pub(super) fn reject_frozen_module_admission(
        &mut self,
        action_id: ActionId,
        manifest: &ModuleManifest,
    ) -> Result<bool, WorldError> {
        if let Err(error) = self.ensure_module_admission_allowed(manifest) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("{error:?}")],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        Ok(false)
    }
}
