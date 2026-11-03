//! Explicit derived-dependency profile. v1 canonical bytes remain intact.
use super::*;
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::StateNamespaceV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub const COGNITION_HISTORY_VIEW_V2_MAGIC: &[u8] = b"oasis7.cognition.history-view.v2\0";
pub const COGNITION_HISTORY_VIEW_V2_BYTES: usize = COGNITION_HISTORY_VIEW_V2_MAGIC.len()
    + COGNITION_HISTORY_VIEW_BYTES
    + 3 * 33
    + 1
    + RECORD_ROOT_BYTES;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitionHistoryDerivedRefsV2 {
    pub caches: [Option<[u8; 32]>; 3],
    pub lifecycle: Option<RecordRootRefV1>,
}
impl CognitionHistoryDerivedRefsV2 {
    pub fn validate(&self, view: &CognitionHistoryViewV1) -> Result<(), ValueTreeError> {
        view.validate_shape()?;
        for (i, index) in [0, 2, 3].into_iter().enumerate() {
            if self.caches[i].is_some() != (view.enabled && view.indexes[index].present) {
                return Err(bad());
            }
        }
        if self.lifecycle.is_some() != view.enabled {
            return Err(bad());
        }
        if let Some(root) = &self.lifecycle {
            root.validate_binding(
                &lifecycle_schema(),
                &lifecycle_owner(),
                StateNamespaceV1::Artifacts,
            )
            .map_err(|_| bad())?;
        }
        Ok(())
    }
}
pub fn lifecycle_schema() -> [u8; 32] {
    *blake3::hash(b"oasis7.cognition.lifecycle-index.v2").as_bytes()
}
pub fn lifecycle_owner() -> [u8; 32] {
    *blake3::hash(b"oasis7.cognition.history-derived-owner.v2").as_bytes()
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition History v2 dependencies")
}
pub fn cognition_history_view_v2_bytes(
    view: &CognitionHistoryViewV1,
    refs: &CognitionHistoryDerivedRefsV2,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<[u8; COGNITION_HISTORY_VIEW_V2_BYTES]>, ValueTreeError> {
    let mut charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    refs.validate(view)?;
    let legacy = view.canonical_bytes(memory, &mut cancelled)?;
    let mut bytes = [0u8; COGNITION_HISTORY_VIEW_V2_BYTES];
    let mut at = 0;
    let mut put = |part: &[u8]| {
        bytes[at..at + part.len()].copy_from_slice(part);
        at += part.len();
    };
    put(COGNITION_HISTORY_VIEW_V2_MAGIC);
    put(legacy.as_slice());
    for id in refs.caches {
        put(&[u8::from(id.is_some())]);
        put(&id.unwrap_or([0; 32]));
    }
    put(&[u8::from(refs.lifecycle.is_some())]);
    match &refs.lifecycle {
        Some(root) => put(&root.canonical_bytes().map_err(|_| bad())?),
        None => put(&[0; RECORD_ROOT_BYTES]),
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge
        .split(COGNITION_HISTORY_VIEW_V2_BYTES)
        .map_err(|_| bad())?
        .retain(bytes))
}
pub(super) fn decode_profile(
    bytes: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    mut cancelled: impl FnMut() -> bool,
) -> Result<
    (
        ReservedValue<CognitionHistoryViewV1>,
        Option<CognitionHistoryDerivedRefsV2>,
    ),
    ValueTreeError,
> {
    if bytes.starts_with(COGNITION_HISTORY_VIEW_MAGIC) {
        return CognitionHistoryViewV1::from_canonical_bytes(bytes, memory, class, cancelled)
            .map(|v| (v, None));
    }
    if bytes.len() != COGNITION_HISTORY_VIEW_V2_BYTES
        || !bytes.starts_with(COGNITION_HISTORY_VIEW_V2_MAGIC)
    {
        return Err(bad());
    }
    let begin = COGNITION_HISTORY_VIEW_V2_MAGIC.len();
    let end = begin + COGNITION_HISTORY_VIEW_BYTES;
    let view = CognitionHistoryViewV1::from_canonical_bytes(
        &bytes[begin..end],
        memory,
        class,
        &mut cancelled,
    )?;
    let mut at = end;
    let mut caches = [None; 3];
    for id in &mut caches {
        let tag = bytes[at];
        let value: [u8; 32] = bytes[at + 1..at + 33].try_into().unwrap();
        at += 33;
        *id = match tag {
            0 if value == [0; 32] => None,
            1 => Some(value),
            _ => return Err(bad()),
        };
    }
    let root = &bytes[at + 1..];
    let lifecycle = match bytes[at] {
        0 if root.iter().all(|b| *b == 0) => None,
        1 => Some(RecordRootRefV1::from_canonical_bytes(root).map_err(|_| bad())?),
        _ => return Err(bad()),
    };
    let refs = CognitionHistoryDerivedRefsV2 { caches, lifecycle };
    refs.validate(&view)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok((view, Some(refs)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_formal_protocol::incremental_state::StateRootV1;
    #[test]
    fn enabled_dependency_cohorts_require_journal_and_paired_economy() {
        let index = |present| {
            let mut root = StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap();
            if present {
                root.leaf_count = 2;
                // Public metadata-shape input, not a qualified tree fixture.
                root.node_root = hex::encode([2; 32]);
            }
            CognitionHistoryIndexRefV1 {
                present,
                records: 0,
                root,
            }
        };
        let mut view = CognitionHistoryViewV1 {
            roots: [3; 32],
            enabled: true,
            revision: 1,
            previous_view: None,
            heads: std::array::from_fn(|_| CognitionHistoryHeadV1 {
                sequence: 0,
                digest: None,
            }),
            indexes: [index(true), index(true), index(false), index(false)],
        };
        let root = RecordRootRefV1::new(
            lifecycle_schema(),
            lifecycle_owner(),
            StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
        )
        .unwrap();
        let refs = CognitionHistoryDerivedRefsV2 {
            caches: [Some([1; 32]), None, None],
            lifecycle: Some(root.clone()),
        };
        refs.validate(&view).unwrap();
        let mut bad = refs.clone();
        bad.caches[0] = None;
        assert!(bad.validate(&view).is_err());
        bad = refs.clone();
        bad.caches[1] = Some([2; 32]);
        assert!(bad.validate(&view).is_err());
        bad = refs.clone();
        bad.lifecycle = None;
        assert!(bad.validate(&view).is_err());
        for field in [0, 1, 2] {
            let mut wrong = root.clone();
            match field {
                0 => wrong.schema[0] ^= 1,
                1 => wrong.owner_key[0] ^= 1,
                _ => {
                    wrong = RecordRootRefV1::new(
                        lifecycle_schema(),
                        lifecycle_owner(),
                        StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
                    )
                    .unwrap()
                }
            };
            bad = refs.clone();
            bad.lifecycle = Some(wrong);
            assert!(bad.validate(&view).is_err());
        }
        view.indexes[2] = index(true);
        assert!(refs.validate(&view).is_err());
        view.indexes[3] = index(true);
        let mut all = refs;
        all.caches[1] = Some([2; 32]);
        all.caches[2] = Some([3; 32]);
        all.validate(&view).unwrap();
    }
    #[test]
    fn explicit_v2_preserves_v1_bytes_and_rejects_unknown_padding() {
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let view = CognitionHistoryViewV1 {
            roots: [7; 32],
            enabled: false,
            revision: 0,
            previous_view: None,
            heads: std::array::from_fn(|_| CognitionHistoryHeadV1 {
                sequence: 0,
                digest: None,
            }),
            indexes: std::array::from_fn(|_| CognitionHistoryIndexRefV1 {
                present: false,
                records: 0,
                root: StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
            }),
        };
        let refs = CognitionHistoryDerivedRefsV2 {
            caches: [None; 3],
            lifecycle: None,
        };
        let v1 = view.canonical_bytes(&memory, || false).unwrap();
        let bytes = cognition_history_view_v2_bytes(&view, &refs, &memory, || false).unwrap();
        assert_eq!(
            &bytes[COGNITION_HISTORY_VIEW_V2_MAGIC.len()
                ..COGNITION_HISTORY_VIEW_V2_MAGIC.len() + COGNITION_HISTORY_VIEW_BYTES],
            v1.as_slice()
        );
        assert!(
            CognitionHistoryViewV1::from_canonical_bytes(
                bytes.as_slice(),
                &memory,
                MemoryClass::Recovery,
                || false
            )
            .is_err()
        );
        let (decoded, derived) =
            decode_profile(bytes.as_slice(), &memory, MemoryClass::Recovery, || false).unwrap();
        assert_eq!(*decoded, view);
        assert_eq!(derived, Some(refs));
        let (_, legacy) =
            decode_profile(v1.as_slice(), &memory, MemoryClass::Recovery, || false).unwrap();
        assert!(legacy.is_none());
        for position in [
            0,
            COGNITION_HISTORY_VIEW_V2_MAGIC.len() + COGNITION_HISTORY_VIEW_BYTES,
            COGNITION_HISTORY_VIEW_V2_BYTES - 1,
        ] {
            let mut bad = *bytes;
            bad[position] ^= 1;
            assert!(decode_profile(&bad, &memory, MemoryClass::Recovery, || false).is_err());
        }
        assert!(
            decode_profile(
                &bytes[..bytes.len() - 1],
                &memory,
                MemoryClass::Recovery,
                || false
            )
            .is_err()
        );
    }
}

#[derive(Debug)]
pub enum CognitionHistoryDependencyVisitErrorV2<E> {
    Value(ValueTreeError),
    Visitor(E),
}
pub enum CognitionHistoryDirectDependencyV2<'a> {
    Index(
        CognitionHistoryFieldV1,
        &'a oasis7_formal_protocol::incremental_state::StateRootV1,
    ),
    HeadCache(
        crate::incremental_cognition_history_hash_outboard::CognitionHeadHashV1,
        [u8; 32],
    ),
    Lifecycle(&'a oasis7_formal_protocol::incremental_state::StateRootV1),
}
/// Enumerate authenticated direct graph edges. The visitor must then traverse
/// value pages and typed index rows/original locators; this is not a GC receipt.
pub fn visit_cognition_history_direct_dependencies_v2<E>(
    selected: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    memory: &MemoryBudget,
    edges_remaining: &mut u64,
    mut visitor: impl FnMut(CognitionHistoryDirectDependencyV2<'_>) -> Result<(), E>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(), CognitionHistoryDependencyVisitErrorV2<E>> {
    use CognitionHistoryDependencyVisitErrorV2::*;
    if cancelled() {
        return Err(Value(ValueTreeError::Cancelled));
    }
    if !selected.uses_budget(memory) || selected.reserved_bytes() < 16 << 10 {
        return Err(Value(bad()));
    }
    let refs = selected.derived().ok_or_else(|| Value(bad()))?;
    refs.validate(selected.manifest()).map_err(Value)?;
    let mut visit = |edge| {
        if cancelled() {
            return Err(Value(ValueTreeError::Cancelled));
        }
        *edges_remaining = edges_remaining.checked_sub(1).ok_or_else(|| {
            Value(ValueTreeError::ResourceLimited(
                "Cognition History dependency edges",
            ))
        })?;
        visitor(edge).map_err(Visitor)
    };
    for field in CognitionHistoryFieldV1::ALL {
        let index = selected.manifest().index_ref(field);
        if index.present {
            visit(CognitionHistoryDirectDependencyV2::Index(
                field,
                &index.root,
            ))?;
        }
    }
    for (i, kind) in crate::incremental_cognition_history_hash_outboard::CognitionHeadHashV1::ALL
        .into_iter()
        .enumerate()
    {
        if let Some(id) = refs.caches[i] {
            visit(CognitionHistoryDirectDependencyV2::HeadCache(kind, id))?;
        }
    }
    if let Some(root) = &refs.lifecycle {
        let root = root
            .validate_binding(
                &lifecycle_schema(),
                &lifecycle_owner(),
                StateNamespaceV1::Artifacts,
            )
            .map_err(|_| Value(bad()))?;
        visit(CognitionHistoryDirectDependencyV2::Lifecycle(root))?;
    }
    if cancelled() {
        return Err(Value(ValueTreeError::Cancelled));
    }
    Ok(())
}
