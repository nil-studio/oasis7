//! Construct the ordinary claim from an authenticated typed offer observation.
use crate::{encode, parse};
use oasis7_agent_api::world_action::{
    Action, OrdinaryStarterCommandV1, OrdinaryStarterStatusV1, WorldRuleCommandV1,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimInput {
    status_json: String,
    agent_id: String,
    offer_id: String,
    slot_id: String,
    request_id: String,
}

pub(super) fn claim(input: &str) -> Result<String, &'static str> {
    let input: ClaimInput = parse(input)?;
    let status: OrdinaryStarterStatusV1 = parse(&input.status_json)?;
    if !status.eligible
        || status.blocker.is_some()
        || status.claim.is_some()
        || input.agent_id.trim().is_empty()
        || input.request_id.trim().is_empty()
        || input.request_id.len() > 128
        || input.agent_id.len() > 128
        || status
            .owner_generation
            .is_none_or(|generation| generation == 0)
        || status
            .owner_binding
            .as_ref()
            .is_none_or(|owner| owner.trim().is_empty())
    {
        return Err("ordinary_starter_not_eligible");
    }
    let offered = status
        .offers
        .get(&input.offer_id)
        .ok_or("ordinary_starter_offer_missing")?;
    if offered.blocker.is_some()
        || !offered.available_slots.contains(&input.slot_id)
        || !offered.offer.slots.contains_key(&input.slot_id)
        || status.world_binding.as_ref() != Some(&offered.offer.world_binding)
    {
        return Err("ordinary_starter_offer_unavailable");
    }
    encode(&Action::WorldRule {
        actor_id: input.agent_id,
        request_id: input.request_id,
        command: WorldRuleCommandV1::OrdinaryStarter {
            command: Box::new(OrdinaryStarterCommandV1::Claim {
                offer_id: input.offer_id,
                offer_revision: offered.offer.revision,
                slot_id: input.slot_id,
                offer_digest: offered.offer_digest.clone(),
                owner_binding: status
                    .owner_binding
                    .ok_or("ordinary_starter_owner_missing")?,
                owner_generation: status
                    .owner_generation
                    .ok_or("ordinary_starter_owner_missing")?,
            }),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_agent_api::world_quote::builtin_factory_spec;
    use serde_json::json;

    #[test]
    fn claim_codec_binds_observed_owner_offer_and_slot_and_rejects_stale_availability() {
        let spec = builtin_factory_spec("factory.smelter.mk1", "factory").unwrap();
        let mut status = json!({
            "world_binding":"bound-world", "eligible":true, "blocker":null,
            "owner_binding":"owner", "owner_generation":7, "claim":null,
            "offers":{"offer":{
                "offer":{"offer_id":"offer", "revision":1, "world_binding":"bound-world",
                    "supplier_agent_id":"supplier", "profile_id":"ordinary-industrial-starter-v1",
                    "profile_revision":1, "factory_kind":"factory.smelter.mk1",
                    "recipe_id":"recipe.smelter.iron_ingot", "input_batches":2,
                    "materials":{"iron_ore":8}, "recipe_electricity":16,
                    "dependency_digest":"recipe-digest", "published_at":3,
                    "slots":{"slot":{"slot_id":"slot", "site_id":"site", "factory_id":"factory",
                        "location_id":"yard", "location_anchor_revision":1, "site_authority_revision":1,
                        "factory_profile":{"factory_id":"factory", "tier":spec.tier,
                            "recipe_slots":spec.recipe_slots, "tags":spec.tags},
                        "construction_power_revision":1, "construction_electricity":10}}},
                "offer_digest":"observed-digest", "available_slots":["slot"],
                "supplier_materials":{"iron_ore":64}, "supplier_electricity":208, "blocker":null
            }}
        });
        let build =
            |status: &serde_json::Value| {
                crate::build::build("ordinary_starter_claim", &json!({
            "status_json":status.to_string(), "agent_id":"participant", "offer_id":"offer",
            "slot_id":"slot", "request_id":"player-confirmation"
        }).to_string())
            };
        let action: Action = parse(&build(&status).unwrap()).unwrap();
        let Action::WorldRule {
            actor_id,
            request_id,
            command,
        } = action
        else {
            panic!("native WorldRule required")
        };
        assert_eq!(actor_id, "participant");
        assert_eq!(request_id, "player-confirmation");
        let WorldRuleCommandV1::OrdinaryStarter { command } = command else {
            panic!("typed ordinary starter required")
        };
        assert_eq!(
            *command,
            OrdinaryStarterCommandV1::Claim {
                offer_id: "offer".into(),
                offer_revision: 1,
                slot_id: "slot".into(),
                offer_digest: "observed-digest".into(),
                owner_binding: "owner".into(),
                owner_generation: 7,
            }
        );
        status["offers"]["offer"]["available_slots"] = json!([]);
        assert_eq!(build(&status), Err("ordinary_starter_offer_unavailable"));
        status["eligible"] = json!(false);
        assert_eq!(build(&status), Err("ordinary_starter_not_eligible"));
    }
}
