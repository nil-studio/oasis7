use super::*;
use crate::{
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer, io},
};
use oasis7_formal_protocol::incremental_value::ValueReadContext;
fn work() -> RuntimeHistoryIndexWorkV1 {
    RuntimeHistoryIndexWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 1 << 20,
    }
}
fn row(n: u64) -> RuntimeHistoryIndexRowV1 {
    RuntimeHistoryIndexRowV1 {
        location: FactLocationV1 {
            height: n / 8,
            ordinal: n % 8,
            original: *blake3::hash(&n.to_be_bytes()).as_bytes(),
        },
        encoded_bytes: 128 + n,
    }
}
fn install(pages: &Pages, memory: &MemoryBudget, count: u64) -> RuntimeHistoryWindowV1 {
    let root = prepare_runtime_history_index_v1(
        RuntimeHistoryLaneV1::Journal,
        count,
        (0..count).map(|n| Ok(row(n))),
        &mut MemberRecordWriteContext {
            sink: &mut Writer(pages),
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 8192,
                record_bytes: 8192,
            },
            cancelled: || false,
        },
        &mut work(),
    )
    .unwrap();
    RuntimeHistoryWindowV1 {
        root,
        total: count,
        start: 0,
        count,
    }
}
fn context<'a>(
    pages: &'a Pages,
    memory: &'a MemoryBudget,
    window: &'a RuntimeHistoryWindowV1,
    quota: &'a mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root: &window.root,
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || None,
        cancelled: || false,
    }
}
#[test]
fn runtime_history_index_real_append_eviction_and_truncation_preserve_immutable_prefix() {
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let pages = Pages::new(&memory);
    let before = install(&pages, &memory, 32);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = context(&pages, &memory, &before, &mut quota, &mut keys);
    let parent = qualify_runtime_history_index_v1(
        RuntimeHistoryLaneV1::Journal,
        &before,
        &mut read,
        &mut work(),
    )
    .unwrap();
    let evicted = select_runtime_history_window_v1(&parent, 16, 16, &memory, || false).unwrap();
    assert_eq!(evicted.window().root, before.root);
    let truncated = select_runtime_history_window_v1(&parent, 0, 16, &memory, || false).unwrap();
    let mut write_io = io();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut writer = Writer(&pages);
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 8192,
            record_bytes: 8192,
        },
        cancelled: || false,
    };
    assert!(
        prepare_runtime_history_index_append_v1(
            &truncated,
            row(32),
            &mut read,
            &mut write,
            &mut work()
        )
        .is_err()
    );
    let after = prepare_runtime_history_index_append_v1(
        &evicted,
        row(32),
        &mut read,
        &mut write,
        &mut work(),
    )
    .unwrap();
    assert_eq!(after.window().start, 16);
    assert_eq!(after.window().count, 17);
    assert_eq!(after.window().total, 33);
    assert_eq!(after.lineage(), Some((&before.root, 32)));
    assert_eq!(
        read_runtime_history_index_point_v1(
            RuntimeHistoryLaneV1::Journal,
            after.window(),
            32,
            &mut read,
            &mut work()
        )
        .unwrap(),
        row(32)
    );
    assert_eq!(
        read_runtime_history_index_point_v1(
            RuntimeHistoryLaneV1::Journal,
            &before,
            31,
            &mut read,
            &mut work()
        )
        .unwrap(),
        row(31)
    );
    let reread = qualify_runtime_history_index_v1(
        RuntimeHistoryLaneV1::Journal,
        after.window(),
        &mut read,
        &mut work(),
    )
    .unwrap();
    assert_eq!(reread.window(), after.window());
    assert!(select_runtime_history_window_v1(&evicted, 0, 1, &memory, || false).is_err());
    assert!(select_runtime_history_window_v1(&parent, u64::MAX, 2, &memory, || false).is_err());
}
#[test]
fn runtime_history_index_finite_work_and_cross_lane_keys_fail_closed() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let pages = Pages::new(&memory);
    let window = install(&pages, &memory, 2);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = context(&pages, &memory, &window, &mut quota, &mut keys);
    assert!(
        qualify_runtime_history_index_v1(
            RuntimeHistoryLaneV1::Tick,
            &window,
            &mut read,
            &mut work()
        )
        .is_err()
    );
    let mut limited = work();
    limited.cells_remaining = 1;
    assert!(
        qualify_runtime_history_index_v1(
            RuntimeHistoryLaneV1::Journal,
            &window,
            &mut read,
            &mut limited
        )
        .is_err()
    );
    assert!(
        read_runtime_history_index_point_v1(
            RuntimeHistoryLaneV1::Journal,
            &window,
            2,
            &mut read,
            &mut work()
        )
        .is_err()
    );
    let q = qualify_runtime_history_index_v1(
        RuntimeHistoryLaneV1::Journal,
        &window,
        &mut read,
        &mut work(),
    )
    .unwrap();
    assert!(select_runtime_history_window_v1(&q, 0, 2, &memory, || true).is_err());
    assert!(readmit_runtime_history_index_ordinary_v1(&q, &window, &memory, || false).is_err());
}
#[test]
fn runtime_history_capsule_optional_zero_and_window_bounds_preserve_exact_framing() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let pages = Pages::new(&memory);
    let window = install(&pages, &memory, 2);
    let manifest = RuntimeHistoryManifestV1 {
        roots: [7; 32],
        previous_view: Some([0; 32]),
        lanes: std::array::from_fn(|_| window.clone()),
        max_snapshots: 2,
        archived_tick_count: 0,
    };
    let bytes = manifest.canonical_bytes().unwrap();
    assert_eq!(
        RuntimeHistoryManifestV1::from_canonical_bytes(&bytes).unwrap(),
        manifest
    );
    let mut absent = manifest.clone();
    absent.previous_view = None;
    assert_ne!(absent.canonical_bytes().unwrap(), bytes);
    let mut forged = absent.canonical_bytes().unwrap();
    forged[RUNTIME_HISTORY_MAGIC_V1.len() + 32 + 1] = 1;
    assert!(RuntimeHistoryManifestV1::from_canonical_bytes(&forged).is_err());
    let mut invalid = manifest.clone();
    invalid.lanes[0].start = u64::MAX;
    invalid.lanes[0].count = 1;
    assert!(invalid.canonical_bytes().is_err());
    assert!(RuntimeHistoryManifestV1::from_canonical_bytes(&bytes[..bytes.len() - 1]).is_err());
}
