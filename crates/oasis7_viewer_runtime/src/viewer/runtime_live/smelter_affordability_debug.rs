use super::*;
use crate::runtime::MaterialLedgerId;

const LOCAL_TEST_PLAYER_ID_PREFIX: &str = "local-test-player-";

impl ViewerRuntimeLiveServer {
    /// Seeds the opt-in local S6 world used to verify smelter affordability UI.
    pub fn seed_smelter_affordability_debug_scenario(&mut self) -> Result<(), String> {
        self.seed_smelter_affordability_debug_scenario_inner()
    }

    /// Seeds a local, opt-in S6 diagnostic world through normal runtime actions.
    /// The viewer-live CLI is responsible for keeping this unreachable without
    /// `--allow-debug-scenario`.
    pub(super) fn seed_smelter_affordability_debug_scenario_inner(&mut self) -> Result<(), String> {
        let agent_id = self
            .world
            .state()
            .agents
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| "debug scenario requires a seeded runtime agent".to_string())?;
        self.world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, 100)
            .map_err(|err| format!("seed smelter electricity failed: {err:?}"))?;
        self.world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, 100)
            .map_err(|err| format!("seed smelter data failed: {err:?}"))?;
        let build = crate::viewer::gameplay_actions::runtime_factory_build_action(
            agent_id.as_str(),
            "site-smelter",
            crate::viewer::FACTORY_SMELTER_MK1,
            crate::viewer::FACTORY_SMELTER_MK1,
        )
        .ok_or_else(|| "debug scenario could not construct smelter build action".to_string())?;
        self.world.submit_action(build);
        self.world
            .step()
            .map_err(|err| format!("debug smelter build failed: {err:?}"))?;
        if !self.world.has_factory(crate::viewer::FACTORY_SMELTER_MK1) {
            self.world
                .step()
                .map_err(|err| format!("debug smelter build settlement failed: {err:?}"))?;
        }
        if !self.world.has_factory(crate::viewer::FACTORY_SMELTER_MK1) {
            return Err("debug scenario did not build the smelter".to_string());
        }
        let smelter_ledger = MaterialLedgerId::site("site-smelter");
        for (material, amount) in [("iron_ore", 8), ("carbon_fuel", 2)] {
            self.world
                .set_ledger_material_balance(smelter_ledger.clone(), material, amount)
                .map_err(|err| format!("seed smelter recipe material failed: {err:?}"))?;
        }
        self.world.submit_action(RuntimeAction::ClaimStarterOc {
            agent_id: agent_id.clone(),
            player_id: "smelter-affordability-debug-player".to_string(),
            public_key: None,
        });
        self.world
            .step()
            .map_err(|err| format!("seed debug starter OC failed: {err:?}"))?;
        if !self.world.state().starter_oc_claims.contains_key(&agent_id) {
            return Err("debug scenario did not credit starter OC".to_string());
        }
        self.world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, 0)
            .map_err(|err| format!("drain debug electricity failed: {err:?}"))?;
        self.world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, 0)
            .map_err(|err| format!("drain debug data failed: {err:?}"))?;
        self.smelter_affordability_debug_agent_id = Some(agent_id);
        Ok(())
    }

    pub(super) fn smelter_affordability_debug_agent_for_local_test_player(
        &self,
        player_id: &str,
    ) -> Option<&str> {
        player_id
            .trim()
            .starts_with(LOCAL_TEST_PLAYER_ID_PREFIX)
            .then_some(self.smelter_affordability_debug_agent_id.as_deref())
            .flatten()
    }
}
