//! Selected continuity settlement data and fixed path CAS; no action permission.
//! The Runtime owner remains responsible for original terms/controller predicates.
use super::*;
use crate::world_rules::{
    OrganizationActivityKindV1, OrganizationActivityReceiptV1, OrganizationRegistrationV1,
};
type Registration = OrganizationRegistrationV1;
pub struct CurrentContinuitySettlementRowV1 {
    source: ReservedValue<WorldRuleCompositeOverlayV1<State>>,
    organization: String,
    registration: RecoveredWorldRuleCompositeV1<Registration>,
    identity: PreparedGuard,
    memory: MemoryBudget,
    _header: MemoryReservation,
}
impl CurrentContinuitySettlementRowV1 {
    pub fn registration(&self) -> &Registration {
        self.registration.value()
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.identity, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(bad().into());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_continuity_settlement_row_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<OrganizationContinuityQualificationV1>,
    roots: &RootSetV1,
    organization: &str,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<CurrentContinuitySettlementRowV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        parent.check_budget(context.values.memory, MemoryClass::Ordinary)?;
        if !parent.uses_budget(context.values.memory)
            || !context.transaction.uses_budget(context.values.memory)
        {
            return Err(bad().into());
        }
        let header = context
            .values
            .memory
            .reserve(
                size_of::<CurrentContinuitySettlementRowV1>()
                    .checked_add(organization.len())
                    .and_then(|n| n.checked_add(4096))
                    .ok_or_else(limited)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let fresh = context.transaction.guard();
        let source = bind_current_world_rule_branch_v1(
            parent, roots, state, context, scan, work, &fresh, live,
        )?;
        let row = point(
            source.body(),
            &[Key::Field("registrations"), Key::Entry(organization)],
            &mut context.values,
            &mut *context.key_work,
            context.semantic_bytes,
            context.record_bytes,
            &mut context.take_state_error,
            &mut context.cancelled,
        )?;
        let Some((_, ColumnCellV1::Root(root))) = row else {
            if row.is_some() {
                return Err(bad().into());
            }
            return Ok(None);
        };
        let mut read = WorldFieldReadContextV1 {
            root: &root,
            state,
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
        let registration = recover_typed_record_body_v1::<Registration, _, _, _, _>(
            &root,
            &State::BRANCH.profile(),
            &mut read,
            scan,
            work,
        )?;
        drop(read);
        let copied = organization.to_owned();
        if copied.capacity() > organization.len() {
            return Err(limited().into());
        }
        Ok(Some(CurrentContinuitySettlementRowV1 {
            source,
            organization: copied,
            registration,
            identity: context.transaction.guard(),
            memory: context.values.memory.clone(),
            _header: header,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
enum Effect {
    Pause,
    Receipt(OrganizationActivityReceiptV1),
    Noop,
}
pub struct PreparedContinuitySettlementEffectV1 {
    source: CurrentContinuitySettlementRowV1,
    effect: Effect,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
/// Fixed invalid-source projection. It conveys no assessment/Accepted authority.
pub fn prepare_continuity_settlement_pause_v1<M: MerkleNodeReader>(
    source: CurrentContinuitySettlementRowV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedContinuitySettlementEffectV1, ContinuityMembershipErrorV1> {
    source.validate(tx, common.memory(), live)?;
    charge(common, 0)?;
    let fee = common
        .memory()
        .reserve(
            size_of::<PreparedContinuitySettlementEffectV1>()
                + size_of::<Option<String>>()
                + "activity_source_integrity_failed".len(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(PreparedContinuitySettlementEffectV1 {
        source,
        effect: Effect::Pause,
        core: common.work().identity(),
        _fee: fee,
    })
}
/// Fixed original source-ref/length/time/dedup decisions, from actual Current.
#[allow(clippy::too_many_arguments)]
pub fn prepare_continuity_settlement_receipt_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source: CurrentContinuitySettlementRowV1,
    contract: &str,
    now: u64,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedContinuitySettlementEffectV1, ContinuityMembershipErrorV1> {
    source.validate(context.transaction, common.memory(), live)?;
    charge(common, contract.len())?;
    let n = "economic-contract-settlement:"
        .len()
        .checked_add(contract.len())
        .ok_or_else(limited)?;
    let fee = common
        .memory()
        .reserve(
            size_of::<PreparedContinuitySettlementEffectV1>()
                + size_of::<OrganizationActivityReceiptV1>()
                + size_of::<String>()
                + n,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut source_ref = String::with_capacity(n);
    if source_ref.capacity() > n {
        return Err(limited().into());
    }
    source_ref.push_str("economic-contract-settlement:");
    source_ref.push_str(contract);
    charge(common, n)?;
    let effect = if source_ref.is_empty() || source_ref.len() > 256 || now == 0 {
        Effect::Noop
    } else {
        let found = point(
            source.source.body(),
            &[
                Key::Field("activity_receipts"),
                Key::Entry(&source.organization),
                Key::Entry(&source_ref),
            ],
            &mut context.values,
            &mut *context.key_work,
            context.semantic_bytes,
            context.record_bytes,
            &mut context.take_state_error,
            &mut context.cancelled,
        )?;
        if found.is_some() {
            Effect::Noop
        } else {
            Effect::Receipt(OrganizationActivityReceiptV1 {
                source_ref,
                world_tick: now,
                kind: OrganizationActivityKindV1::ExternalAtomicExchangeSettlement,
            })
        }
    };
    Ok(PreparedContinuitySettlementEffectV1 {
        source,
        effect,
        core: common.work().identity(),
        _fee: fee,
    })
}
#[allow(clippy::too_many_arguments)]
pub fn apply_prepared_continuity_settlement_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    prepared: PreparedContinuitySettlementEffectV1,
    parent: &ReservedValue<OrganizationContinuityQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut WorldRuleCompositeReadWorkV1,
    write_work: &mut WorldRuleCompositeWriteWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), ContinuityMembershipErrorV1> {
    let result = (|| {
        context
            .transaction
            .validate_guard(guard, live)
            .map_err(WorldFieldOverlayErrorV1::from)?;
        context
            .transaction
            .validate_transaction_identity(&prepared.source.identity)
            .map_err(WorldFieldOverlayErrorV1::from)?;
        charge(common, 0)?;
        if prepared.core != common.work().identity()
            || !prepared.source.memory.same_pool(common.memory())
            || !write.memory.same_pool(common.memory())
        {
            return Err(bad().into());
        }
        if matches!(prepared.effect, Effect::Noop) {
            return Ok(());
        }
        let fresh = context.transaction.guard();
        let actual = bind_current_world_rule_branch_v1(
            parent, roots, state, context, scan, read_work, &fresh, live,
        )?;
        charge(common, 2 * size_of::<StateRootV1>())?;
        if actual.body() != prepared.source.source.body() {
            return Err(bad().into());
        }
        // At most two patches, depths <=3. Payload keys own individual reservations.
        let cap = 2usize;
        let layout = 2
            * (size_of::<Vec<ReservedValue<Vec<u8>>>>()
                + size_of::<Vec<&[u8]>>()
                + size_of::<CompositePathPatchV1<'_>>()
                + size_of::<Option<Encoded<'_>>>()
                + size_of::<Option<[u8; 32]>>())
            + 6 * (size_of::<ReservedValue<Vec<u8>>>() + size_of::<&[u8]>())
            + 5 * size_of::<Vec<usize>>();
        let receipt_payload = match &prepared.effect {
            Effect::Receipt(receipt) => receipt.source_ref.len(),
            _ => 0,
        };
        let layout = layout
            .checked_add(
                6 * size_of::<Key<'_>>()
                    + 2 * size_of::<Vec<Key<'_>>>()
                    + size_of::<Option<String>>()
                    + "activity_source_integrity_failed".len(),
            )
            .and_then(|n| n.checked_add(receipt_payload))
            .and_then(|n| {
                n.checked_add(btree_output_heap::<String, OrganizationActivityReceiptV1>(1).ok()?)
            })
            .ok_or_else(limited)?;
        let _layout = common
            .memory()
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        charge(common, layout)?;
        let mut paths = Vec::with_capacity(cap);
        let mut expected = Vec::with_capacity(cap);
        let mut encoded = Vec::with_capacity(cap);
        if paths.capacity() > cap || expected.capacity() > cap || encoded.capacity() > cap {
            return Err(limited().into());
        }
        let profile = State::BRANCH.profile();
        let pause = Some("activity_source_integrity_failed".to_owned());
        let mut receipt_map = None;
        let mut receipt_value = None;
        match prepared.effect {
            Effect::Pause => {
                let keys = [
                    Key::Field("registrations"),
                    Key::Entry(&prepared.source.organization),
                    Key::Field("activity_evidence_paused_reason"),
                ];
                let value = point(
                    actual.body(),
                    &keys,
                    &mut context.values,
                    &mut *context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut context.take_state_error,
                    &mut context.cancelled,
                )?
                .ok_or_else(bad)?
                .0;
                paths.push(prepare_path(&keys, write)?);
                expected.push(Some(value));
                encoded.push(Some(pause.encode(&profile, write, write_work)?));
            }
            Effect::Receipt(receipt) => {
                let organization = &prepared.source.organization;
                let parent_keys = [Key::Field("activity_receipts"), Key::Entry(organization)];
                let parent_row = point(
                    actual.body(),
                    &parent_keys,
                    &mut context.values,
                    &mut *context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut context.take_state_error,
                    &mut context.cancelled,
                )?;
                let keys = if parent_row.is_some() {
                    vec![
                        Key::Field("activity_receipts"),
                        Key::Entry(organization),
                        Key::Entry(&receipt.source_ref),
                    ]
                } else {
                    vec![Key::Field("activity_receipts"), Key::Entry(organization)]
                };
                let length_keys = if parent_row.is_some() {
                    vec![
                        Key::Field("activity_receipts"),
                        Key::Entry(organization),
                        Key::Field("_length"),
                    ]
                } else {
                    vec![Key::Field("activity_receipts"), Key::Field("_length")]
                };
                let (length_value, ColumnCellV1::U64(length)) = point(
                    actual.body(),
                    &length_keys,
                    &mut context.values,
                    &mut *context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut context.take_state_error,
                    &mut context.cancelled,
                )?
                .ok_or_else(bad)?
                else {
                    return Err(bad().into());
                };
                paths.push(prepare_path(&keys, write)?);
                expected.push(None);
                if parent_row.is_some() {
                    receipt_value = Some(receipt);
                } else {
                    let mut map = BTreeMap::new();
                    let key = receipt.source_ref.clone();
                    if key.capacity() > receipt.source_ref.len() {
                        return Err(limited().into());
                    }
                    map.insert(key, receipt);
                    receipt_map = Some(map);
                }
                // Encode only after moving owners into their final local holders, so every
                // borrowed Encoded lives until patch_world_rule_branch completes.
                if let Some(value) = &receipt_value {
                    encoded.push(Some(value.encode(&profile, write, write_work)?));
                } else {
                    encoded.push(Some(
                        receipt_map
                            .as_ref()
                            .ok_or_else(bad)?
                            .encode(&profile, write, write_work)?,
                    ));
                }
                paths.push(prepare_path(&length_keys, write)?);
                expected.push(Some(length_value));
                encoded.push(Some(Encoded::Scalar(MemberPayloadRefV1::U64(
                    length.checked_add(1).ok_or_else(bad)?,
                ))));
            }
            Effect::Noop => return Ok(()),
        }
        let views: Vec<Vec<&[u8]>> = paths
            .iter()
            .map(|path| path.iter().map(|key| key.as_slice()).collect())
            .collect();
        if views.capacity() > cap || views.iter().any(|path| path.capacity() > 3) {
            return Err(limited().into());
        }
        let changes: Vec<_> = views
            .iter()
            .zip(expected)
            .zip(encoded)
            .map(|((path, expected), after)| CompositePathPatchV1 {
                path,
                expected,
                after,
            })
            .collect();
        if changes.capacity() > cap {
            return Err(limited().into());
        }
        let fresh = context.transaction.guard();
        patch_world_rule_branch_v1(
            parent,
            roots,
            Some(&actual),
            changes,
            state,
            context,
            write,
            scan,
            read_work,
            write_work,
            &fresh,
            live,
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
