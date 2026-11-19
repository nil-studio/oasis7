//! Fixed claim-epoch decisions from selected Current facts. This module
//! produces only event DATA; state mutation and private event provenance are
//! supplied by the fixed Current committer, never by these arguments.
use crate::{AgentClaimState,DomainEvent,WorldError,RestrictedStarterClaimRefundSink,RestrictedStarterClaimGrantStatus};
use crate::agent_claims::{split_agent_claim_spend,split_agent_claim_bond_refund};
use crate::world::restricted_claim_grants::RestrictedStarterClaimRefundDecision;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_epoch_v1(current_epoch:u64,latest_claim:&AgentClaimState,liquid_balance:u64,restricted_balance:u64,last_control_epoch:u64,refund_sink:&RestrictedStarterClaimRefundDecision)->Result<Option<DomainEvent>,WorldError>{
        let Some((charged_epochs, amount_due)) = claim_amount_due(latest_claim, current_epoch)?
        else {
            return prepare_release_or_idle_v1(current_epoch, latest_claim, last_control_epoch, refund_sink);
        };

        let funding = split_agent_claim_spend(
            latest_claim.slot_index,
            liquid_balance,
            restricted_balance,
            amount_due,
        );
        if let Ok(funding) = funding {
            return Ok(Some(DomainEvent::AgentClaimUpkeepSettled {
                activity_contract_version: 1,
                claimer_agent_id: latest_claim.claim_owner_id.clone(),
                target_agent_id: latest_claim.target_agent_id.clone(),
                settled_at_epoch: current_epoch,
                charged_epochs,
                amount: amount_due,
                restricted_spent_amount: funding.restricted_amount,
                liquid_spent_amount: funding.liquid_amount,
                upkeep_paid_through_epoch: current_epoch,
            }));
        }

        if let Some(grace_deadline_epoch) = latest_claim.grace_deadline_epoch {
            if current_epoch > grace_deadline_epoch {
                let (collected_upkeep_amount, penalty_amount, refunded_bond_amount) =
                    reclaim_bond_settlement(
                        latest_claim.locked_bond_amount,
                        amount_due,
                        latest_claim.forced_reclaim_penalty_bps,
                    );
                let refunded_bond_split = split_agent_claim_bond_refund(
                    latest_claim.claim_bond_locked_restricted_amount,
                    latest_claim.claim_bond_locked_liquid_amount,
                    collected_upkeep_amount.saturating_add(penalty_amount),
                )
                .map_err(|reason| WorldError::ResourceBalanceInvalid { reason })?;
                return Ok(Some(DomainEvent::AgentClaimReclaimed {
                    claimer_agent_id: latest_claim.claim_owner_id.clone(),
                    target_agent_id: latest_claim.target_agent_id.clone(),
                    reclaimed_at_epoch: current_epoch,
                    reason: "upkeep_delinquent".to_string(),
                    upkeep_arrears_amount: amount_due,
                    collected_upkeep_amount,
                    penalty_amount,
                    refunded_bond_amount,
                    refunded_bond_restricted_amount: refunded_bond_split.restricted_amount,
                    refunded_bond_liquid_amount: refunded_bond_split.liquid_amount,
                    refunded_bond_restricted_sink: refund_sink.sink,
                    refunded_bond_restricted_sink_bucket_id: refund_sink
                        .treasury_bucket_id.clone()
                        .unwrap_or_default(),
                }));
            }
            return Ok(None);
        }

        let grace_deadline_epoch =
            current_epoch.saturating_add(latest_claim.grace_epochs.saturating_sub(1));
        Ok(Some(DomainEvent::AgentClaimEnteredGrace {
            claimer_agent_id: latest_claim.claim_owner_id.clone(),
            target_agent_id: latest_claim.target_agent_id.clone(),
            delinquent_since_epoch: latest_claim.upkeep_paid_through_epoch.saturating_add(1),
            grace_deadline_epoch,
            upkeep_arrears_amount: amount_due,
        }))
    }


pub(in crate::world) fn prepare_release_or_idle_v1(current_epoch:u64,claim:&AgentClaimState,last_control_epoch:u64,refund_sink:&RestrictedStarterClaimRefundDecision)->Result<Option<DomainEvent>,WorldError>{
        if let Some(ready_at_epoch) = claim.release_ready_at_epoch
            && current_epoch >= ready_at_epoch
        {
            let refunded_bond_split = split_agent_claim_bond_refund(
                claim.claim_bond_locked_restricted_amount,
                claim.claim_bond_locked_liquid_amount,
                0,
            )
            .map_err(|reason| WorldError::ResourceBalanceInvalid { reason })?;
            return Ok(Some(DomainEvent::AgentClaimReleased {
                claimer_agent_id: claim.claim_owner_id.clone(),
                target_agent_id: claim.target_agent_id.clone(),
                released_at_epoch: current_epoch,
                refunded_bond_amount: claim.locked_bond_amount,
                refunded_bond_restricted_amount: refunded_bond_split.restricted_amount,
                refunded_bond_liquid_amount: refunded_bond_split.liquid_amount,
                refunded_bond_restricted_sink: refund_sink.sink,
                refunded_bond_restricted_sink_bucket_id: refund_sink
                    .treasury_bucket_id.clone()
                    .unwrap_or_default(),
            }));
        }

        let idle_epochs = current_epoch.saturating_sub(last_control_epoch);
        if idle_epochs >= claim.forced_idle_reclaim_epochs {
            let (_, penalty_amount, refunded_bond_amount) = reclaim_bond_settlement(
                claim.locked_bond_amount,
                0,
                claim.forced_reclaim_penalty_bps,
            );
            let refunded_bond_split = split_agent_claim_bond_refund(
                claim.claim_bond_locked_restricted_amount,
                claim.claim_bond_locked_liquid_amount,
                penalty_amount,
            )
            .map_err(|reason| WorldError::ResourceBalanceInvalid { reason })?;
            return Ok(Some(DomainEvent::AgentClaimReclaimed {
                claimer_agent_id: claim.claim_owner_id.clone(),
                target_agent_id: claim.target_agent_id.clone(),
                reclaimed_at_epoch: current_epoch,
                reason: "idle_timeout".to_string(),
                upkeep_arrears_amount: 0,
                collected_upkeep_amount: 0,
                penalty_amount,
                refunded_bond_amount,
                refunded_bond_restricted_amount: refunded_bond_split.restricted_amount,
                refunded_bond_liquid_amount: refunded_bond_split.liquid_amount,
                refunded_bond_restricted_sink: refund_sink.sink,
                refunded_bond_restricted_sink_bucket_id: refund_sink
                    .treasury_bucket_id.clone()
                    .unwrap_or_default(),
            }));
        }

        if idle_epochs >= claim.idle_warning_epochs && claim.idle_warning_emitted_at_epoch.is_none()
        {
            return Ok(Some(DomainEvent::AgentClaimIdleWarning {
                claimer_agent_id: claim.claim_owner_id.clone(),
                target_agent_id: claim.target_agent_id.clone(),
                warning_emitted_at_epoch: current_epoch,
                forced_reclaim_at_epoch: last_control_epoch
                    .saturating_add(claim.forced_idle_reclaim_epochs),
            }));
        }
        Ok(None)
    }

fn claim_amount_due(
    claim: &AgentClaimState,
    current_epoch: u64,
) -> Result<Option<(u64, u64)>, WorldError> {
    let charged_epochs = current_epoch.saturating_sub(claim.upkeep_paid_through_epoch);
    if charged_epochs == 0 {
        return Ok(None);
    }
    let amount = claim
        .upkeep_per_epoch
        .checked_mul(charged_epochs)
        .ok_or_else(|| WorldError::ResourceBalanceInvalid {
            reason: format!(
                "agent claim upkeep overflow: target={} upkeep_per_epoch={} charged_epochs={}",
                claim.target_agent_id, claim.upkeep_per_epoch, charged_epochs
            ),
        })?;
    Ok(Some((charged_epochs, amount)))
}

fn reclaim_bond_settlement(
    locked_bond_amount: u64,
    upkeep_arrears_amount: u64,
    penalty_bps: u16,
) -> (u64, u64, u64) {
    let collected_upkeep_amount = locked_bond_amount.min(upkeep_arrears_amount);
    let remaining_after_upkeep = locked_bond_amount.saturating_sub(collected_upkeep_amount);
    let penalty_amount = remaining_after_upkeep.saturating_mul(u64::from(penalty_bps)) / 10_000;
    let refunded_bond_amount = remaining_after_upkeep.saturating_sub(penalty_amount);
    (
        collected_upkeep_amount,
        penalty_amount,
        refunded_bond_amount,
    )
}


pub(in crate::world) fn refund_sink_v1(claim:&AgentClaimState,grant:Option<&crate::RestrictedStarterClaimGrantState>)->RestrictedStarterClaimRefundDecision{
        if let Some(bucket_id) = claim
            .claim_bond_restricted_source_treasury_bucket_id
            .clone()
        {
            return RestrictedStarterClaimRefundDecision {
                sink: RestrictedStarterClaimRefundSink::SourceTreasuryBucket,
                treasury_bucket_id: Some(bucket_id),
            };
        }
        let account_id = claim.claim_owner_id.as_str();
        let Some(grant) = grant else {
            return RestrictedStarterClaimRefundDecision {
                sink: RestrictedStarterClaimRefundSink::BeneficiaryRestrictedBalance,
                treasury_bucket_id: None,
            };
        };
        match grant.status {
            RestrictedStarterClaimGrantStatus::Issued => RestrictedStarterClaimRefundDecision {
                sink: RestrictedStarterClaimRefundSink::BeneficiaryRestrictedBalance,
                treasury_bucket_id: None,
            },
            RestrictedStarterClaimGrantStatus::Expired
            | RestrictedStarterClaimGrantStatus::Revoked => RestrictedStarterClaimRefundDecision {
                sink: RestrictedStarterClaimRefundSink::SourceTreasuryBucket,
                treasury_bucket_id: Some(grant.source_treasury_bucket_id.clone()),
            },
        }

}

pub(in crate::world) fn grant_expiry_v1(account_id:&str,grant:&crate::RestrictedStarterClaimGrantState,current_epoch:u64,expired_amount:u64)->DomainEvent{
DomainEvent::RestrictedStarterClaimGrantExpired{beneficiary_account_id:account_id.to_owned(),issuer_id:grant.issuer_id.clone(),issuance_reason:grant.issuance_reason.clone(),spend_scope:grant.spend_scope.clone(),source_treasury_bucket_id:grant.source_treasury_bucket_id.clone(),issued_amount:grant.issued_amount,expired_amount,issued_at_epoch:grant.issued_at_epoch,expired_at_epoch:current_epoch,configured_expires_at_epoch:grant.expires_at_epoch}}
