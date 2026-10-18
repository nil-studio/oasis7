//! Current-owner commands and exact pending-intent authority dispositions.
use super::{AgentDecisionCausalContextV1, AgentDelegationGrantV1, Ledger, World, denied};
use crate::{AgentIntentReplayDisposition, WorldError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOwnerControlKindV1 {
    Override,
    Interrupt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentOwnerControlV1 {
    pub control_id: String,
    pub agent_id: String,
    pub intent_id: String,
    pub request_digest: String,
    pub grant_id: Option<String>,
    pub expected_grant_revision: Option<u64>,
    pub kind: AgentOwnerControlKindV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct OwnerControlRecord {
    pub owner_id: String,
    pub control: AgentOwnerControlV1,
    pub disposition: AgentIntentReplayDisposition,
    pub consumed_receipt_id: Option<String>,
}

impl World {
    pub(crate) fn agent_owner_control_replay(
        &self,
        player_id: &str,
        control_id: &str,
    ) -> Result<Option<(AgentOwnerControlV1, AgentIntentReplayDisposition)>, WorldError> {
        let ledger = self.delegation_ledger()?;
        let Some(record) = ledger.controls.get(control_id) else {
            return Ok(None);
        };
        if record.owner_id != player_id
            || self
                .state
                .starter_oc_claims
                .get(&record.control.agent_id)
                .is_none_or(|owner| owner.player_id != player_id)
        {
            return Err(denied("owner_control_denied"));
        }
        Ok(Some((record.control.clone(), record.disposition.clone())))
    }
    pub(super) fn validate_recorded_owner_override(
        &self,
        ledger: &Ledger,
        grant: &AgentDelegationGrantV1,
        context: &AgentDecisionCausalContextV1,
    ) -> Result<(), WorldError> {
        if context.owner_control_refs.len() != 1 {
            return Err(denied("override_context_untrusted"));
        }
        let record = ledger
            .controls
            .get(&context.owner_control_refs[0])
            .ok_or_else(|| denied("override_context_untrusted"))?;
        if record.control.kind != AgentOwnerControlKindV1::Override
            || record.consumed_receipt_id.is_some()
            || context.intent_id.as_ref() != Some(&record.control.intent_id)
            || context.override_actor.as_ref() != Some(&record.owner_id)
            || record.control.grant_id.as_ref() != Some(&grant.grant_id)
            || record.control.expected_grant_revision != Some(grant.revision)
        {
            return Err(denied("override_authority_stale"));
        }
        Ok(())
    }
    /// Called only after the transport verifies the principal/signature.
    /// The runtime independently rechecks the live owner and exact intent.
    pub(crate) fn bind_agent_owner_control(
        &mut self,
        player_id: &str,
        control: AgentOwnerControlV1,
    ) -> Result<AgentIntentReplayDisposition, WorldError> {
        let owner = self
            .state
            .starter_oc_claims
            .get(&control.agent_id)
            .ok_or_else(|| denied("owner_source_unavailable"))?;
        if player_id != owner.player_id || control.control_id.trim().is_empty() {
            return Err(denied("owner_control_denied"));
        }
        let mut transaction = self.clone();
        let mut ledger = transaction.delegation_ledger()?;
        if let Some(old) = ledger.controls.get(&control.control_id) {
            if old.control == control && old.owner_id == player_id {
                return Ok(old.disposition.clone());
            }
            return Err(denied("owner_control_idempotency_conflict"));
        }
        let intent = transaction
            .state
            .agents
            .get(&control.agent_id)
            .and_then(|cell| cell.intent.as_ref())
            .ok_or_else(|| denied("intent_missing"))?;
        if intent.intent_id != control.intent_id
            || intent.request_digest != control.request_digest
            || !matches!(intent.status.as_str(), "accepted" | "blocked")
        {
            return Err(denied("owner_control_intent_stale"));
        }
        if control.grant_id.is_some() != control.expected_grant_revision.is_some() {
            return Err(denied("owner_control_grant_tuple_invalid"));
        }
        if let Some(id) = &control.grant_id {
            let grant = ledger
                .grants
                .get(id)
                .ok_or_else(|| denied("grant_missing"))?;
            if grant.agent_id != control.agent_id
                || grant.owner_id != player_id
                || control.expected_grant_revision != Some(grant.revision)
            {
                return Err(denied("grant_revision_stale"));
            }
            transaction.validate_grant_source(grant)?;
            if control.kind == AgentOwnerControlKindV1::Override
                && (grant.revoked
                    || transaction.state.time < grant.valid_from_tick
                    || transaction.state.time > grant.valid_until_tick)
            {
                return Err(denied("override_grant_unavailable"));
            }
        } else if control.kind == AgentOwnerControlKindV1::Override {
            return Err(denied("override_requires_grant"));
        }
        if control.kind == AgentOwnerControlKindV1::Override
            && ledger.controls.values().any(|record| {
                record.control.intent_id == control.intent_id
                    && record.control.kind == AgentOwnerControlKindV1::Override
                    && record.consumed_receipt_id.is_none()
            })
        {
            return Err(denied("override_already_pending"));
        }
        let disposition = if control.kind == AgentOwnerControlKindV1::Interrupt {
            transaction.terminate_agent_authority_intent_exact(
                &control.agent_id,
                &control.intent_id,
                &control.request_digest,
                "cancelled",
                "owner_interrupted_replan_required",
            )?
        } else {
            AgentIntentReplayDisposition::from(intent)
        };
        ledger.controls.insert(
            control.control_id.clone(),
            OwnerControlRecord {
                owner_id: player_id.into(),
                control,
                disposition: disposition.clone(),
                consumed_receipt_id: None,
            },
        );
        transaction.save_delegation_ledger(&ledger)?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(disposition)
    }

    pub(super) fn apply_current_owner_control(
        &self,
        ledger: &Ledger,
        grant: Option<&AgentDelegationGrantV1>,
        context: &mut AgentDecisionCausalContextV1,
    ) -> Result<(), WorldError> {
        // Only this persisted authenticated record can author an override.
        if context.override_actor.is_some() || !context.owner_control_refs.is_empty() {
            return Err(denied("override_context_untrusted"));
        }
        let Some(intent_id) = &context.intent_id else {
            return Ok(());
        };
        let Some(record) = ledger.controls.values().find(|record| {
            &record.control.intent_id == intent_id
                && record.control.kind == AgentOwnerControlKindV1::Override
                && record.consumed_receipt_id.is_none()
        }) else {
            return Ok(());
        };
        let grant = grant.ok_or_else(|| denied("override_requires_grant"))?;
        let intent = self
            .state
            .agent_intent_ledger
            .get(intent_id)
            .ok_or_else(|| denied("intent_missing"))?;
        if record.control.request_digest != intent.request_digest
            || record.control.grant_id.as_ref() != Some(&grant.grant_id)
            || record.control.expected_grant_revision != Some(grant.revision)
            || record.owner_id != grant.owner_id
        {
            return Err(denied("override_authority_stale"));
        }
        self.validate_grant_source(grant)?;
        context.override_actor = Some(record.owner_id.clone());
        context
            .owner_control_refs
            .push(record.control.control_id.clone());
        Ok(())
    }

    /// Persist the metadata-only disposition while returning the original
    /// execution error. Prior effects and settlement receipts are immutable.
    pub(super) fn persist_agent_delegation_denial(
        &mut self,
        agent_id: &str,
        context: &AgentDecisionCausalContextV1,
        error: &WorldError,
    ) -> Result<(), WorldError> {
        let WorldError::ResourceBalanceInvalid { reason } = error else {
            return Ok(());
        };
        let Some(code) = reason.strip_prefix("agent_delegation:") else {
            return Ok(());
        };
        let Some(intent_id) = &context.intent_id else {
            return Ok(());
        };
        let Some(intent) = self.state.agent_intent_ledger.get(intent_id).cloned() else {
            return Ok(());
        };
        if intent.agent_id != agent_id {
            return Ok(());
        }
        if !matches!(intent.status.as_str(), "accepted" | "blocked") {
            return Ok(());
        }
        let mut transaction = self.clone();
        transaction.terminate_agent_authority_intent_exact(
            &intent.agent_id,
            intent_id,
            &intent.request_digest,
            if code == "agent_dissent" {
                "blocked"
            } else {
                "rejected"
            },
            &format!("agency_{code}_replan_required"),
        )?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(())
    }

    pub(in crate::world) fn persist_agent_delegation_commit_denial(
        &mut self,
        commit_id: &str,
        error: &WorldError,
    ) -> Result<(), WorldError> {
        let request_id = self
            .cognition
            .get("commit_records")
            .and_then(serde_json::Value::as_array)
            .and_then(|records| {
                records.iter().find(|r| {
                    r.get("commit_id").and_then(serde_json::Value::as_str) == Some(commit_id)
                })
            })
            .and_then(|r| r.get("decision_request_id"))
            .and_then(serde_json::Value::as_str);
        let decision = request_id.and_then(|id| {
            self.delegation_ledger()
                .ok()?
                .decisions
                .get(id)
                .map(|d| (d.agent_id.clone(), d.context.clone()))
        });
        if let Some((agent_id, context)) = decision {
            self.persist_agent_delegation_denial(&agent_id, &context, error)?;
        }
        Ok(())
    }
}
