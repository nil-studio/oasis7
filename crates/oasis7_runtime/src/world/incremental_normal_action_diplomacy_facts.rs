//! Actual Current read-only source of the sole fixed Alliance predicates.
use crate::world::{event_processing::alliance_fixed::*, incremental_body::BodyMutationError};
use crate::{Action, WorldError};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionFence, RootSetV1},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_diplomacy::{
        DiplomacyFamilyQualificationV1, DiplomacyFieldV1, DiplomacyIndexWorkV1,
        observe_current_active_wars_v1, observe_current_alliance_memberships_v1,
        open_current_diplomacy_relation_record_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
};
use std::mem::size_of;
#[derive(Debug)]
pub(super) enum AllianceEvaluationErrorV1 {
    World(WorldError),
    Body(BodyMutationError),
}
impl From<WorldError> for AllianceEvaluationErrorV1 {
    fn from(e: WorldError) -> Self {
        Self::World(e)
    }
}
impl From<BodyMutationError> for AllianceEvaluationErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
fn bad() -> AllianceEvaluationErrorV1 {
    BodyMutationError::Invalid("Current Alliance source").into()
}
fn limited() -> AllianceEvaluationErrorV1 {
    BodyMutationError::from(ValueTreeError::ResourceLimited(
        "Current Alliance output layout",
    ))
    .into()
}
fn overlay(e: WorldFieldOverlayErrorV1) -> AllianceEvaluationErrorV1 {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => BodyMutationError::from(e),
        WorldFieldOverlayErrorV1::Value(e) => BodyMutationError::from(e),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
    .into()
}
pub(super) struct CurrentAllianceFactsV1<
    'a,
    'tx,
    'state,
    'values,
    'read,
    'core,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
> {
    pub agents: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub diplomacy: &'a ReservedValue<DiplomacyFamilyQualificationV1>,
    pub roots: &'a RootSetV1,
    pub context: &'a mut WorldFieldOverlayContextV1<'tx, 'state, 'values, M, R, E, C>,
    pub read: &'a mut WorldFieldReadContextV1<'read, P, V, PE, PC>,
    pub common: &'a mut FormalBoundedContextV1<'core>,
    pub index: &'a mut DiplomacyIndexWorkV1,
    pub pending: &'a mut usize,
    pub live: &'a ExecutionFence,
    pub max_text: usize,
    pub fees: [Option<ReservedValue<()>>; 4],
}
impl<M, P, R, V, E, C, PE, PC> sealed::Sealed
    for CurrentAllianceFactsV1<'_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
}
impl<M, P, R, V, E, C, PE, PC>
    CurrentAllianceFactsV1<'_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    fn keep_fee(&mut self, slot: usize, n: usize) -> Result<(), AllianceEvaluationErrorV1> {
        if let Some(fee) = &self.fees[slot] {
            if !fee.uses_budget(self.common.memory()) || fee.reserved_bytes() < n {
                return Err(limited());
            }
            return Ok(());
        }
        self.fees[slot] = Some(
            self.common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)
                .map_err(BodyMutationError::from)?
                .retain(()),
        );
        Ok(())
    }
}
impl<'source, M, P, R, V, E, C, PE, PC> AllianceFactsV1<'source>
    for CurrentAllianceFactsV1<'_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    type Error = AllianceEvaluationErrorV1;
    fn charge(&mut self, n: usize) -> Result<(), Self::Error> {
        self.common
            .charge(1, u64::try_from(n).map_err(|_| limited())?)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))
            .map_err(Into::into)
    }
    fn reserve_outputs(&mut self, action: &Action) -> Result<(), Self::Error> {
        let fields: &[&str] = match action {
            Action::FormAlliance {
                proposer_agent_id,
                alliance_id,
                charter,
                ..
            } => &[proposer_agent_id, alliance_id, charter],
            Action::JoinAlliance {
                operator_agent_id,
                alliance_id,
                member_agent_id,
            }
            | Action::LeaveAlliance {
                operator_agent_id,
                alliance_id,
                member_agent_id,
            } => &[operator_agent_id, alliance_id, member_agent_id],
            Action::DissolveAlliance {
                operator_agent_id,
                alliance_id,
                reason,
            } => &[operator_agent_id, alliance_id, reason],
            _ => return Err(bad()),
        };
        let mut payload = 0usize;
        for field in fields {
            self.charge(field.len())?;
            payload = payload.checked_add(field.len()).ok_or_else(limited)?;
        }
        // At most three copied input strings in a successful event. Rejection formats
        // contain at most two input fields; String formatting may grow to twice len.
        // Sum all fixed rejection templates, rather than an unexplained fixed slack.
        const NOTES: &[&str] = &[
            "alliance_id cannot be empty",
            "alliance already exists: ",
            "alliance charter cannot be empty",
            "alliance requires at least 2 unique members",
            "alliance members exceed max ",
            "member  already belongs to alliance ",
            "alliance not found: ",
            "operator  is not a member of alliance ",
            "member  already in alliance ",
            "alliance  has active war and cannot change members",
            "member  is not in alliance ",
            "alliance  has active war and cannot dissolve",
            "dissolve reason cannot be empty",
        ];
        let fixed = NOTES
            .iter()
            .try_fold(0usize, |n, s| n.checked_add(s.len()))
            .ok_or_else(limited)?;
        let layout = payload
            .checked_mul(3)
            .and_then(|n| n.checked_add(fixed.checked_mul(2)?))
            .and_then(|n| {
                n.checked_add(
                    size_of::<crate::WorldEventBody>()
                        + size_of::<crate::RejectReason>()
                        + size_of::<Vec<String>>()
                        + 8 * size_of::<String>(),
                )
            })
            .ok_or_else(limited)?;
        self.keep_fee(0, layout)
    }
    fn reserve_normalization(&mut self, p: &str, m: &[String]) -> Result<(), Self::Error> {
        if self.fees[1].is_none() {
            self.fees[1] = Some(super::alloc::reserve_normalization_v1(p, m, self.common)?);
        } else {
            super::alloc::check_normalization_fee_v1(
                p,
                m,
                self.fees[1].as_ref().ok_or_else(bad)?,
                self.common,
            )?;
        }
        Ok(())
    }
    fn reserve_former_members(&mut self, m: &[String]) -> Result<(), Self::Error> {
        if self.fees[2].is_none() {
            self.fees[2] = Some(super::alloc::reserve_former_members_v1(m, self.common)?);
        } else {
            super::alloc::check_former_members_fee_v1(
                m,
                self.fees[2].as_ref().ok_or_else(bad)?,
                self.common,
            )?;
        }
        Ok(())
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.charge(id.len())?;
        let guard = self.context.transaction.guard();
        Ok(read_current_agent_presence_v1(
            id,
            self.agents,
            self.roots,
            self.context,
            self.read,
            self.pending,
            &guard,
            self.live,
        )
        .map_err(overlay)?
            == CurrentAgentPresenceV1::Existing)
    }
    fn alliance_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.charge(id.len())?;
        let guard = self.context.transaction.guard();
        Ok(open_current_diplomacy_relation_record_v1(
            DiplomacyFieldV1::Alliances,
            id,
            self.diplomacy,
            self.roots,
            self.context.transaction,
            self.read,
            self.index,
            &guard,
            self.live,
        )
        .map_err(overlay)?
        .is_some())
    }
    fn alliance(&mut self, id: &str) -> Result<Option<AllianceRecordV1<'source>>, Self::Error> {
        self.charge(id.len())?;
        let guard = self.context.transaction.guard();
        let Some(proof) = open_current_diplomacy_relation_record_v1(
            DiplomacyFieldV1::Alliances,
            id,
            self.diplomacy,
            self.roots,
            self.context.transaction,
            self.read,
            self.index,
            &guard,
            self.live,
        )
        .map_err(overlay)?
        else {
            return Ok(None);
        };
        let record =
            super::read::read_alliance_v1(id, &proof, self.read, self.max_text, self.common)?;
        // Fixed predicates inspect this selected member list no more than three times.
        for member in &record.members {
            self.charge(member.len().checked_mul(3).ok_or_else(limited)?)?;
        }
        Ok(Some(AllianceRecordV1::Current(record)))
    }
    fn membership(&mut self, id: &str) -> Result<Option<AllianceIdV1<'source>>, Self::Error> {
        self.charge(id.len())?;
        let guard = self.context.transaction.guard();
        let observed = observe_current_alliance_memberships_v1(
            id,
            self.diplomacy,
            self.roots,
            self.context.transaction,
            self.read,
            self.index,
            &guard,
            self.live,
        )
        .map_err(overlay)?;
        observed
            .validate(self.context.transaction, self.common.memory(), self.live)
            .map_err(overlay)?;
        let Some(id) = observed.ids().next() else {
            return Ok(None);
        };
        self.charge(id.len())?;
        self.keep_fee(
            3,
            id.len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(size_of::<String>()))
                .ok_or_else(limited)?,
        )?;
        let fee = self
            .common
            .memory()
            .reserve(
                id.len()
                    .checked_add(size_of::<String>())
                    .ok_or_else(limited)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)
            .map_err(BodyMutationError::from)?;
        let value = id.to_owned();
        if value.capacity() > id.len() {
            return Err(limited());
        }
        Ok(Some(AllianceIdV1::Current(fee.retain(value))))
    }
    fn active_war(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.charge(id.len())?;
        let guard = self.context.transaction.guard();
        let observed = observe_current_active_wars_v1(
            id,
            self.diplomacy,
            self.roots,
            self.context.transaction,
            self.read,
            self.index,
            &guard,
            self.live,
        )
        .map_err(overlay)?;
        observed
            .validate(self.context.transaction, self.common.memory(), self.live)
            .map_err(overlay)?;
        Ok(observed.ids().len() != 0)
    }
}
