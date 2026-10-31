//! Root-only bootstrap of both Supply owner families.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{StateEntryV1, StateScanContext, StateScanError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::RefCell;
#[path = "incremental_world_rule_supply_outer_key.rs"]
pub(super) mod outer_key;
#[derive(Debug)]
pub struct SupplyFamilyQualificationV1 {
    roots: [u8; 32],
    records: [u64; 2],
    present: bool,
}
impl SupplyFamilyQualificationV1 {
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if self.roots != *roots {
            return Err(ValueTreeError::Invalid("supply family root binding"));
        }
        Ok(())
    }
    pub fn record_counts(&self) -> [u64; 2] {
        self.records
    }
    pub fn rules_present(&self) -> bool {
        self.present
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("supply family closure")
}
fn outer<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
) -> Result<Option<usize>, ValueTreeError> {
    if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if binding.name != "world_rules" {
        return Ok(None);
    }
    let key = WorldFieldKeyV1::new(
        crate::incremental_world_rule_parent::descriptor()?,
        binding.kind,
        binding.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key || entry.namespace != StateNamespaceV1::Authorization {
        return Err(invalid());
    }
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(None);
    }
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Err(invalid());
    }
    let info = crate::incremental_field_member::read_member_record_header_v1(
        &entry.value,
        &key,
        &mut context.values,
        MemberRecordLimitsV1 {
            semantic_bytes: context.semantic_bytes,
            payload_bytes: context.record_bytes,
            record_bytes: context.record_bytes,
        },
        &mut context.cancelled,
    )?;
    let root = context.values.load_root(&entry.value)?;
    let mut sink = std::io::sink();
    let mut probe = outer_key::OuterProbe::new(&mut sink);
    let result = context.values.read_range(
        &root,
        (FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()) as u64,
        info.semantic_bytes,
        &mut probe,
        &mut context.cancelled,
    );
    if let Some(reason) = probe.bad {
        return Err(ValueTreeError::Invalid(reason));
    }
    result?;
    let Some(owner) = probe.finish()? else {
        return Ok(None);
    };
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let mut bytes = [0; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &entry.value,
        &key,
        &mut context.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: info.semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: context.semantic_bytes,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: context.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut context.cancelled,
    )?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let owner_digest = binding.semantic_key.ok_or_else(invalid)?;
    let child = reference
        .validate_binding(
            &owner.schema(),
            &owner_digest,
            StateNamespaceV1::Authorization,
        )
        .map_err(|_| invalid())?;
    let mut inner = WorldFieldReadContextV1 {
        root: child,
        state: context.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: context.values.source,
            memory: context.values.memory,
            class: context.values.class,
            io: &mut *context.values.io,
        },
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        take_state_error: &mut context.take_state_error,
        cancelled: &mut context.cancelled,
    };
    qualify_supply_body_v1(
        owner,
        owner_digest,
        &mut inner,
        inner_scan,
        cells,
        max_payload,
    )?;
    Ok(Some(match owner {
        SupplyRootKeyV1::Authorization(_) => 0,
        SupplyRootKeyV1::Batch(_) => 1,
    }))
}
/// Full bootstrap/recovery only. All six authenticated roots are checked for
/// misplaced Supply field keys; unchanged normal updates use a receiver.
pub fn qualify_supply_family_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
) -> Result<ReservedValue<SupplyFamilyQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    context.check()?;
    let auth = roots
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Authorization)
        .ok_or_else(invalid)?;
    if context.root != auth {
        return Err(invalid());
    }
    let (_, present) = crate::incremental_world_rule_parent::read_parent_fixed(context)?;
    let memory = context.values.memory;
    let class = context.values.class;
    let state = context.state;
    let retained = memory.reserve(std::mem::size_of::<SupplyFamilyQualificationV1>(), class)?;
    let cancelled = RefCell::new(&mut context.cancelled);
    let mut records = [0u64; 2];
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
            state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: context.values.source,
                memory,
                class,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            take_state_error: &mut context.take_state_error,
            cancelled: || (cancelled.borrow_mut())(),
        };
        let result = StateScanContext {
            source: state,
            memory,
            class,
            io: &mut *outer_scan,
        }
        .visit_entries(
            root,
            |entry| {
                *cells = cells
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("supply family cells"))?;
                if let Some(index) = outer(entry, &mut local, inner_scan, cells, max_payload)? {
                    records[index] = records[index].checked_add(1).ok_or_else(invalid)?;
                }
                Ok(())
            },
            || (cancelled.borrow_mut())(),
        );
        let original = (local.take_state_error)();
        if (cancelled.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = original {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(error)) => return Err(error),
            Err(StateScanError::Memory(error)) => return Err(ValueTreeError::Memory(error)),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("supply family scan"));
            }
            Err(StateScanError::Invalid(_) | StateScanError::Tree(_)) => return Err(invalid()),
        }
    }
    if !present && records != [0; 2] {
        return Err(invalid());
    }
    let digest = roots.digest().map_err(|_| invalid())?;
    if (cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(SupplyFamilyQualificationV1 {
        roots: digest,
        records,
        present,
    }))
}

pub(super) fn advanced(
    roots: [u8; 32],
    records: [u64; 2],
    present: bool,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<SupplyFamilyQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(std::mem::size_of::<SupplyFamilyQualificationV1>(), class)?
        .retain(SupplyFamilyQualificationV1 {
            roots,
            records,
            present,
        }))
}
