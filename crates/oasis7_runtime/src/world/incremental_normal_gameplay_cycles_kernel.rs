//! Sole original gameplay-cycle DATA types and arithmetic; no source or tick authority.
use crate::{DomainEvent, EconomicContractStatus, WarParticipantOutcome};
use serde::Deserialize;
pub(in crate::world) const CRISIS_AUTO_INTERVAL_TICKS: u64 = 8;
pub(in crate::world) const CRISIS_DEFAULT_DURATION_TICKS: u64 = 6;
pub(in crate::world) const CRISIS_TIMEOUT_PENALTY_PER_SEVERITY: i64 = 10;
pub(in crate::world) const WAR_SCORE_PER_MEMBER: i64 = 10;
pub(in crate::world) const WAR_SCORE_REPUTATION_DIVISOR: i64 = 10;
pub(in crate::world) const WAR_WINNER_REPUTATION_PER_INTENSITY: i64 = 2;
pub(in crate::world) const WAR_LOSER_REPUTATION_PER_INTENSITY: i64 = 3;
pub(in crate::world) const WAR_LOSER_ELECTRICITY_PENALTY_PER_INTENSITY: i64 = 6;
pub(in crate::world) const WAR_LOSER_DATA_PENALTY_PER_INTENSITY: i64 = 4;
pub(in crate::world) const CONTRACT_EXPIRY_COUNTERPARTY_PENALTY_DIVISOR: i64 = 2;
pub(in crate::world) const GAMEPLAY_LIFECYCLE_EMIT_KIND: &str = "gameplay.lifecycle.directives";

#[derive(Debug, Deserialize)]
pub(in crate::world) struct GameplayDirectiveEnvelope {
    #[serde(default)]
    pub(in crate::world) directives: Vec<GameplayLifecycleDirective>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(in crate::world) enum GameplayLifecycleDirective {
    GovernanceFinalize {
        proposal_key: String,
        #[serde(default)]
        winning_option: Option<String>,
        winning_weight: u64,
        total_weight: u64,
        passed: bool,
    },
    CrisisSpawn {
        crisis_id: String,
        kind: String,
        severity: u32,
        expires_at: u64,
    },
    CrisisTimeout {
        crisis_id: String,
        penalty_impact: i64,
    },
    WarConclude {
        war_id: String,
        winner_alliance_id: String,
        #[serde(default)]
        loser_alliance_id: Option<String>,
        aggressor_score: i64,
        defender_score: i64,
        summary: String,
        #[serde(default)]
        participant_outcomes: Vec<WarParticipantOutcome>,
    },
    MetaGrant {
        operator_agent_id: String,
        target_agent_id: String,
        track: String,
        points: i64,
        #[serde(default)]
        achievement_id: Option<String>,
    },
}

pub(in crate::world) fn prepare_gameplay_directive_event_v1(
    directive: GameplayLifecycleDirective,
    war: Option<(&str, &str)>,
) -> Option<DomainEvent> {
    match directive {
        GameplayLifecycleDirective::GovernanceFinalize {
            proposal_key,
            winning_option,
            winning_weight,
            total_weight,
            passed,
        } => Some(DomainEvent::GovernanceProposalFinalized {
            proposal_key,
            winning_option,
            winning_weight,
            total_weight,
            passed,
        }),
        GameplayLifecycleDirective::CrisisSpawn {
            crisis_id,
            kind,
            severity,
            expires_at,
        } => Some(DomainEvent::CrisisSpawned {
            crisis_id,
            kind,
            severity,
            expires_at,
        }),
        GameplayLifecycleDirective::CrisisTimeout {
            crisis_id,
            penalty_impact,
        } => Some(DomainEvent::CrisisTimedOut {
            crisis_id,
            penalty_impact,
        }),
        GameplayLifecycleDirective::WarConclude {
            war_id,
            winner_alliance_id,
            loser_alliance_id,
            aggressor_score,
            defender_score,
            summary,
            participant_outcomes,
        } => Some(DomainEvent::WarConcluded {
            loser_alliance_id: loser_alliance_id.unwrap_or_else(|| {
                war.map(|war| {
                    if war.0 == winner_alliance_id {
                        war.1.to_owned()
                    } else {
                        war.0.to_owned()
                    }
                })
                .unwrap_or_default()
            }),
            war_id,
            winner_alliance_id,
            aggressor_score,
            defender_score,
            summary,
            participant_outcomes,
        }),
        GameplayLifecycleDirective::MetaGrant {
            operator_agent_id,
            target_agent_id,
            track,
            points,
            achievement_id,
        } => {
            if points == 0 {
                None
            } else {
                Some(DomainEvent::MetaProgressGranted {
                    operator_agent_id,
                    target_agent_id,
                    track,
                    points,
                    achievement_id,
                })
            }
        }
    }
}

pub(in crate::world) fn prepare_contract_expiry_event_v1(
    contract_id: String,
    creator_agent_id: String,
    counterparty_agent_id: String,
    status: EconomicContractStatus,
    reputation_stake: i64,
) -> DomainEvent {
    let (creator_reputation_delta, counterparty_reputation_delta) = match status {
        EconomicContractStatus::Open => (-reputation_stake, 0),
        EconomicContractStatus::Accepted => (
            -reputation_stake,
            -reputation_stake
                .saturating_div(CONTRACT_EXPIRY_COUNTERPARTY_PENALTY_DIVISOR)
                .max(1),
        ),
        EconomicContractStatus::Settled | EconomicContractStatus::Expired => (0, 0),
    };
    DomainEvent::EconomicContractExpired {
        contract_id,
        creator_agent_id,
        counterparty_agent_id,
        creator_reputation_delta,
        counterparty_reputation_delta,
    }
}
pub(in crate::world) fn prepare_crisis_auto_spawn_event_v1(
    has_active_crisis: bool,
    now: u64,
) -> Option<DomainEvent> {
    if has_active_crisis || now == 0 || !now.is_multiple_of(CRISIS_AUTO_INTERVAL_TICKS) {
        return None;
    }
    let sequence = now / CRISIS_AUTO_INTERVAL_TICKS;
    let severity = ((sequence % 3) + 1) as u32;
    let kind = match severity {
        1 => "supply_shock",
        2 => "solar_storm",
        _ => "network_outage",
    }
    .to_string();
    Some(DomainEvent::CrisisSpawned {
        crisis_id: format!("crisis.auto.{now}"),
        kind,
        severity,
        expires_at: now
            .saturating_add(CRISIS_DEFAULT_DURATION_TICKS)
            .saturating_add(u64::from(severity)),
    })
}
pub(in crate::world) fn prepare_crisis_timeout_event_v1(
    crisis_id: String,
    severity: u32,
) -> DomainEvent {
    DomainEvent::CrisisTimedOut {
        crisis_id,
        penalty_impact: -i64::from(severity).saturating_mul(CRISIS_TIMEOUT_PENALTY_PER_SEVERITY),
    }
}
/// Only operation charging/cancellation is delegated; the original winning
/// tally, lexical tie break, quorum and threshold predicates remain fixed.
pub(in crate::world) fn prepare_governance_finalization_event_v1<'a, E>(
    proposal_key: String,
    quorum_weight: u64,
    pass_threshold_bps: u16,
    total_weight: u64,
    tallies: impl Iterator<Item = (&'a str, u64)>,
    fee: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<DomainEvent, E> {
    let mut winner: Option<(&str, u64)> = None;
    for (option, weight) in tallies {
        fee(option.len())?;
        let replace = winner.is_none_or(|(old, old_weight)| {
            weight.cmp(&old_weight).then_with(|| old.cmp(option)) != std::cmp::Ordering::Less
        });
        if replace {
            winner = Some((option, weight));
        }
    }
    let (winning_option, winning_weight) = if let Some((option, weight)) = winner {
        fee(option.len())?;
        (Some(option.to_owned()), weight)
    } else {
        (None, 0)
    };
    let reached_quorum = total_weight >= quorum_weight;
    let reached_threshold = total_weight != 0
        && (u128::from(winning_weight) * 10_000_u128)
            >= (u128::from(total_weight) * u128::from(pass_threshold_bps));
    let passed = reached_quorum && reached_threshold && winning_option.is_some();
    Ok(DomainEvent::GovernanceProposalFinalized {
        proposal_key,
        winning_option,
        winning_weight,
        total_weight,
        passed,
    })
}

/// Original participant arithmetic over complete selected member facts. Vectors
/// are already in original lexical order; duplicate members remain distinct.
pub(in crate::world) fn prepare_war_participant_outcomes_v1<E>(
    mut loser_members: Vec<(String, i64, i64)>,
    mut winner_members: Vec<String>,
    intensity: u32,
    mut outcomes: Vec<crate::WarParticipantOutcome>,
    fee: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<Vec<crate::WarParticipantOutcome>, E> {
    for i in 1..loser_members.len() {
        let mut j = i;
        while j > 0 {
            fee(loser_members[j - 1].0.len().min(loser_members[j].0.len()))?;
            if loser_members[j - 1].0 <= loser_members[j].0 {
                break;
            }
            fee(2 * std::mem::size_of::<(String, i64, i64)>())?;
            loser_members.swap(j - 1, j);
            j -= 1;
        }
    }
    for i in 1..winner_members.len() {
        let mut j = i;
        while j > 0 {
            fee(winner_members[j - 1].len().min(winner_members[j].len()))?;
            if winner_members[j - 1] <= winner_members[j] {
                break;
            }
            fee(2 * std::mem::size_of::<String>())?;
            winner_members.swap(j - 1, j);
            j -= 1;
        }
    }
    let intensity = i64::from(intensity.max(1));
    let loser_electricity_penalty = intensity
        .saturating_mul(WAR_LOSER_ELECTRICITY_PENALTY_PER_INTENSITY)
        .max(1);
    let loser_data_penalty = intensity
        .saturating_mul(WAR_LOSER_DATA_PENALTY_PER_INTENSITY)
        .max(1);
    let loser_reputation_delta = intensity
        .saturating_mul(WAR_LOSER_REPUTATION_PER_INTENSITY)
        .saturating_neg();
    let winner_reputation_delta = intensity.saturating_mul(WAR_WINNER_REPUTATION_PER_INTENSITY);

    let mut total_electricity_spoils = 0_i64;
    let mut total_data_spoils = 0_i64;

    for (member, available_electricity, available_data) in loser_members {
        fee(0)?;
        let available_electricity = available_electricity.max(0);
        let available_data = available_data.max(0);
        let electricity_loss = available_electricity.min(loser_electricity_penalty);
        let data_loss = available_data.min(loser_data_penalty);
        total_electricity_spoils = total_electricity_spoils.saturating_add(electricity_loss);
        total_data_spoils = total_data_spoils.saturating_add(data_loss);
        outcomes.push(WarParticipantOutcome {
            agent_id: member,
            electricity_delta: electricity_loss.saturating_neg(),
            data_delta: data_loss.saturating_neg(),
            reputation_delta: loser_reputation_delta,
        });
    }

    if !winner_members.is_empty() {
        let winner_count = i64::try_from(winner_members.len()).unwrap_or(1).max(1);
        let base_electricity_gain = total_electricity_spoils.saturating_div(winner_count);
        let base_data_gain = total_data_spoils.saturating_div(winner_count);
        let mut electricity_remainder = total_electricity_spoils
            .saturating_sub(base_electricity_gain.saturating_mul(winner_count));
        let mut data_remainder =
            total_data_spoils.saturating_sub(base_data_gain.saturating_mul(winner_count));

        for member in winner_members {
            fee(0)?;
            let mut electricity_gain = base_electricity_gain;
            let mut data_gain = base_data_gain;
            if electricity_remainder > 0 {
                electricity_gain = electricity_gain.saturating_add(1);
                electricity_remainder = electricity_remainder.saturating_sub(1);
            }
            if data_remainder > 0 {
                data_gain = data_gain.saturating_add(1);
                data_remainder = data_remainder.saturating_sub(1);
            }
            outcomes.push(WarParticipantOutcome {
                agent_id: member,
                electricity_delta: electricity_gain,
                data_delta: data_gain,
                reputation_delta: winner_reputation_delta,
            });
        }
    }

    for i in 1..outcomes.len() {
        let mut j = i;
        while j > 0 {
            fee(outcomes[j - 1]
                .agent_id
                .len()
                .min(outcomes[j].agent_id.len()))?;
            if outcomes[j - 1].agent_id <= outcomes[j].agent_id {
                break;
            }
            fee(2 * std::mem::size_of::<WarParticipantOutcome>())?;
            outcomes.swap(j - 1, j);
            j -= 1;
        }
    }
    Ok(outcomes)
}

pub(in crate::world) fn prepare_war_scores_v1(
    aggressor_members: i64,
    defender_members: i64,
    aggressor_reputation: i64,
    defender_reputation: i64,
    intensity: u32,
) -> (i64, i64) {
    (
        aggressor_members
            .saturating_mul(WAR_SCORE_PER_MEMBER)
            .saturating_add(i64::from(intensity))
            .saturating_add(aggressor_reputation.saturating_div(WAR_SCORE_REPUTATION_DIVISOR)),
        defender_members
            .saturating_mul(WAR_SCORE_PER_MEMBER)
            .saturating_add(defender_reputation.saturating_div(WAR_SCORE_REPUTATION_DIVISOR)),
    )
}
