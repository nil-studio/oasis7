//! Actual current point readers for the original module-visual anchor rule.
//! No location/agent map or materialized World is built for these checks.
use oasis7_agent_api::world_entity::ModuleVisualAnchor;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError,
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    WorldError,
    incremental_agent_authority_records::*,
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1,
        AgentSubtreeStructureQualificationV1, read_agent_subtree_scalar_overlay_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    state::{LocationAnchorV1, require_active_location_anchor_point_v1},
};

#[derive(Debug)]
pub enum RootModuleVisualAnchorErrorV1 {
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Business(ReservedValue<WorldError>),
}
impl From<ValueTreeError> for RootModuleVisualAnchorErrorV1 {
    fn from(error: ValueTreeError) -> Self {
        Self::Value(error)
    }
}
impl From<WorldFieldOverlayErrorV1> for RootModuleVisualAnchorErrorV1 {
    fn from(error: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(error)
    }
}
impl From<ExecutionError> for RootModuleVisualAnchorErrorV1 {
    fn from(error: ExecutionError) -> Self {
        Self::Overlay(error.into())
    }
}

/// Reads every datum used by the old anchor kernel from the actual current
/// transaction. `now` is independently checked by the enclosing event/step
/// owner before visual contribution and canonical Event publication.
#[expect(
    clippy::too_many_arguments,
    reason = "The actual two qualified owners and their continuous IO/body work remain explicit."
)]
pub fn check_root_module_visual_anchor_v1<M, R, E, C>(
    anchor: &ModuleVisualAnchor,
    now: u64,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    authority: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    body_scan: &mut StateScanBudget,
    cells: &mut u64,
    max_owner_bytes: usize,
) -> Result<(), RootModuleVisualAnchorErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    if (ctx.cancelled)() {
        return ctx
            .transaction
            .contribute(|_| Err(ValueTreeError::Cancelled.into()));
    }
    let id = match anchor {
        ModuleVisualAnchor::Agent { agent_id } => Some(agent_id.as_str()),
        ModuleVisualAnchor::Location { location_id } => Some(location_id.as_str()),
        ModuleVisualAnchor::Absolute { .. } => None,
    };
    if id.is_some_and(|id| id.len() > max_owner_bytes) {
        return ctx.transaction.contribute(|_| {
            Err(ValueTreeError::ResourceLimited("module visual anchor identity").into())
        });
    }
    let Some(fee) = max_owner_bytes
        .checked_mul(16)
        .and_then(|n| n.checked_add(8192))
    else {
        return ctx.transaction.contribute(|_| {
            Err(ValueTreeError::ResourceLimited("module visual anchor workspace").into())
        });
    };
    let phase = match ctx.values.memory.reserve(fee, MemoryClass::Ordinary) {
        Ok(phase) => phase,
        Err(error) => {
            return ctx
                .transaction
                .contribute(|_| Err(ValueTreeError::Memory(error).into()));
        }
    };
    // Reborrow the concrete contexts rather than accepting a permission callback.
    let values = &mut ctx.values;
    let keys = &mut *ctx.key_work;
    let errors = &mut ctx.take_state_error;
    let cancelled = &mut ctx.cancelled;
    let semantic_bytes = ctx.semantic_bytes;
    let record_bytes = ctx.record_bytes;
    ctx.transaction.contribute(|tx| {
        let mut local = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: values.source,
                memory: values.memory,
                class: values.class,
                io: &mut *values.io,
            },
            key_work: keys,
            semantic_bytes,
            record_bytes,
            take_state_error: &mut *errors,
            cancelled: &mut *cancelled,
        };
        if local.values.class != MemoryClass::Ordinary
            || !local.transaction.uses_budget(local.values.memory)
        {
            return Err(ValueTreeError::Invalid("module visual anchor pool/class").into());
        }
        let result = match anchor {
            ModuleVisualAnchor::Absolute { .. } => Ok(()),
            ModuleVisualAnchor::Agent { agent_id } => {
                let id = agent_id.trim();
                if id.is_empty() {
                    Err(WorldError::Serde("invalid anchor.agent_id module visual payload: anchor.agent_id is required".to_owned()))
                } else {
                    let guard = local.transaction.guard();
                    let live = local.transaction.parent_fence();
                    let value = read_agent_subtree_scalar_overlay_v1(
                        AgentSubtreeQualifiedRequestV1 {
                            cell: AgentSubtreeScalarRequestV1 {
                                agent: id,
                                field: AgentFieldV1::Identity,
                                ordinal: 0,
                            },
                            qualification: agents,
                        },
                        &mut local,
                        max_owner_bytes as u64,
                        &mut std::io::sink(),
                        &guard,
                        &live,
                    )?;
                    if value.is_some() {
                        Ok(())
                    } else {
                        Err(WorldError::AgentNotFound {
                            agent_id: id.to_owned(),
                        })
                    }
                }
            }
            ModuleVisualAnchor::Location { location_id } => {
                let id = location_id.trim();
                if id.is_empty() {
                    Err(WorldError::Serde("invalid anchor.location_id module visual payload: anchor.location_id is required".to_owned()))
                } else {
                    let key = AgentAuthorityRecordKeyV1 {
                        kind: AgentAuthorityRecordKindV1::LocationAnchor,
                        id,
                    };
                    let proof = open_agent_authority_body_overlay_v1(
                        key,
                        authority,
                        state,
                        &mut local,
                        body_scan,
                        cells,
                        max_owner_bytes as u64,
                    )?;
                    let mut point = None;
                    if let Some(proof) = proof {
                        let mut bytes = Vec::with_capacity(max_owner_bytes);
                        let mut active = None;
                        let mut revision = None;
                        let mut effective = None;
                        for column in [
                            AgentAuthorityColumnV1::LocationId,
                            AgentAuthorityColumnV1::Active,
                            AgentAuthorityColumnV1::AuthorityRevision,
                            AgentAuthorityColumnV1::EffectiveAt,
                        ] {
                            let guard = local.transaction.guard();
                            let live = local.transaction.parent_fence();
                            let value = read_qualified_agent_authority_cell_overlay_v1(
                                AgentAuthorityCellRequestV1 {
                                    key,
                                    cell: AgentAuthorityCellV1::Column(column),
                                    qualification: &proof,
                                },
                                &mut local,
                                max_owner_bytes as u64,
                                &mut bytes,
                                &guard,
                                &live,
                            )?;
                            match (column, value) {
                                (
                                    AgentAuthorityColumnV1::LocationId,
                                    AgentAuthorityValueV1::Text { .. },
                                ) => {}
                                (
                                    AgentAuthorityColumnV1::Active,
                                    AgentAuthorityValueV1::Bool(value),
                                ) => active = Some(value),
                                (
                                    AgentAuthorityColumnV1::AuthorityRevision,
                                    AgentAuthorityValueV1::Unsigned(value),
                                ) => revision = Some(value),
                                (
                                    AgentAuthorityColumnV1::EffectiveAt,
                                    AgentAuthorityValueV1::Unsigned(value),
                                ) => effective = Some(value),
                                _ => {
                                    return Err(ValueTreeError::Invalid(
                                        "module visual location anchor cell",
                                    )
                                    .into());
                                }
                            }
                        }
                        point = Some(LocationAnchorV1 {
                            location_id: String::from_utf8(bytes).map_err(|_| {
                                ValueTreeError::Invalid("module visual anchor UTF-8")
                            })?,
                            active: active
                                .ok_or(ValueTreeError::Invalid("module visual anchor active"))?,
                            authority_revision: revision
                                .ok_or(ValueTreeError::Invalid("module visual anchor revision"))?,
                            effective_at: effective
                                .ok_or(ValueTreeError::Invalid("module visual anchor effective"))?,
                        });
                    }
                    require_active_location_anchor_point_v1(point.as_ref(), id, now)
                }
            }
        };
        if let Some(error) = (local.take_state_error)() {
            return Err(error.into());
        }
        if (local.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        result.map_err(|error| RootModuleVisualAnchorErrorV1::Business(phase.retain(error)))
    })
}
