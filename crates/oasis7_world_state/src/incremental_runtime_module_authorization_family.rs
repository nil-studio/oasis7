//! Full saved-family bootstrap and fixed metadata resource readmission.
use super::{read::*, *};
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_key_read::{OriginalKeyInfo, OriginalKeyKind, OriginalSemanticKeyProbe},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{cell::RefCell, io::Cursor};
pub struct ModuleAuthorizationFamilyQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    grants: u64,
    rules: u64,
}
impl ModuleAuthorizationFamilyQualificationV1 {
    pub(super) fn parent_digest(&self) -> &[u8; 32] {
        &self.roots
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn grant_count(&self) -> u64 {
        self.grants
    }
    pub fn policy_length(&self) -> u64 {
        self.rules
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
}
pub(super) fn mint(
    roots: [u8; 32],
    class: MemoryClass,
    grants: u64,
    rules: u64,
    memory: &MemoryBudget,
) -> Result<ReservedValue<ModuleAuthorizationFamilyQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(ModuleAuthorizationFamilyQualificationV1 {
            roots,
            class,
            grants,
            rules,
        }))
}
pub fn readmit_module_authorization_family_ordinary_v1(
    source: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<ModuleAuthorizationFamilyQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let _scratch = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(roots)?;
    let output = mint(
        source.roots,
        MemoryClass::Ordinary,
        source.grants,
        source.rules,
        memory,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output)
}
pub(super) fn field(
    namespace: StateNamespaceV1,
    bytes: &[u8],
) -> Result<Option<(ModuleAuthorizationFieldV1, WorldFieldKeyV1)>, ValueTreeError> {
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
    if binding.owner != FieldOwner::Runtime {
        return Ok(None);
    }
    let Some(field) = ModuleAuthorizationFieldV1::ALL
        .into_iter()
        .find(|field| field.descriptor().name == binding.name)
    else {
        return Ok(None);
    };
    let expected = WorldFieldKeyV1::new(field.descriptor(), binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if namespace != StateNamespaceV1::Authorization || bytes != expected.as_bytes() {
        return Err(invalid());
    }
    Ok(Some((field, expected)))
}
pub(super) fn length_key<R: ValueNodeReader, C: FnMut() -> bool>(
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        ModuleAuthorizationFieldV1::PolicyRules.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(genesis::FORMAT, "length"),
    )
}
pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    field: ModuleAuthorizationFieldV1,
    key: &WorldFieldKeyV1,
    id: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(ReservedValue<StateRootV1>, Option<u64>), ValueTreeError> {
    let charge = records.values.memory.reserve(8192, records.values.class)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: 165,
        record_bytes: records.record_bytes,
    };
    let header =
        read_member_record_header_v1(id, key, records.values, limits, &mut *records.cancelled)?;
    let profile = MemberReadProfileV1 {
        codec: MemberPayloadCodecV1::RecordRoot,
        semantic_bytes: header.semantic_bytes,
        limits,
    };
    let mut bytes = [0; 165];
    let mut sink = std::io::sink();
    let mut probe = OriginalSemanticKeyProbe::new(
        match field {
            ModuleAuthorizationFieldV1::Capabilities => OriginalKeyKind::Text,
            ModuleAuthorizationFieldV1::PolicyRules => OriginalKeyKind::Unsigned,
        },
        &mut sink,
    );
    let result = read_member_record_profile_v1(
        id,
        key,
        records.values,
        profile,
        &mut probe,
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut *records.cancelled,
    );
    if probe.error().is_some() {
        return Err(invalid());
    }
    result?;
    let index = match probe.finish()? {
        OriginalKeyInfo::Unsigned(index) => Some(index),
        OriginalKeyInfo::Text { .. } => None,
    };
    let binding = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let root = reference
        .validate_binding(
            &field.schema(),
            &binding.semantic_key.ok_or_else(invalid)?,
            StateNamespaceV1::Authorization,
        )
        .map_err(|_| invalid())?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok((charge.retain(root.clone()), index))
}
#[derive(Clone, Copy, Debug)]
pub enum ModuleAuthorizationRecordKeyV1<'a> {
    Grant(&'a str),
    Policy(u64),
}
impl ModuleAuthorizationRecordKeyV1<'_> {
    pub fn field(self) -> ModuleAuthorizationFieldV1 {
        match self {
            Self::Grant(_) => ModuleAuthorizationFieldV1::Capabilities,
            Self::Policy(_) => ModuleAuthorizationFieldV1::PolicyRules,
        }
    }
}
pub(super) fn record_key<R: ValueNodeReader, C: FnMut() -> bool>(
    key: ModuleAuthorizationRecordKeyV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    let field = key.field().descriptor();
    match key {
        ModuleAuthorizationRecordKeyV1::Grant(key) => {
            records.key(field, WorldFieldRecordKindV1::Entry, key)
        }
        ModuleAuthorizationRecordKeyV1::Policy(index) => {
            records.key(field, WorldFieldRecordKindV1::Entry, &index)
        }
    }
}
/// Inherit one saved child without scanning long grant values or other rules.
/// This remains local structural scope, not business invocation permission.
pub fn open_qualified_module_authorization_body_v1<M, R, E, C>(
    key: ModuleAuthorizationRecordKeyV1<'_>,
    roots: &RootSetV1,
    family: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<ModuleAuthorizationBodyQualificationV1>>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if !family.uses_budget(ctx.values.memory)
        || family.reserved_bytes() < 512
        || family.class != ctx.values.class
        || ctx.root != roots.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid());
    }
    family.validate_binding(roots)?;
    let path = record_key(key, &mut ctx.records())?;
    let id = ctx.lookup(&path.0)?;
    if let ModuleAuthorizationRecordKeyV1::Policy(index) = key
        && (index < family.rules) != id.is_some()
    {
        return Err(invalid());
    }
    let Some(id) = id else {
        ctx.check()?;
        return Ok(None);
    };
    let (root, _) = outer(key.field(), &path.0, &id, &mut ctx.records())?;
    let output = ctx.values.memory.reserve(512, ctx.values.class)?.retain(
        ModuleAuthorizationBodyQualificationV1 {
            root: (*root).clone(),
            field: key.field(),
            class: ctx.values.class,
        },
    );
    ctx.check()?;
    Ok(Some(output))
}
pub(super) fn scan_error(error: StateScanError<ValueTreeError>) -> ValueTreeError {
    match error {
        StateScanError::Visitor(e) => e,
        StateScanError::Cancelled => ValueTreeError::Cancelled,
        StateScanError::Memory(e) => ValueTreeError::Memory(e),
        StateScanError::ResourceLimited => {
            ValueTreeError::ResourceLimited("module authorization family scan")
        }
        StateScanError::Invalid(_) | StateScanError::Tree(_) => invalid(),
    }
}
/// Exact full six-root scope, retaining no list of cap refs or rule records.
/// Each saved child is checked from its own root; original DTOs are not inputs.
pub fn qualify_module_authorization_family_v1<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    body_scan: &mut StateScanBudget,
    work: &mut ModuleAuthorizationReadWorkV1,
) -> Result<ReservedValue<ModuleAuthorizationFamilyQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if ctx.root != roots.state_root(StateNamespaceV1::Authorization) {
        return Err(invalid());
    }
    let _scratch = ctx.values.memory.reserve(64 << 10, ctx.values.class)?;
    let mut metadata = [false; 2];
    let mut counts = [0u64; 2];
    let mut length = None;
    let mut maximum = None;
    {
        let cancel = RefCell::new(&mut ctx.cancelled);
        let errors = RefCell::new(&mut ctx.take_state_error);
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
                take_state_error: || (errors.borrow_mut())(),
                cancelled: || (cancel.borrow_mut())(),
            };
            let result = StateScanContext {
                source: ctx.state,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: outer_scan,
            }
            .visit_entries(
                root,
                |entry: &StateEntryV1| {
                    let Some((field, key)) = field(entry.namespace, &entry.key)? else {
                        return Ok(());
                    };
                    work.cell()?;
                    let slot = match field {
                        ModuleAuthorizationFieldV1::Capabilities => 0,
                        ModuleAuthorizationFieldV1::PolicyRules => 1,
                    };
                    if WorldFieldKeyV1::from_bytes(key.as_bytes())
                        .map_err(ValueTreeError::Invalid)?
                        .kind
                        == WorldFieldRecordKindV1::Metadata
                    {
                        if std::mem::replace(&mut metadata[slot], true) {
                            return Err(invalid());
                        }
                        local.records().presence(&entry.value, &key, 0)?;
                        return Ok(());
                    }
                    if field == ModuleAuthorizationFieldV1::PolicyRules {
                        let path = length_key(&mut local.records())?;
                        if path.0 == key {
                            let bytes = local.records().fixed::<8>(
                                &entry.value,
                                &key,
                                path.1,
                                MemberPayloadCodecV1::U64,
                            )?;
                            if length.replace(u64::from_be_bytes(bytes)).is_some() {
                                return Err(invalid());
                            }
                            return Ok(());
                        }
                    }
                    let (child, index) = outer(field, &key, &entry.value, &mut local.records())?;
                    if let Some(index) = index {
                        maximum = Some(maximum.map_or(index, |old: u64| old.max(index)));
                    }
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
                    qualify_module_authorization_body_v1(field, &mut body, body_scan, work)?;
                    counts[slot] = counts[slot].checked_add(1).ok_or_else(invalid)?;
                    Ok(())
                },
                || (cancel.borrow_mut())(),
            );
            let source_error = (errors.borrow_mut())();
            if (cancel.borrow_mut())() {
                return Err(ValueTreeError::Cancelled);
            }
            if let Some(error) = source_error {
                return Err(error);
            }
            result.map_err(scan_error)?;
        }
    }
    let length = length.ok_or_else(invalid)?;
    if metadata != [true; 2] || counts[1] != length || maximum.is_some_and(|index| index >= length)
    {
        return Err(invalid());
    }
    ctx.check()?;
    mint(
        roots.digest().map_err(|_| invalid())?,
        ctx.values.class,
        counts[0],
        length,
        ctx.values.memory,
    )
}
