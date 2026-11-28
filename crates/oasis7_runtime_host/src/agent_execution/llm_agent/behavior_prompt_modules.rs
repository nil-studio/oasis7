use super::*;

impl<C: LlmCompletionClient> LlmAgentBehavior<C> {
    pub(super) fn environment_current_observation_module(
        &self,
        observation: &Observation,
    ) -> Result<serde_json::Value, String> {
        const DEFAULT_FACTORY_BUILD_ELECTRICITY_COST: i64 = 10;
        const DEFAULT_FACTORY_BUILD_DATA_COST: i64 = 5;

        let mut payload = serde_json::to_value(observation)
            .map_err(|err| format!("serialize observation failed: {err}"))?;
        let payload_object = payload
            .as_object_mut()
            .ok_or_else(|| "serialize observation failed: expected object".to_string())?;

        let current_location = observation
            .visible_locations
            .iter()
            .find(|location| location.distance_cm == 0);
        let current_location_id = current_location.map(|location| location.location_id.clone());
        let current_location_name = current_location.map(|location| location.name.clone());
        let available_electricity = observation.self_resources.get(ResourceKind::Electricity);
        let available_data = observation.self_resources.get(ResourceKind::Data);
        let has_required_electricity =
            available_electricity >= DEFAULT_FACTORY_BUILD_ELECTRICITY_COST;
        let has_required_data = available_data >= DEFAULT_FACTORY_BUILD_DATA_COST;
        let has_current_location = current_location_id.is_some();
        let known_factory_ids = self
            .known_factory_locations
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let known_factory_locations = self
            .known_factory_locations
            .iter()
            .map(|(factory_id, location_id)| {
                serde_json::json!({
                    "factory_id": factory_id,
                    "location_id": location_id,
                    "factory_kind": self.known_factory_kind_for_id(factory_id.as_str()),
                })
            })
            .collect::<Vec<_>>();
        let has_known_smelter_factory = self
            .canonical_factory_id_for_kind("factory.smelter.mk1")
            .is_some();
        let can_build_factory_smelter_mk1_now =
            has_current_location && has_required_electricity && has_required_data;
        let build_ready_summary = if can_build_factory_smelter_mk1_now {
            "Current observation includes a colocated location and enough electricity/data to build the first smelter now."
        } else if !has_current_location {
            "No colocated current location is visible yet, so build_factory should wait until a distance_cm=0 location is known."
        } else if !has_required_electricity && !has_required_data {
            "Not enough electricity or data for the default first smelter build cost yet."
        } else if !has_required_electricity {
            "Not enough electricity for the default first smelter build cost yet."
        } else {
            "Not enough data for the default first smelter build cost yet."
        };
        let missing_build_prerequisites = [
            (!has_current_location).then_some("current_location_id"),
            (!has_required_electricity).then_some("electricity"),
            (!has_required_data).then_some("data"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        let recommended_build_factory_action = current_location_id
            .as_ref()
            .filter(|_| can_build_factory_smelter_mk1_now && !has_known_smelter_factory)
            .map(|location_id| {
                serde_json::json!({
                    "decision": "build_factory",
                    "owner": "self",
                    "location_id": location_id,
                    "factory_id": "factory.smelter.mk1",
                    "factory_kind": "factory.smelter.mk1",
                })
            });
        let recommended_schedule_recipe_action = self
            .canonical_factory_id_for_kind("factory.smelter.mk1")
            .map(|factory_id| {
                serde_json::json!({
                    "decision": "schedule_recipe",
                    "owner": "self",
                    "factory_id": factory_id,
                    "recipe_id": "recipe.smelter.iron_ingot",
                    "batches": 1,
                })
            });

        payload_object.insert(
            "current_location_id".to_string(),
            serde_json::json!(current_location_id),
        );
        payload_object.insert(
            "current_location_name".to_string(),
            serde_json::json!(current_location_name),
        );
        payload_object.insert(
            "factory_build_costs_default".to_string(),
            serde_json::json!({
                "electricity": DEFAULT_FACTORY_BUILD_ELECTRICITY_COST,
                "data": DEFAULT_FACTORY_BUILD_DATA_COST,
            }),
        );
        payload_object.insert(
            "can_build_factory_smelter_mk1_now".to_string(),
            serde_json::json!(can_build_factory_smelter_mk1_now),
        );
        payload_object.insert(
            "has_known_smelter_factory".to_string(),
            serde_json::json!(has_known_smelter_factory),
        );
        payload_object.insert(
            "known_factory_ids".to_string(),
            serde_json::json!(known_factory_ids),
        );
        payload_object.insert(
            "known_factory_locations".to_string(),
            serde_json::json!(known_factory_locations),
        );
        payload_object.insert(
            "missing_build_prerequisites".to_string(),
            serde_json::json!(missing_build_prerequisites),
        );
        payload_object.insert(
            "build_ready_summary".to_string(),
            serde_json::json!(build_ready_summary),
        );
        payload_object.insert(
            "recommended_build_factory_action".to_string(),
            serde_json::json!(recommended_build_factory_action),
        );
        payload_object.insert(
            "recommended_schedule_recipe_action".to_string(),
            serde_json::json!(recommended_schedule_recipe_action),
        );

        Ok(payload)
    }
}
