//! Complete eight-field storage qualification, not a complete Runtime receipt.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1, incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{
        MerkleNodeReader, StateRootV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("runtime scheduler controls closure")
}
fn spend(cells: &mut u64) -> Result<(), ValueTreeError> {
    *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
        "runtime control closure cells",
    ))?;
    Ok(())
}
#[derive(Debug)]
pub struct RuntimeSchedulerControlsQualificationV1 {
    roots: [u8; 32],
    routing: StateRootV1,
}
impl RuntimeSchedulerControlsQualificationV1 {
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if self.roots == *roots {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    pub fn routing_root(&self) -> &StateRootV1 {
        &self.routing
    }
}
#[derive(Debug)]
pub struct RuntimeRoutingQualificationV1 {
    root: StateRootV1,
}
impl RuntimeRoutingQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub(super) fn validate(&self, root: &StateRootV1) -> Result<(), ValueTreeError> {
        if self.root == *root {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
pub(super) fn mint_routing(
    root: StateRootV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<RuntimeRoutingQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(RuntimeRoutingQualificationV1 { root }))
}
pub(super) fn mint(
    roots: [u8; 32],
    routing: StateRootV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<RuntimeSchedulerControlsQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(1024, class)?
        .retain(RuntimeSchedulerControlsQualificationV1 { roots, routing }))
}
pub(super) fn routing_body<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != StateNamespaceV1::Scheduler || ctx.root.leaf_count != 8 {
        return Err(invalid());
    }
    let _admission = ctx.values.memory.reserve(16 << 10, ctx.values.class)?;
    let meta = WorldFieldKeyV1::new(descriptor(ROUTING), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let mut keys = Vec::with_capacity(7);
    keys.push(read::routing_key(None, &mut ctx.records())?);
    for c in RuntimeRoutingColumnV1::ALL {
        keys.push(read::routing_key(Some(c), &mut ctx.records())?)
    }
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut seen = 0u8;
    let result = StateScanContext {
        source: ctx.state,
        memory: ctx.values.memory,
        class: ctx.values.class,
        io: scan,
    }
    .visit_entries(
        ctx.root,
        |entry| -> Result<(), ValueTreeError> {
            spend(cells)?;
            if entry.namespace != StateNamespaceV1::Scheduler {
                return Err(invalid());
            }
            let index = if entry.key == meta.as_bytes() {
                0
            } else {
                1 + keys
                    .iter()
                    .position(|(k, _)| k.as_bytes() == entry.key)
                    .ok_or_else(invalid)?
            };
            let bit = 1u8 << index;
            if seen & bit != 0 {
                return Err(invalid());
            }
            seen |= bit;
            ctx.values
                .verify_closure(&entry.value, ctx.record_bytes, &mut || {
                    cancel.borrow_mut()()
                })?;
            let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut || cancel.borrow_mut()(),
            };
            if index == 0 {
                records.presence(&entry.value, &meta, 0)?;
            } else {
                read::routing_value(
                    if index == 1 {
                        None
                    } else {
                        Some(RuntimeRoutingColumnV1::ALL[index - 2])
                    },
                    &entry.value,
                    &keys[index - 1],
                    &mut records,
                )?;
            }
            Ok(())
        },
        || cancel.borrow_mut()(),
    );
    let original = (ctx.take_state_error)();
    if cancel.borrow_mut()() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(e) = original {
        return Err(e);
    }
    match result {
        Ok(8) if seen == u8::MAX => Ok(()),
        Err(StateScanError::Visitor(e)) => Err(e),
        Err(StateScanError::Memory(e)) => Err(e.into()),
        Err(StateScanError::Cancelled) => Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            Err(ValueTreeError::ResourceLimited("runtime routing scan"))
        }
        _ => Err(invalid()),
    }
}
pub fn qualify_runtime_scheduler_controls_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max_cursor_bytes: u64,
) -> Result<ReservedValue<RuntimeSchedulerControlsQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(1024, ctx.values.class)?;
    let mut routing = None;
    let cancel = RefCell::new(&mut ctx.cancelled);
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: || cancel.borrow_mut()(),
        };
        if root.namespace == StateNamespaceV1::Scheduler {
            for field in RuntimeSchedulerScalarV1::ALL {
                spend(cells)?;
                read::scalar_at(
                    field,
                    &mut local,
                    if field == RuntimeSchedulerScalarV1::Cursor {
                        max_cursor_bytes
                    } else {
                        8
                    },
                    &mut std::io::sink(),
                )?;
            }
            let meta =
                WorldFieldKeyV1::new(descriptor(ROUTING), WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            let value = local.lookup(&meta)?.ok_or_else(invalid)?;
            local.records().presence(&value, &meta, 0)?;
            let key = read::outer_key(ROUTING, &mut local.records())?;
            let value = local.lookup(&key.0)?.ok_or_else(invalid)?;
            routing = Some(read::routing_root(&value, &key, &mut local.records())?);
        }
        let result = StateScanContext {
            source: ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *outer_scan,
        }
        .visit_entries(
            root,
            |entry| -> Result<(), ValueTreeError> {
                spend(cells)?;
                if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1") {
                    return Ok(());
                }
                let binding =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if binding.owner != FieldOwner::Runtime {
                    return Ok(());
                }
                let known = binding.name == ROUTING
                    || RuntimeSchedulerScalarV1::ALL
                        .iter()
                        .any(|f| f.name() == binding.name);
                if !known {
                    return Ok(());
                }
                if entry.namespace != StateNamespaceV1::Scheduler {
                    return Err(invalid());
                }
                let field = descriptor(if binding.name == ROUTING {
                    ROUTING
                } else {
                    RuntimeSchedulerScalarV1::ALL
                        .into_iter()
                        .find(|f| f.name() == binding.name)
                        .ok_or_else(invalid)?
                        .name()
                });
                let expected = if binding.kind == WorldFieldRecordKindV1::Metadata {
                    WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
                        .map_err(ValueTreeError::Invalid)?
                } else if binding.kind == WorldFieldRecordKindV1::Entry {
                    local.key(field, WorldFieldRecordKindV1::Entry, &())?.0
                } else {
                    return Err(invalid());
                };
                if expected.as_bytes() != entry.key {
                    return Err(invalid());
                }
                Ok(())
            },
            || cancel.borrow_mut()(),
        );
        let error = (local.take_state_error)();
        if cancel.borrow_mut()() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(e) = error {
            return Err(e);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited(
                    "runtime controls outer scan",
                ));
            }
            _ => return Err(invalid()),
        }
    }
    let child = routing.ok_or_else(invalid)?;
    let mut body = WorldFieldReadContextV1 {
        root: &child,
        state: ctx.state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: || cancel.borrow_mut()(),
    };
    routing_body(&mut body, inner_scan, cells)?;
    if cancel.borrow_mut()() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(RuntimeSchedulerControlsQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        routing: child,
    }))
}
