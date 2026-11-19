//! Actual selected Economy fields and genuine historical Original arrays.
//! Only this necessary single cohort is materialized; no World is cloned or
//! serialized. The original typed provisioning/invariant kernel owns mutation.
use super::*;
use crate::world::{
    cognition_economy::{
        CognitionEconomyStateV1, CognitionProvisioningRequestV1,
        bounded_work::{CognitionEconomyWorkOpsV1, CurrentEconomyWorkV1},
    },
    incremental_module_invocation::*,
    incremental_normal_cognition_expiry::with_current_cognition_io_v1,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        bounded_cbor_data::{self as codec, CborDataLayoutV1, PaidCborDataV1, PaidCborRefusalV1},
        bounded_context::FormalBoundedContextV1,
    },
    incremental_execution::ExecutionFence,
    incremental_material::{OriginalObjectReader, OriginalObjectStreamReader},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use oasis7_world_state::{
    incremental_cognition_fields::CognitionControlFieldV1 as Control,
    incremental_cognition_history::*,
    incremental_cognition_history_cbor::*,
    incremental_cognition_json::CognitionJsonFieldV1 as JsonField,
    incremental_cognition_read::CognitionControlReadV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_normal_cognition_current::{
        CurrentCognitionJsonRecordV1, read_current_cognition_json_record_v1,
    },
};
use serde_json::Value;
use std::borrow::Borrow;
const MAPS: [(JsonField, &str); 9] = [
    (JsonField::EconomyBalances, "balances"),
    (
        JsonField::EconomyProvisionedBalances,
        "provisioned_balances",
    ),
    (JsonField::EconomyLeaseBindingKeys, "lease_binding_keys"),
    (JsonField::EconomyProvisions, "provisions"),
    (JsonField::EconomyProvisionReceipts, "provision_receipts"),
    (JsonField::EconomyLeases, "leases"),
    (JsonField::EconomyReceipts, "receipts"),
    (JsonField::EconomyIdempotency, "idempotency"),
    (JsonField::EconomyOperations, "operations"),
];
const CONTROLS: [(Control, &str); 5] = [
    (Control::EconomySchema, "schema_version"),
    (Control::EconomyProvisionHeadSeq, "provision_head_seq"),
    (Control::EconomyProvisionHeadDigest, "provision_head_digest"),
    (Control::EconomyHeadSeq, "head_seq"),
    (Control::EconomyHeadDigest, "head_digest"),
];
struct Refusal(PaidCborRefusalV1);
impl std::fmt::Debug for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Economy original codec")
            .field(&self.0.message())
            .finish()
    }
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.message())
    }
}
impl std::error::Error for Refusal {}
/// Closed Economy graph: the only nominal allocation carriers are these
/// original records, nested string-keyed balance maps and JSON Value. Maxima
/// apply to every actual input node in the shared bounded serde admission.
fn json_layout<T>() -> CborDataLayoutV1 {
    use crate::world::cognition_economy::*;
    let carrier = [
        std::mem::size_of::<Value>(),
        std::mem::size_of::<CognitionLeaseV1>(),
        std::mem::size_of::<CognitionLeaseQuoteV1>(),
        std::mem::size_of::<CognitionReceiptV1>(),
        std::mem::size_of::<CognitionProvisioningRequestV1>(),
        std::mem::size_of::<CognitionProvisioningRecordV1>(),
        std::mem::size_of::<CognitionProvisioningReceiptV1>(),
        std::mem::size_of::<CognitionProvisioningEventV1>(),
        std::mem::size_of::<CognitionResourceBalanceV1>(),
        std::mem::size_of::<CognitionEconomyIdempotencyRecordV1>(),
        std::mem::size_of::<CognitionEconomyOperationRecordV1>(),
        std::mem::size_of::<CognitionEconomyEventV1>(),
        std::mem::size_of::<std::collections::BTreeMap<String, CognitionResourceBalanceV1>>(),
    ]
    .into_iter()
    .max()
    .unwrap();
    // Optional/default numeric/empty-container fields allocate no children.
    // Quote defaults (96 literal bytes), the one absent provisioning digest
    // (71 bytes), and fixed schema/codec symbols fit this per-node ceiling.
    CborDataLayoutV1 {
        root: std::mem::size_of::<T>(),
        sequence_element: carrier,
        map_pair: std::mem::size_of::<String>() + carrier,
        set_element: std::mem::size_of::<String>(),
        box_pointee: std::mem::size_of::<Value>(),
        default_heap_per_value: 512,
        symbols_bytes: 4096,
        visits: 64,
        missing: None,
    }
}

pub(super) struct CurrentEconomyBeforeV1<'p> {
    pub(super) maps: [Option<ReservedValue<CurrentCognitionJsonRecordV1>>; 9],
    pub(super) controls: [Value; 5],
    pub(super) frontiers: PendingCurrentCognitionEconomyV1<'p>,
    _controls_graph: MemoryReservation,
    _control_fees: [Option<MemoryReservation>; 5],
    _fee: MemoryReservation,
}
struct Records {
    rows: Vec<PaidCborDataV1<Value>>,
    _slots: MemoryReservation,
}
impl serde::Serialize for Records {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut out = s.serialize_seq(Some(self.rows.len()))?;
        for row in &self.rows {
            out.serialize_element(Borrow::<Value>::borrow(row))?
        }
        out.end()
    }
}
pub(super) struct NoExternal;
impl crate::world::incremental_factory_phase::io::FactoryPhaseMeterLanesV1 for NoExternal {
    fn visit_lanes(
        &mut self,
        _: &mut dyn for<'a> FnMut(
            crate::world::incremental_factory_phase::io::FactoryPhaseMeterLaneV1<'a>,
        ),
    ) {
    }
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    fn materialize_economy_originals_v1(
        &mut self,
        field: CognitionHistoryFieldV1,
        source: &ReservedValue<CompleteCognitionHistorySourceV3>,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Records, RootModuleExecutionErrorV1<O::Error>> {
        let index = match field {
            CognitionHistoryFieldV1::EconomyJournal => 2,
            CognitionHistoryFieldV1::EconomyProvisionJournal => 3,
            _ => return Err(BodyMutationError::Invalid("fixed Economy history field").into()),
        };
        let index = source.indexes()[index]
            .as_ref()
            .ok_or(BodyMutationError::Invalid(
                "actual Economy original index absent",
            ))?;
        let count = usize::try_from(index.record_count())
            .map_err(|_| BodyMutationError::Invalid("Economy original count"))?;
        common
            .charge(
                count as u64,
                (count
                    .checked_mul(std::mem::size_of::<PaidCborDataV1<Value>>())
                    .ok_or(BodyMutationError::Invalid("Economy Original slots"))?)
                    as u64,
            )
            .map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
        let slots = common
            .memory()
            .reserve(
                count
                    .checked_mul(std::mem::size_of::<PaidCborDataV1<Value>>())
                    .ok_or(BodyMutationError::Invalid("Economy Original slots"))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut rows = Vec::with_capacity(count);
        if rows.capacity() != count {
            return Err(BodyMutationError::Invalid("Economy Original vector capacity").into());
        }
        for ordinal in 0..count {
            common.consume_steps(1).map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
            let point = with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.cognition,
                common,
                |ctx, _, work| {
                    let mut read = WorldFieldReadContextV1 {
                        root: index.root(),
                        state: self.state,
                        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                            source: ctx.values.source,
                            memory: ctx.values.memory,
                            class: ctx.values.class,
                            io: &mut *ctx.values.io,
                        },
                        key_work: &mut *ctx.key_work,
                        semantic_bytes: ctx.semantic_bytes,
                        record_bytes: ctx.record_bytes,
                        take_state_error: &mut ctx.take_state_error,
                        cancelled: &mut ctx.cancelled,
                    };
                    read_attested_cognition_history_index_point_v3(
                        index,
                        ordinal as u64,
                        &mut read,
                        &mut work.index,
                    )
                    .map_err(Into::into)
                },
            )?;
            let row = point.row();
            let raw=self.with_factory_prologue_external_meter_v1(common,live_owner,&mut NoExternal,|root,_,_|{
                if !root.originals.allocation_budget().same_pool(root.ctx.values.memory)||root.originals.allocation_class()!=MemoryClass::Ordinary{return Err(BodyMutationError::Invalid("Economy actual Original pool/class").into())}
                root.work.input.bytes_remaining=root.work.input.bytes_remaining.checked_sub(row.encoded_bytes).ok_or(ValueTreeError::ResourceLimited("Economy Original read bytes"))?;
                let bytes=root.originals.read_original(&row.location.original,usize::try_from(row.encoded_bytes).map_err(|_|BodyMutationError::Invalid("Economy Original size"))?,root.ctx.values.memory).map_err(|e|RootModuleExecutionErrorV1::Invocation(ModuleInvocationPreparationErrorV1::Original(oasis7_world_state::incremental_runtime_artifact_content::RuntimeArtifactOriginalErrorV1::Source(e))))?;
                if !bytes.uses_budget(root.ctx.values.memory)||bytes.reserved_bytes()<bytes.capacity()||bytes.len()as u64!=row.encoded_bytes{return Err(BodyMutationError::Invalid("Economy Original actual bytes").into())}Ok(bytes)
            })?;
            let schema = with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.cognition,
                common,
                |ctx, _, work| {
                    verify_cognition_history_original_record_schema_v1(
                        CognitionOriginalCborRequestV1 {
                            field,
                            id: row.location.original,
                            bytes: row.encoded_bytes,
                        },
                        &mut &raw[..],
                        ctx.values.memory,
                        MemoryClass::Ordinary,
                        &mut work.cbor,
                        &mut ctx.cancelled,
                    )
                    .map_err(Into::into)
                },
            )?;
            if schema.semantic().field() != field {
                return Err(
                    BodyMutationError::Invalid("Economy actual Original schema field").into(),
                );
            }
            let data = match codec::decode_closed_cbor_data_v1::<Value>(
                &raw,
                json_layout::<Value>(),
                work.data_limits,
                common,
            )
            .map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })? {
                Ok(v) => v,
                Err(note) => {
                    return Err(BodyMutationError::Original(Box::new(Refusal(note))).into());
                }
            };
            rows.push(data);
        }
        Ok(Records {
            rows,
            _slots: slots,
        })
    }
    /// Read every necessary current map/control before constructing the exact
    /// original JSON economy value. Accepted history arrays come only from the
    /// actual complete source's authenticated ordinal index and Original reader.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn open_current_bootstrap_economy_v1<'p>(
        &mut self,
        parent: &'p BoundNormalTickParentV1<'p>,
        sources: &CurrentBootstrapSourcesV1<'_, '_>,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        (
            CurrentEconomyBeforeV1<'p>,
            PaidCborDataV1<CognitionEconomyStateV1>,
        ),
        RootModuleExecutionErrorV1<O::Error>,
    > {
        parent.validate_context(common).map_err(|e| {
            RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
        })?;
        let history = parent.refs().cognition;
        let frontiers = with_current_cognition_io_v1(
            self.ctx,
            self.members,
            work.cognition,
            common,
            |ctx, write, cog| {
                PendingCurrentCognitionEconomyV1::open_v1(
                    history,
                    parent.refs().head,
                    sources.cognition,
                    sources.lifecycle.families.roots,
                    ctx,
                    write,
                    &mut cog.index,
                    &mut cog.hash,
                    &live_owner(),
                )
                .map_err(overlay_error)
            },
        )?;
        let mut maps: [Option<ReservedValue<CurrentCognitionJsonRecordV1>>; 9] =
            std::array::from_fn(|_| None);
        for ((field, _), slot) in MAPS.iter().zip(&mut maps) {
            *slot = with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.cognition,
                common,
                |ctx, _, cog| {
                    let guard = ctx.transaction.guard();
                    read_current_cognition_json_record_v1(
                        *field,
                        sources.cognition,
                        sources.lifecycle.families.roots,
                        ctx,
                        &mut cog.read,
                        cog.max_selected_heap,
                        &guard,
                        &live_owner(),
                    )
                    .map_err(overlay_error)
                },
            )?;
        }
        let controls_graph = common
            .memory()
            .reserve(
                5 * std::mem::size_of::<Value>() + 5 * 4096,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut controls: [Value; 5] = std::array::from_fn(|_| Value::Null);
        let mut control_fees: [Option<MemoryReservation>; 5] = std::array::from_fn(|_| None);
        for (index, ((field, _), output)) in CONTROLS.iter().zip(&mut controls).enumerate() {
            let first = with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.cognition,
                common,
                |ctx, _, cog| {
                    let guard = ctx.transaction.guard();
                    oasis7_world_state::incremental_cognition_control_overlay::read_current_cognition_economy_control_v1(*field,sources.cognition,sources.lifecycle.families.roots,ctx,&mut std::io::sink(),cog.max_selected_heap as u64,&guard,&live_owner()).map_err(overlay_error)
                },
            )?;
            *output = match first {
                CognitionControlReadV1::Absent => Value::Null,
                CognitionControlReadV1::Unsigned(n) => Value::from(n),
                CognitionControlReadV1::Text { bytes } => {
                    let n = usize::try_from(bytes)
                        .map_err(|_| BodyMutationError::Invalid("Economy control text bound"))?;
                    let fee = common
                        .memory()
                        .reserve(n + std::mem::size_of::<String>(), MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    let mut raw = Vec::with_capacity(n);
                    if raw.capacity() != n {
                        return Err(BodyMutationError::Invalid("Economy control capacity").into());
                    }
                    let second = with_current_cognition_io_v1(
                        self.ctx,
                        self.members,
                        work.cognition,
                        common,
                        |ctx, _, _| {
                            let guard = ctx.transaction.guard();
                            oasis7_world_state::incremental_cognition_control_overlay::read_current_cognition_economy_control_v1(*field,sources.cognition,sources.lifecycle.families.roots,ctx,&mut raw,bytes,&guard,&live_owner()).map_err(overlay_error)
                        },
                    )?;
                    if second != first || raw.len() != n {
                        return Err(
                            BodyMutationError::Invalid("Economy actual control changed").into()
                        );
                    }
                    let text = String::from_utf8(raw)
                        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                    // Transfer into the aggregate controls phase before dropping
                    // this source-copy reservation (the aggregate phase is sized
                    // from the actual strings below before it can escape).
                    let value = Value::String(text);
                    control_fees[index] = Some(fee);
                    value
                }
            };
        }
        let present = maps.iter().all(Option::is_some) && controls.iter().all(|v| !v.is_null());
        let absent = maps.iter().all(Option::is_none) && controls.iter().all(Value::is_null);
        if !present && !absent {
            return Err(BodyMutationError::Invalid("Current Economy paired cohort").into());
        }
        let mut ops = CurrentEconomyWorkV1::new(common, &mut work.cognition.hashes)?;
        let control_phase = ops.before_value(&controls);
        let settled = ops.finish();
        let _ = settled?;
        let _control_phase =
            control_phase.map_err(|_| BodyMutationError::Invalid("Economy controls admission"))?;
        let journals = if present {
            Some((
                self.materialize_economy_originals_v1(
                    CognitionHistoryFieldV1::EconomyJournal,
                    history,
                    work,
                    common,
                    live_owner,
                )?,
                self.materialize_economy_originals_v1(
                    CognitionHistoryFieldV1::EconomyProvisionJournal,
                    history,
                    work,
                    common,
                    live_owner,
                )?,
            ))
        } else {
            None
        };
        let mut ops = CurrentEconomyWorkV1::new(common, &mut work.cognition.hashes)?;
        let map_values: [Option<&Value>; 9] =
            std::array::from_fn(|n| maps[n].as_ref().map(|v| v.value()));
        let phase = ops.before_value(&(
            map_values,
            &controls,
            journals.as_ref().map(|(a, b)| (a, b)),
            crate::world::cognition_economy::COGNITION_ECONOMY_SCHEMA_VERSION,
        ));
        let settled = ops.finish();
        let _ = settled?;
        let _phase = phase
            .map_err(|_| BodyMutationError::Invalid("Current Economy materialization admission"))?;
        let mut value = serde_json::Map::new();
        if present {
            for ((_, name), before) in MAPS.iter().zip(&maps) {
                value.insert(
                    (*name).to_owned(),
                    before
                        .as_ref()
                        .ok_or(BodyMutationError::Invalid("Economy field source"))?
                        .value()
                        .clone(),
                );
            }
            for ((_, name), before) in CONTROLS.iter().zip(&controls) {
                value.insert((*name).to_owned(), before.clone());
            }
            let (journal, provision) = journals
                .as_ref()
                .ok_or(BodyMutationError::Invalid("Economy Original arrays"))?;
            value.insert(
                "journal".into(),
                Value::Array(
                    journal
                        .rows
                        .iter()
                        .map(|v| Borrow::<Value>::borrow(v).clone())
                        .collect(),
                ),
            );
            value.insert(
                "provision_journal".into(),
                Value::Array(
                    provision
                        .rows
                        .iter()
                        .map(|v| Borrow::<Value>::borrow(v).clone())
                        .collect(),
                ),
            );
        } else {
            // This branch is reachable only after the genuine Current cohort
            // and both accepted history fields proved absence above.
            let mut ops = CurrentEconomyWorkV1::new(common, &mut work.cognition.hashes)?;
            let default = default_economy_with_work(&mut ops);
            let settled = ops.finish();
            let _default_fees = settled?;
            let default = default.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            value = serde_json::to_value(default)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?
                .as_object()
                .ok_or(BodyMutationError::Invalid("default Economy object"))?
                .clone();
        }
        let data = match codec::decode_closed_json_value_data_v1::<CognitionEconomyStateV1>(
            Value::Object(value),
            json_layout::<CognitionEconomyStateV1>(),
            work.data_limits,
            common,
        )
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?
        {
            Ok(v) => v,
            Err(note) => return Err(BodyMutationError::Original(Box::new(Refusal(note))).into()),
        };
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<CurrentEconomyBeforeV1<'p>>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok((
            CurrentEconomyBeforeV1 {
                maps,
                controls,
                frontiers,
                _controls_graph: controls_graph,
                _control_fees: control_fees,
                _fee: fee,
            },
            data,
        ))
    }
}

fn default_economy_with_work<O: CognitionEconomyWorkOpsV1>(
    ops: &mut O,
) -> Result<CognitionEconomyStateV1, crate::world::cognition_economy::CognitionEconomyError> {
    use crate::world::cognition_economy::{
        COGNITION_ECONOMY_SCHEMA_VERSION, CognitionEconomyEventV1, CognitionProvisioningEventV1,
    };
    let _phase = ops.before_value(&COGNITION_ECONOMY_SCHEMA_VERSION)?;
    let provision_head_digest = ops.digest(
        "oasis7.cognition.economy.provisioning-journal.v1",
        &(0_u64, Vec::<CognitionProvisioningEventV1>::new()),
    )?;
    let head_digest = ops.digest(
        "oasis7.cognition.economy.journal-head.v1",
        &(0_u64, Vec::<CognitionEconomyEventV1>::new()),
    )?;
    Ok(CognitionEconomyStateV1 {
        schema_version: COGNITION_ECONOMY_SCHEMA_VERSION.to_owned(),
        balances: Default::default(),
        provisioned_balances: Default::default(),
        lease_binding_keys: Default::default(),
        provisions: Default::default(),
        provision_receipts: Default::default(),
        provision_journal: Vec::new(),
        provision_head_seq: 0,
        provision_head_digest,
        leases: Default::default(),
        receipts: Default::default(),
        idempotency: Default::default(),
        operations: Default::default(),
        journal: Vec::new(),
        head_seq: 0,
        head_digest,
    })
}

/// Pure local transition DATA. Data drops before the growth reservations, so
/// even a failed original mutation cannot release capacity while its partial
/// local graph is still live. No Current or installation permission is held.
pub(super) struct EconomyMutationSessionV1 {
    pub(super) state: CognitionEconomyStateV1,
    dirty: bool,
    growth: Vec<MemoryReservation>,
    growth_slots: Option<MemoryReservation>,
}
pub(super) struct EconomySessionWrapV1;
impl codec::CborDataMoveWrapV1<CognitionEconomyStateV1> for EconomySessionWrapV1 {
    type Output = EconomyMutationSessionV1;
    fn wrap_data(state: CognitionEconomyStateV1) -> Self::Output {
        EconomyMutationSessionV1 {
            state,
            dirty: false,
            growth: Vec::new(),
            growth_slots: None,
        }
    }
}
impl EconomyMutationSessionV1 {
    fn admit_growth_v1(
        &mut self,
        input: &crate::ProviderBackedBootstrapAuthorityV1,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        if self.growth.len() == self.growth.capacity() {
            let capacity = self
                .growth
                .capacity()
                .checked_mul(2)
                .ok_or(ValueTreeError::ResourceLimited("Economy growth slots"))?
                .max(4);
            let bytes = capacity
                .checked_mul(std::mem::size_of::<MemoryReservation>())
                .ok_or(ValueTreeError::ResourceLimited("Economy growth slots"))?;
            common
                .charge(1, bytes as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            self.growth
                .try_reserve_exact(capacity - self.growth.len())
                .map_err(|_| ValueTreeError::ResourceLimited("Economy growth slots"))?;
            if self.growth.capacity() != capacity {
                return Err(BodyMutationError::Invalid("Economy growth slot capacity"));
            }
            self.growth_slots = Some(fee);
        }
        let fee = super::execute::admit_data(input, work, common)?;
        self.growth.push(fee);
        Ok(())
    }
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// Execute the exact original provision transition, then append only an
    /// actual new Provision event. Replay returns the old receipt with no new
    /// Original, fact, index row or allowance.
    pub(super) fn provision_bootstrap_economy_v1(
        &mut self,
        input: &crate::ProviderBackedBootstrapAuthorityV1,
        session: &mut EconomyMutationSessionV1,
        before: &mut CurrentEconomyBeforeV1<'_>,
        sources: &CurrentBootstrapSourcesV1<'_, '_>,
        tick: u64,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>> {
        session.admit_growth_v1(input, work, common)?;
        let original_count = session.state.provision_journal.len();
        let mut ops = CurrentEconomyWorkV1::new(common, &mut work.cognition.hashes)?;
        let result = (|| {
            let request = CognitionProvisioningRequestV1::new_with_work_v1(
                &input.provision_id,
                &input.owner_binding,
                &input.owner_binding,
                input.owner_generation,
                &input.world_id,
                &input.branch_id,
                input.reorg_epoch,
                input.allowance,
                &input.authority_context,
                &mut ops,
            )?;
            // Original cognition_economy() validates before provision() begins.
            session.state.validate_with_work_v1(&mut ops)?;
            request.provision_into_v1(&mut session.state, tick, &mut ops)
        })();
        let reconciled = ops.finish();
        let _fees = reconciled?;
        let output = match result {
            Ok(value) => value,
            Err(error) => {
                return Err(RootModuleExecutionErrorV1::Business(
                    super::execute::paid_business_error_v1(
                        crate::WorldError::DistributedValidationFailed {
                            reason: error.code().to_owned(),
                        },
                        common,
                    )?,
                ));
            }
        };
        let receipt = &output.receipt;
        if receipt.request.owner_binding != input.owner_binding
            || receipt.request.owner_generation != input.owner_generation
            || receipt.request.world_id != input.world_id
            || receipt.request.branch_id != input.branch_id
            || receipt.request.reorg_epoch != input.reorg_epoch
            || receipt.request.authority_digest != input.authority_digest
            || receipt.request.provisioning_digest != input.provisioning_digest
        {
            return Err(RootModuleExecutionErrorV1::Business(
                super::execute::paid_business_error_v1(
                    crate::world::provider_backed_bootstrap::bootstrap_denied(
                        "provider bootstrap provisioning receipt identity mismatch",
                    ),
                    common,
                )?,
            ));
        }
        if session.state.provision_journal.len() == original_count {
            return Ok(());
        }
        if session.state.provision_journal.len()
            != original_count
                .checked_add(1)
                .ok_or(BodyMutationError::Invalid("Provision actual event count"))?
        {
            return Err(
                BodyMutationError::Invalid("Provision original transition event count").into(),
            );
        }
        session.dirty = true;
        let event = session
            .state
            .provision_journal
            .last()
            .ok_or(BodyMutationError::Invalid("Provision actual new event"))?;
        let value = crate::world::incremental_normal_cognition_expiry::values::json(
            event,
            work.cognition.max_selected_heap,
            common,
        )?;
        self.append_current_cognition_journal_original_v2(
            sources.lifecycle.families.roots,
            &value.value,
            before.frontiers.provision_mut(),
            work.cognition,
            common,
            live_owner,
        )?;
        Ok(())
    }
    /// Commit all nine exact map expectations as one original fixed-profile
    /// batch, then the original five control CAS operations. This method is
    /// called only after the complete original provisioning/invariant kernel.
    pub(super) fn commit_bootstrap_economy_v1(
        &mut self,
        before: &mut CurrentEconomyBeforeV1<'_>,
        session: &EconomyMutationSessionV1,
        sources: &CurrentBootstrapSourcesV1<'_, '_>,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>> {
        if !session.dirty {
            // Exact replays retain both real frontiers and make no state CAS.
            with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.cognition,
                common,
                |ctx, _, cog| {
                    before
                        .frontiers
                        .confirm_actual_heads_v1(
                            sources.cognition,
                            sources.lifecycle.families.roots,
                            ctx,
                            &mut cog.hash,
                            &live_owner(),
                        )
                        .map_err(overlay_error)
                },
            )?;
            return Ok(());
        }
        let output = crate::world::incremental_normal_cognition_expiry::values::json(
            &session.state,
            work.cognition.max_selected_heap,
            common,
        )?;
        if before.maps.iter().all(Option::is_none) {
            let expected = before
                .frontiers
                .activation_expected()
                .ok_or(BodyMutationError::Invalid("Economy actual absence source"))?;
            struct GroupLoan<'a>(
                &'a mut oasis7_world_state::incremental_cognition_groups::CognitionGroupWriteWorkV1,
            );
            impl crate::world::incremental_factory_phase::io::FactoryPhaseMeterLanesV1 for GroupLoan<'_> {
                fn visit_lanes(
                    &mut self,
                    v: &mut dyn for<'a> FnMut(
                        crate::world::incremental_factory_phase::io::FactoryPhaseMeterLaneV1<'a>,
                    ),
                ) {
                    use crate::world::incremental_factory_phase::io::FactoryPhaseMeterLaneV1::Steps;
                    v(Steps(&mut self.0.records_remaining));
                    v(Steps(&mut self.0.json.cells_remaining));
                    v(Steps(&mut self.0.json.json_nodes_remaining));
                }
            }
            self.with_factory_prologue_external_meter_v1(common,live_owner,&mut GroupLoan(work.groups),|root,live,loan|{
                let guard=root.ctx.transaction.guard();
                oasis7_world_state::incremental_cognition_groups::create_current_cognition_economy_group_v1(expected,&output.value,root.ctx,root.members,loan.0,&guard,&live()).map_err(overlay_error)?;Ok(())
            })?;
            with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.cognition,
                common,
                |ctx, _, cog| {
                    before
                        .frontiers
                        .confirm_actual_heads_v1(
                            sources.cognition,
                            sources.lifecycle.families.roots,
                            ctx,
                            &mut cog.hash,
                            &live_owner(),
                        )
                        .map_err(overlay_error)
                },
            )?;
            return Ok(());
        }
        let changes: [(&ReservedValue<CurrentCognitionJsonRecordV1>, &Value); 9] =
            std::array::from_fn(|n| {
                (
                    before.maps[n]
                        .as_ref()
                        .expect("fixed private all-present Economy"),
                    output
                        .value
                        .get(MAPS[n].1)
                        .expect("original serialized Economy map"),
                )
            });
        with_current_cognition_io_v1(
            self.ctx,
            self.members,
            work.cognition,
            common,
            |ctx, write, cog| {
                let guard = ctx.transaction.guard();
                oasis7_world_state::incremental_normal_cognition_current::replace_current_cognition_economy_batch_v1(&changes,ctx,write,&mut cog.write,&guard,&live_owner()).map_err(overlay_error)
            },
        )?;
        for (index, (field, name)) in CONTROLS.iter().enumerate() {
            use oasis7_world_state::incremental_cognition_control_overlay::{
                CognitionControlValueV1 as V, change_cognition_control_v1,
            };
            let old = match &before.controls[index] {
                Value::String(v) => V::Text(v),
                Value::Number(v) => V::Unsigned(
                    v.as_u64()
                        .ok_or(BodyMutationError::Invalid("Economy control scalar"))?,
                ),
                _ => return Err(BodyMutationError::Invalid("Economy expected control type").into()),
            };
            let after = output
                .value
                .get(*name)
                .ok_or(BodyMutationError::Invalid("original Economy control field"))?;
            let after = match after {
                Value::String(v) => V::Text(v),
                Value::Number(v) => V::Unsigned(
                    v.as_u64()
                        .ok_or(BodyMutationError::Invalid("Economy after control scalar"))?,
                ),
                _ => return Err(BodyMutationError::Invalid("Economy after control type").into()),
            };
            with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.cognition,
                common,
                |ctx, write, _| {
                    let guard = ctx.transaction.guard();
                    change_cognition_control_v1(
                        *field,
                        (old, after),
                        ctx,
                        write,
                        &guard,
                        &live_owner(),
                    )
                    .map(|_| ())
                    .map_err(overlay_error)
                },
            )?;
        }
        with_current_cognition_io_v1(
            self.ctx,
            self.members,
            work.cognition,
            common,
            |ctx, _, cog| {
                before
                    .frontiers
                    .confirm_actual_heads_v1(
                        sources.cognition,
                        sources.lifecycle.families.roots,
                        ctx,
                        &mut cog.hash,
                        &live_owner(),
                    )
                    .map_err(overlay_error)
            },
        )?;
        Ok(())
    }
}

/// The historical replay tail reads the actual staged economy and returns no
/// mutation permission or new receipt graph. The original receipt already
/// lives in this paid state; exact replay neither appends nor grants credit.
pub(super) fn replay_expired_bootstrap_economy_v1(
    input: &crate::ProviderBackedBootstrapAuthorityV1,
    session: &EconomyMutationSessionV1,
    work: &mut CurrentBootstrapWorkV1<'_, '_>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), super::catalog::CatalogErrorV1> {
    let _phase = super::execute::admit_data(input, work, common)?;
    let mut ops = CurrentEconomyWorkV1::new(common, &mut work.cognition.hashes)?;
    let validated = session.state.validate_with_work_v1(&mut ops);
    let settled = ops.finish();
    let _validation_fees = settled?;
    if let Err(e) = validated {
        return Err(crate::WorldError::DistributedValidationFailed {
            reason: e.code().to_owned(),
        }
        .into());
    }
    common
        .charge(1, (input.provision_id.len() * 64) as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let provision = session
        .state
        .provisions
        .get(&input.provision_id)
        .ok_or_else(|| {
            crate::world::provider_backed_bootstrap::bootstrap_denied(
                "expired bootstrap original provision missing",
            )
        })?;
    common
        .charge(1, (provision.receipt_id.len() * 64) as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let receipt = session
        .state
        .provision_receipts
        .get(&provision.receipt_id)
        .ok_or_else(|| {
            crate::world::provider_backed_bootstrap::bootstrap_denied(
                "expired bootstrap original receipt missing",
            )
        })?;
    let mut ops = CurrentEconomyWorkV1::new(common, &mut work.cognition.hashes)?;
    let expected = CognitionProvisioningRequestV1::new_with_work_v1(
        &input.provision_id,
        &input.owner_binding,
        &input.owner_binding,
        input.owner_generation,
        &input.world_id,
        &input.branch_id,
        input.reorg_epoch,
        input.allowance,
        &input.authority_context,
        &mut ops,
    );
    let settled = ops.finish();
    let _request_fees = settled?;
    let expected = expected.map_err(|e| crate::WorldError::DistributedValidationFailed {
        reason: e.code().to_owned(),
    })?;
    if provision.request != *expected.value()
        || receipt.request != *expected.value()
        || receipt.request.authority_digest != input.authority_digest
        || receipt.request.provisioning_digest != input.provisioning_digest
    {
        return Err(crate::world::provider_backed_bootstrap::bootstrap_denied(
            "expired bootstrap original provision identity changed",
        )
        .into());
    }
    Ok(())
}
