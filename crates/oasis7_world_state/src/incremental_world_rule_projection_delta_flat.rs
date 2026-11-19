//! Seven non-composite children. Relative records first use their original
//! complete Spec qualifier, then decode only the selected actual typed DTO.
use super::*;
use crate::incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1};
use oasis7_formal_protocol::incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1};
#[path = "incremental_world_rule_projection_delta_relative.rs"]
mod relative;
fn identity(raw: &[u8]) -> Result<(&str, SemanticKeyRefV1<'_>), ValueTreeError> {
    let SemanticKeyRefV1::Pair { left, right } =
        read_semantic_key_v1(raw).map_err(ValueTreeError::Invalid)?
    else {
        return Err(bad());
    };
    match left.decode().map_err(ValueTreeError::Invalid)? {
        SemanticKeyRefV1::Text(name) => {
            Ok((name, right.decode().map_err(ValueTreeError::Invalid)?))
        }
        SemanticKeyRefV1::Pair { left, right: id } => {
            let SemanticKeyRefV1::Text(name) = left.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(bad());
            };
            Ok((name, id.decode().map_err(ValueTreeError::Invalid)?))
        }
        _ => Err(bad()),
    }
}
fn emit<C: FnMut() -> bool>(
    parts: &[&str],
    semantic: &[u8],
    payload: Option<ReservedValue<Vec<u8>>>,
    p: &Processing<'_>,
    b: &mut Build,
    stop: &mut C,
) -> Result<(), ValueTreeError> {
    let (path, mut fee) = super::path(parts, &b.memory, p, stop)?;
    spend(p, semantic.len(), stop)?;
    let semantic_fee = b.memory.reserve(
        semantic
            .len()
            .checked_add(size_of::<Vec<u8>>())
            .ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    let mut copy = Vec::new();
    copy.try_reserve_exact(semantic.len())
        .map_err(|_| limit())?;
    if copy.capacity() > semantic.len() {
        return Err(limit());
    }
    copy.extend_from_slice(semantic);
    let mut slot = b.memory.reserve(size_of::<Row>(), MemoryClass::Ordinary)?;
    fee.merge(&mut slot).map_err(ValueTreeError::Invalid)?;
    b.push(
        Row {
            path,
            semantic: semantic_fee.retain(copy),
            json: payload,
            _fee: fee,
        },
        p,
        stop,
    )
}
enum Selection<'a> {
    Current(&'a Pending),
    Qualified(&'a oasis7_formal_protocol::incremental_state::StateEntryV1),
}
impl Selection<'_> {
    fn key(&self) -> &[u8] {
        match self {
            Self::Current(v) => &v.key,
            Self::Qualified(v) => &v.key,
        }
    }
    fn value(&self) -> Option<[u8; 32]> {
        match self {
            Self::Current(v) => v.after,
            Self::Qualified(v) => Some(v.value),
        }
    }
}
fn edge<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    row: &Selection<'_>,
    semantic_len: usize,
    schema: [u8; 32],
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, WorldFieldOverlayErrorV1> {
    let Some(id) = row.value() else {
        return Ok(None);
    };
    let parsed = WorldFieldKeyV1::from_bytes(row.key()).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(
        crate::incremental_world_rule_parent::descriptor()?,
        parsed.kind,
        parsed.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    let mut raw = [0u8; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &id,
        &key,
        &mut ctx.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: semantic_len as u64,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: semantic_len as u64,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: ctx.record_bytes,
            },
        },
        &mut io::sink(),
        &mut io::Cursor::new(raw.as_mut_slice()),
        &mut ctx.cancelled,
    )?;
    let reference = RecordRootRefV1::from_canonical_bytes(&raw).map_err(|_| bad())?;
    let root = reference
        .validate_binding(
            &schema,
            &parsed.semantic_key.ok_or_else(bad)?,
            StateNamespaceV1::Authorization,
        )
        .map_err(|_| bad())?;
    let fee = ctx
        .values
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(Some(fee.retain(root.clone())))
}
#[allow(clippy::too_many_arguments)]
fn project_selection<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    row: &Selection<'_>,
    semantic: &[u8],
    child: Option<&'static str>,
    _roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    _work: &mut TypedRecordReadWorkV1,
    p: &Processing<'_>,
    b: &mut Build,
    _live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let Some(child) = child else { return Ok(()) }; // The actual optional parent is read once by the enclosing owner.
    let (_, id) = identity(semantic)?;
    match child {
        "governance_effects" => {
            let SemanticKeyRefV1::Text(id) = id else {
                return Err(bad().into());
            };
            let payload = if row.value().is_some() {
                let parsed =
                    WorldFieldKeyV1::from_bytes(row.key()).map_err(ValueTreeError::Invalid)?;
                let key = WorldFieldKeyV1::new(
                    crate::incremental_world_rule_parent::descriptor()?,
                    parsed.kind,
                    parsed.semantic_key,
                )
                .map_err(ValueTreeError::Invalid)?;
                let limits = MemberRecordLimitsV1 {
                    semantic_bytes: semantic.len() as u64,
                    payload_bytes: ctx.record_bytes,
                    record_bytes: ctx.record_bytes,
                };
                let original = row.value().ok_or_else(bad)?;
                let h = read_member_record_header_v1(
                    &original,
                    &key,
                    &mut ctx.values,
                    limits,
                    &mut ctx.cancelled,
                )?;
                let n = usize::try_from(h.payload_bytes).map_err(|_| limit())?;
                spend(p, n, &mut ctx.cancelled)?;
                let _phase = b
                    .memory
                    .reserve(
                        n.checked_add(size_of::<String>()).ok_or_else(limit)?,
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                let mut raw = Vec::new();
                raw.try_reserve_exact(n).map_err(|_| limit())?;
                if raw.capacity() > n {
                    return Err(limit().into());
                }
                crate::incremental_utf8_output::read_text_value(
                    &original,
                    &key,
                    &mut ctx.values,
                    MemberReadProfileV1 {
                        codec: MemberPayloadCodecV1::Text,
                        semantic_bytes: semantic.len() as u64,
                        limits,
                    },
                    &mut raw,
                    "projection rule effect UTF-8",
                    &mut ctx.cancelled,
                )?;
                let value = String::from_utf8(raw).map_err(|_| bad())?;
                Some(json(&value, &b.memory, p, &mut ctx.cancelled)?)
            } else {
                None
            };
            emit(
                &["world_rules", child, id],
                semantic,
                payload,
                p,
                b,
                &mut ctx.cancelled,
            )?;
        }
        "supply_lineages" => {
            let SemanticKeyRefV1::Text(id) = id else {
                return Err(bad().into());
            };
            let value = crate::incremental_world_rule_lineages::read_world_rule_lineage_overlay_v1(
                id, ctx,
            )?;
            let payload = match value {
                crate::incremental_world_rule_lineages::WorldRuleLineageValueV1::Value(value) => {
                    Some(json(&value, &b.memory, p, &mut ctx.cancelled)?)
                }
                crate::incremental_world_rule_lineages::WorldRuleLineageValueV1::RulesAbsent
                | crate::incremental_world_rule_lineages::WorldRuleLineageValueV1::LineageAbsent => {
                    None
                }
            };
            emit(
                &["world_rules", child, id],
                semantic,
                payload,
                p,
                b,
                &mut ctx.cancelled,
            )?;
        }
        "supply_shipments" => {
            let SemanticKeyRefV1::Unsigned(id) = id else {
                return Err(bad().into());
            };
            shipment(id, semantic, ctx, p, b)?;
        }
        "supply_authorizations"
        | "supply_batches"
        | "organization_cases"
        | "organization_estate" => {
            let SemanticKeyRefV1::Text(id) = id else {
                return Err(bad().into());
            };
            let schema = match child {
                "supply_authorizations" => {
                    crate::incremental_world_rule_supply_root::SupplyRootKeyV1::Authorization(id)
                        .schema()
                }
                "supply_batches" => {
                    crate::incremental_world_rule_supply_root::SupplyRootKeyV1::Batch(id).schema()
                }
                "organization_cases" => {
                    crate::incremental_world_rule_organization::root::OrganizationRootKeyV1::Case(
                        id,
                    )
                    .schema()
                }
                "organization_estate" => {
                    crate::incremental_world_rule_organization::root::OrganizationRootKeyV1::Estate(
                        id,
                    )
                    .schema()
                }
                _ => return Err(bad().into()),
            };
            let current = edge(row, semantic.len(), schema, ctx)?;
            let payload = if let Some(root) = current {
                let mut read = WorldFieldReadContextV1 {
                    root: &root,
                    state,
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
                let digest = WorldFieldKeyV1::from_bytes(row.key())
                    .map_err(ValueTreeError::Invalid)?
                    .semantic_key
                    .ok_or_else(bad)?;
                let max = read.record_bytes;
                // Original Spec/relations/complete-record kernel authenticates this exact
                // Current point family before any typed DTO construction or serialization.
                let (source, json) = match child {
                    "supply_authorizations" => {
                        let q=crate::incremental_world_rule_supply_root::qualify_supply_body_v1(crate::incremental_world_rule_supply_root::SupplyRootKeyV1::Authorization(id),digest,&mut read,scan,&mut _work.json.cells_remaining,max)?;
                        let value = relative::original_json::<
                            crate::world_rules::SupplyAuthorizationV1,
                            _,
                            _,
                            _,
                            _,
                        >(&mut read, scan, p)?;
                        (Source::Supply(q), value)
                    }
                    "supply_batches" => {
                        let q = crate::incremental_world_rule_supply_root::qualify_supply_body_v1(
                            crate::incremental_world_rule_supply_root::SupplyRootKeyV1::Batch(id),
                            digest,
                            &mut read,
                            scan,
                            &mut _work.json.cells_remaining,
                            max,
                        )?;
                        let value = relative::original_json::<
                            crate::world_rules::SupplyBatchV1,
                            _,
                            _,
                            _,
                            _,
                        >(&mut read, scan, p)?;
                        (Source::Supply(q), value)
                    }
                    "organization_cases" => {
                        let q=crate::incremental_world_rule_organization::root::qualify_organization_body_v1(crate::incremental_world_rule_organization::root::OrganizationRootKeyV1::Case(id),digest,&mut read,scan,&mut _work.json.cells_remaining,max)?;
                        let value = relative::original_json::<
                            crate::world_rules::OrganizationCaseV1,
                            _,
                            _,
                            _,
                            _,
                        >(&mut read, scan, p)?;
                        (Source::Organization(q), value)
                    }
                    "organization_estate" => {
                        let q=crate::incremental_world_rule_organization::root::qualify_organization_body_v1(crate::incremental_world_rule_organization::root::OrganizationRootKeyV1::Estate(id),digest,&mut read,scan,&mut _work.json.cells_remaining,max)?;
                        let value = relative::original_json::<
                            crate::world_rules::OrganizationEstateDepositV1,
                            _,
                            _,
                            _,
                            _,
                        >(&mut read, scan, p)?;
                        (Source::Organization(q), value)
                    }
                    _ => return Err(bad().into()),
                };
                if b.sources.len() == b.sources.capacity() {
                    return Err(limit().into());
                }
                b.sources.push(source);
                Some(json)
            } else {
                None
            };
            emit(
                &["world_rules", child, id],
                semantic,
                payload,
                p,
                b,
                &mut ctx.cancelled,
            )?;
        }
        _ => return Err(bad().into()),
    }
    Ok(())
}
fn shipment<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: u64,
    semantic: &[u8],
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    p: &Processing<'_>,
    b: &mut Build,
) -> Result<(), WorldFieldOverlayErrorV1> {
    use crate::incremental_world_rule_shipments::{
        WorldRuleShipmentValueV1 as V, read_world_rule_shipment_overlay_v1 as read,
    };
    let max = ctx.record_bytes;
    let (mut a, mut c, mut d, mut e) = (io::sink(), io::sink(), io::sink(), io::sink());
    let first = read(id, ctx, max, &mut [&mut a, &mut c, &mut d, &mut e])?;
    let payload = match first {
        V::RulesAbsent | V::ShipmentAbsent => None,
        V::Value {
            identity_utf8_bytes,
            input_amount,
            delivered_amount,
            settled,
        } => {
            let n = identity_utf8_bytes
                .into_iter()
                .try_fold(4 * size_of::<String>(), |a, n| {
                    a.checked_add(usize::try_from(n).map_err(|_| limit())?)
                        .ok_or_else(limit)
                })?;
            spend(p, n, &mut ctx.cancelled)?;
            let _phase = b
                .memory
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut values: [Vec<u8>; 4] = std::array::from_fn(|_| Vec::new());
            for (out, n) in values.iter_mut().zip(identity_utf8_bytes) {
                let n = usize::try_from(n).map_err(|_| limit())?;
                out.try_reserve_exact(n).map_err(|_| limit())?;
                if out.capacity() > n {
                    return Err(limit().into());
                }
            }
            let [a, c, d, e] = &mut values;
            let second = read(id, ctx, max, &mut [a, c, d, e])?;
            if !matches!(second,V::Value{identity_utf8_bytes:counts,input_amount:i,delivered_amount:d,settled:s}if counts==identity_utf8_bytes&&i==input_amount&&d==delivered_amount&&s==settled)
            {
                return Err(bad().into());
            }
            let [a, c, d, e] = values;
            let value = crate::world_rules::SupplyShipmentV1 {
                authorization_id: String::from_utf8(a).map_err(|_| bad())?,
                batch_id: String::from_utf8(c).map_err(|_| bad())?,
                beneficiary_id: String::from_utf8(d).map_err(|_| bad())?,
                lineage_id: String::from_utf8(e).map_err(|_| bad())?,
                input_amount,
                delivered_amount,
                settled,
            };
            Some(json(&value, &b.memory, p, &mut ctx.cancelled)?)
        }
    };
    let _text = b
        .memory
        .reserve(20 + size_of::<String>(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    spend(p, 20, &mut ctx.cancelled)?;
    let mut text = String::new();
    text.try_reserve_exact(20).map_err(|_| limit())?;
    if text.capacity() > 20 {
        return Err(limit().into());
    }
    std::fmt::Write::write_fmt(&mut text, format_args!("{id}")).map_err(|_| bad())?;
    emit(
        &["world_rules", "supply_shipments", &text],
        semantic,
        payload,
        p,
        b,
        &mut ctx.cancelled,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn project<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    row: &Pending,
    semantic: &[u8],
    child: Option<&'static str>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    p: &Processing<'_>,
    b: &mut Build,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    project_selection(
        &Selection::Current(row),
        semantic,
        child,
        roots,
        state,
        ctx,
        scan,
        work,
        p,
        b,
        live,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn project_qualified<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    row: &oasis7_formal_protocol::incremental_state::StateEntryV1,
    semantic: &[u8],
    child: Option<&'static str>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    p: &Processing<'_>,
    b: &mut Build,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    project_selection(
        &Selection::Qualified(row),
        semantic,
        child,
        roots,
        state,
        ctx,
        scan,
        work,
        p,
        b,
        live,
    )
}
