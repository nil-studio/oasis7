//! Resident grant selector DATA, built from genuine originals once and folded
//! from actual net writes. No permission/GEN acceptance is minted by this index.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1, SealedExecutionBindingV1},
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{io::Write, mem::size_of, sync::Arc};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("restricted grant directory")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("restricted grant directory")
}
fn step(cells: &mut u64, c: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
    if c() {
        return Err(ValueTreeError::Cancelled);
    }
    *cells = cells.checked_sub(1).ok_or_else(limited)?;
    Ok(())
}
pub struct RestrictedClaimGrantIdentityV1 {
    key: Vec<u8>,
    root: [u8; 32],
    account: ReservedValue<String>,
    status: RestrictedStarterClaimGrantStatus,
    expires: u64,
}
impl RestrictedClaimGrantIdentityV1 {
    pub fn outer_id(&self) -> &str {
        &self.account
    }
    pub fn status(&self) -> RestrictedStarterClaimGrantStatus {
        self.status
    }
    pub fn expires_at_epoch(&self) -> u64 {
        self.expires
    }
}
pub struct RestrictedClaimGrantDirectoryV1 {
    roots: [u8; 32],
    rows: Vec<Arc<ReservedValue<RestrictedClaimGrantIdentityV1>>>,
}
// Private construction checks every retained row against the same pool. Guard
// and seal validation use this captured identity rather than rescan resident rows.
pub struct CurrentRestrictedClaimGrantSnapshotV1 {
    rows: Vec<Arc<ReservedValue<RestrictedClaimGrantIdentityV1>>>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl CurrentRestrictedClaimGrantSnapshotV1 {
    pub fn entries(&self) -> impl ExactSizeIterator<Item = &RestrictedClaimGrantIdentityV1> {
        self.rows.iter().map(|r| &***r)
    }
}
impl RestrictedClaimGrantDirectoryV1 {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
}
fn matches_key(key: &[u8]) -> Result<bool, ValueTreeError> {
    if !key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(false);
    }
    let p = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
    Ok(p.owner == FieldOwner::WorldState
        && p.name == MainTokenPolicyKindV1::Grant.name()
        && p.kind == WorldFieldRecordKindV1::Entry)
}
fn original<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<String>, ValueTreeError> {
    let p = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(
        MainTokenPolicyKindV1::Grant.descriptor()?,
        p.kind,
        p.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key {
        return Err(invalid());
    }
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: ctx.semantic_bytes,
        payload_bytes: oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES as u64,
        record_bytes: ctx.record_bytes,
    };
    step(cells, &mut ctx.cancelled)?;
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        limits,
        &mut ctx.cancelled,
    )?;
    let n = usize::try_from(info.semantic_bytes).map_err(|_| limited())?;
    if info.semantic_bytes > max.checked_add(128).ok_or_else(limited)? {
        return Err(limited());
    }
    ctx.key_work.bytes_remaining = ctx
        .key_work
        .bytes_remaining
        .checked_sub(info.semantic_bytes)
        .ok_or_else(limited)?;
    let fee = ctx.values.memory.reserve(
        n.checked_mul(2)
            .and_then(|n| n.checked_add(size_of::<String>()))
            .ok_or_else(limited)?,
        MemoryClass::Ordinary,
    )?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(n).map_err(|_| limited())?;
    if bytes.capacity() > n {
        return Err(limited());
    }
    read_member_record_key_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        limits,
        &mut bytes,
        &mut ctx.cancelled,
    )?;
    let SemanticKeyRefV1::Text(id) =
        read_semantic_key_v1(&bytes).map_err(ValueTreeError::Invalid)?
    else {
        return Err(invalid());
    };
    Ok(fee.retain(id.to_owned()))
}
fn inspect<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<Option<Arc<ReservedValue<RestrictedClaimGrantIdentityV1>>>, ValueTreeError> {
    if !matches_key(&entry.key)? {
        return Ok(None);
    }
    step(cells, &mut ctx.cancelled)?;
    let Some((kind, owner, child)) = family::outer(entry, ctx)? else {
        return Ok(None);
    };
    if kind != MainTokenPolicyKindV1::Grant {
        return Err(invalid());
    }
    let account = original(entry, ctx, cells, max)?;
    let mut body = WorldFieldReadContextV1 {
        root: &child,
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
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let _q = qualify_main_token_policy_body_v1(kind, owner, &mut body, scan, cells, max)?;
    let mut scalar =
        |column, mut out: &mut dyn Write| -> Result<MainTokenPolicyValueV1, ValueTreeError> {
            step(cells, &mut body.cancelled)?;
            let key = read::cell_key(
                kind,
                MainTokenPolicyCellV1::Column(column),
                &mut body.records(),
            )?;
            let root = body.lookup(&key.0)?.ok_or_else(invalid)?;
            read::value(
                MainTokenPolicyCellV1::Column(column),
                &root,
                &key,
                &mut body.records(),
                max,
                &mut out,
            )
        };
    let mut status_bytes = [0u8; 7];
    let mut status_out = std::io::Cursor::new(status_bytes.as_mut_slice());
    let status_value = scalar(MainTokenPolicyColumnV1::Status, &mut status_out)?;
    let n = usize::try_from(status_out.position()).map_err(|_| invalid())?;
    drop(status_out);
    if status_value
        != (MainTokenPolicyValueV1::Text {
            utf8_bytes: n as u64,
        })
    {
        return Err(invalid());
    }
    let status = match &status_bytes[..n] {
        b"issued" => RestrictedStarterClaimGrantStatus::Issued,
        b"expired" => RestrictedStarterClaimGrantStatus::Expired,
        b"revoked" => RestrictedStarterClaimGrantStatus::Revoked,
        _ => return Err(invalid()),
    };
    let MainTokenPolicyValueV1::Unsigned(expires) = scalar(
        MainTokenPolicyColumnV1::ExpiresAtEpoch,
        &mut std::io::sink(),
    )?
    else {
        return Err(invalid());
    };
    drop(scalar);
    body.check()?;
    let fee = body.values.memory.reserve(
        size_of::<ReservedValue<RestrictedClaimGrantIdentityV1>>()
            .checked_add(64)
            .and_then(|n| n.checked_add(entry.key.len()))
            .ok_or_else(limited)?,
        MemoryClass::Ordinary,
    )?;
    Ok(Some(Arc::new(fee.retain(RestrictedClaimGrantIdentityV1 {
        key: entry.key.clone(),
        root: entry.value,
        account,
        status,
        expires,
    }))))
}
fn sort(
    rows: &mut [Arc<ReservedValue<RestrictedClaimGrantIdentityV1>>],
    cells: &mut u64,
    keys: &mut u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    // In-place heap sort bounds a cold selector build to O(n log n), without
    // allocating another copy of the paid row graph. Each comparison admits
    // work and cancellation before examining the original outer account key.
    fn greater(
        rows: &[Arc<ReservedValue<RestrictedClaimGrantIdentityV1>>],
        a: usize,
        b: usize,
        cells: &mut u64,
        keys: &mut u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<bool, ValueTreeError> {
        step(cells, cancelled)?;
        *keys = keys
            .checked_sub(rows[a].outer_id().len().min(rows[b].outer_id().len()) as u64)
            .ok_or_else(limited)?;
        Ok(rows[a].outer_id() > rows[b].outer_id())
    }
    fn sift(
        rows: &mut [Arc<ReservedValue<RestrictedClaimGrantIdentityV1>>],
        mut root: usize,
        end: usize,
        cells: &mut u64,
        keys: &mut u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        loop {
            let Some(left) = root
                .checked_mul(2)
                .and_then(|v| v.checked_add(1))
                .filter(|v| *v < end)
            else {
                break;
            };
            let mut largest = root;
            if greater(rows, left, largest, cells, keys, cancelled)? {
                largest = left;
            }
            let right = left + 1;
            if right < end && greater(rows, right, largest, cells, keys, cancelled)? {
                largest = right;
            }
            if largest == root {
                break;
            }
            rows.swap(root, largest);
            root = largest;
        }
        Ok(())
    }
    let len = rows.len();
    for root in (0..len / 2).rev() {
        sift(rows, root, len, cells, keys, cancelled)?;
    }
    for end in (1..len).rev() {
        step(cells, cancelled)?;
        rows.swap(0, end);
        sift(rows, 0, end, cells, keys, cancelled)?;
    }
    for pair in rows.windows(2) {
        step(cells, cancelled)?;
        *keys = keys
            .checked_sub(pair[0].outer_id().len().min(pair[1].outer_id().len()) as u64)
            .ok_or_else(limited)?;
        if pair[0].outer_id() == pair[1].outer_id() {
            return Err(invalid());
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub fn build_restricted_claim_grant_directory_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    policy: &ReservedValue<MainTokenPolicyFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<RestrictedClaimGrantDirectoryV1>, ValueTreeError> {
    ctx.check()?;
    if ctx.values.class != MemoryClass::Ordinary
        || !policy.uses_budget(ctx.values.memory)
        || policy.allocation_class() != MemoryClass::Ordinary
        || ctx.root != roots.state_root(MainTokenPolicyKindV1::Grant.namespace())
    {
        return Err(invalid());
    }
    let digest = roots.digest().map_err(|_| invalid())?;
    policy.validate_binding(&digest)?;
    let count = usize::try_from(policy.record_counts()[3]).map_err(|_| limited())?;
    let fee = ctx.values.memory.reserve(
        count
            .checked_mul(size_of::<Arc<ReservedValue<RestrictedClaimGrantIdentityV1>>>())
            .and_then(|n| n.checked_add(size_of::<RestrictedClaimGrantDirectoryV1>()))
            .ok_or_else(limited)?,
        MemoryClass::Ordinary,
    )?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(|_| limited())?;
    if rows.capacity() > count {
        return Err(limited());
    }
    let _page_fee = ctx.values.memory.reserve(
        128usize
            .checked_mul(
                size_of::<StateEntryV1>()
                    + crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES,
            )
            .ok_or_else(limited)?,
        MemoryClass::Ordinary,
    )?;
    let mut pending = Vec::new();
    pending.try_reserve_exact(128).map_err(|_| limited())?;
    if pending.capacity() > 128 {
        return Err(limited());
    }
    let mut cursor = None;
    loop {
        ctx.check()?;
        let stop = std::cell::RefCell::new(&mut ctx.cancelled);
        let result = StateScanContext {
            source: ctx.state,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *scan,
        }
        .visit_entries_page(
            ctx.root,
            cursor,
            128,
            |entry| {
                step(cells, &mut || (stop.borrow_mut())())?;
                if matches_key(&entry.key)? {
                    if pending.len() == 128 {
                        return Err(limited());
                    }
                    pending.push(entry.clone());
                }
                Ok::<_, ValueTreeError>(())
            },
            || (stop.borrow_mut())(),
        );
        drop(stop);
        let page = match result {
            Ok(page) => page,
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => return Err(limited()),
            Err(_) => return Err(invalid()),
        };
        for entry in pending.drain(..) {
            if let Some(row) = inspect(&entry, ctx, scan, cells, max)? {
                if rows.len() == count {
                    return Err(invalid());
                }
                rows.push(row);
            }
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
    if rows.len() != count {
        return Err(invalid());
    }
    sort(
        &mut rows,
        cells,
        &mut ctx.key_work.bytes_remaining,
        &mut ctx.cancelled,
    )?;
    ctx.check()?;
    Ok(fee.retain(RestrictedClaimGrantDirectoryV1 {
        roots: digest,
        rows,
    }))
}
struct Change {
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
#[allow(clippy::too_many_arguments)]
pub fn recover_current_restricted_claim_grant_snapshot_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    dir: &ReservedValue<RestrictedClaimGrantDirectoryV1>,
    policy: &ReservedValue<MainTokenPolicyFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
    pending: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentRestrictedClaimGrantSnapshotV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        dir.validate_binding(roots)?;
        policy.validate_binding(&dir.roots)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !dir.uses_budget(ctx.values.memory)
            || !policy.uses_budget(ctx.values.memory)
            || policy.allocation_class() != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || dir.roots != ctx.transaction.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        let marker = {
            let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            records
                .key(
                    MainTokenPolicyKindV1::Grant.descriptor()?,
                    WorldFieldRecordKindV1::Entry,
                    &(FORMAT, ()),
                )?
                .0
        };
        let mut count = 0usize;
        let mut bytes = 0usize;
        ctx.transaction.visit_pending_writes(
            MainTokenPolicyKindV1::Grant.namespace(),
            pending,
            || Ok::<_, WorldFieldOverlayErrorV1>(()),
            |key, _, _| {
                step(cells, &mut ctx.cancelled)?;
                if key != marker.as_bytes() && matches_key(key)? {
                    count = count.checked_add(1).ok_or_else(limited)?;
                    bytes = bytes.checked_add(key.len()).ok_or_else(limited)?;
                }
                Ok(())
            },
        )?;
        let scratch = ctx
            .values
            .memory
            .reserve(
                count
                    .checked_mul(size_of::<Change>())
                    .and_then(|n| n.checked_add(bytes))
                    .ok_or_else(limited)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut changes = Vec::new();
        changes.try_reserve_exact(count).map_err(|_| limited())?;
        if changes.capacity() > count {
            return Err(limited().into());
        }
        ctx.transaction.visit_pending_writes(
            MainTokenPolicyKindV1::Grant.namespace(),
            pending,
            || Ok::<_, WorldFieldOverlayErrorV1>(()),
            |key, before, after| {
                step(cells, &mut ctx.cancelled)?;
                if key != marker.as_bytes() && matches_key(key)? {
                    changes.push(Change {
                        key: key.to_vec(),
                        before,
                        after,
                    });
                }
                Ok(())
            },
        )?;
        let cap = dir.rows.len().checked_add(count).ok_or_else(limited)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                cap.checked_mul(size_of::<Arc<ReservedValue<RestrictedClaimGrantIdentityV1>>>())
                    .and_then(|n| n.checked_add(size_of::<CurrentRestrictedClaimGrantSnapshotV1>()))
                    .ok_or_else(limited)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(cap).map_err(|_| limited())?;
        if rows.capacity() > cap {
            return Err(limited().into());
        }
        for row in &dir.rows {
            step(cells, &mut ctx.cancelled)?;
            rows.push(Arc::clone(row));
        }
        for change in changes {
            let mut found = None;
            for (i, row) in rows.iter().enumerate() {
                step(cells, &mut ctx.cancelled)?;
                ctx.key_work.bytes_remaining = ctx
                    .key_work
                    .bytes_remaining
                    .checked_sub(row.key.len().min(change.key.len()) as u64)
                    .ok_or_else(limited)?;
                if row.key == change.key {
                    found = Some(i);
                    break;
                }
            }
            if found.map(|i| rows[i].root) != change.before
                || ctx
                    .transaction
                    .read(MainTokenPolicyKindV1::Grant.namespace(), &change.key)?
                    != change.after
            {
                return Err(invalid().into());
            }
            if let Some(i) = found {
                rows.swap_remove(i);
            }
            if let Some(root) = change.after {
                let entry = StateEntryV1 {
                    namespace: MainTokenPolicyKindV1::Grant.namespace(),
                    key: change.key,
                    value: root,
                };
                let mut read = WorldFieldReadContextV1 {
                    root: roots.state_root(MainTokenPolicyKindV1::Grant.namespace()),
                    state,
                    values: ValueReadContext {
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
                rows.push(inspect(&entry, &mut read, scan, cells, max)?.ok_or_else(invalid)?);
            }
        }
        drop(scratch);
        sort(
            &mut rows,
            cells,
            &mut ctx.key_work.bytes_remaining,
            &mut ctx.cancelled,
        )?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if state.allocation_class() != Some(ctx.values.class)
            || !state
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(ctx.values.memory))
        {
            return Err(invalid().into());
        }
        let guard = ctx.transaction.guard();
        ctx.transaction.validate_guard(&guard, live)?;
        Ok(fee.retain(CurrentRestrictedClaimGrantSnapshotV1 {
            rows,
            guard,
            memory: ctx.values.memory.clone(),
        }))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
impl CurrentRestrictedClaimGrantSnapshotV1 {
    pub fn validate_guard<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub fn validate_sealed(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        if !self.memory.same_pool(memory) {
            return Err(invalid().into());
        }
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        Ok(())
    }
    pub fn rebind_after_seal_v1(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
        cells: &mut u64,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<RestrictedClaimGrantDirectoryV1>, WorldFieldOverlayErrorV1> {
        self.validate_sealed(binding, roots, memory)?;
        let fee = memory
            .reserve(
                self.rows
                    .len()
                    .checked_mul(size_of::<Arc<ReservedValue<RestrictedClaimGrantIdentityV1>>>())
                    .and_then(|n| n.checked_add(size_of::<RestrictedClaimGrantDirectoryV1>()))
                    .ok_or_else(limited)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(self.rows.len())
            .map_err(|_| limited())?;
        if rows.capacity() > self.rows.len() {
            return Err(limited().into());
        }
        for row in &self.rows {
            step(cells, &mut cancelled)?;
            rows.push(Arc::clone(row));
        }
        Ok(fee.retain(RestrictedClaimGrantDirectoryV1 {
            rows,
            roots: roots.digest().map_err(|_| invalid())?,
        }))
    }
}
