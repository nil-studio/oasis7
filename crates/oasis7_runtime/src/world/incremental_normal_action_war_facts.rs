//! NEW unhooked Current DeclareWar source; future child of the Diplomacy owner.
use super::*;
use crate::world::event_processing::{
    alliance_fixed::{AllianceFactsV1, AllianceRecordV1},
    war_fixed::{WarFactsV1, sealed},
};
use oasis7_world_state::incremental_agent_fields::AgentFieldV1;
use oasis7_world_state::{
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1, AgentSubtreeValueV1,
        read_agent_subtree_scalar_overlay_v1,
    },
    incremental_diplomacy::{DiplomacyFieldV1, open_current_diplomacy_relation_record_v1},
};
pub(super) struct CurrentWarFactsV1<
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
    pub source: facts::CurrentAllianceFactsV1<
        'a,
        'tx,
        'state,
        'values,
        'read,
        'core,
        M,
        P,
        R,
        V,
        E,
        C,
        PE,
        PC,
    >,
}
impl<M, P, R, V, E, C, PE, PC> sealed::Sealed
    for CurrentWarFactsV1<'_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
impl<'source, M, P, R, V, E, C, PE, PC> WarFactsV1<'source>
    for CurrentWarFactsV1<'_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
        self.source.charge(n)
    }
    fn reserve_outputs(&mut self, action: &crate::Action) -> Result<(), Self::Error> {
        let crate::Action::DeclareWar {
            initiator_agent_id,
            war_id,
            aggressor_alliance_id,
            defender_alliance_id,
            objective,
            ..
        } = action
        else {
            return Err(bad().into());
        };
        let mut payload = 0usize;
        for text in [
            initiator_agent_id,
            war_id,
            aggressor_alliance_id,
            defender_alliance_id,
            objective,
        ] {
            self.charge(text.len())?;
            payload = payload.checked_add(text.len()).ok_or_else(limited)?;
        }
        const NOTES: &[&str] = &[
            "war_id cannot be empty",
            "war already exists: ",
            "aggressor_alliance_id and defender_alliance_id cannot be empty",
            "aggressor_alliance_id and defender_alliance_id must differ",
            "aggressor alliance not found: ",
            "aggressor alliance requires at least  members",
            "initiator  is not a member of aggressor alliance ",
            "defender alliance not found: ",
            "defender alliance requires at least  members",
            "aggressor and defender alliances cannot share members",
            "war objective cannot be empty",
            "war intensity must be > 0",
            "war intensity exceeds max ",
            "aggressor or defender alliance already has an active war",
        ];
        let fixed = NOTES
            .iter()
            .try_fold(20usize, |n, s| n.checked_add(s.len()))
            .ok_or_else(limited)?;
        let n = payload
            .checked_mul(2)
            .and_then(|n| n.checked_add(fixed.checked_mul(2)?))
            .and_then(|n| {
                n.checked_add(
                    size_of::<crate::WorldEventBody>()
                        + size_of::<crate::RejectReason>()
                        + size_of::<Vec<String>>()
                        + 4 * size_of::<String>(),
                )
            })
            .ok_or_else(limited)?;
        if let Some(fee) = &self.source.fees[0] {
            if !fee.uses_budget(self.source.common.memory()) || fee.reserved_bytes() < n {
                return Err(limited().into());
            }
        } else {
            self.source.fees[0] = Some(
                self.source
                    .common
                    .memory()
                    .reserve(n, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)
                    .map_err(BodyMutationError::from)?
                    .retain(()),
            );
        }
        Ok(())
    }
    fn reserve_overlap(&mut self, a: &[String], b: &[String]) -> Result<(), Self::Error> {
        let mut ap = 0usize;
        let mut bp = 0usize;
        for text in a {
            self.charge(text.len())?;
            ap = ap.checked_add(text.len()).ok_or_else(limited)?;
        }
        for text in b {
            self.charge(text.len())?;
            bp = bp.checked_add(text.len()).ok_or_else(limited)?;
        }
        let n =
            oasis7_world_state::incremental_collection_layout::string_set_output_heap_v1(a.len())
                .map_err(BodyMutationError::from)?
                .checked_add(size_of::<std::collections::BTreeSet<&str>>())
                .ok_or_else(limited)?;
        if let Some(fee) = &self.source.fees[1] {
            if !fee.uses_budget(self.source.common.memory()) || fee.reserved_bytes() < n {
                return Err(limited().into());
            }
        } else {
            self.source.fees[1] = Some(
                self.source
                    .common
                    .memory()
                    .reserve(n, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)
                    .map_err(BodyMutationError::from)?
                    .retain(()),
            );
        }
        let bytes = ap
            .checked_mul(a.len())
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(ap.checked_mul(b.len())?))
            .and_then(|n| n.checked_add(bp.checked_mul(a.len())?))
            .ok_or_else(limited)?;
        self.charge(bytes)
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.source.agent_exists(id)
    }
    fn war_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.charge(id.len())?;
        let guard = self.source.context.transaction.guard();
        Ok(open_current_diplomacy_relation_record_v1(
            DiplomacyFieldV1::Wars,
            id,
            self.source.diplomacy,
            self.source.roots,
            self.source.context.transaction,
            self.source.read,
            self.source.index,
            &guard,
            self.source.live,
        )
        .map_err(overlay)?
        .is_some())
    }
    fn alliance(&mut self, id: &str) -> Result<Option<AllianceRecordV1<'source>>, Self::Error> {
        self.source.alliance(id)
    }
    fn active_conflict(&mut self, a: &str, b: &str) -> Result<bool, Self::Error> {
        Ok(self.source.active_war(a)? || self.source.active_war(b)?)
    }
    fn resource(
        &mut self,
        id: &str,
        kind: oasis7_agent_api::world_action::ResourceKind,
    ) -> Result<i64, Self::Error> {
        self.charge(id.len())?;
        let guard = self.source.context.transaction.guard();
        let ordinal = match kind {
            oasis7_agent_api::world_action::ResourceKind::Electricity => 0,
            oasis7_agent_api::world_action::ResourceKind::Data => 1,
        };
        match read_agent_subtree_scalar_overlay_v1(
            AgentSubtreeQualifiedRequestV1 {
                cell: AgentSubtreeScalarRequestV1 {
                    agent: id,
                    field: AgentFieldV1::ResourceAmount,
                    ordinal,
                },
                qualification: self.source.agents,
            },
            self.source.context,
            64,
            &mut std::io::sink(),
            &guard,
            self.source.live,
        )
        .map_err(overlay)?
        {
            None => Ok(0),
            Some(AgentSubtreeValueV1::I64(n)) => Ok(n),
            _ => Err(bad().into()),
        }
    }
}
