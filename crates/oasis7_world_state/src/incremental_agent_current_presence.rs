//! Narrow current Agent existence, never a materialized or authority view.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentAgentPresenceV1 {
    Existing,
    Absent,
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_agent_presence_v1<M, R, E, C, P, V, ER, CR>(
    agent: &str,
    qualification: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    parent: &RootSetV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    remaining_pending_entries: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentAgentPresenceV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent_read.check()?;
        // RootSet::digest encodes its bounded validated frame. Admit that
        // temporary encoding before checking the supplied parent binding.
        let _phase = context
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if context.values.class != MemoryClass::Ordinary
            || parent_read.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !context.values.memory.same_pool(parent_read.values.memory)
            || !qualification.uses_budget(context.values.memory)
            || qualification.allocation_class() != MemoryClass::Ordinary
            || qualification.reserved_bytes() < 512
            || parent
                .digest()
                .map_err(|_| ValueTreeError::Invalid("agent presence parent"))?
                != tx.parent_fence().parent_roots
        {
            return Err(ValueTreeError::Invalid("agent presence scope").into());
        }
        qualification.validate_digest(&tx.parent_fence().parent_roots)?;
        let identity = AgentFieldKeyV1::new(agent, AgentFieldV1::Identity, 0)
            .map_err(ValueTreeError::Invalid)?;
        let owner = AgentFieldKeyV1::from_bytes(identity.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .0;
        let exists = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut context.cancelled,
            };
            let resolved = point::resolve(
                AgentSubtreeQualifiedRequestV1 {
                    cell: AgentSubtreeScalarRequestV1 {
                        agent,
                        field: AgentFieldV1::Identity,
                        ordinal: 0,
                    },
                    qualification,
                },
                tx,
                &mut records,
                &mut context.take_state_error,
                context.record_bytes,
                &mut std::io::sink(),
            )?;
            if resolved.agent_exists {
                // Authenticate every required base scalar using the original point kernel.
                // Optional groups and their collections retain their own normal readers.
                for &field in AgentFieldV1::ALL {
                    if schema::group(field) == schema::Group::Base
                        && field != AgentFieldV1::Identity
                    {
                        point::resolve(
                            AgentSubtreeQualifiedRequestV1 {
                                cell: AgentSubtreeScalarRequestV1 {
                                    agent,
                                    field,
                                    ordinal: 0,
                                },
                                qualification,
                            },
                            tx,
                            &mut records,
                            &mut context.take_state_error,
                            context.record_bytes,
                            &mut std::io::sink(),
                        )?;
                    }
                }
            }
            resolved.agent_exists
        };
        if !exists {
            if parent_read
                .lookup_raw(
                    parent.state_root(AgentFieldV1::Identity.namespace()),
                    identity.as_bytes(),
                )?
                .is_some()
            {
                return Err(ValueTreeError::Invalid("current agent deletion unsupported").into());
            }
            for namespace in [
                StateNamespaceV1::Business,
                StateNamespaceV1::Allocator,
                StateNamespaceV1::Scheduler,
                StateNamespaceV1::Authorization,
                StateNamespaceV1::Effects,
                StateNamespaceV1::Artifacts,
            ] {
                tx.visit_pending_writes(
                    namespace,
                    remaining_pending_entries,
                    || {
                        if (context.cancelled)() || (parent_read.cancelled)() {
                            Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                        } else {
                            Ok(())
                        }
                    },
                    |key, _, after| {
                        let owned = match parse_agent_subtree_key_v1(namespace, key)? {
                            Some(AgentSubtreeKeyV1::Field { owner: actual, .. }) => actual == owner,
                            Some(
                                AgentSubtreeKeyV1::Body { agent: actual }
                                | AgentSubtreeKeyV1::Mailbox { agent: actual, .. },
                            ) => actual == agent,
                            None => false,
                        };
                        if owned && after.is_some() {
                            return Err(
                                ValueTreeError::Invalid("partial current agent creation").into()
                            );
                        }
                        Ok(())
                    },
                )?;
            }
        }
        parent_read.check()?;
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (context.take_state_error)() {
            return Err(error.into());
        }
        if let Some(error) = (parent_read.take_state_error)() {
            return Err(error.into());
        }
        Ok(if exists {
            CurrentAgentPresenceV1::Existing
        } else {
            CurrentAgentPresenceV1::Absent
        })
    })
}
