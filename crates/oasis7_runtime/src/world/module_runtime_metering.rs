use oasis7_wasm_abi::{ModuleCallErrorCode, ModuleCallFailure, ModuleOutput};

use super::super::{ModuleManifest, ModuleRuntimeChargeEvent, WorldError, WorldEventBody};
use super::World;
use oasis7_agent_api::world_action::ResourceKind;
use std::collections::BTreeMap;

pub(super) struct PreparedModuleRuntimeCharge {
    pub(super) agents: BTreeMap<String, super::super::agent_cell::AgentCell>,
    pub(super) resources: BTreeMap<ResourceKind, i64>,
}

impl PreparedModuleRuntimeCharge {
    pub(super) fn install_infallible(self, world: &mut World) {
        world.state.agents.extend(self.agents);
        world.state.resources.extend(self.resources);
    }
}

impl World {
    /// Check the worst-case deterministic runtime charge before entering a
    /// sandbox.  The actual charge is still appended after the returned
    /// output is validated, but a caller can never spend sandbox work that
    /// the live resource account cannot afford.
    pub(super) fn check_module_runtime_resources(
        &self,
        module_id: &str,
        trace_id: &str,
        manifest: &ModuleManifest,
        input_bytes: u64,
    ) -> Result<(), ModuleCallFailure> {
        let Some(payer_agent_id) = self
            .state
            .module_artifact_owners
            .get(manifest.wasm_hash.as_str())
            .filter(|owner| self.state.agents.contains_key(owner.as_str()))
            .cloned()
        else {
            return Ok(());
        };

        let amounts = super::incremental_module_fee::upper_bound(&manifest.limits, input_bytes);
        let compute_fee_amount = amounts.compute;
        let electricity_fee_amount = amounts.electricity;
        let available_compute = self
            .state
            .agents
            .get(&payer_agent_id)
            .map(|cell| cell.state.resources.get(ResourceKind::Data))
            .unwrap_or(0);
        let available_electricity = self
            .state
            .agents
            .get(&payer_agent_id)
            .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
            .unwrap_or(0);
        if available_compute < compute_fee_amount || available_electricity < electricity_fee_amount
        {
            return Err(ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::PolicyDenied,
                detail: format!(
                    "module runtime upper-bound fee insufficient payer={} data {}/{} electricity {}/{}",
                    payer_agent_id,
                    available_compute,
                    compute_fee_amount,
                    available_electricity,
                    electricity_fee_amount
                ),
            });
        }
        Ok(())
    }

    pub(super) fn apply_module_runtime_charge_event(
        &mut self,
        charge: &ModuleRuntimeChargeEvent,
        now: super::super::WorldTime,
    ) -> Result<(), WorldError> {
        let prepared = self.prepare_module_runtime_charge_event(charge, now)?;
        prepared.install_infallible(self);
        Ok(())
    }

    pub(super) fn prepare_module_runtime_charge_event(
        &self,
        charge: &ModuleRuntimeChargeEvent,
        now: super::super::WorldTime,
    ) -> Result<PreparedModuleRuntimeCharge, WorldError> {
        super::incremental_module_fee::validate_amounts(charge)?;
        let mut cell = self
            .state
            .agents
            .get(&charge.payer_agent_id)
            .ok_or_else(|| WorldError::AgentNotFound {
                agent_id: charge.payer_agent_id.clone(),
            })?
            .clone();
        let resources = super::incremental_module_fee::prepare_balances(
            charge,
            &mut cell.state.resources,
            |kind| self.state.resources.get(&kind).copied(),
        )?;
        cell.last_active = now;
        Ok(PreparedModuleRuntimeCharge {
            agents: BTreeMap::from([(charge.payer_agent_id.clone(), cell)]),
            resources,
        })
    }

    pub(super) fn try_charge_module_runtime(
        &mut self,
        module_id: &str,
        trace_id: &str,
        manifest: &ModuleManifest,
        input_bytes: u64,
        output: &ModuleOutput,
    ) -> Result<(), ModuleCallFailure> {
        let Some(payer_agent_id) = self
            .state
            .module_artifact_owners
            .get(manifest.wasm_hash.as_str())
            .filter(|owner| self.state.agents.contains_key(owner.as_str()))
            .cloned()
        else {
            return Ok(());
        };

        let amounts = super::incremental_module_fee::actual(input_bytes, output);
        let compute_fee_amount = amounts.compute;
        let electricity_fee_amount = amounts.electricity;
        let available_compute = self
            .state
            .agents
            .get(&payer_agent_id)
            .map(|cell| cell.state.resources.get(ResourceKind::Data))
            .unwrap_or(0);
        let available_electricity = self
            .state
            .agents
            .get(&payer_agent_id)
            .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
            .unwrap_or(0);
        if available_compute < compute_fee_amount || available_electricity < electricity_fee_amount
        {
            return Err(ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::PolicyDenied,
                detail: format!(
                    "module runtime fee insufficient payer={} data {}/{} electricity {}/{}",
                    payer_agent_id,
                    available_compute,
                    compute_fee_amount,
                    available_electricity,
                    electricity_fee_amount
                ),
            });
        }

        let charge = ModuleRuntimeChargeEvent {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            payer_agent_id,
            compute_fee_kind: ResourceKind::Data,
            compute_fee_amount,
            electricity_fee_kind: ResourceKind::Electricity,
            electricity_fee_amount,
            input_bytes,
            output_bytes: output.output_bytes,
            effect_count: u32::try_from(output.effects.len()).unwrap_or(u32::MAX),
            emit_count: u32::try_from(output.emits.len()).unwrap_or(u32::MAX),
        };
        self.append_event(WorldEventBody::ModuleRuntimeCharged(charge), None)
            .map_err(|err| ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::PolicyDenied,
                detail: format!("module runtime fee apply failed: {err:?}"),
            })?;
        Ok(())
    }
}
