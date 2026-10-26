use crate::runtime::WorldState;
#[cfg(test)]
use crate::runtime::{
    MaterialLedgerId, STARTER_INDUSTRIAL_PROFILE_ID, STARTER_SMELTER_FACTORY_ID,
    STARTER_SMELTER_RECIPE_ID,
};
use crate::runtime::{StarterIndustrialFeasibilityResult, StarterIndustrialFeasibilityStatus};
use crate::runtime_host::archive::PlayerStarterIndustrialSettledOutcome;
use crate::runtime_host::archive::{
    PlayerStarterIndustrialFeasibility, PlayerStarterIndustrialFeasibilityStatus,
};

/// Project only a recorded settlement for the currently controlled owner.
/// Current balances and mutable recipe profiles cannot reconstruct this fact.
pub(super) fn settled_outcome(
    state: &WorldState,
    controlled_agent_id: Option<&str>,
) -> Option<PlayerStarterIndustrialSettledOutcome> {
    oasis7_world_state::gameplay_capability::settled_outcome(state, controlled_agent_id).map(
        |fact| PlayerStarterIndustrialSettledOutcome {
            settlement_job_id: fact.settlement_job_id,
            settled_at: fact.settled_at,
            output_ledger: fact.output_ledger,
            settlement: fact.settlement,
        },
    )
}

pub(super) fn player_starter_industrial_feasibility(
    result: &StarterIndustrialFeasibilityResult,
    state: &WorldState,
    controlled_agent_id: Option<&str>,
) -> PlayerStarterIndustrialFeasibility {
    PlayerStarterIndustrialFeasibility {
        settled_outcome: super::starter_industrial_outcome::settled_outcome(
            state,
            controlled_agent_id,
        ),
        profile_id: result.profile_id.clone(),
        profile_revision: result.profile_revision,
        authority_snapshot: result.authority_snapshot.clone(),
        status: match result.status {
            StarterIndustrialFeasibilityStatus::CandidateAvailable => {
                PlayerStarterIndustrialFeasibilityStatus::CandidateAvailable
            }
            StarterIndustrialFeasibilityStatus::NoSafeStarterChain => {
                PlayerStarterIndustrialFeasibilityStatus::NoSafeStarterChain
            }
        },
        evidence_class: result.evidence_class.clone(),
        completion_boundary: result.completion_boundary.clone(),
        blocker: result.blocker.clone(),
        next_action: result.next_action.clone(),
        next_recheck: result.next_recheck,
        progression_effect: result.progression_effect.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        FactorySiteAuthorityV1, StarterIndustrialMilestoneV1, StarterIndustrialSettlementSummaryV1,
    };
    use oasis7_wasm_abi::MaterialStack;

    fn fixture() -> WorldState {
        let mut state = WorldState::default();
        let origin = crate::runtime::CommittedRecipeOrigin {
            submission: crate::runtime::GameplaySubmissionOrigin {
                verified_player_id: "browser-player".into(),
                public_key: "a".repeat(64),
                auth_nonce: 7,
                hosted_registration_nonce: None,
                requester_agent_id: "owner".into(),
                factory_id: STARTER_SMELTER_FACTORY_ID.into(),
                recipe_id: STARTER_SMELTER_RECIPE_ID.into(),
            },
            consensus_action_id: 2,
            consensus_submitter_player_id: "node-transport".into(),
            action_payload_hash: "b".repeat(64),
            committed_height: 4,
            action_root: "c".repeat(64),
        };
        state.factories.insert(STARTER_SMELTER_FACTORY_ID.into(), serde_json::from_value(serde_json::json!({
            "factory_id": STARTER_SMELTER_FACTORY_ID, "site_id":"s", "builder_agent_id":"owner", "built_at":1,
            "output_ledger":"site:s", "spec": {"factory_id":STARTER_SMELTER_FACTORY_ID,"display_name":"Smelter","tier":1,"build_time_ticks":1,"base_power_draw":1,"recipe_slots":1,"throughput_bps":10000,"maintenance_per_tick":0}
        })).unwrap());
        state.factory_site_authorities.insert(
            "s".into(),
            FactorySiteAuthorityV1 {
                site_id: "s".into(),
                owner_agent_id: "owner".into(),
                active: true,
                ..Default::default()
            },
        );
        state
            .material_ledgers
            .insert(MaterialLedgerId::site("s"), Default::default());
        state.industry_progress.starter_industrial_milestone = Some(StarterIndustrialMilestoneV1 {
            settlement_summary: Some(StarterIndustrialSettlementSummaryV1 {
                requester_agent_id: "owner".into(),
                accepted_batches: 12,
                consume: vec![MaterialStack::new("iron_ore", 48)],
                power_required: 24,
                produce: vec![MaterialStack::new("iron_ingot", 36)],
            }),
            committed_recipe_origin: Some(origin),
            profile_id: STARTER_INDUSTRIAL_PROFILE_ID.into(),
            profile_revision: 1,
            factory_id: STARTER_SMELTER_FACTORY_ID.into(),
            recipe_id: STARTER_SMELTER_RECIPE_ID.into(),
            output_ledger: MaterialLedgerId::site("s"),
            settlement_job_id: 5,
            settled_at: 27,
        });
        state
    }

    #[test]
    fn starter_settled_outcome_survives_receipt_pruning_and_rejects_foreign_owner() {
        let mut state = fixture();
        let expected = settled_outcome(&state, Some("owner")).unwrap();
        assert_eq!(expected.settlement.produce[0].amount, 36);
        assert_eq!(expected.settlement.consume[0].amount, 48);
        assert_eq!(expected.settlement.power_required, 24);
        state.recipe_completion_receipts.clear();
        assert_eq!(
            settled_outcome(&state, Some("owner")),
            Some(expected.clone())
        );
        assert!(settled_outcome(&state, Some("foreign")).is_none());
        state
            .factories
            .get_mut(STARTER_SMELTER_FACTORY_ID)
            .unwrap()
            .production
            .status = crate::runtime::FactoryProductionStatus::Paused;
        let paused = state.gameplay_capability_query(
            oasis7_world_state::gameplay_capability::GameplaySubject::Personal {
                owner_agent_id: "owner".into(),
            },
            "current_industrial_capability",
        );
        assert_eq!(paused.current_stable_line, Some(false));
        assert_eq!(
            paused.first_settlement.unwrap().settlement_job_id,
            expected.settlement_job_id
        );

        let mut mismatched_origin = state.clone();
        mismatched_origin
            .industry_progress
            .starter_industrial_milestone
            .as_mut()
            .unwrap()
            .committed_recipe_origin
            .as_mut()
            .unwrap()
            .submission
            .requester_agent_id = "foreign".into();
        assert!(settled_outcome(&mismatched_origin, Some("owner")).is_none());
        state
            .factory_site_authorities
            .get_mut("s")
            .unwrap()
            .owner_agent_id = "foreign".into();
        assert!(settled_outcome(&state, Some("owner")).is_none());
    }

    #[test]
    fn starter_settled_outcome_legacy_absence_is_omitted_without_backfill() {
        let mut state = fixture();
        let milestone = state
            .industry_progress
            .starter_industrial_milestone
            .as_mut()
            .unwrap();
        milestone.settlement_summary = None;
        let legacy = serde_json::to_value(&*milestone).unwrap();
        assert!(legacy.get("settlement_summary").is_none());
        let restored: StarterIndustrialMilestoneV1 =
            serde_json::from_value(legacy.clone()).unwrap();
        assert!(restored.settlement_summary.is_none());
        assert_eq!(serde_json::to_value(restored).unwrap(), legacy);
        assert!(settled_outcome(&state, Some("owner")).is_none());
    }
}
