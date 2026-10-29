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
            ActionTag::EmitResourceTransfer,
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
            ActionTag::BuildFactory,
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
            ActionTag::BuildFactoryWithModule,
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
            ActionTag::MaintainFactory,
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
            ActionTag::RecycleFactory,
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
            ActionTag::ScheduleEssentialRecipe,
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
            ActionTag::ScheduleRecipe,
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
            ActionTag::PauseFactoryProduction,
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
            ActionTag::ScheduleRecipeWithModule,
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
            ActionTag::ValidateProduct,
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
            ActionTag::ValidateProductWithModule,
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
            ActionTag::GovernMaterialProfile,
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
            ActionTag::GovernProductProfile,
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
            ActionTag::GovernRecipeProfile,
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
            ActionTag::GovernFactoryProfile,
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
