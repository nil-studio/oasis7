//! Paid selected predicate DATA. No row grants scheduler, source or execution authority.
use super::*;
use crate::world::{
    incremental_normal_action_body_interface::{
        CurrentBodyInterfaceV1, read_current_body_interface_v1,
    },
    incremental_normal_tick_finalize::commitment::PaidIwsCurrentCommitmentV1,
};
use oasis7_agent_api::{
    iws_scheduler_v2::{IWS_CURRENT_COMMITMENT_PATH_V1, IwsWakeEvaluationContextV2},
    legacy_scheduler::PreconditionSubjectV1,
};
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_world_state::{
    incremental_agent_authority_records::*,
    incremental_agent_fields::AgentFieldV1 as Field,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1, AgentSubtreeValueV1 as Scalar,
        read_agent_subtree_scalar_overlay_v1,
    },
    incremental_field_overlay::WorldFieldOverlayContextV1,
};
#[derive(Clone, Copy)]
enum Path {
    Status,
    Position,
    Inventory,
    Capability,
    Electricity,
    Data,
    Intent,
    Tick,
    Reorg,
    Manifest,
    Cut,
}
impl Path {
    fn parse(subject: &str, path: &str) -> Option<Self> {
        Some(match (subject, path) {
            ("agent", "agent.status") => Self::Status,
            ("agent", "agent.position") => Self::Position,
            ("agent", "agent.inventory_digest") => Self::Inventory,
            ("agent", "agent.capability_snapshot_hash") => Self::Capability,
            ("agent", "agent.resource.electricity") => Self::Electricity,
            ("agent", "agent.resource.data") => Self::Data,
            ("intent", "intent.status") => Self::Intent,
            ("world", "world.logical_tick") => Self::Tick,
            ("world", "world.reorg_epoch") => Self::Reorg,
            ("world", "world.runtime_manifest_hash") => Self::Manifest,
            ("world", IWS_CURRENT_COMMITMENT_PATH_V1) => Self::Cut,
            _ => return None,
        })
    }
    fn name(self) -> &'static str {
        match self {
            Self::Status => "agent.status",
            Self::Position => "agent.position",
            Self::Inventory => "agent.inventory_digest",
            Self::Capability => "agent.capability_snapshot_hash",
            Self::Electricity => "agent.resource.electricity",
            Self::Data => "agent.resource.data",
            Self::Intent => "intent.status",
            Self::Tick => "world.logical_tick",
            Self::Reorg => "world.reorg_epoch",
            Self::Manifest => "world.runtime_manifest_hash",
            Self::Cut => IWS_CURRENT_COMMITMENT_PATH_V1,
        }
    }
    fn world(self) -> bool {
        matches!(self, Self::Tick | Self::Reorg | Self::Manifest | Self::Cut)
    }
}
struct Row {
    subject: PreconditionSubjectV1,
    path: Path,
    value: Option<ReservedValue<Vec<u8>>>,
    _body: Option<CurrentBodyInterfaceV1>,
    _intent: Option<ReservedValue<AgentAuthorityRecordQualificationV1>>,
    _fee: MemoryReservation,
}
pub(in crate::world::incremental_normal_cognition_service) struct PaidCurrentPredicateValuesV2 {
    rows: Vec<Row>,
    world_id: String,
    core: WorkIdentityV1,
    parent: ExecutionFence,
    parent_record: [u8; 32],
    input: [u8; 32],
    _fee: ReservedValue<()>,
}
impl PaidCurrentPredicateValuesV2 {
    pub(in crate::world::incremental_normal_cognition_service) fn enrich(
        &self,
        mut context: IwsWakeEvaluationContextV2,
        cut: &PaidIwsCurrentCommitmentV1,
        tick: u64,
        reorg: u64,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<IwsWakeEvaluationContextV2, BodyMutationError> {
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || cut.roots().simulation_tick != tick
            || cut.frame().parent_record != self.parent_record
            || cut.frame().parent_body != self.parent.parent_commit
            || cut.frame().parent_roots != self.parent.parent_roots
            || cut.frame().frozen_material != self.input
        {
            return Err(bad().into());
        }
        for row in &self.rows {
            common
                .charge(
                    1,
                    (row.subject.id.len() + row.subject.kind.len() + row.path.name().len()) as u64,
                )
                .map_err(formal)?;
            if row.path.world() {
                if row.subject.id != self.world_id {
                    continue;
                }
                let value = match row.path {
                    Path::Tick => encode(&tick, 16, common)?,
                    Path::Reorg => encode(&reorg, 16, common)?,
                    Path::Manifest => {
                        encode(&cut.manifest_hash(), cut.manifest_hash().len() + 9, common)?
                    }
                    Path::Cut => {
                        common.charge(1, 64).map_err(formal)?;
                        let digest = format!("sha256:{}", hex::encode(cut.digest()));
                        encode(&digest, 80, common)?
                    }
                    _ => return Err(bad().into()),
                };
                common
                    .charge(
                        1,
                        (row.subject.id.len()
                            + row.subject.kind.len()
                            + row.path.name().len()
                            + value.len()) as u64,
                    )
                    .map_err(formal)?;
                context =
                    context.with_subject_predicate_value(&row.subject, row.path.name(), &value);
            } else if let Some(value) = &row.value {
                common.charge(1, value.len() as u64).map_err(formal)?;
                context =
                    context.with_subject_predicate_value(&row.subject, row.path.name(), value);
            }
        }
        Ok(context)
    }
}
fn encode<T: serde::Serialize>(
    value: &T,
    bound: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
    let bytes = bound
        .checked_mul(4)
        .and_then(|n| n.checked_add(4096))
        .ok_or(ValueTreeError::ResourceLimited("predicate CBOR footprint"))?;
    common.charge(bytes as u64, bytes as u64).map_err(formal)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let bytes = serde_cbor::to_vec(value).map_err(formal)?;
    if bytes.len() > bound {
        return Err(bad().into());
    }
    Ok(fee.retain(bytes))
}
#[allow(clippy::too_many_arguments)]
fn scalar<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    agent: &str,
    field: Field,
    ordinal: u64,
    q: &ReservedValue<
        oasis7_world_state::incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    >,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    live: &ExecutionFence,
) -> Result<Option<Scalar>, BodyMutationError> {
    let guard = ctx.transaction.guard();
    read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent,
                field,
                ordinal,
            },
            qualification: q,
        },
        ctx,
        64,
        &mut std::io::sink(),
        &guard,
        live,
    )
    .map_err(overlay)
}
fn signed(value: Option<Scalar>) -> Result<i64, BodyMutationError> {
    match value {
        Some(Scalar::I64(v)) => Ok(v),
        _ => Err(bad().into()),
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world::incremental_normal_cognition_service) fn read_current_cognition_predicate_values_v2<
        P,
        PE,
        PC,
    >(
        &mut self,
        sources: &CurrentCognitionServiceSourcesV2<'_, '_>,
        conditions: &[IwsWakeConditionV2],
        actual_runtime_world_id: &str,
        work: &mut CurrentCognitionServiceWorkV2<'_>,
        parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<PaidCurrentPredicateValuesV2, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result = (|| {
            sources
                .parent
                .validate_context(common)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), live)?;
            if !self.ctx.transaction.uses_budget(common.memory())
                || !self.ctx.values.memory.same_pool(common.memory())
                || self.ctx.values.class != MemoryClass::Ordinary
                || self.ctx.transaction.parent_fence() != sources.parent.fence()
                || conditions.len() > 16
                || actual_runtime_world_id.is_empty()
            {
                return Err(bad().into());
            }
            let bytes = conditions
                .len()
                .checked_mul(std::mem::size_of::<Row>())
                .and_then(|n| n.checked_add(actual_runtime_world_id.len()))
                .and_then(|n| n.checked_add(std::mem::size_of::<PaidCurrentPredicateValuesV2>()))
                .ok_or_else(bad)?;
            common
                .charge(conditions.len() as u64, bytes as u64)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?
                .retain(());
            let mut rows = Vec::with_capacity(conditions.len());
            if rows.capacity() > conditions.len() {
                return Err(bad().into());
            }
            let q = sources.parent.refs().storage.current().raw().agents();
            for condition in conditions {
                if condition.kind() != "state_predicate" {
                    continue;
                }
                let Some(subject) = condition.subject() else {
                    continue;
                };
                let Some(path) = condition
                    .path()
                    .and_then(|path| Path::parse(&subject.kind, path))
                else {
                    continue;
                };
                let row_bytes = subject
                    .id
                    .len()
                    .checked_add(subject.kind.len())
                    .and_then(|n| n.checked_add(path.name().len() + 4096))
                    .ok_or_else(bad)?;
                common
                    .charge(1, row_bytes as u64)
                    .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
                let row_fee = common
                    .memory()
                    .reserve(row_bytes, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let mut body = None;
                let mut intent = None;
                let value = if path.world() {
                    None
                } else if matches!(path, Path::Intent) {
                    let selected =
                        super::io::with_intent_aggregate_v2(self.ctx, work, common, |ctx, work| {
                            open_agent_authority_body_overlay_v1(
                                AgentAuthorityRecordKeyV1 {
                                    kind: AgentAuthorityRecordKindV1::IntentLedger,
                                    id: &subject.id,
                                },
                                sources
                                    .parent
                                    .refs()
                                    .storage
                                    .current()
                                    .native()
                                    .agent_authority(),
                                parent_read.state,
                                ctx,
                                &mut work.authority.scan,
                                work.authority_cells,
                                work.expiry.max_selected_heap as u64,
                            )
                            .map_err(overlay)
                        })
                        .map_err(RootModuleExecutionErrorV1::Body)?;
                    if let Some(selected) = selected {
                        let request = || AgentAuthorityCellRequestV1 {
                            key: AgentAuthorityRecordKeyV1 {
                                kind: AgentAuthorityRecordKindV1::IntentLedger,
                                id: &subject.id,
                            },
                            cell: AgentAuthorityCellV1::Column(AgentAuthorityColumnV1::Intent(
                                oasis7_world_state::incremental_agent_authority_records::IntentFieldV1::Status,
                            )),
                            qualification: &selected,
                        };
                        let length = super::io::with_intent_aggregate_v2(
                            self.ctx,
                            work,
                            common,
                            |ctx, work| {
                                let guard = ctx.transaction.guard();
                                match read_qualified_agent_authority_cell_overlay_v1(
                                    request(),
                                    ctx,
                                    work.expiry.max_selected_heap as u64,
                                    &mut std::io::sink(),
                                    &guard,
                                    live,
                                )
                                .map_err(overlay)?
                                {
                                    AgentAuthorityValueV1::Text { utf8_bytes } => Ok(utf8_bytes),
                                    _ => Err(bad().into()),
                                }
                            },
                        )
                        .map_err(RootModuleExecutionErrorV1::Body)?;
                        let length = usize::try_from(length).map_err(|_| bad())?;
                        let text_fee = common
                            .memory()
                            .reserve(
                                length + std::mem::size_of::<String>(),
                                MemoryClass::Ordinary,
                            )
                            .map_err(ValueTreeError::Memory)?;
                        let mut text = Vec::with_capacity(length);
                        if text.capacity() > length {
                            return Err(bad().into());
                        }
                        super::io::with_intent_aggregate_v2(self.ctx, work, common, |ctx, _| {
                            let guard = ctx.transaction.guard();
                            let actual = read_qualified_agent_authority_cell_overlay_v1(
                                request(),
                                ctx,
                                length as u64,
                                &mut text,
                                &guard,
                                live,
                            )
                            .map_err(overlay)?;
                            if actual
                                != (AgentAuthorityValueV1::Text {
                                    utf8_bytes: length as u64,
                                })
                            {
                                return Err(bad().into());
                            }
                            Ok(())
                        })
                        .map_err(RootModuleExecutionErrorV1::Body)?;
                        let text = text_fee.retain(String::from_utf8(text).map_err(|_| bad())?);
                        let value = encode(&*text, length + 9, common)
                            .map_err(RootModuleExecutionErrorV1::Body)?;
                        drop(request);
                        intent = Some(selected);
                        Some(value)
                    } else {
                        None
                    }
                } else {
                    let x = with_current_cognition_io_v1(
                        self.ctx,
                        self.members,
                        work.expiry,
                        common,
                        |ctx, _, _| scalar(&subject.id, Field::PositionX, 0, q, ctx, live),
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                    if x.is_none() {
                        None
                    } else {
                        let x = signed(x).map_err(RootModuleExecutionErrorV1::Body)?;
                        let value = match path {
                            Path::Position => {
                                let y = with_current_cognition_io_v1(
                                    self.ctx,
                                    self.members,
                                    work.expiry,
                                    common,
                                    |ctx, _, _| {
                                        scalar(&subject.id, Field::PositionY, 0, q, ctx, live)
                                    },
                                )
                                .map_err(RootModuleExecutionErrorV1::Body)?;
                                encode(
                                    &(x, signed(y).map_err(RootModuleExecutionErrorV1::Body)?),
                                    32,
                                    common,
                                )
                            }
                            Path::Electricity | Path::Data => {
                                let ordinal = if matches!(path, Path::Electricity) {
                                    0
                                } else {
                                    1
                                };
                                let value = with_current_cognition_io_v1(
                                    self.ctx,
                                    self.members,
                                    work.expiry,
                                    common,
                                    |ctx, _, _| {
                                        scalar(
                                            &subject.id,
                                            Field::ResourceAmount,
                                            ordinal,
                                            q,
                                            ctx,
                                            live,
                                        )
                                    },
                                )
                                .map_err(RootModuleExecutionErrorV1::Body)?;
                                encode(
                                    &match value {
                                        None => 0,
                                        Some(Scalar::I64(value)) => value,
                                        _ => return Err(bad().into()),
                                    },
                                    16,
                                    common,
                                )
                            }
                            Path::Status => {
                                let present = with_current_cognition_io_v1(
                                    self.ctx,
                                    self.members,
                                    work.expiry,
                                    common,
                                    |ctx, _, _| {
                                        scalar(&subject.id, Field::ActivityPresent, 0, q, ctx, live)
                                    },
                                )
                                .map_err(RootModuleExecutionErrorV1::Body)?;
                                let status = match present {
                                    Some(Scalar::Bool(false)) => crate::AgentActivityStatus::Idle,
                                    Some(Scalar::Bool(true)) => {
                                        let status = with_current_cognition_io_v1(
                                            self.ctx,
                                            self.members,
                                            work.expiry,
                                            common,
                                            |ctx, _, _| {
                                                scalar(
                                                    &subject.id,
                                                    Field::ActivityStatus,
                                                    0,
                                                    q,
                                                    ctx,
                                                    live,
                                                )
                                            },
                                        )
                                        .map_err(RootModuleExecutionErrorV1::Body)?;
                                        match status {
                                            Some(Scalar::U8(0)) => crate::AgentActivityStatus::Idle,
                                            Some(Scalar::U8(1)) => {
                                                crate::AgentActivityStatus::Executing
                                            }
                                            Some(Scalar::U8(2)) => {
                                                crate::AgentActivityStatus::Blocked
                                            }
                                            Some(Scalar::U8(3)) => {
                                                crate::AgentActivityStatus::Waiting
                                            }
                                            Some(Scalar::U8(4)) => {
                                                crate::AgentActivityStatus::Unavailable
                                            }
                                            _ => return Err(bad().into()),
                                        }
                                    }
                                    _ => return Err(bad().into()),
                                };
                                encode(&status, 32, common)
                            }
                            Path::Inventory => {
                                let selected = read_current_body_interface_v1(
                                    &subject.id,
                                    q,
                                    self.ctx,
                                    common,
                                    work.expiry.max_selected_heap,
                                    None,
                                    live,
                                )
                                .map_err(RootModuleExecutionErrorV1::Body)?
                                .ok_or_else(bad)?;
                                let bound = selected
                                    .layout_bytes
                                    .checked_mul(64)
                                    .and_then(|n| n.checked_add(4096))
                                    .ok_or_else(bad)?;
                                common
                                    .charge(bound as u64, bound as u64)
                                    .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
                                let _scratch = common
                                    .memory()
                                    .reserve(bound, MemoryClass::Ordinary)
                                    .map_err(ValueTreeError::Memory)?;
                                let hash =
                                    oasis7_wasm_abi::canonical_hash(&selected.body.cargo_entries)
                                        .map_err(|e| {
                                        RootModuleExecutionErrorV1::Body(
                                            BodyMutationError::Original(Box::new(PredicateError(
                                                e,
                                            ))),
                                        )
                                    })?;
                                body = Some(selected);
                                encode(&hash, 80, common)
                            }
                            Path::Capability => {
                                let bytes = with_current_cognition_io_v1(
                                    self.ctx,
                                    self.members,
                                    work.expiry,
                                    common,
                                    |ctx, _, _| read_capability(ctx),
                                )
                                .map_err(RootModuleExecutionErrorV1::Body)?;
                                encode(&bytes.as_str(), bytes.len() + 9, common)
                            }
                            _ => return Err(bad().into()),
                        }
                        .map_err(RootModuleExecutionErrorV1::Body)?;
                        Some(value)
                    }
                };
                rows.push(Row {
                    subject: subject.clone(),
                    path,
                    value,
                    _body: body,
                    _intent: intent,
                    _fee: row_fee,
                });
            }
            Ok(PaidCurrentPredicateValuesV2 {
                rows,
                world_id: actual_runtime_world_id.to_owned(),
                core: common.work().identity(),
                parent: sources.parent.fence(),
                parent_record: sources.parent.refs().head.verified_record_id(),
                input: sources.input.checked().material_id(),
                _fee: fee,
            })
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = self
                .ctx
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
fn read_capability<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    use oasis7_world_state::{
        incremental_field_member::*,
        incremental_runtime_authorization_root::authorization_root_descriptor_v1,
        incremental_semantic_key::hash_semantic_key_v1,
    };
    let field = authorization_root_descriptor_v1();
    let semantic = hash_semantic_key_v1(
        &(),
        ctx.values.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(formal)?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Entry, Some(semantic.digest))
        .map_err(ValueTreeError::Invalid)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: ctx.semantic_bytes,
        payload_bytes: 64,
        record_bytes: ctx.record_bytes,
    };
    let meta = current_leaf(ctx, metadata.as_bytes())?.ok_or_else(bad)?;
    read_member_record_profile_v1(
        &meta,
        &metadata,
        &mut ctx.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Unit,
            semantic_bytes: 0,
            limits,
        },
        &mut std::io::sink(),
        &mut std::io::sink(),
        &mut ctx.cancelled,
    )?;
    let actual = current_leaf(ctx, key.as_bytes())?.ok_or_else(bad)?;
    let fee = ctx
        .values
        .memory
        .reserve(128, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut text = Vec::with_capacity(64);
    read_member_record_profile_v1(
        &actual,
        &key,
        &mut ctx.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Text,
            semantic_bytes: semantic.encoded_bytes,
            limits,
        },
        &mut std::io::sink(),
        &mut text,
        &mut ctx.cancelled,
    )?;
    if text.len() != 64 {
        return Err(bad().into());
    }
    let text = String::from_utf8(text).map_err(|_| bad())?;
    Ok(fee.retain(text))
}

#[derive(Debug)]
struct PredicateError<T: std::fmt::Debug>(T);
impl<T: std::fmt::Debug> std::fmt::Display for PredicateError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Current predicate encoding: {:?}", self.0)
    }
}
impl<T: std::fmt::Debug> std::error::Error for PredicateError<T> {}

fn current_leaf<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    key: &[u8],
) -> Result<Option<[u8; 32]>, BodyMutationError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let value = ctx.transaction.read(
        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
        key,
    );
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    let value = value?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(value)
}
