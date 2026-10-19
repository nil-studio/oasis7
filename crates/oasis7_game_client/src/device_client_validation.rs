use super::*;
pub(super) fn validate_original_schedule(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    agent: &str,
    factory: &str,
    recipe: &str,
    batches: u32,
) -> Result<(), String> {
    let WorldServicePayloadV1::Cognition(signed) = &request.signed_payload else {
        return Err("invalid cached action".into());
    };
    validate_cached_schedule_action(&signed.request.action, agent, factory, recipe, batches)
}
pub(super) fn validate_cached_schedule_action(
    action: &crate_action::Action,
    agent: &str,
    factory: &str,
    recipe: &str,
    batches: u32,
) -> Result<(), String> {
    match action {
        crate_action::Action::ScheduleRecipe {
            requester_agent_id,
            factory_id,
            recipe_id,
            plan,
            logistics_route_ids,
            logistics_path_ids,
        } if requester_agent_id == agent
            && factory_id == factory
            && recipe_id == recipe
            && plan.accepted_batches == batches
            && logistics_route_ids.is_empty()
            && logistics_path_ids.is_empty() =>
        {
            Ok(())
        }
        _ => Err("context already has a different original action; query its result".into()),
    }
}

pub(super) fn validate_original_action(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    action: &crate_action::Action,
) -> Result<(), String> {
    let WorldServicePayloadV1::Cognition(signed) = &request.signed_payload else {
        return Err("invalid cached action".into());
    };
    validate_action_identity(&signed.request.action, action)
}
pub(super) fn validate_action_identity(
    original: &crate_action::Action,
    proposed: &crate_action::Action,
) -> Result<(), String> {
    if original != proposed {
        return Err("context already has a different original action; query its result".into());
    }
    Ok(())
}
pub(super) fn bounded_collection(value: Value) -> Value {
    match value {
        Value::Object(values) => {
            let count = values.len();
            let items: serde_json::Map<_, _> = values.into_iter().take(128).collect();
            json!({"items":items,"count":count,"truncated":count>128})
        }
        Value::Array(values) => {
            let count = values.len();
            let items: Vec<_> = values.into_iter().take(128).collect();
            json!({"items":items,"count":count,"truncated":count>128})
        }
        other => json!({"value":other,"truncated":false}),
    }
}

/// Bound each business table independently; an outer object cap is insufficient.
pub(super) fn bounded_world_rules(value: Value) -> Value {
    match value {
        Value::Object(tables) => Value::Object(
            tables
                .into_iter()
                .map(|(key, value)| (key, bounded_collection(value)))
                .collect(),
        ),
        _ => Value::Null,
    }
}
