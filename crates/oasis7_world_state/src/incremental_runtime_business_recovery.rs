//! Bounded recovery streaming of original counters and threat-map entries.
use super::*;
use crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN;
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_formal_protocol::incremental_state::{
    StateScanBudget, StateScanContext, StateScanError,
};
use std::cell::RefCell;
/// The receiver owns admission for any retained map/cache. This function keeps
/// one charged original key at a time and never constructs a whole-world DTO.
pub fn recover_runtime_business_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    mut receive_threat: impl FnMut(&str, i64) -> Result<(), ValueTreeError>,
) -> Result<[u64; 8], ValueTreeError> {
    let proof = qualify_runtime_business_v1(roots, ctx, scan)?;
    proof.validate_binding(roots)?;
    let mut metrics = [0u64; 8];
    for (index, column) in LOGISTICS_SLA_COLUMNS_V1.into_iter().enumerate() {
        let Some(RuntimeBusinessNumberV1::Unsigned(value)) =
            read_runtime_business_v1(RuntimeBusinessFieldV1::LogisticsSlaMetrics, column, ctx)?
        else {
            return Err(invalid());
        };
        metrics[index] = value;
    }
    let bound = usize::try_from(ctx.semantic_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("Runtime business key bound"))?;
    let _charge = ctx.values.memory.reserve(
        bound.checked_add(4096).ok_or_else(invalid)?,
        ctx.values.class,
    )?;
    let mut key = Vec::with_capacity(bound);
    if key.capacity() > _charge.bytes() {
        return Err(invalid());
    }
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut seen = 0u64;
    let result = StateScanContext {
        source: ctx.state,
        memory: ctx.values.memory,
        class: ctx.values.class,
        io: scan,
    }
    .visit_entries(
        roots.state_root(StateNamespaceV1::Business),
        |entry| {
            if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                return Ok(());
            }
            let parsed =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            if parsed.owner != FieldOwner::Runtime
                || parsed.name != "threat_heatmap"
                || parsed.kind == WorldFieldRecordKindV1::Metadata
            {
                return Ok(());
            }
            let field_key = WorldFieldKeyV1::new(
                RuntimeBusinessFieldV1::ThreatHeatmap.descriptor(),
                parsed.kind,
                parsed.semantic_key,
            )
            .map_err(ValueTreeError::Invalid)?;
            if field_key.as_bytes() != entry.key
                || parsed.namespace != entry.namespace
                || parsed.kind != WorldFieldRecordKindV1::Entry
            {
                return Err(invalid());
            }
            key.clear();
            let mut payload = [0u8; 8];
            // Fixed slice output cannot grow past the pre-admitted original-key bound.
            // The limited writer checks capacity before every copy.
            struct KeyWriter<'a> {
                key: &'a mut Vec<u8>,
                bound: usize,
            }
            impl std::io::Write for KeyWriter<'_> {
                fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                    if bytes.len() > self.bound.saturating_sub(self.key.len()) {
                        return Err(std::io::ErrorKind::OutOfMemory.into());
                    }
                    self.key.extend_from_slice(bytes);
                    Ok(bytes.len())
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            let info = read_member_record_v1(
                &entry.value,
                &field_key,
                &mut ctx.values,
                MemberRecordLimitsV1 {
                    semantic_bytes: ctx.semantic_bytes,
                    payload_bytes: 8,
                    record_bytes: ctx.record_bytes,
                },
                &mut KeyWriter {
                    key: &mut key,
                    bound,
                },
                &mut &mut payload[..],
                &mut || (cancel.borrow_mut())(),
            )?;
            if info.codec != MemberPayloadCodecV1::I64
                || info.payload_bytes != 8
                || key.len() < SEMANTIC_KEY_DOMAIN.len() + 9
                || &key[..SEMANTIC_KEY_DOMAIN.len()] != SEMANTIC_KEY_DOMAIN
                || key[SEMANTIC_KEY_DOMAIN.len()] != 1
            {
                return Err(invalid());
            }
            let start = SEMANTIC_KEY_DOMAIN.len() + 9;
            let len = u64::from_be_bytes(
                key[SEMANTIC_KEY_DOMAIN.len() + 1..start]
                    .try_into()
                    .map_err(|_| invalid())?,
            );
            if len != (key.len() - start) as u64 {
                return Err(invalid());
            }
            let original = std::str::from_utf8(&key[start..]).map_err(|_| invalid())?;
            receive_threat(original, i64::from_be_bytes(payload))?;
            seen = seen.checked_add(1).ok_or_else(invalid)?;
            if (cancel.borrow_mut())() {
                return Err(ValueTreeError::Cancelled);
            }
            Ok(())
        },
        || (cancel.borrow_mut())(),
    );
    let error = (ctx.take_state_error)();
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = error {
        return Err(error);
    }
    match result {
        Ok(_) => (),
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited(
                "Runtime business recovery scan",
            ));
        }
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(_) => return Err(invalid()),
    }
    if seen != proof.heatmap_entries() {
        return Err(invalid());
    }
    Ok(metrics)
}
