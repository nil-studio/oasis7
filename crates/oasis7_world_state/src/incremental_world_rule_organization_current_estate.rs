//! Selected Estate values read from the transaction's actual Current root.
//!
//! The family qualification and Current scope owner are inputs to this
//! adapter.  Neither an outer key nor a caller supplied Estate value can mint
//! the returned owner.
use super::*;
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberReadProfileV1, MemberRecordLimitsV1,
        read_member_record_profile_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_world_rule_record::{RecordPathV1, SupplyRecordKeyV1},
    world_rules::OrganizationEstateDepositV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{io::Cursor, mem::size_of};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("organization Current Estate source")
}

/// A selected Estate body whose outer key, family scope, actual Current root,
/// full closed-record proof, and decoded value all remain attached.
pub struct CurrentOrganizationEstateV1<'p> {
    parent: &'p ReservedValue<OrganizationFamilyQualificationV1>,
    risk: &'p ReservedValue<CurrentOrganizationRiskV1>,
    parent_roots: [u8; 32],
    outer_id: String,
    proof: ReservedValue<OrganizationRecordQualificationV1>,
    deposit: ReservedValue<OrganizationEstateDepositV1>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}

impl CurrentOrganizationEstateV1<'_> {
    pub fn outer_id(&self) -> &str {
        &self.outer_id
    }

    pub fn deposit(&self) -> &OrganizationEstateDepositV1 {
        &self.deposit
    }

    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || !self.parent.uses_budget(memory)
            || !self.risk.uses_budget(memory)
            || !self.proof.uses_budget(memory)
            || !self.deposit.uses_budget(memory)
            || self.parent.validate_binding(&self.parent_roots).is_err()
            || tx.parent_fence().parent_roots != self.parent_roots
        {
            return Err(invalid().into());
        }
        Ok(())
    }
}

fn charge_scope(
    scan: &mut StateScanBudget,
    cells: &mut u64,
    cancelled: &mut impl FnMut() -> bool,
    steps: u64,
    bytes: u64,
) -> Result<(), ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    *cells = cells
        .checked_sub(steps)
        .ok_or(ValueTreeError::ResourceLimited(
            "organization Current scope cells",
        ))?;
    scan.nodes_remaining =
        scan.nodes_remaining
            .checked_sub(steps)
            .ok_or(ValueTreeError::ResourceLimited(
                "organization Current scope steps",
            ))?;
    scan.read_bytes_remaining =
        scan.read_bytes_remaining
            .checked_sub(bytes)
            .ok_or(ValueTreeError::ResourceLimited(
                "organization Current scope bytes",
            ))?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}

fn read_field<M, R, E, C, W>(
    owner: OrganizationRootKeyV1<'_>,
    proof: &ReservedValue<OrganizationRecordQualificationV1>,
    field: OrganizationScalarFieldV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
    output: &mut W,
) -> Result<OrganizationFieldValueV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: std::io::Write,
{
    let value = read_qualified_organization_field_v1(
        OrganizationFieldRequestV1 {
            owner,
            record: OrganizationRelativeRecordV1::Estate,
            field,
            qualification: proof,
        },
        context,
        max_payload,
        output,
    )?;
    match value {
        OrganizationFieldReadV1::Value(value) => Ok(value),
        OrganizationFieldReadV1::RecordAbsent => Err(invalid()),
    }
}

fn text_length(value: OrganizationFieldValueV1) -> Result<u64, ValueTreeError> {
    match value {
        OrganizationFieldValueV1::Text { utf8_bytes } => Ok(utf8_bytes),
        _ => Err(invalid()),
    }
}

fn optional_text_length(value: OrganizationFieldValueV1) -> Result<Option<u64>, ValueTreeError> {
    match value {
        OrganizationFieldValueV1::OptionalText { utf8_bytes } => Ok(utf8_bytes),
        _ => Err(invalid()),
    }
}

fn read_text<M, R, E, C>(
    owner: OrganizationRootKeyV1<'_>,
    proof: &ReservedValue<OrganizationRecordQualificationV1>,
    field: OrganizationScalarFieldV1,
    expected: u64,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
) -> Result<String, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let capacity = usize::try_from(expected)
        .map_err(|_| ValueTreeError::ResourceLimited("organization Estate text length"))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| ValueTreeError::ResourceLimited("organization Estate text allocation"))?;
    let capacity_bound = capacity
        .checked_mul(2)
        .and_then(|value| value.checked_add(64))
        .ok_or(ValueTreeError::ResourceLimited(
            "organization Estate text capacity",
        ))?;
    if bytes.capacity() < capacity || bytes.capacity() > capacity_bound {
        return Err(ValueTreeError::ResourceLimited(
            "organization Estate text capacity",
        ));
    }
    let actual = read_field(owner, proof, field, context, max_payload, &mut bytes)?;
    if text_length(actual)? != expected || bytes.len() != capacity {
        return Err(invalid());
    }
    String::from_utf8(bytes).map_err(|_| invalid())
}

/// Resolve one selected Estate from actual Current. The Current risk owner is
/// built from the same family and transaction; this function rechecks its
/// binding before resolving the root and performs the original full Estate
/// closure check before reading individual typed columns.
#[allow(clippy::too_many_arguments)]
pub fn read_current_organization_estate_v1<
    'p,
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    outer_id: &str,
    organization_id: &str,
    parent: &'p ReservedValue<OrganizationFamilyQualificationV1>,
    risk: &'p ReservedValue<CurrentOrganizationRiskV1>,
    roots: &RootSetV1,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells_remaining: &mut u64,
    max_payload: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<CurrentOrganizationEstateV1<'p>>>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        let memory = context.values.memory;
        if context.values.class != MemoryClass::Ordinary
            || !context.transaction.uses_budget(memory)
            || !parent.uses_budget(memory)
            || !parent.permits_class(MemoryClass::Ordinary)
            || !risk.uses_budget(memory)
            || state.allocation_class() != Some(MemoryClass::Ordinary)
            || !state
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(memory))
        {
            return Err(invalid().into());
        }
        let parent_roots = roots.digest().map_err(|_| invalid())?;
        parent.validate_binding(&parent_roots)?;
        if parent_roots != context.transaction.parent_fence().parent_roots {
            return Err(invalid().into());
        }
        risk.validate(context.transaction, memory, live)?;

        let mut in_scope = false;
        risk.visit_current_estate_outers_v1(
            organization_id,
            memory,
            |steps, bytes| {
                charge_scope(scan, cells_remaining, &mut context.cancelled, steps, bytes)
            },
            |candidate| {
                if candidate == outer_id {
                    in_scope = true;
                }
                Ok(())
            },
        )?;
        if !in_scope {
            return Ok(None);
        }

        let (outer_key, semantic_bytes, outer_value) = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut context.cancelled,
            };
            let (key, semantic_bytes) = SupplyRecordKeyV1::OrganizationEstate(outer_id)
                .overlay_path(RecordPathV1::Record, &mut records)?;
            let value = lookup(
                context.transaction,
                &key,
                &mut records,
                &mut context.take_state_error,
            )?
            .ok_or_else(invalid)?;
            (key, semantic_bytes, value)
        };

        let mut root_bytes = [0u8; RECORD_ROOT_BYTES];
        let record_root = {
            let _scratch = memory
                .reserve(RECORD_ROOT_BYTES + 4096, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut values = oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: context.values.source,
                memory,
                class: context.values.class,
                io: &mut *context.values.io,
            };
            read_member_record_profile_v1(
                &outer_value,
                &outer_key,
                &mut values,
                MemberReadProfileV1 {
                    codec: MemberPayloadCodecV1::RecordRoot,
                    semantic_bytes,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: context.semantic_bytes,
                        payload_bytes: RECORD_ROOT_BYTES as u64,
                        record_bytes: context.record_bytes,
                    },
                },
                &mut std::io::sink(),
                &mut Cursor::new(root_bytes.as_mut_slice()),
                &mut context.cancelled,
            )?;
            RecordRootRefV1::from_canonical_bytes(&root_bytes).map_err(|_| invalid())?
        };
        let owner_key = WorldFieldKeyV1::from_bytes(outer_key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let owner = OrganizationRootKeyV1::Estate(outer_id);
        let child_root = record_root
            .validate_binding(&owner.schema(), &owner_key, StateNamespaceV1::Authorization)
            .map_err(|_| invalid())?;
        let _root_string_fee = memory
            .reserve(
                child_root
                    .node_root
                    .len()
                    .checked_mul(2)
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;

        let mut read_context = WorldFieldReadContextV1 {
            root: child_root,
            state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: context.values.source,
                memory,
                class: context.values.class,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            take_state_error: &mut context.take_state_error,
            cancelled: &mut context.cancelled,
        };
        let qualification = qualify_organization_body_v1(
            owner,
            owner_key,
            &mut read_context,
            scan,
            cells_remaining,
            max_payload,
        )?;

        let mut sink = std::io::sink();
        let id_len = text_length(read_field(
            owner,
            &qualification,
            OrganizationScalarFieldV1::Id,
            &mut read_context,
            max_payload,
            &mut sink,
        )?)?;
        let organization_len = text_length(read_field(
            owner,
            &qualification,
            OrganizationScalarFieldV1::OrganizationId,
            &mut read_context,
            max_payload,
            &mut sink,
        )?)?;
        let contributor_len = text_length(read_field(
            owner,
            &qualification,
            OrganizationScalarFieldV1::ContributorId,
            &mut read_context,
            max_payload,
            &mut sink,
        )?)?;
        let material_len = text_length(read_field(
            owner,
            &qualification,
            OrganizationScalarFieldV1::Material,
            &mut read_context,
            max_payload,
            &mut sink,
        )?)?;
        let amount = match read_field(
            owner,
            &qualification,
            OrganizationScalarFieldV1::Amount,
            &mut read_context,
            max_payload,
            &mut sink,
        )? {
            OrganizationFieldValueV1::I64(value) => value,
            _ => return Err(invalid().into()),
        };
        let held_in_trust = match read_field(
            owner,
            &qualification,
            OrganizationScalarFieldV1::HeldInTrust,
            &mut read_context,
            max_payload,
            &mut sink,
        )? {
            OrganizationFieldValueV1::Bool(value) => value,
            _ => return Err(invalid().into()),
        };
        let disposed = match read_field(
            owner,
            &qualification,
            OrganizationScalarFieldV1::Disposed,
            &mut read_context,
            max_payload,
            &mut sink,
        )? {
            OrganizationFieldValueV1::Bool(value) => value,
            _ => return Err(invalid().into()),
        };
        let effect_len = optional_text_length(read_field(
            owner,
            &qualification,
            OrganizationScalarFieldV1::EffectReceipt,
            &mut read_context,
            max_payload,
            &mut sink,
        )?)?;
        let total_text = [id_len, organization_len, contributor_len, material_len]
            .into_iter()
            .chain(effect_len)
            .try_fold(0u64, |total, length| {
                total.checked_add(length).ok_or_else(invalid)
            })?;
        if total_text > max_payload.saturating_mul(5) {
            return Err(invalid().into());
        }
        let text_count = 4u64 + u64::from(effect_len.is_some());
        let deposit_bytes = u64::try_from(size_of::<OrganizationEstateDepositV1>())
            .map_err(|_| invalid())?
            .checked_add(total_text.checked_mul(2).ok_or_else(invalid)?)
            .and_then(|bytes| bytes.checked_add(text_count * 64))
            .ok_or_else(invalid)?;
        let deposit_fee = memory
            .reserve(
                usize::try_from(deposit_bytes).map_err(|_| invalid())?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let outer_len = u64::try_from(outer_id.len()).map_err(|_| invalid())?;
        let owner_bytes = u64::try_from(size_of::<CurrentOrganizationEstateV1<'_>>())
            .map_err(|_| invalid())?
            .checked_add(outer_len.checked_mul(2).ok_or_else(invalid)?)
            .and_then(|bytes| bytes.checked_add(64))
            .ok_or_else(invalid)?;
        let owner_fee = memory
            .reserve(
                usize::try_from(owner_bytes).map_err(|_| invalid())?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut outer = String::new();
        outer
            .try_reserve_exact(outer_id.len())
            .map_err(|_| ValueTreeError::ResourceLimited("organization Estate outer"))?;
        if outer.capacity() > outer_id.len().saturating_mul(2).saturating_add(64) {
            return Err(
                ValueTreeError::ResourceLimited("organization Estate outer capacity").into(),
            );
        }
        outer.push_str(outer_id);

        let id = read_text(
            owner,
            &qualification,
            OrganizationScalarFieldV1::Id,
            id_len,
            &mut read_context,
            max_payload,
        )?;
        let organization = read_text(
            owner,
            &qualification,
            OrganizationScalarFieldV1::OrganizationId,
            organization_len,
            &mut read_context,
            max_payload,
        )?;
        let contributor = read_text(
            owner,
            &qualification,
            OrganizationScalarFieldV1::ContributorId,
            contributor_len,
            &mut read_context,
            max_payload,
        )?;
        let material = read_text(
            owner,
            &qualification,
            OrganizationScalarFieldV1::Material,
            material_len,
            &mut read_context,
            max_payload,
        )?;
        let effect_receipt = match effect_len {
            Some(length) => Some(read_text(
                owner,
                &qualification,
                OrganizationScalarFieldV1::EffectReceipt,
                length,
                &mut read_context,
                max_payload,
            )?),
            None => None,
        };
        if organization != organization_id {
            return Err(invalid().into());
        }
        if (read_context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let deposit = deposit_fee.retain(OrganizationEstateDepositV1 {
            id,
            organization_id: organization,
            contributor_id: contributor,
            material,
            amount,
            held_in_trust,
            disposed,
            effect_receipt,
        });
        let guard = context.transaction.guard();
        context.transaction.validate_guard(&guard, live)?;
        Ok(Some(owner_fee.retain(CurrentOrganizationEstateV1 {
            parent,
            risk,
            parent_roots,
            outer_id: outer,
            proof: qualification,
            deposit,
            guard,
            memory: memory.clone(),
        })))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
