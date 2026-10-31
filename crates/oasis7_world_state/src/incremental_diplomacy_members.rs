//! Ordered collection operations touch one ordinal and its authenticated length.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{StatePageWriter, StateWriteV1},
};
fn invalid() -> ValueTreeError {
    closure::invalid()
}
#[derive(Clone, Copy)]
pub enum DiplomacyMemberV1<'a> {
    Alliance(&'a str),
    Outcome(&'a WarParticipantOutcome),
}
pub struct DiplomacyMemberChangeV1<'a> {
    pub field: DiplomacyFieldV1,
    pub id: &'a str,
    pub qualification: &'a ReservedValue<DiplomacyRecordQualificationV1>,
    pub expected_length: u64,
    pub ordinal: u64,
    pub expected: Option<DiplomacyMemberV1<'a>>,
    pub after: Option<DiplomacyMemberV1<'a>>,
    pub max_current_payload_bytes: u64,
    pub max_pending_node_bytes: usize,
}
type Projection<'a> = (
    [Option<Path>; 6],
    [Option<MemberPayloadRefV1<'a>>; 6],
    [Option<MemberPayloadRefV1<'a>>; 6],
    u64,
);
impl<'a> DiplomacyMemberChangeV1<'a> {
    fn project(&self) -> Result<Projection<'a>, WorldFieldOverlayErrorV1> {
        let length = self.expected_length;
        let next = match (self.expected, self.after) {
            (None, Some(_)) if self.ordinal == length => {
                length.checked_add(1).ok_or_else(invalid)?
            }
            (Some(_), None) if length > 0 && self.ordinal == length - 1 => length - 1,
            (Some(_), Some(_)) if self.ordinal < length => length,
            _ => return Err(invalid().into()),
        };
        let name = match self.field {
            DiplomacyFieldV1::Alliances => "members",
            DiplomacyFieldV1::Wars => "participant_outcomes",
            _ => return Err(invalid().into()),
        };
        let mut paths = [None; 6];
        let mut old = [None; 6];
        let mut new = [None; 6];
        paths[0] = Some(Path::Length(name));
        old[0] = Some(MemberPayloadRefV1::U64(length));
        new[0] = Some(MemberPayloadRefV1::U64(next));
        match self.field {
            DiplomacyFieldV1::Alliances => paths[1] = Some(Path::Ordinal(name, self.ordinal)),
            DiplomacyFieldV1::Wars => {
                paths[1] = Some(Path::Outcome(self.ordinal));
                for (i, name) in [
                    "agent_id",
                    "electricity_delta",
                    "data_delta",
                    "reputation_delta",
                ]
                .into_iter()
                .enumerate()
                {
                    paths[i + 2] = Some(Path::OutcomeField(self.ordinal, name));
                }
            }
            _ => return Err(invalid().into()),
        }
        for (source, dest) in [(self.expected, &mut old), (self.after, &mut new)] {
            match (self.field, source) {
                (_, None) => {}
                (DiplomacyFieldV1::Alliances, Some(DiplomacyMemberV1::Alliance(v))) => {
                    dest[1] = Some(MemberPayloadRefV1::Text(v))
                }
                (DiplomacyFieldV1::Wars, Some(DiplomacyMemberV1::Outcome(v))) => {
                    let WarParticipantOutcome {
                        agent_id,
                        electricity_delta,
                        data_delta,
                        reputation_delta,
                    } = v;
                    dest[1] = Some(MemberPayloadRefV1::Unit);
                    dest[2] = Some(MemberPayloadRefV1::Text(agent_id));
                    dest[3] = Some(MemberPayloadRefV1::I64(*electricity_delta));
                    dest[4] = Some(MemberPayloadRefV1::I64(*data_delta));
                    dest[5] = Some(MemberPayloadRefV1::I64(*reputation_delta));
                }
                _ => return Err(invalid().into()),
            }
        }
        Ok((paths, old, new, next))
    }
}
pub fn change_qualified_diplomacy_member_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: DiplomacyMemberChangeV1<'_>,
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
        let (paths, old, new, _next_length) = change.project()?;
        if !tx.uses_budget(write.memory) {
            return Err(invalid().into());
        }
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut keys: [Option<(WorldFieldKeyV1, u64)>; 6] = std::array::from_fn(|_| None);
        let mut roots = [None; 6];
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
        let mut mutations = Vec::with_capacity(6);
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
