//! Typed one-column updates; no unrelated fixed column or collection is scanned.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{StatePageWriter, StateWriteV1},
};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("diplomacy scalar column/domain")
}
#[derive(Clone, Copy)]
pub enum DiplomacyScalarValueV1<'a> {
    Text(&'a str),
    U64(u64),
    U32(u32),
    I64(i64),
    Bool(bool),
    CrisisStatus(CrisisStatus),
    ContractStatus(EconomicContractStatus),
    FulfillmentKind(EconomicContractFulfillmentKind),
    Resource(ResourceKind),
}
impl<'a> DiplomacyScalarValueV1<'a> {
    fn payload(
        self,
        column: DiplomacyScalarV1,
        kind: Kind,
    ) -> Result<MemberPayloadRefV1<'a>, ValueTreeError> {
        use DiplomacyScalarValueV1 as V;
        let payload = match (self, kind) {
            (V::Text(v), Kind::Text) => MemberPayloadRefV1::Text(v),
            (V::U64(v), Kind::U64) => MemberPayloadRefV1::U64(v),
            (V::U32(v), Kind::U32) => MemberPayloadRefV1::U64(u64::from(v)),
            (V::I64(v), Kind::I64) => MemberPayloadRefV1::I64(v),
            (V::Bool(v), Kind::Bool) => MemberPayloadRefV1::Bool(v),
            (value, Kind::Enum(labels)) => {
                let text = match (value, column) {
                    (V::CrisisStatus(v), DiplomacyScalarV1::Crisis(CrisisScalarV1::Status)) => {
                        match v {
                            CrisisStatus::Active => "active",
                            CrisisStatus::Resolved => "resolved",
                            CrisisStatus::TimedOut => "timed_out",
                        }
                    }
                    (
                        V::ContractStatus(v),
                        DiplomacyScalarV1::Contract(ContractScalarV1::Status),
                    ) => match v {
                        EconomicContractStatus::Open => "open",
                        EconomicContractStatus::Accepted => "accepted",
                        EconomicContractStatus::Settled => "settled",
                        EconomicContractStatus::Expired => "expired",
                    },
                    (
                        V::FulfillmentKind(v),
                        DiplomacyScalarV1::Contract(ContractScalarV1::FulfillmentKind),
                    ) => match v {
                        EconomicContractFulfillmentKind::AtomicExchange => "atomic_exchange",
                        EconomicContractFulfillmentKind::Service => "service",
                    },
                    (
                        V::Resource(v),
                        DiplomacyScalarV1::Contract(ContractScalarV1::SettlementKind),
                    ) => match v {
                        ResourceKind::Electricity => "electricity",
                        ResourceKind::Data => "data",
                    },
                    _ => return Err(invalid()),
                };
                if !labels.contains(&text) {
                    return Err(invalid());
                }
                MemberPayloadRefV1::Text(text)
            }
            _ => return Err(invalid()),
        };
        Ok(payload)
    }
}
pub struct DiplomacyScalarChangeV1<'a> {
    pub field: DiplomacyFieldV1,
    pub id: &'a str,
    pub column: DiplomacyScalarV1,
    pub qualification: &'a ReservedValue<DiplomacyRecordQualificationV1>,
    pub expected: Option<DiplomacyScalarValueV1<'a>>,
    pub after: Option<DiplomacyScalarValueV1<'a>>,
    pub max_current_payload_bytes: u64,
    pub max_pending_node_bytes: usize,
}
type Projection<'a> = (
    [Option<Path>; 2],
    [Option<MemberPayloadRefV1<'a>>; 2],
    [Option<MemberPayloadRefV1<'a>>; 2],
);
impl<'a> DiplomacyScalarChangeV1<'a> {
    fn project(&self) -> Result<Projection<'a>, WorldFieldOverlayErrorV1> {
        let spec = &profile::specs(self.field)[self.column.slot(self.field)?];
        if spec.dynamic {
            return Err(invalid().into());
        }
        let old = self
            .expected
            .map(|v| v.payload(self.column, spec.kind))
            .transpose()?;
        let new = self
            .after
            .map(|v| v.payload(self.column, spec.kind))
            .transpose()?;
        if let Some(flag) = spec.flag {
            Ok((
                [Some(profile::specs(self.field)[flag].path), Some(spec.path)],
                [Some(MemberPayloadRefV1::Bool(old.is_some())), old],
                [Some(MemberPayloadRefV1::Bool(new.is_some())), new],
            ))
        } else {
            if old.is_none() || new.is_none() {
                return Err(invalid().into());
            }
            Ok(([Some(spec.path), None], [old, None], [new, None]))
        }
    }
}
pub fn change_qualified_diplomacy_scalar_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: DiplomacyScalarChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<DiplomacyRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let field = change.field;
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let (paths, old, new) = change.project()?;
        if !tx.uses_budget(write.memory) {
            return Err(invalid().into());
        }
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut keys: [Option<(WorldFieldKeyV1, u64)>; 2] = std::array::from_fn(|_| None);
        let mut roots = [None; 2];
        let (outer, child) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let (outer, child) = access::current(
                field,
                change.id,
                change.qualification,
                tx,
                &mut records,
                error,
            )?;
            for (i, path) in paths.iter().enumerate() {
                let Some(path) = path else { continue };
                let key = path.overlay(field, &mut records)?;
                let root = inner_tree::lookup(child, &key.0, &mut records)?;
                match (old[i], root) {
                    (Some(payload), Some(value)) => scalars::expected(
                        payload,
                        &value,
                        &key,
                        &mut records,
                        change.max_current_payload_bytes,
                    )?,
                    (None, None) => {}
                    _ => return Err(invalid().into()),
                }
                keys[i] = Some(key);
                roots[i] = root;
            }
            (outer, child)
        };
        let mut mutations = Vec::with_capacity(2);
        for (i, path) in paths.iter().enumerate() {
            let Some(path) = path else { continue };
            let unchanged = scalars::equal(old[i], new[i]);
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if unchanged {
                continue;
            }
            let key = keys[i].as_ref().unwrap();
            let after = if let Some(payload) = new[i] {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                let row = path.write(field, payload, &mut combined)?;
                if row.field_key != key.0 {
                    return Err(invalid().into());
                }
                Some(row.value_root)
            } else {
                None
            };
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            mutations.push(StateWriteV1 {
                key: key.0.as_bytes().to_vec(),
                before: roots[i],
                after,
            });
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if mutations.is_empty() {
            return Ok(None);
        }
        mutations.sort_by(|a, b| a.key.cmp(&b.key));
        let mut after = child.clone();
        for mutation in mutations {
            let key =
                WorldFieldKeyV1::from_bytes(&mutation.key).map_err(ValueTreeError::Invalid)?;
            let key = WorldFieldKeyV1::new(field.descriptor()?, key.kind, key.semantic_key)
                .map_err(ValueTreeError::Invalid)?;
            if key.as_bytes() != mutation.key {
                return Err(invalid().into());
            }
            after = crate::incremental_record_tree::edit(
                after,
                &key,
                (mutation.before, mutation.after),
                &mut context.values,
                write,
                cancel,
                crate::incremental_record_tree::RecordTreeWritePolicyV1 {
                    max_pending_node_bytes: change.max_pending_node_bytes,
                    labels: inner_tree::LABELS,
                },
            )?;
        }
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let row = write_diplomacy_root_reference_v1(field, change.id, &after, &mut combined)?;
            if row.field_key != outer {
                return Err(invalid().into());
            }
            row
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Business,
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let digest = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let proof = access::mint(field, digest, &after, write.memory, MemoryClass::Ordinary)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
