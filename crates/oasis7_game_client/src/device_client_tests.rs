use super::*;
#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn unresolved_contexts_never_advise_a_replacement_operation() {
        for outcome in [
            IntentOutcome::<Value>::Unknown,
            IntentOutcome::Pending,
            IntentOutcome::HistoryUnavailable,
            IntentOutcome::Received {
                durability: AdmissionDurability::Volatile,
            },
            IntentOutcome::Received {
                durability: AdmissionDurability::Durable,
            },
        ] {
            assert_eq!(context_next_step(&outcome), "lookup_original");
        }
        for outcome in [
            IntentOutcome::<Value>::Rejected {
                reason: WorldServiceErrorKind::Conflict,
            },
            IntentOutcome::Expired,
        ] {
            assert_eq!(context_next_step(&outcome), "refresh_after_settled_result");
        }
    }
}

#[cfg(test)]
mod action_tests {
    use super::*;
    #[test]
    fn cached_schedule_recovery_compares_selection_without_requoting_original() {
        let recipe = "recipe.smelter.iron_ingot";
        let original = crate_action::Action::ScheduleRecipe {
            requester_agent_id: "configured".into(),
            factory_id: "smelter".into(),
            recipe_id: recipe.into(),
            plan: oasis7_agent_api::world_quote::builtin_recipe_plan(recipe, 3).unwrap(),
            logistics_route_ids: vec![],
            logistics_path_ids: vec![],
        };
        assert!(
            validate_cached_schedule_action(&original, "configured", "smelter", recipe, 3).is_ok()
        );
        for (agent, factory, recipe, batches) in [
            ("other", "smelter", recipe, 3),
            ("configured", "other", recipe, 3),
            ("configured", "smelter", "recipe.assembler.gear", 3),
            ("configured", "smelter", recipe, 4),
        ] {
            assert!(
                validate_cached_schedule_action(&original, agent, factory, recipe, batches)
                    .is_err()
            );
        }
        let mut changed = original.clone();
        let crate_action::Action::ScheduleRecipe {
            logistics_route_ids,
            ..
        } = &mut changed
        else {
            unreachable!()
        };
        logistics_route_ids.push("injected".into());
        assert!(
            validate_cached_schedule_action(&changed, "configured", "smelter", recipe, 3).is_err()
        );
        assert!(
            validate_cached_schedule_action(&transfer(), "configured", "smelter", recipe, 3)
                .is_err()
        );
    }
    fn transfer() -> crate_action::Action {
        crate_action::Action::TransferMaterial {
            requester_agent_id: "configured".into(),
            from_ledger: oasis7_agent_api::world_action::MaterialLedgerId::agent("configured"),
            to_ledger: oasis7_agent_api::world_action::MaterialLedgerId::factory("smelter"),
            kind: "iron_ore".into(),
            amount: 2,
            distance_km: 1,
            priority: Some(crate_action::MaterialTransitPriority::Standard),
            route_id: None,
            route_ids: vec!["a".into(), "b".into()],
            auto_reroute: false,
        }
    }
    #[test]
    fn original_transfer_identity_protects_every_business_field_and_action_kind() {
        let original = transfer();
        assert!(validate_action_identity(&original, &original).is_ok());
        for field in 0..10 {
            let mut changed = transfer();
            let crate_action::Action::TransferMaterial {
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                amount,
                distance_km,
                priority,
                route_id,
                route_ids,
                auto_reroute,
            } = &mut changed
            else {
                unreachable!()
            };
            match field {
                0 => *requester_agent_id = "foreign".into(),
                1 => *from_ledger = oasis7_agent_api::world_action::MaterialLedgerId::world(),
                2 => *to_ledger = oasis7_agent_api::world_action::MaterialLedgerId::site("other"),
                3 => *kind = "copper".into(),
                4 => *amount += 1,
                5 => *distance_km += 1,
                6 => *priority = Some(crate_action::MaterialTransitPriority::Urgent),
                7 => *route_id = Some("a".into()),
                8 => route_ids.reverse(),
                9 => *auto_reroute = true,
                _ => unreachable!(),
            }
            assert!(validate_action_identity(&original, &changed).is_err());
        }
        let movement = crate_action::Action::MoveAgent {
            agent_id: "configured".into(),
            to: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        };
        assert!(validate_action_identity(&original, &movement).is_err());
        assert!(validate_action_identity(&movement, &original).is_err());
    }
    #[test]
    fn industry_truncation_reports_omitted_entries() {
        let value = bounded_collection(json!((0..129).collect::<Vec<_>>()));
        assert_eq!(value["items"].as_array().unwrap().len(), 128);
        assert_eq!(value["count"], 129);
        assert_eq!(value["truncated"], true);
    }
}

#[cfg(test)]
mod native_capability_tests {
    use super::*;
    use oasis7_agent_api::world_quote::{AgentDelegationAuthorizationV1, AgentDelegationGrantV1};

    fn grant(revoked: bool, valid_until_tick: u64) -> AgentDelegationAuthorizationV1 {
        AgentDelegationAuthorizationV1 {
            grant: AgentDelegationGrantV1 {
                grant_id: "grant-a".into(),
                source_id: "source".into(),
                issuer_id: "issuer".into(),
                owner_id: "owner".into(),
                organization_id: None,
                agent_id: "agent-a".into(),
                object_id: "agent-a".into(),
                action_kinds: vec!["MoveAgent".into()],
                revision: 1,
                period_id: "period".into(),
                valid_from_tick: 0,
                valid_until_tick,
                limit_units: 7,
                resource_kind: "electricity".into(),
                revoked,
            },
            cost_units: 0,
            spent_units: 0,
            remaining_units: 7,
        }
    }
    fn response(grants: &[AgentDelegationAuthorizationV1]) -> Value {
        let (_, fixture) = crate::verified_task_view::tests::fixture("agent:agent-a");
        capability_response(&fixture.version, 4, grants, &None, &None)
    }
    #[test]
    fn native_capabilities_empty_business_grants_require_native_context_confirmation() {
        let result = response(&[]);
        assert_eq!(
            result["native_action_validation"],
            "needs_native_context_confirmation"
        );
        assert_eq!(result["native_authorization_count"], 0);
        assert_eq!(result["native_authorizations_truncated"], false);
        assert_eq!(result["native_delegation_authorizations"], json!([]));
        assert!(result["module_capability_catalog"].is_null());
        assert!(
            result["proposal_forms"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["admission_required"] == true)
        );
    }
    #[test]
    fn legacy_observation_omitting_business_grants_needs_context_confirmation() {
        let legacy: oasis7_client_api::world_read_observation::WorldReadObservationV1 = serde_json::from_value(json!({
            "state":{"time":4,"agents":{}},"runtime_binding":null,"agent_context":null,
            "scheduler_wakes":[],"continuations":[],"cognition_leases":[],"continuation_contexts":{},"events":[]
        })).unwrap();
        assert!(legacy.native_delegation_authorizations.is_empty());
        let result = response(&legacy.native_delegation_authorizations);
        assert_eq!(
            result["native_action_validation"],
            "needs_native_context_confirmation"
        );
    }
    #[test]
    fn native_capabilities_revoked_and_expired_grants_still_scope_validation() {
        for (authorization, status) in [(grant(true, 100), "revoked"), (grant(false, 3), "expired")]
        {
            let result = response(&[authorization]);
            assert_eq!(result["native_action_validation"], "grant_scoped");
            assert_eq!(result["native_authorization_count"], 1);
            assert_eq!(
                result["native_delegation_authorizations"][0]["status"],
                status
            );
            assert_eq!(
                result["native_delegation_authorizations"][0]["admission_required"],
                true
            );
        }
    }
    #[test]
    fn native_capabilities_mode_uses_full_unfiltered_list_and_preserves_display_bound() {
        let mut grants = vec![grant(true, 3); 129];
        grants[128] = grant(false, 100);
        let result = response(&grants);
        assert_eq!(result["native_action_validation"], "grant_scoped");
        assert_eq!(result["native_authorization_count"], 129);
        assert_eq!(result["native_authorizations_truncated"], true);
        let displayed = result["native_delegation_authorizations"]
            .as_array()
            .unwrap();
        assert_eq!(displayed.len(), 128);
        assert!(displayed.iter().all(|g| g["status"] == "revoked"));
        assert!(result["module_capability_catalog"].is_null());
    }
}
