//! Complete agent_authority-owner closure over authenticated top-level roots.
//! This token covers these three families only, never the whole World live set.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::RefCell;
#[path="incremental_agent_authority_intent_epoch_tree.rs"]
pub(super) mod intent_epochs;
use std::sync::Arc;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("agent_authority family closure")
}
#[derive(Debug)]
pub struct AgentAuthorityFamilyQualificationV1 {
    roots: [u8; 32],
    counts: [u64; 3],
    class: oasis7_proto::memory_budget::MemoryClass,
    pub(super) intent_epochs:intent_epochs::Link,
    pub(super) intent_ordinary_cover:Option<Arc<ReservedValue<()>>>,
}
impl AgentAuthorityFamilyQualificationV1 {
    pub fn allocation_class(&self) -> oasis7_proto::memory_budget::MemoryClass {
        self.class
    }

    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if &self.roots == roots && (self.class!=oasis7_proto::memory_budget::MemoryClass::Ordinary || intent_epochs::required_ordinary_cover(&self.intent_epochs)==0 || self.intent_ordinary_cover.as_ref().is_some_and(|fee|fee.reserved_bytes()>=intent_epochs::required_ordinary_cover(&self.intent_epochs))) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    /// Aggregate DATA of every original IntentLedger outer binding, including
    /// records whose outer identity differs from embedded intent.agent_id.
    pub fn max_intent_reorg_epoch_v2(&self)->u64{intent_epochs::max_epoch(&self.intent_epochs)}
    pub(super) fn check_intent_epoch_pool(&self,memory:&oasis7_proto::memory_budget::MemoryBudget)->Result<(),ValueTreeError>{
        if self.intent_epochs.as_ref().is_some_and(|node|!node.uses_budget(memory))||self.intent_ordinary_cover.as_ref().is_some_and(|fee|!fee.uses_budget(memory)){return Err(invalid())}Ok(())
    }
    pub fn record_counts(&self) -> [u64; 3] {
        self.counts
    }
}
fn index(kind: AgentAuthorityRecordKindV1) -> usize {
    match kind {
        AgentAuthorityRecordKindV1::IntentLedger => 0,
        AgentAuthorityRecordKindV1::AgentLocation => 1,
        AgentAuthorityRecordKindV1::LocationAnchor => 2,
    }
}
fn marker<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: AgentAuthorityRecordKindV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        kind.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &(RECORD_FORMAT_KEY, ()),
    )
}
pub(super) fn outer<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<(AgentAuthorityRecordKindV1, [u8; 32], StateRootV1)>, ValueTreeError> {
    outer_parts(entry.namespace,&entry.key,entry.value,ctx)
}

pub(super) fn outer_parts<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(namespace:oasis7_formal_protocol::incremental_state::StateNamespaceV1,key_bytes:&[u8],value:[u8;32],ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>)->Result<Option<(AgentAuthorityRecordKindV1,[u8;32],StateRootV1)>,ValueTreeError>{
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let value=outer_parts_from_records_v1(namespace,key_bytes,value,&mut records)?;if value.is_some(){ctx.check()?;}Ok(value)
}
pub(super) fn outer_parts_from_records_v1<R:ValueNodeReader,C:FnMut()->bool>(namespace:oasis7_formal_protocol::incremental_state::StateNamespaceV1,key_bytes:&[u8],value:[u8;32],records:&mut InventoryValueReader<'_, '_,R,C>)->Result<Option<(AgentAuthorityRecordKindV1,[u8;32],StateRootV1)>,ValueTreeError>{
    if !key_bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(&key_bytes).map_err(ValueTreeError::Invalid)?;
    let Some(kind) = AgentAuthorityRecordKindV1::ALL
        .into_iter()
        .find(|k| k.field_name() == binding.name)
    else {
        return Ok(None);
    };
    let key = WorldFieldKeyV1::new(kind.descriptor()?, binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != key_bytes || namespace != kind.namespace() {
        return Err(invalid());
    }
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        records.presence(&value, &key, 0)?;
        return Ok(None);
    }
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Err(invalid());
    }
    let format = marker(kind, records)?;
    if key == format.0 {
        read::value(
            AgentAuthorityCellV1::Record,
            &value,
            &format,
            records,
            0,
            &mut std::io::sink(),
        )?;
        return Ok(None);
    }
    let info = read_member_record_header_v1(
        &value,
        &key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES
                as u64,
            record_bytes: records.record_bytes,
        },
        &mut *records.cancelled,
    )?;
    let child = read::outer(
        kind,
        &(key, info.semantic_bytes),
        &value,
        records,
    )?;
    let owner = binding.semantic_key.ok_or_else(invalid)?;
    Ok(Some((kind, owner, child)))
}
/// Raw bootstrap/recovery checks every original key, closed child and required
/// metadata/format marker. All scans share caller-owned budgets continuously.
pub fn qualify_agent_authority_family_v1<
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
    max_payload: u64,
) -> Result<ReservedValue<AgentAuthorityFamilyQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    let _phase = ctx.values.memory.reserve(128 << 10, ctx.values.class)?;
    if !roots.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut counts = [0u64; 3];
    let mut intent_epochs=None;
    for state_root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root: state_root,
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
            cancelled: || (cancel.borrow_mut())(),
        };
        for kind in AgentAuthorityRecordKindV1::ALL
            .into_iter()
            .filter(|k| k.namespace() == state_root.namespace)
        {
            let key =
                WorldFieldKeyV1::new(kind.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            let value = local.lookup(&key)?.ok_or_else(invalid)?;
            let format = marker(kind, &mut local.records())?;
            let format_value = local.lookup(&format.0)?.ok_or_else(invalid)?;
            let mut records = local.records();
            records.presence(&value, &key, 0)?;
            read::value(
                AgentAuthorityCellV1::Record,
                &format_value,
                &format,
                &mut records,
                0,
                &mut std::io::sink(),
            )?;
        }
        let result = StateScanContext {
            source: ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *outer_scan,
        }
        .visit_entries(
            state_root,
            |entry| {
                *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
                    "agent_authority family cells",
                ))?;
                if let Some((kind, owner, child)) = outer(entry, &mut local)? {
                    let mut body = WorldFieldReadContextV1 {
                        root: &child,
                        state: local.state,
                        values: ValueReadContext {
                            source: local.values.source,
                            memory: local.values.memory,
                            class: local.values.class,
                            io: &mut *local.values.io,
                        },
                        key_work: &mut *local.key_work,
                        semantic_bytes: local.semantic_bytes,
                        record_bytes: local.record_bytes,
                        take_state_error: &mut local.take_state_error,
                        cancelled: &mut local.cancelled,
                    };
                    qualify_agent_authority_body_v1(
                        kind,
                        owner,
                        &mut body,
                        inner_scan,
                        cells,
                        max_payload,
                    )?;
                    if kind==AgentAuthorityRecordKindV1::IntentLedger{
                        let epoch=read_intent_epoch_from_closed_body(&mut body,cells)?;
                        let mut meter=intent_epochs::Meter{memory:body.values.memory,class:body.values.class,cells,bytes:&mut body.key_work.bytes_remaining,stop:&mut body.cancelled};
                        if intent_epochs::get(&intent_epochs,&entry.key,&mut meter)?.is_some(){return Err(invalid())}
                        intent_epochs=intent_epochs::put(&intent_epochs,&entry.key,epoch,&mut meter)?;
                    }
                    let slot = index(kind);
                    counts[slot] = counts[slot].checked_add(1).ok_or_else(invalid)?;
                }
                Ok(())
            },
            || (cancel.borrow_mut())(),
        );
        let error = (local.take_state_error)();
        if (cancel.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = error {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited(
                    "agent_authority family scan",
                ));
            }
            Err(_) => return Err(invalid()),
        }
    }
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(AgentAuthorityFamilyQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        counts,
        class: ctx.values.class,
        intent_epochs,
        intent_ordinary_cover:None,
    }))
}

pub(super) fn mint(
    roots: [u8; 32],
    counts: [u64; 3],
    intent_epochs:intent_epochs::Link,
    intent_ordinary_cover:Option<Arc<ReservedValue<()>>>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
) -> Result<ReservedValue<AgentAuthorityFamilyQualificationV1>, ValueTreeError> {
    if counts[0]!=intent_epochs::record_count(&intent_epochs){return Err(invalid())}
    Ok(memory
        .reserve(512, class)?
        .retain(AgentAuthorityFamilyQualificationV1 {
            roots,
            counts,
            class,
            intent_epochs,
            intent_ordinary_cover,
        }))
}

/// Open one record under an already-qualified exact family RootSet.
pub fn open_qualified_agent_authority_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: AgentAuthorityRecordKeyV1<'_>,
    roots: &RootSetV1,
    family: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<AgentAuthorityRecordQualificationV1>>, ValueTreeError> {
    ctx.check()?;
    if !family.uses_budget(ctx.values.memory)
        || crate::incremental_record_qualification_phase::check_class_v1(
            family,
            ctx.values.memory,
            ctx.values.class,
        )
        .is_err()
    {
        return Err(invalid());
    }
    let _phase = crate::incremental_record_qualification_phase::bind_phase_v1(
        family,
        crate::incremental_record_qualification_phase::FamilyBindingV1::Roots(roots),
        ctx.values.memory,
        ctx.values.class,
        &mut ctx.cancelled,
    )?;
    let top = roots
        .state
        .iter()
        .find(|r| r.namespace == key.kind.namespace())
        .ok_or_else(invalid)?;
    if top != ctx.root {
        return Err(invalid());
    }
    let Some(child) = inspect_agent_authority_record_root_v1(key, ctx)? else {
        return Ok(None);
    };
    let outer = read::outer_key(key, &mut ctx.records())?;
    let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let proof = closure::mint(
        key.kind,
        owner,
        (*child).clone(),
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(Some(proof))
}

impl crate::incremental_record_qualification_phase::PhaseQualificationV1
    for AgentAuthorityFamilyQualificationV1
{
    const RETAINED_BYTES: usize = 512;
    type Binding<'a> = crate::incremental_record_qualification_phase::FamilyBindingV1<'a>;
    fn phase_class(&self) -> oasis7_proto::memory_budget::MemoryClass {
        self.class
    }
    fn validate_phase_binding(&self, binding: Self::Binding<'_>) -> Result<(), ValueTreeError> {
        self.validate_binding(&binding.digest()?)
    }
    fn readmission_metadata_bytes(&self)->Result<usize,ValueTreeError>{intent_epochs::resident_heap(&self.intent_epochs).checked_add(64).ok_or_else(invalid)}
    fn from_readmission(permit:crate::incremental_record_qualification_phase::ReadmissionPermitV1<'_,Self>)->Self{
        let source=permit.source();let roots=source.roots;let counts=source.counts;let intent_epochs=intent_epochs::share_tree(&source.intent_epochs);let class=permit.class();
        let fee=permit.into_metadata_fee();
        Self{roots,counts,class,intent_epochs,intent_ordinary_cover:Some(Arc::new(fee.retain(())))}
    }
}
pub fn readmit_agent_authority_family_ordinary_v1(
    source: &oasis7_proto::memory_budget::ReservedValue<AgentAuthorityFamilyQualificationV1>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    cancelled: impl FnMut() -> bool,
) -> Result<
    oasis7_proto::memory_budget::ReservedValue<AgentAuthorityFamilyQualificationV1>,
    ValueTreeError,
> {
    crate::incremental_record_qualification_phase::readmit_ordinary_v1(
        source,
        crate::incremental_record_qualification_phase::FamilyBindingV1::Roots(roots),
        memory,
        cancelled,
    )
}

/// Called only after the complete original body has closed, or after its exact
/// parent-qualified changed-column receiver. Uses the original optional u64
/// column codec; Unit is the original None, never a missing-column fallback.
pub(super) fn read_intent_epoch_from_closed_body<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(body:&mut WorldFieldReadContextV1<'_,M,R,E,C>,cells:&mut u64)->Result<Option<u64>,ValueTreeError>{
 *cells=cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited("intent epoch column cells"))?;
 let cell=AgentAuthorityCellV1::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::ReorgEpoch));
 let key=read::cell_key(AgentAuthorityRecordKindV1::IntentLedger,cell,&mut body.records())?;
 let value=body.lookup(&key.0)?.ok_or_else(invalid)?;
 match read::value(cell,&value,&key,&mut body.records(),0,&mut std::io::sink())?{AgentAuthorityValueV1::Unsigned(value)=>Ok(Some(value)),AgentAuthorityValueV1::Unit=>Ok(None),_=>Err(invalid())}
}

pub(super) fn fold_verified_intent_epoch_change<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(source:&intent_epochs::Link,key:&[u8],before:Option<&StateRootV1>,after:Option<&StateRootV1>,ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,cells:&mut u64)->Result<intent_epochs::Link,ValueTreeError>{
 fn epoch<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(root:&StateRootV1,ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,cells:&mut u64)->Result<Option<u64>,ValueTreeError>{let mut body=WorldFieldReadContextV1{root,state:ctx.state,values:ValueReadContext{source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut *ctx.values.io},key_work:&mut *ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};read_intent_epoch_from_closed_body(&mut body,cells)}
 let before=before.map(|root|epoch(root,ctx,cells)).transpose()?;let after=after.map(|root|epoch(root,ctx,cells)).transpose()?;
 let mut meter=intent_epochs::Meter{memory:ctx.values.memory,class:ctx.values.class,cells,bytes:&mut ctx.key_work.bytes_remaining,stop:&mut ctx.cancelled};
 if intent_epochs::get(source,key,&mut meter)?!=before{return Err(invalid())}
 match after{Some(epoch)=>intent_epochs::put(source,key,epoch,&mut meter),None=>intent_epochs::remove(source,key,&mut meter)}
}

#[path="incremental_agent_authority_intent_epoch_current.rs"]
mod intent_epoch_current;
pub use intent_epoch_current::*;
