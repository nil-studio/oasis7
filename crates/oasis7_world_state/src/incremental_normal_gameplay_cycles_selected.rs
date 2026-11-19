//! Fixed selected gameplay DATA projections. These reducers do not establish
//! source provenance, module authority or permission to append an event.
use crate::{
    CrisisState, CrisisStatus, DomainEvent, EconomicContractFulfillmentKind,
    EconomicContractStatus, GovernanceProposalStatus, WarParticipantOutcome, WorldError, WorldTime,
};
fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
/// Original unary Service gate, evaluated before any status-dependent work.
pub fn reject_service_fulfillment_v1(
    fulfillment: EconomicContractFulfillmentKind,
) -> Result<(), WorldError> {
    if fulfillment == EconomicContractFulfillmentKind::Service {
        Err(invalid(
            "service contracts unavailable: collateral/evidence/remedy not implemented",
        ))
    } else {
        Ok(())
    }
}
pub struct EconomicExpiryProjectionV1 {
    pub status: EconomicContractStatus,
    pub settled_at: Option<WorldTime>,
    pub settlement_success: Option<bool>,
    pub transfer_amount: i64,
    pub tax_amount: i64,
    pub settlement_notes: &'static str,
}
pub fn selected_economic_expiry_projection_v1(
    id: &str,
    fulfillment: EconomicContractFulfillmentKind,
    status: EconomicContractStatus,
    now: WorldTime,
) -> Result<EconomicExpiryProjectionV1, WorldError> {
    reject_service_fulfillment_v1(fulfillment)?;
    match status {
        EconomicContractStatus::Open | EconomicContractStatus::Accepted => {}
        EconomicContractStatus::Settled | EconomicContractStatus::Expired => {
            return Err(invalid(format!(
                "economic contract already finalized before expiry: {id}"
            )));
        }
    }
    Ok(EconomicExpiryProjectionV1 {
        status: EconomicContractStatus::Expired,
        settled_at: Some(now),
        settlement_success: Some(false),
        transfer_amount: 0,
        tax_amount: 0,
        settlement_notes: "auto expired by gameplay lifecycle",
    })
}
pub struct GovernanceFinalizationProjectionV1<'a> {
    pub status: GovernanceProposalStatus,
    pub finalized_at: Option<WorldTime>,
    pub winning_option: Option<&'a str>,
    pub winning_weight: u64,
    pub total_weight_at_finalize: u64,
    pub vote_last_updated_at: WorldTime,
}
pub fn selected_governance_finalization_projection_v1(
    event: &DomainEvent,
    now: WorldTime,
) -> GovernanceFinalizationProjectionV1<'_> {
    let DomainEvent::GovernanceProposalFinalized {
        winning_option,
        winning_weight,
        total_weight,
        passed,
        ..
    } = event
    else {
        unreachable!("fixed governance finalization event")
    };
    GovernanceFinalizationProjectionV1 {
        status: if *passed {
            GovernanceProposalStatus::Passed
        } else {
            GovernanceProposalStatus::Rejected
        },
        finalized_at: Some(now),
        winning_option: winning_option.as_deref(),
        winning_weight: *winning_weight,
        total_weight_at_finalize: *total_weight,
        vote_last_updated_at: now,
    }
}
pub fn selected_crisis_spawn_v1(event: &DomainEvent, now: WorldTime) -> CrisisState {
    let DomainEvent::CrisisSpawned {
        crisis_id,
        kind,
        severity,
        expires_at,
    } = event
    else {
        unreachable!("fixed crisis spawn event")
    };
    CrisisState {
        crisis_id: crisis_id.clone(),
        kind: kind.clone(),
        severity: *severity,
        status: CrisisStatus::Active,
        opened_at: now,
        expires_at: *expires_at,
        resolver_agent_id: None,
        strategy: None,
        success: None,
        impact: 0,
        resolved_at: None,
    }
}
pub struct CrisisTimeoutProjectionV1 {
    pub status: CrisisStatus,
    pub success: Option<bool>,
    pub impact: i64,
    pub resolved_at: Option<WorldTime>,
}
pub fn selected_crisis_timeout_projection_v1(
    penalty_impact: i64,
    now: WorldTime,
) -> CrisisTimeoutProjectionV1 {
    CrisisTimeoutProjectionV1 {
        status: CrisisStatus::TimedOut,
        success: Some(false),
        impact: penalty_impact,
        resolved_at: Some(now),
    }
}
pub struct WarConclusionProjectionV1<'a> {
    pub active: bool,
    pub aggressor_score: i64,
    pub defender_score: i64,
    pub concluded_at: Option<WorldTime>,
    pub winner_alliance_id: &'a str,
    pub loser_alliance_id: &'a str,
    pub settlement_summary: &'a str,
    pub participant_outcomes: &'a [WarParticipantOutcome],
}
pub fn selected_war_conclusion_projection_v1<'a>(
    aggressor: &'a str,
    defender: &'a str,
    event: &'a DomainEvent,
    now: WorldTime,
) -> WarConclusionProjectionV1<'a> {
    let DomainEvent::WarConcluded {
        winner_alliance_id,
        loser_alliance_id,
        aggressor_score,
        defender_score,
        summary,
        participant_outcomes,
        ..
    } = event
    else {
        unreachable!("fixed war conclusion event")
    };
    let loser = if loser_alliance_id.is_empty() {
        if aggressor == winner_alliance_id {
            defender
        } else {
            aggressor
        }
    } else {
        loser_alliance_id
    };
    WarConclusionProjectionV1 {
        active: false,
        aggressor_score: *aggressor_score,
        defender_score: *defender_score,
        concluded_at: Some(now),
        winner_alliance_id,
        loser_alliance_id: loser,
        settlement_summary: summary,
        participant_outcomes,
    }
}
/// Original resource arithmetic/error framing shared with selected Current.
pub fn apply_war_resource_delta_v1(
    kind: oasis7_agent_api::world_action::ResourceKind,
    before: i64,
    delta: i64,
    id: &str,
    context: &str,
) -> Result<i64, WorldError> {
    if delta == 0 {
        return Ok(before);
    }
    let result = if delta > 0 {
        oasis7_agent_api::world_resource::checked_resource_add_v1(kind, before, delta)
    } else {
        oasis7_agent_api::world_resource::checked_resource_remove_v1(
            kind,
            before,
            delta.saturating_abs(),
        )
    };
    result.map_err(|error| invalid(format!("{context} apply failed for {id}: {error:?}")))
}
