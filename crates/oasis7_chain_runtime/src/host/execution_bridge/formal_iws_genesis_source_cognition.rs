//! Fresh Cognition bootstrap from the actual borrowed original envelope.
//! This narrow GEN source refuses retained history it cannot authenticate.
use super::*;
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StateRootV1},
};
use oasis7_world_state::{
    incremental_cognition_history::*, incremental_cognition_history_hash_outboard::*,
};

struct HashWriter(blake3::Hasher);
impl std::io::Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn prepare(
    world: &World,
    roots: [u8; 32],
    pages: &mut Pages<'_>,
    memory: &MemoryBudget,
    work: &mut GenesisSourceHistoryWorkV1,
    stop: &AtomicBool,
) -> Result<ReservedValue<[u8; COGNITION_HISTORY_VIEW_V3_BYTES]>> {
    let source = world
        .borrow_iws_cognition_history_source_v1()
        .map_err(ValueTreeError::Invalid)?;
    let enabled = source.enabled();
    if let CognitionHistorySourceRefV1::Enabled(envelope) = source {
        if envelope
            .journal_field("head_seq")
            .and_then(serde_json::Value::as_u64)
            != Some(0)
            || envelope
                .journal_field("head_digest")
                .and_then(serde_json::Value::as_str)
                != Some("")
            || envelope.field("cognition_economy").is_some()
        {
            return Err(PageStoreError::Corrupt(
                "GEN retained Cognition heads require their actual source migration",
            )
            .into());
        }
    }
    let mut indexes = Vec::new();
    indexes
        .try_reserve_exact(4)
        .map_err(|_| PageStoreError::ResourceLimited("GEN Cognition index carrier"))?;
    if indexes.capacity() != 4 {
        return Err(PageStoreError::Corrupt("GEN Cognition index capacity").into());
    }
    for field in CognitionHistoryFieldV1::ALL {
        let actual =
            CognitionHistoryCollectionRefV1::new(field, source).map_err(ValueTreeError::Invalid)?;
        if !actual.is_empty() {
            return Err(PageStoreError::Corrupt(
                "GEN retained Cognition occurrences require their actual source migration",
            )
            .into());
        }
        let root = if actual.present() {
            prepare_cognition_history_index_v1(
                field,
                0,
                std::iter::empty::<std::result::Result<CognitionHistoryIndexRowV1, ValueTreeError>>(
                ),
                &mut MemberRecordWriteContext {
                    sink: &mut *pages,
                    memory,
                    io: &mut work.values,
                    key_work: &mut work.keys,
                    limits: work.limits,
                    cancelled: || stop.load(Ordering::Acquire),
                },
                &mut work.cognition,
            )
            .map_err(member)?
        } else {
            StateRootV1::empty(StateNamespaceV1::Artifacts)
                .map_err(|_| PageStoreError::Corrupt("GEN Cognition absent index"))?
        };
        indexes.push(CognitionHistoryIndexRefV1 {
            present: actual.present(),
            records: 0,
            root,
        });
    }
    let (cache, cache_digest, lifecycle) = if enabled {
        let cache = CognitionHeadHashBuilderV1::new(memory, MemoryClass::Ordinary, || {
            stop.load(Ordering::Acquire)
        })?
        .finish(|| stop.load(Ordering::Acquire))?;
        let cache_id = persist_cognition_head_cache_v1(
            &cache,
            pages,
            memory,
            &mut work.values,
            &mut work.hashes,
            || stop.load(Ordering::Acquire),
        )?;
        let mut hash = HashWriter(blake3::Hasher::new());
        cache.write_canonical_cache_v1(&mut hash, &mut work.hashes, || {
            stop.load(Ordering::Acquire)
        })?;
        let lifecycle = RecordRootRefV1::new(
            lifecycle_schema(),
            lifecycle_owner(),
            StateRootV1::empty(StateNamespaceV1::Artifacts)
                .map_err(|_| PageStoreError::Corrupt("GEN Cognition lifecycle empty root"))?,
        )
        .map_err(|_| PageStoreError::Corrupt("GEN Cognition lifecycle root"))?;
        (
            Some(cache_id),
            Some(*hash.0.finalize().as_bytes()),
            Some(lifecycle),
        )
    } else {
        (None, None, None)
    };
    let view = CognitionHistoryViewV1 {
        roots,
        enabled,
        revision: 0,
        previous_view: None,
        heads: std::array::from_fn(|_| CognitionHistoryHeadV1 {
            sequence: 0,
            digest: None,
        }),
        indexes: indexes
            .try_into()
            .map_err(|_| PageStoreError::Corrupt("GEN Cognition index arity"))?,
    };
    let derived = CognitionHistoryDerivedRefsV2 {
        caches: [cache, None, None],
        lifecycle,
    };
    let source = CognitionHistorySourceSummaryV3 {
        mode: CognitionHistorySourceModeV3::Bootstrap,
        cache_state_digests: [cache_digest, None, None],
        last: std::array::from_fn(|_| None),
        lifecycle_keys: 0,
        lifecycle_journal_records: 0,
    };
    cognition_history_view_v3_bytes(&view, &derived, &source, memory, || {
        stop.load(Ordering::Acquire)
    })
    .map_err(Error::Value)
}
