//! One-field native closure. A complete World still needs the independent
//! global key router, every other family and its complete physical inventory.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_state::{StateEntryV1, StateScanContext, StateScanError},
};
use std::{cell::RefCell, marker::PhantomData};
#[path = "incremental_world_native_family_key.rs"]
mod key;
#[path = "incremental_world_native_family_receiver.rs"]
mod receiver;
pub(crate) use receiver::{
    WorldNativeVerifiedTransitionV1, accept_world_native_field_verified_v1,
    verify_world_native_transition_v1,
};
/// No public constructor, Clone or unbound root-to-qualification conversion.
pub(crate) struct WorldNativeFieldQualificationV1<F> {
    roots: [u8; 32],
    records: u64,
    class: MemoryClass,
    _field: PhantomData<fn() -> F>,
}
impl<F> WorldNativeFieldQualificationV1<F> {
    pub(crate) fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub(crate) fn records(&self) -> u64 {
        self.records
    }
    pub(crate) fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        roots.validate().map_err(|_| invalid())?;
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
}
/// Reuse the same work across fields/records; callers never refill these counters.
pub struct WorldNativeFieldReadWorkV1 {
    pub records_remaining: u64,
    pub trees_remaining: usize,
    pub max_key_heap: usize,
    pub max_pending_records: usize,
    pub typed: TypedRecordReadWorkV1,
}
fn spend(work: &mut WorldNativeFieldReadWorkV1) -> Result<(), ValueTreeError> {
    work.records_remaining = work
        .records_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("native field record work"))?;
    Ok(())
}
fn target_namespace<F: WorldNativeFieldSpecV1>() -> Result<StateNamespaceV1, ValueTreeError> {
    match F::descriptor()?.destination {
        FieldDestination::State(ns) => Ok(ns),
        _ => Err(invalid()),
    }
}
fn check_count<F: WorldNativeFieldSpecV1>(count: u64) -> Result<(), ValueTreeError> {
    if (F::Key::UNIT && count != 1) || F::MAX_RECORDS.is_some_and(|max| count > max) {
        return Err(invalid());
    }
    Ok(())
}
fn body<F: WorldNativeFieldSpecV1>(
    source_key: WorldNativeKeyRefV1<'_>,
    before: Option<&StateRootV1>,
    after: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeFieldReadWorkV1,
) -> Result<(), ValueTreeError> {
    read::check(ctx)?;
    if before == Some(after) {
        return Ok(());
    }
    let profile = F::profile()?;
    let mut decoder = crate::incremental_typed_record_diff::ChangeDecoder {
        ctx,
        scan,
        work: &mut work.typed,
        profile,
        trees_remaining: work.trees_remaining,
        tree_depth: 0,
    };
    let result = crate::incremental_typed_record_diff::accept_body::<
        WorldNativeRecordV1<F::Value>,
        _,
        _,
        _,
        _,
    >(&profile, before, Some(after), &mut decoder);
    work.trees_remaining = decoder.trees_remaining;
    decoder.check()?;
    result?;
    let result = F::validate_record_binding(source_key, after, ctx, scan, &mut work.typed);
    read::check(ctx)?;
    result
}
fn matches_key<F: WorldNativeFieldSpecV1>(bytes: &[u8]) -> Result<bool, ValueTreeError> {
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(false);
    }
    let parsed = WorldFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
    Ok(parsed.owner == FieldOwner::WorldState && parsed.name == F::NAME)
}
fn matches<F: WorldNativeFieldSpecV1>(entry: &StateEntryV1) -> Result<bool, ValueTreeError> {
    matches_key::<F>(&entry.key)
}

/// Streams only the exact original namespace root, in bounded pages. Other
/// fields are deliberately not qualified by this field-scoped capability.
fn qualify_inner<F: WorldNativeFieldSpecV1>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeFieldReadWorkV1,
) -> Result<ReservedValue<WorldNativeFieldQualificationV1<F>>, ValueTreeError> {
    read::check(ctx)?;
    roots.validate().map_err(|_| invalid())?;
    let ns = target_namespace::<F>()?;
    if ctx.root != roots.state_root(ns)
        || work.max_pending_records == 0
        || work.max_pending_records > 256
    {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let admission = work
        .max_pending_records
        .checked_mul(
            std::mem::size_of::<StateEntryV1>()
                + crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES,
        )
        .and_then(|n| n.checked_add(8192))
        .ok_or(ValueTreeError::ResourceLimited("native field page heap"))?;
    let _page = ctx.values.memory.reserve(admission, ctx.values.class)?;
    let mut pending = Vec::with_capacity(work.max_pending_records);
    let mut metadata = false;
    let mut count = 0u64;
    let mut cursor = None;
    loop {
        read::check(ctx)?;
        let cancel = RefCell::new(&mut ctx.cancelled);
        let result = StateScanContext {
            source: ctx.state,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *scan,
        }
        .visit_entries_page(
            ctx.root,
            cursor,
            work.max_pending_records,
            |entry| {
                spend(work)?;
                if !matches::<F>(entry)? {
                    return Ok(());
                }
                key::canonical::<F>(entry)?;
                if pending.len() == pending.capacity() {
                    return Err(ValueTreeError::ResourceLimited("native field page bound"));
                }
                pending.push(entry.clone());
                Ok(())
            },
            || (cancel.borrow_mut())(),
        );
        read::check(ctx)?;
        let page = match result {
            Ok(page) => page,
            Err(StateScanError::Visitor(error)) => return Err(error),
            Err(StateScanError::Memory(error)) => return Err(error.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("native field scan"));
            }
            Err(_) => return Err(invalid()),
        };
        for entry in pending.drain(..) {
            read::check(ctx)?;
            let key = key::canonical::<F>(&entry)?;
            let parsed =
                WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
            match parsed.kind {
                WorldFieldRecordKindV1::Metadata => {
                    if metadata || parsed.semantic_key.is_some() {
                        return Err(invalid());
                    }
                    ctx.values.verify_closure(
                        &entry.value,
                        ctx.record_bytes,
                        &mut ctx.cancelled,
                    )?;
                    ctx.records().presence(&entry.value, &key, 0)?;
                    metadata = true;
                }
                WorldFieldRecordKindV1::Entry => {
                    count = count.checked_add(1).ok_or_else(invalid)?;
                    if F::MAX_RECORDS.is_some_and(|max| count > max) || (F::Key::UNIT && count > 1)
                    {
                        return Err(invalid());
                    }
                    let original = key::original::<F>(&entry, ctx, work.max_key_heap)?;
                    let child =
                        read::read_body_at_entry_v1::<F, _, _, _, _>(&key, &entry.value, ctx)?;
                    body::<F>(
                        key::original_ref::<F>(&original)?,
                        None,
                        &child,
                        ctx,
                        scan,
                        work,
                    )?;
                }
                _ => return Err(invalid()),
            }
            read::check(ctx)?;
        }
        if !page.has_more {
            break;
        }
        let next = page.last_key_digest.ok_or_else(invalid)?;
        if page.visited == 0 || cursor.is_some_and(|old| next <= old) {
            return Err(invalid());
        }
        cursor = Some(next);
    }
    read::check(ctx)?;
    if !metadata {
        return Err(invalid());
    }
    check_count::<F>(count)?;
    let value = WorldNativeFieldQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        records: count,
        class: ctx.values.class,
        _field: PhantomData,
    };
    let proof = retained.retain(value);
    read::check(ctx)?;
    Ok(proof)
}
fn readmit_inner<F: WorldNativeFieldSpecV1>(
    source: &ReservedValue<WorldNativeFieldQualificationV1<F>>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldNativeFieldQualificationV1<F>>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let mut scratch = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(roots)?;
    check_count::<F>(source.records)?;
    let output = scratch.split(512).map_err(ValueTreeError::Invalid)?.retain(
        WorldNativeFieldQualificationV1 {
            roots: source.roots,
            records: source.records,
            class: MemoryClass::Ordinary,
            _field: PhantomData,
        },
    );
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output)
}

pub(crate) fn qualify_world_native_field_v1<F: WorldNativeFieldSpecV1>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeFieldReadWorkV1,
) -> Result<ReservedValue<WorldNativeFieldQualificationV1<F>>, ValueTreeError> {
    let result = qualify_inner::<F>(roots, ctx, scan, work);
    read::check(ctx)?;
    result
}
pub(crate) fn readmit_world_native_field_ordinary_v1<F: WorldNativeFieldSpecV1>(
    source: &ReservedValue<WorldNativeFieldQualificationV1<F>>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldNativeFieldQualificationV1<F>>, ValueTreeError> {
    let result = readmit_inner::<F>(source, roots, memory, &mut cancelled);
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    result
}

#[cfg(test)]
#[path = "incremental_world_native_family_tests.rs"]
mod tests;

#[cfg(test)]
pub(crate) use receiver::accept_world_native_field_transition_v1;

/// Exact original-key decoder for the starter binding index; no new grammar.
pub(crate) fn starter_original_key_v1(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    max_key_heap: usize,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    key::original::<crate::incremental_world_native_gameplay::StarterOcClaimsFieldV1>(
        entry,
        ctx,
        max_key_heap,
    )
}
