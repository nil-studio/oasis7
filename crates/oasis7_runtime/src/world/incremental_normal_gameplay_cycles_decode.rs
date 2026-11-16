//! Owned gameplay directive decoding from an actually selected ModuleEmitted
//! payload. This parser returns DATA; only the fixed role/source consumer can
//! construct a gameplay event source. The original Value is never cloned.
use super::kernel::{GameplayDirectiveEnvelope, GameplayLifecycleDirective};
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::MemoryClass;
use serde::Deserialize;
use serde_json::Value;
use std::{
    mem::{align_of, size_of},
    ptr::NonNull,
    sync::Arc,
};
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("gameplay directives owned layout")
}
#[derive(Default)]
struct Shape {
    tokens: usize,
    seq: usize,
    map: usize,
    strings: usize,
    depth: usize,
}
fn visit(
    value: &Value,
    depth: usize,
    max_depth: usize,
    shape: &mut Shape,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<(), BodyMutationError> {
    tail.charge_record()?;
    if depth > max_depth {
        return Err(limited().into());
    }
    shape.depth = shape.depth.max(depth);
    shape.tokens = shape.tokens.checked_add(1).ok_or_else(limited)?;
    match value {
        Value::String(text) => {
            tail.charge(text.len())?;
            shape.strings = shape.strings.checked_add(text.len()).ok_or_else(limited)?;
        }
        Value::Array(items) => {
            shape.seq = shape.seq.checked_add(items.len()).ok_or_else(limited)?;
            for item in items {
                visit(
                    item,
                    depth.checked_add(1).ok_or_else(limited)?,
                    max_depth,
                    shape,
                    tail,
                )?;
            }
        }
        Value::Object(items) => {
            shape.map = shape.map.checked_add(items.len()).ok_or_else(limited)?;
            for (key, item) in items {
                tail.charge(key.len())?;
                shape.strings = shape.strings.checked_add(key.len()).ok_or_else(limited)?;
                visit(
                    item,
                    depth.checked_add(1).ok_or_else(limited)?,
                    max_depth,
                    shape,
                    tail,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn tree_node<K, V>() -> Result<usize, ValueTreeError> {
    let align = align_of::<K>()
        .max(align_of::<V>())
        .max(align_of::<Option<NonNull<()>>>())
        .max(align_of::<u16>());
    (size_of::<K>() + size_of::<V>())
        .checked_mul(11)
        .and_then(|n| {
            n.checked_add(
                size_of::<Option<NonNull<()>>>()
                    + 2 * size_of::<u16>()
                    + 12 * size_of::<NonNull<()>>(),
            )
        })
        .and_then(|n| n.checked_add((align - 1) * 7))
        .ok_or_else(limited)
}
/// Each moved directive retains the entire original admission. It cannot
/// outlive its fees when a caller consumes or drops the remaining iterator.
pub(super) struct OwnedGameplayDirectiveV1 {
    pub(super) value: GameplayLifecycleDirective,
    pub(super) heap: Arc<ReservedValue<()>>,
}
pub(super) struct GameplayDirectiveIteratorV1 {
    iter: std::vec::IntoIter<GameplayLifecycleDirective>,
    heap: Arc<ReservedValue<()>>,
}
impl Iterator for GameplayDirectiveIteratorV1 {
    type Item = OwnedGameplayDirectiveV1;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|value| OwnedGameplayDirectiveV1 {
            value,
            heap: self.heap.clone(),
        })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
impl ExactSizeIterator for GameplayDirectiveIteratorV1 {}
pub(super) enum GameplayDirectiveDecodeErrorV1 {
    Body(BodyMutationError),
    Json {
        error: serde_json::Error,
        heap: Arc<ReservedValue<()>>,
    },
}
pub(super) fn decode_gameplay_directives_v1(
    value: &Value,
    max_depth: usize,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<GameplayDirectiveIteratorV1, GameplayDirectiveDecodeErrorV1> {
    let bound = (|| {
        let mut shape = Shape::default();
        visit(value, 0, max_depth, &mut shape, tail)?;
        // The pinned serde Content buffering ABI is included explicitly. Vec growth
        // and simultaneous old/new allocations are bounded by four slots per item;
        // internal-tag Content clone stages are bounded by actual input depth+3.
        let content = size_of::<serde::__private229::de::Content<'static>>();
        let target =
            size_of::<GameplayLifecycleDirective>().max(size_of::<crate::WarParticipantOutcome>());
        let seq = content.max(target).checked_mul(4).ok_or_else(limited)?;
        let pair = size_of::<(
            serde::__private229::de::Content<'static>,
            serde::__private229::de::Content<'static>,
        )>();
        let map = tree_node::<String, Value>()?
            .checked_add(pair.checked_mul(4).ok_or_else(limited)?)
            .ok_or_else(limited)?;
        let phase = shape
            .seq
            .checked_mul(seq)
            .and_then(|n| shape.map.checked_mul(map).and_then(|m| n.checked_add(m)))
            .and_then(|n| {
                shape
                    .tokens
                    .checked_mul(content + target)
                    .and_then(|m| n.checked_add(m))
            })
            .and_then(|n| shape.strings.checked_mul(8).and_then(|m| n.checked_add(m)))
            .ok_or_else(limited)?;
        let stages = shape.depth.checked_add(3).ok_or_else(limited)?;
        let heap = phase
            .checked_mul(stages)
            .and_then(|n| {
                n.checked_add(
                    size_of::<ReservedValue<()>>()
                        + 4 * size_of::<usize>()
                        + 4 * align_of::<ReservedValue<()>>()
                        + 2048,
                )
            })
            .ok_or_else(limited)?;
        let operations = shape
            .tokens
            .checked_add(shape.seq)
            .and_then(|n| n.checked_add(shape.map))
            .and_then(|n| n.checked_mul(stages))
            .ok_or_else(limited)?;
        tail.debit(
            u64::try_from(operations).map_err(|_| limited())?,
            u64::try_from(shape.strings.checked_mul(stages).ok_or_else(limited)?)
                .map_err(|_| limited())?,
        )?;
        let fee = tail.reserve(heap, MemoryClass::Ordinary)?;
        Ok::<_, BodyMutationError>(Arc::new(fee.retain(())))
    })()
    .map_err(GameplayDirectiveDecodeErrorV1::Body)?;
    // &Value deserializes through the original serde visitors while borrowing the
    // input; only the admitted directive/Content graph owns new allocations.
    match GameplayDirectiveEnvelope::deserialize(value) {
        Ok(decoded) => Ok(GameplayDirectiveIteratorV1 {
            iter: decoded.directives.into_iter(),
            heap: bound,
        }),
        Err(error) => Err(GameplayDirectiveDecodeErrorV1::Json { error, heap: bound }),
    }
}
