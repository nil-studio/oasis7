//! Fixed Current audience lookup; no source choice is exposed to callers.
use super::scope::{CurrentQueries, Query};
use super::*;
use crate::service_admission::WorldServicePayloadV1;
use crate::world::incremental_factory_phase::io::{
    FactoryPhaseMeterLaneV1 as Lane, FactoryPhaseMeterLanesV1,
};
use oasis7_client_api::world_service::SubmitIntentRequest;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_runtime_revocation::RevocationRecoveredV1,
    incremental_world_native::{StarterBindingObservationV1, observe_current_starter_binding_v1},
};
pub(super) enum ScopeSourceV1 {
    Participant(RevocationRecoveredV1<oasis7_agent_api::game::GameParticipantReceiptV1>),
    Starter(ReservedValue<StarterBindingObservationV1>),
}
pub(super) fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => original(e),
    }
}
struct StarterLoan<'a, 'v, P, V, E, C> {
    read: &'a mut WorldFieldReadContextV1<'v, P, V, E, C>,
    work: &'a mut NormalBusinessReadWorkV1,
}
impl<P, V, E, C> FactoryPhaseMeterLanesV1 for StarterLoan<'_, '_, P, V, E, C> {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(Lane<'a>)) {
        v(Lane::Steps(&mut self.read.values.io.nodes_remaining));
        v(Lane::Bytes(&mut self.read.values.io.read_bytes_remaining));
        v(Lane::Bytes(&mut self.read.values.io.write_bytes_remaining));
        v(Lane::Bytes(&mut self.read.key_work.bytes_remaining));
        let w = &mut self.work.starter_native;
        v(Lane::Steps(&mut w.records_remaining));
        v(Lane::Entries(&mut w.trees_remaining));
        v(Lane::Steps(&mut w.typed.json.cells_remaining));
        v(Lane::Steps(&mut w.typed.json.json_nodes_remaining));
        let w = &mut self.work.starter_scan;
        v(Lane::Steps(&mut w.nodes_remaining));
        v(Lane::Steps(&mut w.leaves_remaining));
        v(Lane::Bytes(&mut w.read_bytes_remaining));
        let w = &mut self.work.starter;
        v(Lane::Steps(&mut w.steps_remaining));
        v(Lane::Bytes(&mut w.bytes_remaining));
        v(Lane::Bytes(&mut w.comparison_bytes_remaining));
        v(Lane::Steps(&mut w.moved_slots_remaining));
        v(Lane::Entries(&mut w.pending_entries_remaining));
    }
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn super::super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(super) fn read_current_business_scope_v1<
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
        parent: &BoundNormalTickParentV1<'_>,
        revocation: &RuntimeRevocationCurrentV1,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut NormalBusinessReadWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(String, MemoryReservation, Vec<ScopeSourceV1>), RootModuleExecutionErrorV1<O::Error>>
    {
        let layout = 2 * std::mem::size_of::<ScopeSourceV1>() + std::mem::size_of::<String>() + 128;
        let mut fee = common
            .memory()
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut sources = Vec::new();
        sources.try_reserve_exact(2).map_err(|_| bad())?;
        if sources.capacity() > 2 {
            return Err(bad().into());
        }
        let mut first = None;
        let mut facts = CurrentQueries(|query: Query<'_>| -> Result<Option<String>, String> {
            let result = (|| -> Result<Option<String>, RootModuleExecutionErrorV1<O::Error>> {
                match query {
                    Query::Participant(account) => {
                        let refs = parent.refs();
                        let guard = self.ctx.transaction.guard();
                        let live = live_owner();
                        let row = with_authorization_source_read_work_v1(
                            refs.roots,
                            self.ctx,
                            read,
                            &mut work.authorization,
                            common,
                            |ctx, read, w| {
                                read_current_business_participant_v1(
                                    account,
                                    refs.storage.current().runtime().revocation(),
                                    revocation,
                                    ctx,
                                    read,
                                    &mut w.scan,
                                    &mut w.typed,
                                    &guard,
                                    &live,
                                )
                                .map_err(SourceError::from)
                            },
                        )
                        .map_err(source)?;
                        let Some(row) = row else { return Ok(None) };
                        let n = row.value().agent_id.len();
                        common.charge(1, n as u64).map_err(original)?;
                        let mut textfee = common
                            .memory()
                            .reserve(n, MemoryClass::Ordinary)
                            .map_err(ValueTreeError::Memory)?;
                        let text = row.value().agent_id.clone();
                        fee.merge(&mut textfee)
                            .map_err(BodyMutationError::Invalid)?;
                        sources.push(ScopeSourceV1::Participant(row));
                        Ok(Some(text))
                    }
                    Query::CollectData { player, key } => {
                        let refs = parent.refs();
                        let mut loan = StarterLoan { read, work };
                        let row=self.with_factory_prologue_external_meter_v1(common,live_owner,&mut loan,|root,live,loan|{
       let guard=root.ctx.transaction.guard();let live=live();
       let mut current=WorldFieldReadContextV1{root:refs.roots.state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization),state:loan.read.state,values:oasis7_formal_protocol::incremental_value::ValueReadContext{source:loan.read.values.source,memory:loan.read.values.memory,class:loan.read.values.class,io:&mut*loan.read.values.io},key_work:&mut*loan.read.key_work,semantic_bytes:loan.read.semantic_bytes,record_bytes:loan.read.record_bytes,take_state_error:&mut loan.read.take_state_error,cancelled:&mut loan.read.cancelled};
       observe_current_starter_binding_v1(player,key,refs.storage.current().native().native().starter_binding_index(),refs.roots,root.ctx.transaction,&mut current,&mut loan.work.starter_scan,&mut loan.work.starter_native,&mut loan.work.starter,&guard,&live).map_err(overlay).map_err(Into::into)
      })?;
                        row.validate(self.ctx.transaction, common.memory(), &live_owner())
                            .map_err(overlay)?;
                        if row.matching_claims() > 1 {
                            return Err(note(
                                "formal result CollectData owner Agent claim is ambiguous".into(),
                                common,
                            )?);
                        }
                        let text = if let Some(agent) = row.unique_agent_id() {
                            common.charge(1, agent.len() as u64).map_err(original)?;
                            let mut f = common
                                .memory()
                                .reserve(agent.len(), MemoryClass::Ordinary)
                                .map_err(ValueTreeError::Memory)?;
                            let text = agent.to_owned();
                            fee.merge(&mut f).map_err(BodyMutationError::Invalid)?;
                            Some(text)
                        } else {
                            None
                        };
                        sources.push(ScopeSourceV1::Starter(row));
                        Ok(text)
                    }
                }
            })();
            match result {
                Ok(v) => Ok(v),
                Err(e) => {
                    first = Some(e);
                    Err("actual Current business scope source failed".into())
                }
            }
        });
        let result = scope::for_request(&mut facts, request);
        drop(facts);
        if let Some(error) = first {
            return Err(error);
        }
        let scope = match result {
            Ok(v) => v,
            Err(e) => return Err(note(e, common)?),
        };
        // Deterministic direct payload identities also allocate their scope here.
        common.charge(1, scope.len() as u64).map_err(original)?;
        let mut f = common
            .memory()
            .reserve(scope.capacity(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        fee.merge(&mut f).map_err(BodyMutationError::Invalid)?;
        Ok((scope, fee, sources))
    }
}
