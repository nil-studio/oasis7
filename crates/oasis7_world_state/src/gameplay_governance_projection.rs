//! The original governance wire-field emitter shared by the legacy DTOs and
//! immutable projection DATA. Generic collections provide bytes, not verdicts.
use super::*;
use serde::{Serialize, Serializer, ser::SerializeStruct};
pub struct GovernanceIdentityProjectionFieldsV1<'a> {
    pub agent_id: &'a str,
    pub stake_locked: u64,
    pub warmup_until_tick: WorldTime,
    pub status: GovernanceIdentityStatus,
    pub slash_count: u32,
    pub updated_at: WorldTime,
}
impl Serialize for GovernanceIdentityProjectionFieldsV1<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct("GovernanceIdentityProfileState", 6)?;
        out.serialize_field("agent_id", self.agent_id)?;
        out.serialize_field("stake_locked", &self.stake_locked)?;
        out.serialize_field("warmup_until_tick", &self.warmup_until_tick)?;
        out.serialize_field("status", &self.status)?;
        out.serialize_field("slash_count", &self.slash_count)?;
        out.serialize_field("updated_at", &self.updated_at)?;
        out.end()
    }
}
impl Serialize for GovernanceIdentityProfileState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            agent_id,
            stake_locked,
            warmup_until_tick,
            status,
            slash_count,
            updated_at,
        } = self;
        GovernanceIdentityProjectionFieldsV1 {
            agent_id,
            stake_locked: *stake_locked,
            warmup_until_tick: *warmup_until_tick,
            status: *status,
            slash_count: *slash_count,
            updated_at: *updated_at,
        }
        .serialize(s)
    }
}
pub struct GovernanceProposalProjectionFieldsV1<'a, O: Serialize + ?Sized, V: Serialize + ?Sized> {
    pub proposal_key: &'a str,
    pub proposer_agent_id: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub options: &'a O,
    pub voting_window_ticks: u64,
    pub quorum_weight: u64,
    pub pass_threshold_bps: u16,
    pub opened_at: WorldTime,
    pub closes_at: WorldTime,
    pub status: GovernanceProposalStatus,
    pub finalized_at: Option<WorldTime>,
    pub winning_option: Option<&'a str>,
    pub winning_weight: u64,
    pub total_weight_at_finalize: u64,
    pub snapshot_at_tick: WorldTime,
    pub vote_weight_snapshot: &'a V,
}
impl<O: Serialize + ?Sized, V: Serialize + ?Sized> Serialize
    for GovernanceProposalProjectionFieldsV1<'_, O, V>
{
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct(
            "GovernanceProposalState",
            15 + usize::from(self.finalized_at.is_some())
                + usize::from(self.winning_option.is_some()),
        )?;
        out.serialize_field("proposal_key", self.proposal_key)?;
        out.serialize_field("proposer_agent_id", self.proposer_agent_id)?;
        out.serialize_field("title", self.title)?;
        out.serialize_field("description", self.description)?;
        out.serialize_field("options", self.options)?;
        out.serialize_field("voting_window_ticks", &self.voting_window_ticks)?;
        out.serialize_field("quorum_weight", &self.quorum_weight)?;
        out.serialize_field("pass_threshold_bps", &self.pass_threshold_bps)?;
        out.serialize_field("opened_at", &self.opened_at)?;
        out.serialize_field("closes_at", &self.closes_at)?;
        out.serialize_field("status", &self.status)?;
        if let Some(value) = self.finalized_at {
            out.serialize_field("finalized_at", &value)?
        }
        if let Some(value) = self.winning_option {
            out.serialize_field("winning_option", value)?
        }
        out.serialize_field("winning_weight", &self.winning_weight)?;
        out.serialize_field("total_weight_at_finalize", &self.total_weight_at_finalize)?;
        out.serialize_field("snapshot_at_tick", &self.snapshot_at_tick)?;
        out.serialize_field("vote_weight_snapshot", self.vote_weight_snapshot)?;
        out.end()
    }
}
impl Serialize for GovernanceProposalState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            proposal_key,
            proposer_agent_id,
            title,
            description,
            options,
            voting_window_ticks,
            quorum_weight,
            pass_threshold_bps,
            opened_at,
            closes_at,
            status,
            finalized_at,
            winning_option,
            winning_weight,
            total_weight_at_finalize,
            snapshot_at_tick,
            vote_weight_snapshot,
        } = self;
        GovernanceProposalProjectionFieldsV1 {
            proposal_key,
            proposer_agent_id,
            title,
            description,
            options,
            voting_window_ticks: *voting_window_ticks,
            quorum_weight: *quorum_weight,
            pass_threshold_bps: *pass_threshold_bps,
            opened_at: *opened_at,
            closes_at: *closes_at,
            status: *status,
            finalized_at: *finalized_at,
            winning_option: winning_option.as_deref(),
            winning_weight: *winning_weight,
            total_weight_at_finalize: *total_weight_at_finalize,
            snapshot_at_tick: *snapshot_at_tick,
            vote_weight_snapshot,
        }
        .serialize(s)
    }
}
pub struct GovernanceVoteProjectionFieldsV1<'a, B: Serialize + ?Sized, T: Serialize + ?Sized> {
    pub proposal_key: &'a str,
    pub votes_by_agent: &'a B,
    pub tallies: &'a T,
    pub total_weight: u64,
    pub last_updated_at: WorldTime,
}
impl<B: Serialize + ?Sized, T: Serialize + ?Sized> Serialize
    for GovernanceVoteProjectionFieldsV1<'_, B, T>
{
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct("GovernanceVoteState", 5)?;
        out.serialize_field("proposal_key", self.proposal_key)?;
        out.serialize_field("votes_by_agent", self.votes_by_agent)?;
        out.serialize_field("tallies", self.tallies)?;
        out.serialize_field("total_weight", &self.total_weight)?;
        out.serialize_field("last_updated_at", &self.last_updated_at)?;
        out.end()
    }
}
impl Serialize for GovernanceVoteState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            proposal_key,
            votes_by_agent,
            tallies,
            total_weight,
            last_updated_at,
        } = self;
        GovernanceVoteProjectionFieldsV1 {
            proposal_key,
            votes_by_agent,
            tallies,
            total_weight: *total_weight,
            last_updated_at: *last_updated_at,
        }
        .serialize(s)
    }
}
