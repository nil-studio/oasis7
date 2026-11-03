// Domain-scoped Action dispatch; payload field serialization is derived.
use super::{Action, ActionTag, payloads, serialize_action_envelope};
use serde::ser::{Error as _, Serializer};

pub(super) fn serialize<S>(action: &Action, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match action {
        Action::EmitResourceTransfer {
            from_agent_id,
            to_agent_id,
            kind,
            amount,
        } => serialize_action_envelope(
            serializer,
            ActionTag::EMIT_RESOURCE_TRANSFER,
            payloads::EmitResourceTransferPayload {
                from_agent_id,
                to_agent_id,
                kind,
                amount,
            },
        ),
        Action::BuildFactory {
            builder_agent_id,
            site_id,
            spec,
        } => serialize_action_envelope(
            serializer,
            ActionTag::BUILD_FACTORY,
            payloads::BuildFactoryPayload {
                builder_agent_id,
                site_id,
                spec,
            },
        ),
        Action::BuildFactoryWithModule {
            builder_agent_id,
            site_id,
            module_id,
            spec,
        } => serialize_action_envelope(
            serializer,
            ActionTag::BUILD_FACTORY_WITH_MODULE,
            payloads::BuildFactoryWithModulePayload {
                builder_agent_id,
                site_id,
                module_id,
                spec,
            },
        ),
        Action::MaintainFactory {
            operator_agent_id,
            factory_id,
            parts,
        } => serialize_action_envelope(
            serializer,
            ActionTag::MAINTAIN_FACTORY,
            payloads::MaintainFactoryPayload {
                operator_agent_id,
                factory_id,
                parts,
            },
        ),
        Action::RecycleFactory {
            operator_agent_id,
            factory_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::RECYCLE_FACTORY,
            payloads::RecycleFactoryPayload {
                operator_agent_id,
                factory_id,
            },
        ),
        Action::ScheduleEssentialRecipe {
            requester_agent_id,
            factory_id,
            recipe_id,
            batches,
            obligation_id,
        } => serialize_action_envelope(
            serializer,
            ActionTag::SCHEDULE_ESSENTIAL_RECIPE,
            payloads::ScheduleEssentialRecipePayload {
                requester_agent_id,
                factory_id,
                recipe_id,
                batches,
                obligation_id,
            },
        ),
        Action::ScheduleRecipe {
            requester_agent_id,
            factory_id,
            recipe_id,
            plan,
            logistics_route_ids,
            logistics_path_ids,
        } => serialize_action_envelope(
            serializer,
            ActionTag::SCHEDULE_RECIPE,
            payloads::ScheduleRecipePayload {
                requester_agent_id,
                factory_id,
                recipe_id,
                plan,
                logistics_route_ids,
                logistics_path_ids,
            },
        ),
        Action::PauseFactoryProduction {
            requester_agent_id,
            factory_id,
            reason,
        } => serialize_action_envelope(
            serializer,
            ActionTag::PAUSE_FACTORY_PRODUCTION,
            payloads::PauseFactoryProductionPayload {
                requester_agent_id,
                factory_id,
                reason,
            },
        ),
        Action::ScheduleRecipeWithModule {
            requester_agent_id,
            factory_id,
            recipe_id,
            module_id,
            desired_batches,
            deterministic_seed,
        } => serialize_action_envelope(
            serializer,
            ActionTag::SCHEDULE_RECIPE_WITH_MODULE,
            payloads::ScheduleRecipeWithModulePayload {
                requester_agent_id,
                factory_id,
                recipe_id,
                module_id,
                desired_batches,
                deterministic_seed,
            },
        ),
        Action::ValidateProduct {
            requester_agent_id,
            module_id,
            stack,
            decision,
        } => serialize_action_envelope(
            serializer,
            ActionTag::VALIDATE_PRODUCT,
            payloads::ValidateProductPayload {
                requester_agent_id,
                module_id,
                stack,
                decision,
            },
        ),
        Action::ValidateProductWithModule {
            requester_agent_id,
            module_id,
            stack,
            deterministic_seed,
        } => serialize_action_envelope(
            serializer,
            ActionTag::VALIDATE_PRODUCT_WITH_MODULE,
            payloads::ValidateProductWithModulePayload {
                requester_agent_id,
                module_id,
                stack,
                deterministic_seed,
            },
        ),
        Action::GovernMaterialProfile {
            operator_agent_id,
            proposal_id,
            profile,
        } => serialize_action_envelope(
            serializer,
            ActionTag::GOVERN_MATERIAL_PROFILE,
            payloads::GovernMaterialProfilePayload {
                operator_agent_id,
                proposal_id,
                profile,
            },
        ),
        Action::GovernProductProfile {
            operator_agent_id,
            proposal_id,
            profile,
        } => serialize_action_envelope(
            serializer,
            ActionTag::GOVERN_PRODUCT_PROFILE,
            payloads::GovernProductProfilePayload {
                operator_agent_id,
                proposal_id,
                profile,
            },
        ),
        Action::GovernRecipeProfile {
            operator_agent_id,
            proposal_id,
            profile,
        } => serialize_action_envelope(
            serializer,
            ActionTag::GOVERN_RECIPE_PROFILE,
            payloads::GovernRecipeProfilePayload {
                operator_agent_id,
                proposal_id,
                profile,
            },
        ),
        Action::GovernFactoryProfile {
            operator_agent_id,
            proposal_id,
            profile,
        } => serialize_action_envelope(
            serializer,
            ActionTag::GOVERN_FACTORY_PROFILE,
            payloads::GovernFactoryProfilePayload {
                operator_agent_id,
                proposal_id,
                profile,
            },
        ),
        _ => Err(S::Error::custom(
            "Action dispatched to wrong industry serializer",
        )),
    }
}
