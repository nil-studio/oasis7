use super::*;

fn recipe_grant(id: &str) -> AgentDelegationGrantV1 {
    let mut g = grant();
    g.grant_id = id.into();
    g.object_id = "factory".into();
    g.action_kinds = vec!["ScheduleRecipe".into()];
    g.limit_units = 12;
    g
}

#[test]
fn replacement_selection_keeps_true_ambiguity_and_current_authority_boundaries() {
    for boundary in [
        "ambiguous",
        "revoked",
        "expired",
        "future",
        "budget",
        "owner",
        "source",
        "scope",
    ] {
        let mut world = recipe_fixture();
        world.step().unwrap();
        let mut old = recipe_grant("old");
        old.revoked = boundary != "ambiguous";
        old.revision = 2;
        world.install_agent_delegation_grant(old).unwrap();
        let mut current = recipe_grant("replacement");
        match boundary {
            "revoked" => current.revoked = true,
            "expired" => current.valid_until_tick = world.state().time - 1,
            "future" => current.valid_from_tick = world.state().time + 1,
            "budget" => current.limit_units = 7,
            "scope" => current.object_id = "other-factory".into(),
            _ => {}
        }
        world.install_agent_delegation_grant(current).unwrap();
        if matches!(boundary, "owner" | "source") {
            let mut snapshot = world.snapshot();
            let field = if boundary == "owner" {
                "owner_id"
            } else {
                "source_id"
            };
            snapshot.cognition["agent_delegation"]["grants"]["replacement"][field] =
                json!("different-authority");
            world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
        }
        let before = serde_json::to_value(world.snapshot()).unwrap();
        let error = world
            .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 1)
            .unwrap_err();
        let expected = match boundary {
            "ambiguous" => "grant_selection_ambiguous",
            "revoked" | "scope" => "grant_revoked",
            "expired" | "future" => "grant_or_quote_expired",
            "budget" => "budget_exhausted",
            "owner" | "source" => "owner_source_changed",
            _ => unreachable!(),
        };
        assert!(
            format!("{error:?}").contains(expected),
            "{boundary}: {error:?}"
        );
        assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
        let state = world.state().clone();
        let journal = world.journal().clone();
        let error = world
            .bind_agent_causal_decision(
                "denied-replacement",
                AGENT_ID,
                &recipe(),
                Default::default(),
            )
            .unwrap_err();
        assert!(
            format!("{error:?}").contains(expected),
            "{boundary}: {error:?}"
        );
        assert_eq!(world.state(), &state, "{boundary}");
        assert_eq!(world.journal(), &journal, "{boundary}");
        assert!(world.agent_causal_receipts(AGENT_ID).unwrap().is_empty());
        assert!(
            world
                .agent_delegation_authorizations(AGENT_ID)
                .unwrap()
                .iter()
                .all(|a| a.spent_units == 0)
        );
    }
}
