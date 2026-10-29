use super::*;

#[derive(serde::Serialize)]
#[serde(rename = "GameplayPolicyUpdated")]
struct GameplayPolicyUpdatedPayload<'a> {
    operator_agent_id: &'a String,
    electricity_tax_bps: &'a u16,
    data_tax_bps: &'a u16,
    power_trade_fee_bps: &'a u16,
    max_open_contracts_per_agent: &'a u16,
    blocked_agents: &'a Vec<String>,
    forbidden_location_ids: &'a Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename = "EconomicContractOpened")]
struct EconomicContractOpenedPayload<'a> {
    creator_agent_id: &'a String,
    contract_id: &'a String,
    counterparty_agent_id: &'a String,
    fulfillment_kind: &'a EconomicContractFulfillmentKind,
    settlement_kind: &'a ResourceKind,
    settlement_amount: &'a i64,
    reputation_stake: &'a i64,
    expires_at: &'a WorldTime,
    description: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "EconomicContractAccepted")]
struct EconomicContractAcceptedPayload<'a> {
    accepter_agent_id: &'a String,
    contract_id: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "EconomicContractSettled")]
struct EconomicContractSettledPayload<'a> {
    operator_agent_id: &'a String,
    contract_id: &'a String,
    #[serde(skip_serializing_if = "Option::is_none")]
    world_binding: &'a Option<String>,
    success: &'a bool,
    transfer_amount: &'a i64,
    tax_amount: &'a i64,
    notes: &'a String,
    creator_reputation_delta: &'a i64,
    counterparty_reputation_delta: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "EconomicContractExpired")]
struct EconomicContractExpiredPayload<'a> {
    contract_id: &'a String,
    creator_agent_id: &'a String,
    counterparty_agent_id: &'a String,
    creator_reputation_delta: &'a i64,
    counterparty_reputation_delta: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "AllianceFormed")]
struct AllianceFormedPayload<'a> {
    proposer_agent_id: &'a String,
    alliance_id: &'a String,
    members: &'a Vec<String>,
    charter: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "AllianceJoined")]
struct AllianceJoinedPayload<'a> {
    operator_agent_id: &'a String,
    alliance_id: &'a String,
    member_agent_id: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "AllianceLeft")]
struct AllianceLeftPayload<'a> {
    operator_agent_id: &'a String,
    alliance_id: &'a String,
    member_agent_id: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "AllianceDissolved")]
struct AllianceDissolvedPayload<'a> {
    operator_agent_id: &'a String,
    alliance_id: &'a String,
    reason: &'a String,
    former_members: &'a Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename = "WarDeclared")]
struct WarDeclaredPayload<'a> {
    initiator_agent_id: &'a String,
    war_id: &'a String,
    aggressor_alliance_id: &'a String,
    defender_alliance_id: &'a String,
    objective: &'a String,
    intensity: &'a u32,
    mobilization_electricity_cost: &'a i64,
    mobilization_data_cost: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "WarConcluded")]
struct WarConcludedPayload<'a> {
    war_id: &'a String,
    winner_alliance_id: &'a String,
    loser_alliance_id: &'a String,
    aggressor_score: &'a i64,
    defender_score: &'a i64,
    summary: &'a String,
    participant_outcomes: &'a Vec<WarParticipantOutcome>,
}

#[derive(serde::Serialize)]
#[serde(rename = "GovernanceProposalOpened")]
struct GovernanceProposalOpenedPayload<'a> {
    proposer_agent_id: &'a String,
    proposal_key: &'a String,
    title: &'a String,
    description: &'a String,
    options: &'a Vec<String>,
    voting_window_ticks: &'a u64,
    closes_at: &'a WorldTime,
    quorum_weight: &'a u64,
    pass_threshold_bps: &'a u16,
}

#[derive(serde::Serialize)]
#[serde(rename = "GovernanceVoteCast")]
struct GovernanceVoteCastPayload<'a> {
    voter_agent_id: &'a String,
    proposal_key: &'a String,
    option: &'a String,
    weight: &'a u32,
}

#[derive(serde::Serialize)]
#[serde(rename = "GovernanceProposalFinalized")]
struct GovernanceProposalFinalizedPayload<'a> {
    proposal_key: &'a String,
    #[serde(skip_serializing_if = "Option::is_none")]
    winning_option: &'a Option<String>,
    winning_weight: &'a u64,
    total_weight: &'a u64,
    passed: &'a bool,
}

#[derive(serde::Serialize)]
#[serde(rename = "CrisisSpawned")]
struct CrisisSpawnedPayload<'a> {
    crisis_id: &'a String,
    kind: &'a String,
    severity: &'a u32,
    expires_at: &'a WorldTime,
}

#[derive(serde::Serialize)]
#[serde(rename = "CrisisResolved")]
struct CrisisResolvedPayload<'a> {
    resolver_agent_id: &'a String,
    crisis_id: &'a String,
    strategy: &'a String,
    success: &'a bool,
    impact: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "CrisisTimedOut")]
struct CrisisTimedOutPayload<'a> {
    crisis_id: &'a String,
    penalty_impact: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "MetaProgressGranted")]
struct MetaProgressGrantedPayload<'a> {
    operator_agent_id: &'a String,
    target_agent_id: &'a String,
    track: &'a String,
    points: &'a i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    achievement_id: &'a Option<String>,
}

pub(super) fn serialize<S: serde::Serializer>(
    event: &DomainEvent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match event {
        DomainEvent::GameplayPolicyUpdated {
            operator_agent_id,
            electricity_tax_bps,
            data_tax_bps,
            power_trade_fee_bps,
            max_open_contracts_per_agent,
            blocked_agents,
            forbidden_location_ids,
        } => serialize_adjacent(
            serializer,
            Tag::GameplayPolicyUpdated,
            &GameplayPolicyUpdatedPayload {
                operator_agent_id,
                electricity_tax_bps,
                data_tax_bps,
                power_trade_fee_bps,
                max_open_contracts_per_agent,
                blocked_agents,
                forbidden_location_ids,
            },
        ),
        DomainEvent::EconomicContractOpened {
            creator_agent_id,
            contract_id,
            counterparty_agent_id,
            fulfillment_kind,
            settlement_kind,
            settlement_amount,
            reputation_stake,
            expires_at,
            description,
        } => serialize_adjacent(
            serializer,
            Tag::EconomicContractOpened,
            &EconomicContractOpenedPayload {
                creator_agent_id,
                contract_id,
                counterparty_agent_id,
                fulfillment_kind,
                settlement_kind,
                settlement_amount,
                reputation_stake,
                expires_at,
                description,
            },
        ),
        DomainEvent::EconomicContractAccepted {
            accepter_agent_id,
            contract_id,
        } => serialize_adjacent(
            serializer,
            Tag::EconomicContractAccepted,
            &EconomicContractAcceptedPayload {
                accepter_agent_id,
                contract_id,
            },
        ),
        DomainEvent::EconomicContractSettled {
            operator_agent_id,
            contract_id,
            world_binding,
            success,
            transfer_amount,
            tax_amount,
            notes,
            creator_reputation_delta,
            counterparty_reputation_delta,
        } => serialize_adjacent(
            serializer,
            Tag::EconomicContractSettled,
            &EconomicContractSettledPayload {
                operator_agent_id,
                contract_id,
                world_binding,
                success,
                transfer_amount,
                tax_amount,
                notes,
                creator_reputation_delta,
                counterparty_reputation_delta,
            },
        ),
        DomainEvent::EconomicContractExpired {
            contract_id,
            creator_agent_id,
            counterparty_agent_id,
            creator_reputation_delta,
            counterparty_reputation_delta,
        } => serialize_adjacent(
            serializer,
            Tag::EconomicContractExpired,
            &EconomicContractExpiredPayload {
                contract_id,
                creator_agent_id,
                counterparty_agent_id,
                creator_reputation_delta,
                counterparty_reputation_delta,
            },
        ),
        DomainEvent::AllianceFormed {
            proposer_agent_id,
            alliance_id,
            members,
            charter,
        } => serialize_adjacent(
            serializer,
            Tag::AllianceFormed,
            &AllianceFormedPayload {
                proposer_agent_id,
                alliance_id,
                members,
                charter,
            },
        ),
        DomainEvent::AllianceJoined {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => serialize_adjacent(
            serializer,
            Tag::AllianceJoined,
            &AllianceJoinedPayload {
                operator_agent_id,
                alliance_id,
                member_agent_id,
            },
        ),
        DomainEvent::AllianceLeft {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => serialize_adjacent(
            serializer,
            Tag::AllianceLeft,
            &AllianceLeftPayload {
                operator_agent_id,
                alliance_id,
                member_agent_id,
            },
        ),
        DomainEvent::AllianceDissolved {
            operator_agent_id,
            alliance_id,
            reason,
            former_members,
        } => serialize_adjacent(
            serializer,
            Tag::AllianceDissolved,
            &AllianceDissolvedPayload {
                operator_agent_id,
                alliance_id,
                reason,
                former_members,
            },
        ),
        DomainEvent::WarDeclared {
            initiator_agent_id,
            war_id,
            aggressor_alliance_id,
            defender_alliance_id,
            objective,
            intensity,
            mobilization_electricity_cost,
            mobilization_data_cost,
        } => serialize_adjacent(
            serializer,
            Tag::WarDeclared,
            &WarDeclaredPayload {
                initiator_agent_id,
                war_id,
                aggressor_alliance_id,
                defender_alliance_id,
                objective,
                intensity,
                mobilization_electricity_cost,
                mobilization_data_cost,
            },
        ),
        DomainEvent::WarConcluded {
            war_id,
            winner_alliance_id,
            loser_alliance_id,
            aggressor_score,
            defender_score,
            summary,
            participant_outcomes,
        } => serialize_adjacent(
            serializer,
            Tag::WarConcluded,
            &WarConcludedPayload {
                war_id,
                winner_alliance_id,
                loser_alliance_id,
                aggressor_score,
                defender_score,
                summary,
                participant_outcomes,
            },
        ),
        DomainEvent::GovernanceProposalOpened {
            proposer_agent_id,
            proposal_key,
            title,
            description,
            options,
            voting_window_ticks,
            closes_at,
            quorum_weight,
            pass_threshold_bps,
        } => serialize_adjacent(
            serializer,
            Tag::GovernanceProposalOpened,
            &GovernanceProposalOpenedPayload {
                proposer_agent_id,
                proposal_key,
                title,
                description,
                options,
                voting_window_ticks,
                closes_at,
                quorum_weight,
                pass_threshold_bps,
            },
        ),
        DomainEvent::GovernanceVoteCast {
            voter_agent_id,
            proposal_key,
            option,
            weight,
        } => serialize_adjacent(
            serializer,
            Tag::GovernanceVoteCast,
            &GovernanceVoteCastPayload {
                voter_agent_id,
                proposal_key,
                option,
                weight,
            },
        ),
        DomainEvent::GovernanceProposalFinalized {
            proposal_key,
            winning_option,
            winning_weight,
            total_weight,
            passed,
        } => serialize_adjacent(
            serializer,
            Tag::GovernanceProposalFinalized,
            &GovernanceProposalFinalizedPayload {
                proposal_key,
                winning_option,
                winning_weight,
                total_weight,
                passed,
            },
        ),
        DomainEvent::CrisisSpawned {
            crisis_id,
            kind,
            severity,
            expires_at,
        } => serialize_adjacent(
            serializer,
            Tag::CrisisSpawned,
            &CrisisSpawnedPayload {
                crisis_id,
                kind,
                severity,
                expires_at,
            },
        ),
        DomainEvent::CrisisResolved {
            resolver_agent_id,
            crisis_id,
            strategy,
            success,
            impact,
        } => serialize_adjacent(
            serializer,
            Tag::CrisisResolved,
            &CrisisResolvedPayload {
                resolver_agent_id,
                crisis_id,
                strategy,
                success,
                impact,
            },
        ),
        DomainEvent::CrisisTimedOut {
            crisis_id,
            penalty_impact,
        } => serialize_adjacent(
            serializer,
            Tag::CrisisTimedOut,
            &CrisisTimedOutPayload {
                crisis_id,
                penalty_impact,
            },
        ),
        DomainEvent::MetaProgressGranted {
            operator_agent_id,
            target_agent_id,
            track,
            points,
            achievement_id,
        } => serialize_adjacent(
            serializer,
            Tag::MetaProgressGranted,
            &MetaProgressGrantedPayload {
                operator_agent_id,
                target_agent_id,
                track,
                points,
                achievement_id,
            },
        ),
        _ => unreachable!("DomainEvent serializer group mismatch"),
    }
}
