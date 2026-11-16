//! Original diplomacy wire emitters. Collection generics borrow immutable DATA;
//! all original order/skip conditions stay in this one kernel.
use super::*;
use serde::{Serialize, Serializer, ser::SerializeStruct};
pub struct AllianceProjectionFieldsV1<'a, M: Serialize + ?Sized> {
    pub alliance_id: &'a str,
    pub members: &'a M,
    pub charter: &'a str,
    pub formed_by_agent_id: &'a str,
    pub formed_at: WorldTime,
}
impl<M: Serialize + ?Sized> Serialize for AllianceProjectionFieldsV1<'_, M> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct("AllianceState", 5)?;
        out.serialize_field("alliance_id", self.alliance_id)?;
        out.serialize_field("members", self.members)?;
        out.serialize_field("charter", self.charter)?;
        out.serialize_field("formed_by_agent_id", self.formed_by_agent_id)?;
        out.serialize_field("formed_at", &self.formed_at)?;
        out.end()
    }
}
impl Serialize for AllianceState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            alliance_id,
            members,
            charter,
            formed_by_agent_id,
            formed_at,
        } = self;
        AllianceProjectionFieldsV1 {
            alliance_id: alliance_id,
            members: members,
            charter: charter,
            formed_by_agent_id: formed_by_agent_id,
            formed_at: *formed_at,
        }
        .serialize(s)
    }
}
pub struct WarProjectionFieldsV1<'a, O: Serialize + ?Sized> {
    pub war_id: &'a str,
    pub initiator_agent_id: &'a str,
    pub aggressor_alliance_id: &'a str,
    pub defender_alliance_id: &'a str,
    pub objective: &'a str,
    pub intensity: u32,
    pub active: bool,
    pub declared_mobilization_electricity_cost: i64,
    pub declared_mobilization_data_cost: i64,
    pub max_duration_ticks: u64,
    pub aggressor_score: i64,
    pub defender_score: i64,
    pub concluded_at: Option<WorldTime>,
    pub winner_alliance_id: Option<&'a str>,
    pub loser_alliance_id: Option<&'a str>,
    pub settlement_summary: Option<&'a str>,
    pub participant_outcomes: &'a O,
    pub declared_at: WorldTime,
}
impl<O: Serialize + ?Sized> Serialize for WarProjectionFieldsV1<'_, O> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct(
            "WarState",
            14 + usize::from(self.concluded_at.is_some())
                + usize::from(self.winner_alliance_id.is_some())
                + usize::from(self.loser_alliance_id.is_some())
                + usize::from(self.settlement_summary.is_some()),
        )?;
        out.serialize_field("war_id", self.war_id)?;
        out.serialize_field("initiator_agent_id", self.initiator_agent_id)?;
        out.serialize_field("aggressor_alliance_id", self.aggressor_alliance_id)?;
        out.serialize_field("defender_alliance_id", self.defender_alliance_id)?;
        out.serialize_field("objective", self.objective)?;
        out.serialize_field("intensity", &self.intensity)?;
        out.serialize_field("active", &self.active)?;
        out.serialize_field(
            "declared_mobilization_electricity_cost",
            &self.declared_mobilization_electricity_cost,
        )?;
        out.serialize_field(
            "declared_mobilization_data_cost",
            &self.declared_mobilization_data_cost,
        )?;
        out.serialize_field("max_duration_ticks", &self.max_duration_ticks)?;
        out.serialize_field("aggressor_score", &self.aggressor_score)?;
        out.serialize_field("defender_score", &self.defender_score)?;
        if let Some(value) = &self.concluded_at {
            out.serialize_field("concluded_at", value)?
        }
        if let Some(value) = &self.winner_alliance_id {
            out.serialize_field("winner_alliance_id", value)?
        }
        if let Some(value) = &self.loser_alliance_id {
            out.serialize_field("loser_alliance_id", value)?
        }
        if let Some(value) = &self.settlement_summary {
            out.serialize_field("settlement_summary", value)?
        }
        out.serialize_field("participant_outcomes", self.participant_outcomes)?;
        out.serialize_field("declared_at", &self.declared_at)?;
        out.end()
    }
}
impl Serialize for WarState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            war_id,
            initiator_agent_id,
            aggressor_alliance_id,
            defender_alliance_id,
            objective,
            intensity,
            active,
            declared_mobilization_electricity_cost,
            declared_mobilization_data_cost,
            max_duration_ticks,
            aggressor_score,
            defender_score,
            concluded_at,
            winner_alliance_id,
            loser_alliance_id,
            settlement_summary,
            participant_outcomes,
            declared_at,
        } = self;
        WarProjectionFieldsV1 {
            war_id: war_id,
            initiator_agent_id: initiator_agent_id,
            aggressor_alliance_id: aggressor_alliance_id,
            defender_alliance_id: defender_alliance_id,
            objective: objective,
            intensity: *intensity,
            active: *active,
            declared_mobilization_electricity_cost: *declared_mobilization_electricity_cost,
            declared_mobilization_data_cost: *declared_mobilization_data_cost,
            max_duration_ticks: *max_duration_ticks,
            aggressor_score: *aggressor_score,
            defender_score: *defender_score,
            concluded_at: *concluded_at,
            winner_alliance_id: winner_alliance_id.as_deref(),
            loser_alliance_id: loser_alliance_id.as_deref(),
            settlement_summary: settlement_summary.as_deref(),
            participant_outcomes: participant_outcomes,
            declared_at: *declared_at,
        }
        .serialize(s)
    }
}
pub struct CrisisProjectionFieldsV1<'a> {
    pub crisis_id: &'a str,
    pub kind: &'a str,
    pub severity: u32,
    pub status: CrisisStatus,
    pub opened_at: WorldTime,
    pub expires_at: WorldTime,
    pub resolver_agent_id: Option<&'a str>,
    pub strategy: Option<&'a str>,
    pub success: Option<bool>,
    pub impact: i64,
    pub resolved_at: Option<WorldTime>,
}
impl Serialize for CrisisProjectionFieldsV1<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct(
            "CrisisState",
            7 + usize::from(self.resolver_agent_id.is_some())
                + usize::from(self.strategy.is_some())
                + usize::from(self.success.is_some())
                + usize::from(self.resolved_at.is_some()),
        )?;
        out.serialize_field("crisis_id", self.crisis_id)?;
        out.serialize_field("kind", self.kind)?;
        out.serialize_field("severity", &self.severity)?;
        out.serialize_field("status", &self.status)?;
        out.serialize_field("opened_at", &self.opened_at)?;
        out.serialize_field("expires_at", &self.expires_at)?;
        if let Some(value) = &self.resolver_agent_id {
            out.serialize_field("resolver_agent_id", value)?
        }
        if let Some(value) = &self.strategy {
            out.serialize_field("strategy", value)?
        }
        if let Some(value) = &self.success {
            out.serialize_field("success", value)?
        }
        out.serialize_field("impact", &self.impact)?;
        if let Some(value) = &self.resolved_at {
            out.serialize_field("resolved_at", value)?
        }
        out.end()
    }
}
impl Serialize for CrisisState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            crisis_id,
            kind,
            severity,
            status,
            opened_at,
            expires_at,
            resolver_agent_id,
            strategy,
            success,
            impact,
            resolved_at,
        } = self;
        CrisisProjectionFieldsV1 {
            crisis_id: crisis_id,
            kind: kind,
            severity: *severity,
            status: *status,
            opened_at: *opened_at,
            expires_at: *expires_at,
            resolver_agent_id: resolver_agent_id.as_deref(),
            strategy: strategy.as_deref(),
            success: *success,
            impact: *impact,
            resolved_at: *resolved_at,
        }
        .serialize(s)
    }
}
pub struct EconomicContractProjectionFieldsV1<'a> {
    pub contract_id: &'a str,
    pub creator_agent_id: &'a str,
    pub counterparty_agent_id: &'a str,
    pub fulfillment_kind: EconomicContractFulfillmentKind,
    pub settlement_kind: ResourceKind,
    pub settlement_amount: i64,
    pub reputation_stake: i64,
    pub expires_at: WorldTime,
    pub description: &'a str,
    pub status: EconomicContractStatus,
    pub accepted_at: Option<WorldTime>,
    pub settled_at: Option<WorldTime>,
    pub settlement_success: Option<bool>,
    pub transfer_amount: i64,
    pub tax_amount: i64,
    pub settlement_notes: Option<&'a str>,
}
impl Serialize for EconomicContractProjectionFieldsV1<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct(
            "EconomicContractState",
            12 + usize::from(self.accepted_at.is_some())
                + usize::from(self.settled_at.is_some())
                + usize::from(self.settlement_success.is_some())
                + usize::from(self.settlement_notes.is_some()),
        )?;
        out.serialize_field("contract_id", self.contract_id)?;
        out.serialize_field("creator_agent_id", self.creator_agent_id)?;
        out.serialize_field("counterparty_agent_id", self.counterparty_agent_id)?;
        out.serialize_field("fulfillment_kind", &self.fulfillment_kind)?;
        out.serialize_field("settlement_kind", &self.settlement_kind)?;
        out.serialize_field("settlement_amount", &self.settlement_amount)?;
        out.serialize_field("reputation_stake", &self.reputation_stake)?;
        out.serialize_field("expires_at", &self.expires_at)?;
        out.serialize_field("description", self.description)?;
        out.serialize_field("status", &self.status)?;
        if let Some(value) = &self.accepted_at {
            out.serialize_field("accepted_at", value)?
        }
        if let Some(value) = &self.settled_at {
            out.serialize_field("settled_at", value)?
        }
        if let Some(value) = &self.settlement_success {
            out.serialize_field("settlement_success", value)?
        }
        out.serialize_field("transfer_amount", &self.transfer_amount)?;
        out.serialize_field("tax_amount", &self.tax_amount)?;
        if let Some(value) = &self.settlement_notes {
            out.serialize_field("settlement_notes", value)?
        }
        out.end()
    }
}
impl Serialize for EconomicContractState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            contract_id,
            creator_agent_id,
            counterparty_agent_id,
            fulfillment_kind,
            settlement_kind,
            settlement_amount,
            reputation_stake,
            expires_at,
            description,
            status,
            accepted_at,
            settled_at,
            settlement_success,
            transfer_amount,
            tax_amount,
            settlement_notes,
        } = self;
        EconomicContractProjectionFieldsV1 {
            contract_id: contract_id,
            creator_agent_id: creator_agent_id,
            counterparty_agent_id: counterparty_agent_id,
            fulfillment_kind: *fulfillment_kind,
            settlement_kind: *settlement_kind,
            settlement_amount: *settlement_amount,
            reputation_stake: *reputation_stake,
            expires_at: *expires_at,
            description: description,
            status: *status,
            accepted_at: *accepted_at,
            settled_at: *settled_at,
            settlement_success: *settlement_success,
            transfer_amount: *transfer_amount,
            tax_amount: *tax_amount,
            settlement_notes: settlement_notes.as_deref(),
        }
        .serialize(s)
    }
}
