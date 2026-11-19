//! Provisional page-backed rollback record production. No authority or head install.
use crate::{
    incremental_field_member::*,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_record_tree::{self, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_rollback_records::*,
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
pub(crate) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "rollback tree read",
    pending: "rollback pending",
    tree: "rollback tree",
    digest: "rollback digest",
    read_allowance: "rollback read allowance",
    admission: "rollback admission",
    bound: "rollback bound",
    producer_pool: "rollback producer pool",
    page: "rollback page",
    write_allowance: "rollback write allowance",
};
pub struct RollbackWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl RollbackWorkV1 {
    fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("rollback cells"))?;
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub enum RollbackRecordRefV1<'a> {
    Authority(&'a crate::snapshot::RollbackAuthorityRecord),
    Outcome(&'a crate::snapshot::RollbackNonceOutcome),
}
impl RollbackRecordRefV1<'_> {
    pub fn schema(self) -> [u8; 32] {
        *blake3::hash(match self {
            Self::Authority(_) => b"runtime.rollback.authority.v1",
            Self::Outcome(_) => b"runtime.rollback.outcome.v1",
        })
        .as_bytes()
    }
}
pub(crate) fn descriptor() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "rollback_record_child_v1",
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
macro_rules! with_key {
    ($part:expr,$key:ident=>$body:expr) => {
        match $part {
            RollbackPathV1::Column(name) => {
                let $key = &("column", name);
                $body
            }
            RollbackPathV1::Ordinal(ordinal) => {
                let $key = &("ordinal", ordinal);
                $body
            }
        }
    };
}
struct Builder<'a, 'b, W, C> {
    ctx: &'a mut MemberRecordWriteContext<'b, W, C>,
    work: &'a mut RollbackWorkV1,
    roots: [Option<StateRootV1>; 4],
    prefix: [RollbackPathV1; 3],
    depth: usize,
    schema: [u8; 32],
}
impl<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool> Builder<'_, '_, W, C> {
    fn policy(&self) -> RecordTreeWritePolicyV1 {
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: self.work.max_pending_node_bytes,
            labels: LABELS,
        }
    }
    fn initialize(&mut self, depth: usize) -> Result<(), MemberRecordError> {
        self.work.cell()?;
        let row = write_member_record_v1(
            descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::Unit,
            self.ctx,
        )?;
        let root = StateRootV1::empty(StateNamespaceV1::Authorization)
            .map_err(|_| MemberRecordError::Invalid("rollback namespace"))?;
        let metadata = crate::incremental_field_presence::write_field_metadata_v1(
            descriptor(),
            &mut crate::incremental_field_presence::PresenceRecordWriteContext {
                sink: &mut *self.ctx.sink,
                memory: self.ctx.memory,
                io: &mut *self.ctx.io,
                key_work: &mut *self.ctx.key_work,
                limits: crate::incremental_field_presence::PresenceRecordLimitsV1 {
                    semantic_bytes: self.ctx.limits.semantic_bytes,
                    record_bytes: self.ctx.limits.record_bytes,
                },
                cancelled: &mut self.ctx.cancelled,
            },
        )?;
        self.work.cell()?;
        let policy = self.policy();
        let root = incremental_record_tree::append(
            root,
            &metadata.field_key,
            metadata.value_root,
            self.ctx,
            policy,
        )?;
        let policy = self.policy();
        self.roots[depth] = Some(incremental_record_tree::append(
            root,
            &row.field_key,
            row.value_root,
            self.ctx,
            policy,
        )?);
        Ok(())
    }
    fn insert(
        &mut self,
        depth: usize,
        part: RollbackPathV1,
        payload: MemberPayloadRefV1<'_>,
    ) -> Result<(), MemberRecordError> {
        self.work.cell()?;
        let row = with_key!(part,key=>write_member_record_v1(descriptor(),WorldFieldRecordKindV1::Entry,key,payload,self.ctx))?;
        let root = self.roots[depth]
            .take()
            .ok_or(MemberRecordError::Invalid("rollback tree stack"))?;
        let policy = self.policy();
        self.roots[depth] = Some(incremental_record_tree::append(
            root,
            &row.field_key,
            row.value_root,
            self.ctx,
            policy,
        )?);
        Ok(())
    }
    fn close(&mut self) -> Result<(), MemberRecordError> {
        let depth = self.depth;
        let root = self.roots[depth]
            .take()
            .ok_or(MemberRecordError::Invalid("rollback missing child"))?;
        let part = self.prefix[depth - 1];
        let key=with_key!(part,key=>crate::incremental_semantic_key::hash_semantic_key_v1(key,self.ctx.memory,MemoryClass::Ordinary,self.ctx.key_work,&mut self.ctx.cancelled)).map_err(MemberRecordError::Key)?;
        let reference = RecordRootRefV1::new(self.schema, key.digest, root)
            .map_err(|_| MemberRecordError::Invalid("rollback child root"))?;
        self.depth -= 1;
        self.insert(self.depth, part, MemberPayloadRefV1::RecordRoot(&reference))
    }
    fn visit(
        &mut self,
        path: &[RollbackPathV1],
        value: RollbackValueRefV1<'_>,
    ) -> Result<(), MemberRecordError> {
        if (self.ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if path.is_empty() || path.len() > 4 {
            return Err(MemberRecordError::Invalid("rollback path bound"));
        }
        let scope = &path[..path.len() - 1];
        let common = self.prefix[..self.depth]
            .iter()
            .zip(scope)
            .take_while(|(a, b)| a == b)
            .count();
        while self.depth > common {
            self.close()?;
        }
        while self.depth < scope.len() {
            self.prefix[self.depth] = scope[self.depth];
            self.depth += 1;
            self.initialize(self.depth)?;
        }
        let payload = match value {
            RollbackValueRefV1::Text(v) => MemberPayloadRefV1::Text(v),
            RollbackValueRefV1::U64(v) => MemberPayloadRefV1::U64(v),
            RollbackValueRefV1::Bool(v) => MemberPayloadRefV1::Bool(v),
        };
        self.insert(self.depth, path[path.len() - 1], payload)
    }
}
/// Produce the full typed child tree from a borrowed original record. The caller
/// binds the returned root to the original outer key and retains family metadata.
pub fn prepare_rollback_record_v1<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    source: RollbackRecordRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RollbackWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let _charge = ctx
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut builder = Builder {
        ctx,
        work,
        roots: std::array::from_fn(|_| None),
        prefix: [RollbackPathV1::Column(""); 3],
        depth: 0,
        schema: source.schema(),
    };
    builder.initialize(0)?;
    let result = match source {
        RollbackRecordRefV1::Authority(v) => {
            visit_rollback_authority_v1(v, |path, value| builder.visit(path, value))
        }
        RollbackRecordRefV1::Outcome(v) => {
            visit_rollback_outcome_v1(v, |path, value| builder.visit(path, value))
        }
    };
    match result {
        Ok(()) => {}
        Err(RollbackVisitErrorV1::Sink(e)) => return Err(e),
        Err(RollbackVisitErrorV1::IntegerOverflow) => {
            return Err(MemberRecordError::Invalid("rollback integer overflow"));
        }
    }
    while builder.depth > 0 {
        builder.close()?;
    }
    if (builder.ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    builder.roots[0]
        .take()
        .ok_or(MemberRecordError::Invalid("rollback root"))
}

#[cfg(test)]
#[path = "incremental_rollback_physical_tests.rs"]
mod tests;

pub(crate) fn empty_scope<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    schema: [u8; 32],
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RollbackWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut builder = Builder {
        ctx,
        work,
        roots: std::array::from_fn(|_| None),
        prefix: [RollbackPathV1::Column(""); 3],
        depth: 0,
        schema,
    };
    builder.initialize(0)?;
    builder.roots[0]
        .take()
        .ok_or(MemberRecordError::Invalid("rollback empty scope"))
}
