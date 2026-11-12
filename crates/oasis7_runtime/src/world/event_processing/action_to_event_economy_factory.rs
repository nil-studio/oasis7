use super::*;
use crate::FACTORY_BUILD_STARTED_MODERN_VERSION;
use crate::FactoryBuildPowerObligationV1;
use oasis7_wasm_abi::FactoryModuleSpec;

impl World {
    fn starter_assembler_build_blocker(&self, factory_id: &str) -> Option<String> {
        if factory_id != crate::STARTER_ASSEMBLER_FACTORY_ID {
            return None;
        }
        self.state
            .starter_industrial_feasibility()
            .disabled_reason()
    }

    pub(in crate::world) fn validate_module_backed_factory_admission(
        &self,
        action_id: ActionId,
        builder_agent_id: &String,
        site_id: &String,
        module_id: &str,
        spec: &FactoryModuleSpec,
    ) -> Result<Option<RejectReason>, WorldError> {
        if module_id.trim().is_empty() {
            return Ok(Some(RejectReason::RuleDenied {
                notes: vec!["factory module_id cannot be empty".to_string()],
            }));
        }
        if let WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) =
            self.build_factory_to_event(action_id, builder_agent_id, site_id, spec, false)?
        {
            return Ok(Some(reason));
        }
        if self
            .state
            .factory_construction_power_profiles
            .get(spec.factory_id.as_str())
            .and_then(|profile| profile.source_module_id.as_deref())
            != Some(module_id)
        {
            return Ok(Some(RejectReason::RuleDenied {
                notes: vec![format!(
                    "construction power profile source mismatch: factory_id={} module_id={}",
                    spec.factory_id, module_id
                )],
            }));
        }
        Ok(None)
    }

    pub(in crate::world) fn build_factory_to_event(
        &self,
        action_id: ActionId,
        builder_agent_id: &String,
        site_id: &String,
        spec: &FactoryModuleSpec,
        validate_materials: bool,
    ) -> Result<WorldEventBody, WorldError> {
        factory_schedule_build::evaluate_build_factory_v1(
            &mut factory_schedule_sources::LegacyFactoryBuildFactsV1::new(self),
            action_id,
            builder_agent_id,
            site_id,
            spec,
            if validate_materials {
                factory_schedule_build::BuildMaterialCheckV1::Required
            } else {
                factory_schedule_build::BuildMaterialCheckV1::LegacyModulePreview
            },
        )
    }
}
