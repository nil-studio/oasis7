//! Actual continuing IO/graph admission around the sole business SMT kernel.
//! The public outputs are index DATA, never an accepted parent or permission.
use super::super::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
};
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, mem::size_of};
const SENTINEL: &str = "paid business index operation failed";
/// Read-only original bytes. Implementations must admit their actual complete
/// Vec graph and IO on this Common before allocation/read. Hash/depth/key and
/// append-only predicates remain in the fixed kernel; this trait grants none.
pub trait PaidBusinessIndexNodeReaderV1 {
    fn read_node_bounded(
        &self,
        digest: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, FormalError>;
}
pub struct PaidBusinessIndexDeltaV1 {
    value: BusinessIndexDeltaV1,
    core: WorkIdentityV1,
    phase: MemoryReservation,
}
pub struct PaidBusinessIndexLeafV1 {
    value: BusinessIndexLeafV1,
    core: WorkIdentityV1,
    phase: MemoryReservation,
}
macro_rules! owner {
    ($name:ident, $method:ident, $ty:ty) => {
        impl $name {
            pub fn $method(&self) -> &$ty {
                &self.value
            }
            pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
                self.phase.uses_budget(memory)
            }
            pub fn reserved_bytes(&self) -> usize {
                self.phase.bytes()
            }
            pub fn validate_context(
                &self,
                common: &mut FormalBoundedContextV1<'_>,
            ) -> Result<(), FormalError> {
                common.consume_steps(1)?;
                if self.core != common.work().identity() || !self.uses_budget(common.memory()) {
                    return Err(FormalError::Invalid(
                        "business index foreign paid owner".into(),
                    ));
                }
                Ok(())
            }
        }
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }
    };
}
owner!(PaidBusinessIndexDeltaV1, delta, BusinessIndexDeltaV1);
owner!(PaidBusinessIndexLeafV1, leaf, BusinessIndexLeafV1);
/// Preserve the original kernel note and the first real typed IO/resource
/// cause. Every partially built graph stays covered until the failure drops.
pub struct PaidBusinessIndexErrorV1 {
    error: FormalError,
    _phase: Option<MemoryReservation>,
}
impl PaidBusinessIndexErrorV1 {
    pub fn error(&self) -> &FormalError {
        &self.error
    }
}
impl std::fmt::Debug for PaidBusinessIndexErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.error, f)
    }
}
impl std::fmt::Display for PaidBusinessIndexErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for PaidBusinessIndexErrorV1 {}
fn bad(s: &'static str) -> FormalError {
    FormalError::Invalid(s.into())
}
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| bad("business index graph overflow"))
}
fn mul(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_mul(b)
        .ok_or_else(|| bad("business index graph overflow"))
}
fn graph(updates: &Vec<BusinessIndexLeafV1>) -> Result<usize, FormalError> {
    // Every insertion produces at most depth+1 immutable nodes. Six live
    // map phases cover Updater pending, moved writes, v2 pending/reachable,
    // final Vec and validation encoding; two sets cover parent paths. Each
    // bound uses an entire real BTree node per entry, including split scratch.
    let map_node = 11 * size_of::<(String, Vec<u8>)>() + 14 * size_of::<usize>() + 128;
    let set_node = 11 * size_of::<String>() + 14 * size_of::<usize>() + 128;
    let node = 6 * map_node + 2 * set_node + 8 * size_of::<BusinessIndexNodeWriteV1>() + 256;
    let nodes = mul(updates.len(), BUSINESS_INDEX_DEPTH + 1)?;
    let payload = updates.iter().try_fold(0usize, |n, u| {
        add(
            n,
            add(u.canonical_value.capacity(), u.key_digest.capacity())?,
        )
    })?;
    // One reusable maximum closed Node decode/roundtrip phase: original raw,
    // typed leaf bytes, original re-encoding and temporary return clone. No
    // unknown generic DTO graph or internally generated Work is involved.
    add(
        add(
            add(mul(nodes, node)?, mul(payload, 16)?)?,
            mul(updates.capacity(), size_of::<BusinessIndexLeafV1>())?,
        )?,
        4 * (BUSINESS_INDEX_MAX_LEAF_BYTES + 128) + 128 * 1024,
    )
}
struct State<'a, 'c> {
    common: &'a mut FormalBoundedContextV1<'c>,
    first: Option<FormalError>,
}
impl State<'_, '_> {
    fn save(&mut self, error: FormalError) -> String {
        if self.first.is_none() {
            self.first = Some(error);
        }
        SENTINEL.into()
    }
    fn pay(&mut self, bytes: usize) -> Result<(), String> {
        if self.first.is_some() {
            return Err(SENTINEL.into());
        }
        let bytes =
            u64::try_from(bytes).map_err(|_| self.save(bad("business index work overflow")))?;
        self.common.charge(1, bytes).map_err(|e| self.save(e))
    }
}
struct Reader<'a, 'ctx, 'c, R> {
    reader: &'a R,
    state: &'a RefCell<State<'ctx, 'c>>,
}
impl<R: PaidBusinessIndexNodeReaderV1> BusinessIndexNodeReader for Reader<'_, '_, '_, R> {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        let mut state = self.state.borrow_mut();
        state.pay(digest.len())?;
        let raw = match self.reader.read_node_bounded(digest, state.common) {
            Ok(v) => v,
            Err(e) => return Err(state.save(e)),
        };
        let Some(raw) = raw else { return Ok(None) };
        if !raw.uses_budget(state.common.memory())
            || raw.reserved_bytes() < raw.capacity().saturating_add(size_of::<Vec<u8>>())
        {
            return Err(state.save(bad("business index raw reader pool/graph")));
        }
        if raw.len() > BUSINESS_INDEX_MAX_LEAF_BYTES + 128 {
            return Err("business index node byte bound".into());
        }
        // The original String reader boundary needs owned bytes. The actual
        // raw owner stays live through this exact, prepaid copy, and one shared
        // fixed decode phase covers the returned buffer until the kernel ends.
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(raw.len())
            .map_err(|_| state.save(bad("business index raw copy admission")))?;
        if bytes.capacity() > raw.len() {
            return Err(state.save(bad("business index raw copy capacity")));
        }
        for chunk in raw.chunks(8192) {
            state.pay(chunk.len())?;
            bytes.extend_from_slice(chunk);
        }
        Ok(Some(bytes))
    }
}
fn failure(error: FormalError, phase: Option<MemoryReservation>) -> PaidBusinessIndexErrorV1 {
    PaidBusinessIndexErrorV1 {
        error,
        _phase: phase,
    }
}
pub fn build_business_index_v2_bounded_v1(
    base: BusinessIndexRootV1,
    updates: Vec<BusinessIndexLeafV1>,
    reader: &impl PaidBusinessIndexNodeReaderV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidBusinessIndexDeltaV1, PaidBusinessIndexErrorV1> {
    common.consume_steps(1).map_err(|e| failure(e, None))?;
    for update in &updates {
        common.consume_steps(1).map_err(|e| failure(e, None))?;
        common
            .charge(1, update.key_digest.len() as u64)
            .map_err(|e| failure(e, None))?;
    }
    let bytes = graph(&updates).map_err(|e| failure(e, None))?;
    let total = add(bytes, size_of::<PaidBusinessIndexDeltaV1>()).map_err(|e| failure(e, None))?;
    let phase = common
        .memory()
        .reserve(total, MemoryClass::Ordinary)
        .map_err(|_| failure(bad("business index complete phase admission"), None))?;
    let core = common.work().identity();
    let state = RefCell::new(State {
        common,
        first: None,
    });
    let result = (|| {
        let meter = |n| state.borrow_mut().pay(n);
        let empty = build_empty_hashes_with(Some(&meter))?;
        let kernel = Kernel {
            empty,
            meter: Some(&meter),
        };
        let reader = Reader {
            reader,
            state: &state,
        };
        v2::build_v2_with(base, updates, &reader, &kernel)
    })();
    let state = state.into_inner();
    match result {
        Ok(value) => {
            if let Some(error) = state.first {
                return Err(failure(error, Some(phase)));
            }
            Ok(PaidBusinessIndexDeltaV1 { value, core, phase })
        }
        Err(note) => Err(failure(
            state.first.unwrap_or(FormalError::Invalid(note)),
            Some(phase),
        )),
    }
}
pub fn get_business_index_leaf_bounded_v1(
    base: &BusinessIndexRootV1,
    key: &str,
    reader: &impl PaidBusinessIndexNodeReaderV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<PaidBusinessIndexLeafV1>, PaidBusinessIndexErrorV1> {
    common.consume_steps(1).map_err(|e| failure(e, None))?;
    let bytes = 4 * (BUSINESS_INDEX_MAX_LEAF_BYTES + 128)
        + 128 * 1024
        + size_of::<PaidBusinessIndexLeafV1>();
    let phase = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| failure(bad("business index complete leaf phase admission"), None))?;
    let core = common.work().identity();
    let state = RefCell::new(State {
        common,
        first: None,
    });
    let result = (|| {
        let meter = |n| state.borrow_mut().pay(n);
        let empty = build_empty_hashes_with(Some(&meter))?;
        let kernel = Kernel {
            empty,
            meter: Some(&meter),
        };
        let reader = Reader {
            reader,
            state: &state,
        };
        kernel.get(base, key, &reader)
    })();
    let state = state.into_inner();
    match result {
        Ok(value) => {
            if let Some(error) = state.first {
                return Err(failure(error, Some(phase)));
            }
            Ok(value.map(|value| PaidBusinessIndexLeafV1 { value, core, phase }))
        }
        Err(note) => Err(failure(
            state.first.unwrap_or(FormalError::Invalid(note)),
            Some(phase),
        )),
    }
}

/// Original delta decoder admitted as a complete closed binary DTO graph.
/// It is DATA: even a successfully decoded delta is not a verified transition.
pub struct PaidDecodedBusinessIndexDeltaV1 {
    value: BusinessIndexDeltaV1,
    core: WorkIdentityV1,
    phase: MemoryReservation,
}
owner!(PaidDecodedBusinessIndexDeltaV1, delta, BusinessIndexDeltaV1);
pub fn decode_business_index_delta_bounded_v1(
    raw: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidDecodedBusinessIndexDeltaV1, PaidBusinessIndexErrorV1> {
    common.consume_steps(1).map_err(|e| failure(e, None))?;
    if raw.len() > BUSINESS_INDEX_MAX_DELTA_BYTES {
        return Err(failure(bad("business index delta byte bound"), None));
    }
    let bytes = mul(raw.len(), 128)
        .and_then(|n| add(n, 128 * 1024))
        .map_err(|e| failure(e, None))?;
    let phase = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| failure(bad("business index decode complete graph"), None))?;
    if let Err(e) = common.charge(bytes as u64, raw.len() as u64) {
        return Err(failure(e, Some(phase)));
    }
    // Custom bounded vectors and original byte-string codecs are unchanged.
    let value = match serde_cbor::from_slice::<BusinessIndexDeltaV1>(raw) {
        Ok(v) => v,
        Err(e) => return Err(failure(FormalError::Invalid(e.to_string()), Some(phase))),
    };
    let core = common.work().identity();
    Ok(PaidDecodedBusinessIndexDeltaV1 { value, core, phase })
}
pub fn verify_business_index_delta_bounded_v1(
    delta: &PaidDecodedBusinessIndexDeltaV1,
    expected_base: &str,
    reader: &impl PaidBusinessIndexNodeReaderV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<BusinessIndexRootV1>, PaidBusinessIndexErrorV1> {
    delta
        .validate_context(common)
        .map_err(|e| failure(e, None))?;
    let bytes = graph(&delta.value.updates).map_err(|e| failure(e, None))?;
    let phase = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| failure(bad("business index verify complete graph"), None))?;
    let state = RefCell::new(State {
        common,
        first: None,
    });
    let result = (|| {
        let meter = |n| state.borrow_mut().pay(n);
        let empty = build_empty_hashes_with(Some(&meter))?;
        let kernel = Kernel {
            empty,
            meter: Some(&meter),
        };
        let reader = Reader {
            reader,
            state: &state,
        };
        delta.value.verify_with(expected_base, &reader, &kernel)
    })();
    let state = state.into_inner();
    match result {
        Ok(value) => {
            if let Some(e) = state.first {
                return Err(failure(e, Some(phase)));
            }
            Ok(phase.retain(value))
        }
        Err(note) => Err(failure(
            state.first.unwrap_or(FormalError::Invalid(note)),
            Some(phase),
        )),
    }
}

/// Canonical root DATA via the same fixed root codec; no accepted qualification.
pub fn canonical_business_index_root_bounded_v1(
    root: &BusinessIndexRootV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, PaidBusinessIndexErrorV1> {
    common.consume_steps(1).map_err(|e| failure(e, None))?;
    let phase = common
        .memory()
        .reserve(128 * 1024, MemoryClass::Ordinary)
        .map_err(|_| failure(bad("business root complete codec phase"), None))?;
    let state = RefCell::new(State {
        common,
        first: None,
    });
    let result = (|| {
        let meter = |n| state.borrow_mut().pay(n);
        let empty = build_empty_hashes_with(Some(&meter))?;
        let kernel = Kernel {
            empty,
            meter: Some(&meter),
        };
        kernel.canonical_root(root)
    })();
    let state = state.into_inner();
    match result {
        Ok(value) => {
            if let Some(e) = state.first {
                return Err(failure(e, Some(phase)));
            }
            Ok(phase.retain(value))
        }
        Err(note) => Err(failure(
            state.first.unwrap_or(FormalError::Invalid(note)),
            Some(phase),
        )),
    }
}

/// Original empty root DATA with the same continuing empty-table kernel.
pub fn empty_business_index_root_bounded_v1(
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<BusinessIndexRootV1>, PaidBusinessIndexErrorV1> {
    common.consume_steps(1).map_err(|e| failure(e, None))?;
    let phase = common
        .memory()
        .reserve(128 * 1024, MemoryClass::Ordinary)
        .map_err(|_| failure(bad("business empty root complete phase"), None))?;
    let state = RefCell::new(State {
        common,
        first: None,
    });
    let result = (|| {
        let meter = |n| state.borrow_mut().pay(n);
        let empty = build_empty_hashes_with(Some(&meter))?;
        meter(64)?;
        Ok::<_, String>(BusinessIndexRootV1 {
            node_root: empty[0].clone(),
            leaf_count: 0,
        })
    })();
    let state = state.into_inner();
    match result {
        Ok(value) => {
            if let Some(e) = state.first {
                return Err(failure(e, Some(phase)));
            }
            Ok(phase.retain(value))
        }
        Err(note) => Err(failure(
            state.first.unwrap_or(FormalError::Invalid(note)),
            Some(phase),
        )),
    }
}

/// Validate only the original root predicates. Retained consumers invoke this
/// before their enclosing digest checks and before performing any node IO.
pub fn validate_business_index_root_bounded_v1(
    root: &BusinessIndexRootV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), PaidBusinessIndexErrorV1> {
    common.consume_steps(1).map_err(|e| failure(e, None))?;
    let phase = common
        .memory()
        .reserve(128 * 1024, MemoryClass::Ordinary)
        .map_err(|_| failure(bad("business root validation phase"), None))?;
    let state = RefCell::new(State {
        common,
        first: None,
    });
    let result = (|| {
        let meter = |n| state.borrow_mut().pay(n);
        let empty = build_empty_hashes_with(Some(&meter))?;
        Kernel {
            empty,
            meter: Some(&meter),
        }
        .validate_root(root)
    })();
    let state = state.into_inner();
    match result {
        Ok(()) => match state.first {
            Some(error) => Err(failure(error, Some(phase))),
            None => Ok(()),
        },
        Err(note) => Err(failure(
            state.first.unwrap_or(FormalError::Invalid(note)),
            Some(phase),
        )),
    }
}
