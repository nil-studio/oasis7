use crate::runtime::{
    AgentClaimState, M1_BODY_MODULE_ID, M1_MEMORY_MODULE_ID, M1_MOBILITY_MODULE_ID,
    M1_MOVE_RULE_MODULE_ID, M1_RADIATION_POWER_MODULE_ID, M1_SENSOR_MODULE_ID,
    M1_STORAGE_CARGO_MODULE_ID, M1_STORAGE_POWER_MODULE_ID, M1_TRANSFER_RULE_MODULE_ID,
    M1_VISIBILITY_RULE_MODULE_ID, MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
    RestrictedStarterClaimGrantStatus, WorldState, agent_claim_cap_for_tier, agent_claim_quote,
    agent_claim_reputation_tier,
};
use crate::simulator::persist::{
    PlayerAgentClaimChoiceCandidateSnapshot, PlayerAgentClaimChoiceQuoteSnapshot,
    PlayerAgentClaimOwnedSnapshot, PlayerAgentClaimQuoteSnapshot, PlayerAgentClaimSnapshot,
    PlayerFirstChatUnlockPreviewSnapshot,
};

const STARTER_OC_CHAT_GATE_EXPLANATION: &str = concat!(
    "First chat passes the starter OC gate with a positive liquid OC balance ",
    "or an existing starter OC claim record. An existing starter OC record keeps ",
    "this gate open even after the balance is spent.",
);

pub(super) fn build_player_agent_claim_snapshot(
    state: &WorldState,
    primary_agent_id: &str,
    epoch_length_ticks: u64,
) -> Option<PlayerAgentClaimSnapshot> {
    if !state.agents.contains_key(primary_agent_id) {
        return None;
    }

    let current_epoch = agent_claim_epoch(state.time, epoch_length_ticks);
    let owned_claims_count = state
        .agent_claims
        .values()
        .filter(|claim| claim.claim_owner_id == primary_agent_id)
        .count();
    let reputation_score = state
        .reputation_scores
        .get(primary_agent_id)
        .copied()
        .unwrap_or(0);
    let liquid_main_token_balance = state
        .main_token_balances
        .get(primary_agent_id)
        .map(|balance| balance.liquid_balance)
        .unwrap_or(0);
    let restricted_starter_claim_balance = state
        .main_token_balances
        .get(primary_agent_id)
        .map(|balance| balance.restricted_starter_claim_balance)
        .unwrap_or(0);
    let has_active_restricted_grant = state
        .restricted_starter_claim_grants
        .get(primary_agent_id)
        .is_some_and(|grant| grant.status == RestrictedStarterClaimGrantStatus::Issued);
    let liveops_pool_balance = state
        .main_token_treasury_balances
        .get(MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL)
        .copied()
        .unwrap_or(0);

    let next_claim_quote = match agent_claim_quote(reputation_score, owned_claims_count) {
        Ok(quote) => {
            let total_upfront_amount = quote
                .activation_fee_amount
                .saturating_add(quote.claim_bond_amount)
                .saturating_add(quote.upkeep_per_epoch);
            let auto_restricted_starter_claim_amount = quote.preview_restricted_starter_grant(
                liquid_main_token_balance,
                restricted_starter_claim_balance,
                liveops_pool_balance,
                has_active_restricted_grant,
            );
            let eligible_claim_balance = if quote.slot_index == 1 {
                liquid_main_token_balance
                    .saturating_add(restricted_starter_claim_balance)
                    .saturating_add(auto_restricted_starter_claim_amount)
            } else {
                liquid_main_token_balance
            };
            let blocked_reason = if quote.slot_index > 1
                && liquid_main_token_balance < total_upfront_amount
                && restricted_starter_claim_balance > 0
            {
                Some(format!(
                    "restricted_balance_not_eligible_for_slot slot={} liquid={} restricted={} required={}",
                    quote.slot_index,
                    liquid_main_token_balance,
                    restricted_starter_claim_balance,
                    total_upfront_amount
                ))
            } else if eligible_claim_balance < total_upfront_amount {
                Some(format!(
                    "insufficient_claim_eligible_main_token eligible={} liquid={} restricted={} required={}",
                    eligible_claim_balance,
                    liquid_main_token_balance,
                    restricted_starter_claim_balance,
                    total_upfront_amount
                ))
            } else {
                None
            };
            let eligible_balance_after = if blocked_reason.is_none() {
                eligible_claim_balance.saturating_sub(total_upfront_amount)
            } else {
                0
            };
            let upkeep_runway_epochs = if blocked_reason.is_none() && quote.upkeep_per_epoch > 0 {
                eligible_balance_after / quote.upkeep_per_epoch
            } else {
                0
            };
            let next_upkeep_due_epoch = blocked_reason
                .is_none()
                .then(|| current_epoch.saturating_add(1));
            let projected_grace_entry_epoch = next_upkeep_due_epoch
                .map(|next_due_epoch| next_due_epoch.saturating_add(upkeep_runway_epochs));
            let low_runway_warning =
                blocked_reason.is_none() && upkeep_runway_epochs < quote.grace_epochs;
            let recommended_claim_action = blocked_reason.is_none().then(|| {
                if low_runway_warning {
                    "wait_or_fund_first"
                } else {
                    "compare_candidates_first"
                }
                .to_string()
            });
            let slot_1_claim_choice_quote = (quote.slot_index == 1).then(|| {
                build_slot_1_claim_choice_quote(
                    state,
                    primary_agent_id,
                    total_upfront_amount,
                    eligible_claim_balance,
                    upkeep_runway_epochs,
                    quote.grace_epochs,
                    blocked_reason.is_none(),
                )
            });
            Some(PlayerAgentClaimQuoteSnapshot {
                slot_index: quote.slot_index,
                reputation_tier: quote.reputation_tier,
                claim_cap: quote.claim_cap,
                owned_claim_count: u8::try_from(owned_claims_count).unwrap_or(u8::MAX),
                activation_fee_amount: quote.activation_fee_amount,
                claim_bond_amount: quote.claim_bond_amount,
                upkeep_per_epoch: quote.upkeep_per_epoch,
                total_upfront_amount,
                transferable_liquid_balance: liquid_main_token_balance,
                restricted_starter_claim_balance,
                auto_restricted_starter_claim_amount,
                eligible_claim_balance,
                eligible_balance_after,
                upkeep_runway_epochs,
                next_upkeep_due_epoch,
                projected_grace_entry_epoch,
                low_runway_warning,
                recommended_claim_action,
                slot_1_claim_choice_quote,
                release_cooldown_epochs: quote.release_cooldown_epochs,
                grace_epochs: quote.grace_epochs,
                idle_warning_epochs: quote.idle_warning_epochs,
                forced_idle_reclaim_epochs: quote.forced_idle_reclaim_epochs,
                forced_reclaim_penalty_bps: quote.forced_reclaim_penalty_bps,
                blocked_reason,
            })
        }
        Err(reason) => Some(PlayerAgentClaimQuoteSnapshot {
            slot_index: 0,
            reputation_tier: agent_claim_reputation_tier(reputation_score),
            claim_cap: agent_claim_cap_for_tier(agent_claim_reputation_tier(reputation_score)),
            owned_claim_count: u8::try_from(owned_claims_count).unwrap_or(u8::MAX),
            activation_fee_amount: 0,
            claim_bond_amount: 0,
            upkeep_per_epoch: 0,
            total_upfront_amount: 0,
            transferable_liquid_balance: liquid_main_token_balance,
            restricted_starter_claim_balance,
            auto_restricted_starter_claim_amount: 0,
            eligible_claim_balance: liquid_main_token_balance
                .saturating_add(restricted_starter_claim_balance),
            eligible_balance_after: 0,
            upkeep_runway_epochs: 0,
            next_upkeep_due_epoch: None,
            projected_grace_entry_epoch: None,
            low_runway_warning: false,
            recommended_claim_action: None,
            slot_1_claim_choice_quote: None,
            release_cooldown_epochs: 0,
            grace_epochs: 0,
            idle_warning_epochs: 0,
            forced_idle_reclaim_epochs: 0,
            forced_reclaim_penalty_bps: 0,
            blocked_reason: Some(reason),
        }),
    };

    let mut owned_claims = state
        .agent_claims
        .values()
        .filter(|claim| claim.claim_owner_id == primary_agent_id)
        .cloned()
        .collect::<Vec<_>>();
    owned_claims.sort_by(|left, right| left.target_agent_id.cmp(&right.target_agent_id));
    let slot_1_auto_restricted_starter_claim_amount = next_claim_quote
        .as_ref()
        .filter(|quote| quote.slot_index == 1)
        .map(|quote| quote.auto_restricted_starter_claim_amount)
        .unwrap_or(0);
    let first_chat_unlock_preview = (liquid_main_token_balance == 0
        && !state.starter_oc_claims.contains_key(primary_agent_id))
    .then(|| PlayerFirstChatUnlockPreviewSnapshot {
        chat_purpose: "Start a first conversation with the Agent you control.".to_string(),
        immediate_playable_help: "Ask what the Agent can do next for the current gameplay goal."
            .to_string(),
        first_question_or_action_hint: "Ask: What should we do first?".to_string(),
        resource_boundary: format!(
            "Starter OC grants initial liquid OC and a one-time first-chat unlock. \
             {STARTER_OC_CHAT_GATE_EXPLANATION} \
             It is separate from economic slot-1 claim and upkeep funding."
        ),
        defer_effect: format!(
            "{} {STARTER_OC_CHAT_GATE_EXPLANATION}",
            // Local state and the current service projection both retain agent_claims.
            // A missing legacy preview field cannot supply this ownership evidence.
            if owned_claims.is_empty() {
                "Deferring leaves your Agent binding unchanged and creates no economic claim \
                 or upkeep obligation."
            } else {
                "Deferring leaves your existing economic claims and their current upkeep \
                 rules unchanged."
            }
        ),
        recommended_unlock_action: crate::viewer::ACTION_CLAIM_STARTER_OC.to_string(),
    });

    Some(PlayerAgentClaimSnapshot {
        claimer_agent_id: primary_agent_id.to_string(),
        current_epoch,
        reputation_tier: agent_claim_reputation_tier(reputation_score),
        claim_cap: agent_claim_cap_for_tier(agent_claim_reputation_tier(reputation_score)),
        owned_claim_count: u8::try_from(owned_claims_count).unwrap_or(u8::MAX),
        liquid_main_token_balance,
        restricted_starter_claim_balance,
        slot_1_auto_restricted_starter_claim_amount,
        slot_1_eligible_claim_balance: liquid_main_token_balance
            .saturating_add(restricted_starter_claim_balance)
            .saturating_add(slot_1_auto_restricted_starter_claim_amount),
        next_claim_quote,
        first_chat_unlock_preview,
        owned_claims: owned_claims
            .iter()
            .map(|claim| owned_claim_to_snapshot(state, claim, current_epoch, epoch_length_ticks))
            .collect(),
    })
}

fn build_slot_1_claim_choice_quote(
    state: &WorldState,
    primary_agent_id: &str,
    total_upfront_amount: u64,
    eligible_claim_balance: u64,
    upkeep_runway_epochs: u64,
    grace_epochs: u64,
    claim_is_affordable: bool,
) -> PlayerAgentClaimChoiceQuoteSnapshot {
    let candidates = state
        .agents
        .iter()
        .filter(|(agent_id, cell)| {
            agent_id.as_str() != primary_agent_id
                && !state.agent_claims.contains_key(*agent_id)
                && known_body_kind(cell.state.body.kind.as_str())
                && known_frame_kind(cell.state.body_state.frame_kind.as_str())
        })
        .map(|(agent_id, cell)| {
            let mut installed_module_ids = cell
                .state
                .body_state
                .slots
                .iter()
                .filter_map(|slot| slot.installed_module.clone())
                .collect::<Vec<_>>();
            installed_module_ids.sort();
            installed_module_ids.dedup();

            PlayerAgentClaimChoiceCandidateSnapshot {
                agent_id: agent_id.clone(),
                location_x_cm: cell.state.pos.x_cm,
                location_y_cm: cell.state.pos.y_cm,
                location_z_cm: cell.state.pos.z_cm,
                body_kind: cell.state.body.kind.clone(),
                frame_kind: cell.state.body_state.frame_kind.clone(),
                installed_module_ids,
            }
        })
        .collect::<Vec<_>>();
    let mut candidates = candidates;
    candidates.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));

    let has_unknown_candidate_input = candidates
        .iter()
        .any(|candidate| !candidate_inputs_known(state, candidate));
    let rationale_candidates = candidates
        .iter()
        .filter(|candidate| candidate_inputs_known(state, candidate))
        .collect::<Vec<_>>();
    let complete_candidates = candidates
        .iter()
        .filter(|candidate| candidate_is_complete(state, candidate))
        .collect::<Vec<_>>();
    let unique_rationale_candidate =
        (rationale_candidates.len() == 1).then(|| rationale_candidates[0]);
    let unique_candidate_rationale = unique_rationale_candidate
        .and_then(|candidate| build_candidate_rationale(state, candidate));
    let candidate_has_high_risk = unique_candidate_rationale
        .as_ref()
        .is_some_and(|rationale| rationale.high_risk);

    let rationale_available = !rationale_candidates.is_empty() && !has_unknown_candidate_input;
    let has_funding_and_runway = claim_is_affordable
        && eligible_claim_balance >= total_upfront_amount
        && upkeep_runway_epochs >= grace_epochs;
    let (status, fallback_reason, claim_choice_class) = if !rationale_available {
        (
            "candidate_rationale_missing",
            Some("candidate_rationale_missing"),
            "wait_or_fund_first",
        )
    } else if complete_candidates.is_empty() {
        (
            "candidate_rationale_published",
            Some("candidate_capability_gap"),
            "wait_or_fund_first",
        )
    } else if !has_funding_and_runway {
        (
            "candidate_rationale_published",
            Some("claim_funding_or_runway_insufficient"),
            "wait_or_fund_first",
        )
    } else if candidate_has_high_risk {
        (
            "candidate_rationale_published",
            Some("candidate_risk_detected"),
            "wait_or_fund_first",
        )
    } else if rationale_candidates.len() == 1 && complete_candidates.len() == 1 {
        ("candidate_rationale_published", None, "claim_now_route_fit")
    } else {
        (
            "candidate_rationale_published",
            None,
            "compare_candidates_first",
        )
    };

    let rationale = unique_candidate_rationale;

    PlayerAgentClaimChoiceQuoteSnapshot {
        status: status.to_string(),
        candidates,
        candidate_starting_location: rationale
            .as_ref()
            .map(|rationale| rationale.starting_location.clone()),
        candidate_specialty_summary: rationale
            .as_ref()
            .map(|rationale| rationale.specialty_summary.clone()),
        first_industrial_goal_help: rationale
            .as_ref()
            .map(|rationale| rationale.first_industrial_goal_help.clone()),
        candidate_risk_summary: rationale
            .as_ref()
            .map(|rationale| rationale.risk_summary.clone()),
        candidate_recommendation_reason: rationale
            .as_ref()
            .map(|rationale| rationale.recommendation_reason.clone()),
        fallback_reason: fallback_reason.map(str::to_string),
        claim_choice_class: claim_choice_class.to_string(),
        recommended_claim_action: claim_choice_class.to_string(),
    }
}

const KNOWN_BODY_KINDS: &[&str] = &["humanoid", "industrial_worker"];
const KNOWN_FRAME_KINDS: &[&str] = &["light_frame", "standard_frame"];

fn known_body_kind(body_kind: &str) -> bool {
    KNOWN_BODY_KINDS.contains(&body_kind)
}

fn known_frame_kind(frame_kind: &str) -> bool {
    KNOWN_FRAME_KINDS.contains(&frame_kind)
}

fn known_claim_module(module_id: &str) -> bool {
    matches!(
        module_id,
        M1_BODY_MODULE_ID
            | M1_MEMORY_MODULE_ID
            | M1_MOBILITY_MODULE_ID
            | M1_MOVE_RULE_MODULE_ID
            | M1_TRANSFER_RULE_MODULE_ID
            | M1_VISIBILITY_RULE_MODULE_ID
            | M1_SENSOR_MODULE_ID
            | M1_STORAGE_CARGO_MODULE_ID
            | M1_RADIATION_POWER_MODULE_ID
            | M1_STORAGE_POWER_MODULE_ID
    )
}

fn candidate_is_complete(
    state: &WorldState,
    candidate: &PlayerAgentClaimChoiceCandidateSnapshot,
) -> bool {
    candidate_inputs_known(state, candidate)
        && candidate
            .installed_module_ids
            .iter()
            .any(|module_id| module_id == M1_RADIATION_POWER_MODULE_ID)
        && candidate
            .installed_module_ids
            .iter()
            .any(|module_id| module_id == M1_STORAGE_POWER_MODULE_ID)
        && candidate
            .installed_module_ids
            .iter()
            .any(|module_id| module_id == M1_SENSOR_MODULE_ID)
        && candidate
            .installed_module_ids
            .iter()
            .any(|module_id| module_id == M1_MOBILITY_MODULE_ID)
        && candidate
            .installed_module_ids
            .iter()
            .any(|module_id| module_id == M1_STORAGE_CARGO_MODULE_ID)
}

fn candidate_inputs_known(
    state: &WorldState,
    candidate: &PlayerAgentClaimChoiceCandidateSnapshot,
) -> bool {
    state.industry_progress.stage == crate::runtime::IndustryStage::Bootstrap
        && known_body_kind(candidate.body_kind.as_str())
        && known_frame_kind(candidate.frame_kind.as_str())
        && !candidate.installed_module_ids.is_empty()
        && candidate
            .installed_module_ids
            .iter()
            .all(|module_id| known_claim_module(module_id.as_str()))
}

#[derive(Debug, Clone)]
struct CandidateRationale {
    starting_location: String,
    specialty_summary: String,
    first_industrial_goal_help: String,
    risk_summary: String,
    recommendation_reason: String,
    high_risk: bool,
}

fn build_candidate_rationale(
    state: &WorldState,
    candidate: &PlayerAgentClaimChoiceCandidateSnapshot,
) -> Option<CandidateRationale> {
    if !candidate_inputs_known(state, candidate) {
        return None;
    }

    let mut risks = Vec::new();
    if !has_module(candidate, M1_RADIATION_POWER_MODULE_ID)
        || !has_module(candidate, M1_STORAGE_POWER_MODULE_ID)
    {
        risks.push("energy capability is missing or not fully proven");
    }
    if !has_module(candidate, M1_SENSOR_MODULE_ID) {
        risks.push("sensing/input discovery capability is missing");
    }
    if !has_module(candidate, M1_MOBILITY_MODULE_ID) {
        risks.push("mobility/routing capability is missing");
    }
    if !has_module(candidate, M1_STORAGE_CARGO_MODULE_ID) {
        risks.push("cargo/input carrying capability is missing");
    }
    if candidate.frame_kind == "light_frame" {
        risks.push("light-frame operating pressure is present");
    }
    let high_risk = !risks.is_empty();
    let risk_summary = if risks.is_empty() {
        "No provable high-risk capability gap is present in the current canonical snapshot."
            .to_string()
    } else {
        format!("Provable candidate risks: {}.", risks.join("; "))
    };
    let complete = candidate_is_complete(state, candidate);
    let recommendation_reason = if complete && !high_risk {
        "Exactly one complete candidate is known for the first industrial goal, with canonical energy, sensing, mobility, and cargo support.".to_string()
    } else if complete {
        "The candidate is complete, but its provable risk makes waiting or comparing safer than recommending immediate claim.".to_string()
    } else {
        "The first industrial goal is known, but canonical capability evidence is incomplete; wait for a supported route instead of inferring output.".to_string()
    };
    let first_industrial_goal_help = if complete {
        "Supports the first industrial goal by covering energy, sensing, mobility, and cargo/input carrying; it does not guarantee output."
            .to_string()
    } else {
        "The first industrial goal is known, but this candidate cannot yet prove complete energy, sensing, mobility, and cargo/input carrying support; no output is promised."
            .to_string()
    };

    Some(CandidateRationale {
        starting_location: format!(
            "({}, {}, {}) cm",
            candidate.location_x_cm, candidate.location_y_cm, candidate.location_z_cm
        ),
        specialty_summary: specialty_summary(candidate),
        first_industrial_goal_help,
        risk_summary,
        recommendation_reason,
        high_risk,
    })
}

fn has_module(candidate: &PlayerAgentClaimChoiceCandidateSnapshot, module_id: &str) -> bool {
    candidate
        .installed_module_ids
        .iter()
        .any(|installed| installed == module_id)
}

fn specialty_summary(candidate: &PlayerAgentClaimChoiceCandidateSnapshot) -> String {
    let mut specialties = Vec::new();
    if has_module(candidate, M1_RADIATION_POWER_MODULE_ID)
        || has_module(candidate, M1_STORAGE_POWER_MODULE_ID)
    {
        specialties.push("energy");
    }
    if has_module(candidate, M1_SENSOR_MODULE_ID) {
        specialties.push("sensing/input discovery");
    }
    if has_module(candidate, M1_MOBILITY_MODULE_ID) {
        specialties.push("mobility/routing");
    }
    if has_module(candidate, M1_STORAGE_CARGO_MODULE_ID) {
        specialties.push("cargo/input carrying");
    }
    if specialties.is_empty() {
        "No canonical industrial capability is proven.".to_string()
    } else {
        format!("Canonical specialties: {}.", specialties.join(", "))
    }
}

fn owned_claim_to_snapshot(
    state: &WorldState,
    claim: &AgentClaimState,
    current_epoch: u64,
    epoch_length_ticks: u64,
) -> PlayerAgentClaimOwnedSnapshot {
    let status = claim_status(claim, current_epoch);
    let last_control_epoch = state
        .agents
        .get(&claim.target_agent_id)
        .map(|cell| agent_claim_epoch(cell.last_active, epoch_length_ticks))
        .unwrap_or(current_epoch);
    let release_ready_in_epochs = claim
        .release_ready_at_epoch
        .map(|epoch| epoch.saturating_sub(current_epoch));
    let grace_remaining_epochs = claim
        .grace_deadline_epoch
        .map(|epoch| epoch.saturating_sub(current_epoch));
    let idle_warning_in_epochs = (claim.idle_warning_emitted_at_epoch.is_none()).then(|| {
        last_control_epoch
            .saturating_add(claim.idle_warning_epochs)
            .saturating_sub(current_epoch)
    });
    let forced_reclaim_in_epochs = Some(
        last_control_epoch
            .saturating_add(claim.forced_idle_reclaim_epochs)
            .saturating_sub(current_epoch),
    );

    PlayerAgentClaimOwnedSnapshot {
        target_agent_id: claim.target_agent_id.clone(),
        status: status.to_string(),
        upkeep_paid_through_epoch: claim.upkeep_paid_through_epoch,
        upfront_restricted_spent_amount: claim.upfront_restricted_spent_amount,
        upfront_liquid_spent_amount: claim.upfront_liquid_spent_amount,
        claim_bond_locked_restricted_amount: claim.claim_bond_locked_restricted_amount,
        claim_bond_locked_liquid_amount: claim.claim_bond_locked_liquid_amount,
        release_ready_at_epoch: claim.release_ready_at_epoch,
        release_ready_in_epochs,
        grace_deadline_epoch: claim.grace_deadline_epoch,
        grace_remaining_epochs,
        idle_warning_in_epochs,
        forced_reclaim_in_epochs,
    }
}

fn claim_status(claim: &AgentClaimState, current_epoch: u64) -> &'static str {
    if claim.grace_deadline_epoch.is_some() {
        "upkeep_grace"
    } else if let Some(ready_epoch) = claim.release_ready_at_epoch {
        if current_epoch >= ready_epoch {
            "release_ready"
        } else {
            "release_cooldown"
        }
    } else if claim.idle_warning_emitted_at_epoch.is_some() {
        "idle_reclaim_candidate"
    } else {
        "claimed_active"
    }
}

fn agent_claim_epoch(time: u64, epoch_length_ticks: u64) -> u64 {
    time / epoch_length_ticks.max(1)
}
