//! The history writer is tested with genuine original Qualified history data.
//! These lower tests issue no GEN permit, receipt, acceptance or cache setter.
use super::super::super::tests::{with_history_read_fixture, with_store_for_bounded_index_fixture};
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(100_000_000, 2u64 << 30).unwrap()
}
#[test]
fn original_first_history_node_is_really_staged_retained_and_read_by_old_codec_before_oracle_put() {
    with_history_read_fixture(|source, ids, _| {
        let original: HistoryNode = source.get(&ids[0]).unwrap();
        let wire = serde_cbor::to_vec(&original).unwrap();
        let expected = blake3::hash(&wire).to_hex().to_string();
        with_store_for_bounded_index_fixture(|target, _| {
            let before = target.checkpoint_bytes().unwrap();
            let memory = memory();
            let work = work();
            assert!(target.raw_object(&expected).unwrap().is_none());
            let directory = target.path.join("formal-objects-v1");
            let guard =
                super::super::stage_io::guard_initialized(&directory, &memory, &work, &mut || {
                    false
                })
                .unwrap();
            let node = super::super::stage_io::stage_history_node_bounded_v1(
                &guard,
                &original,
                &memory,
                &work,
                &mut || false,
            )
            .unwrap();
            assert_eq!(node.id(), expected);
            let retained=crate::controlled_authority::formal::physical::roots::append_staged_history_bounded_v1(&node,&memory,&work,&mut ||false).unwrap();
            let recovered: HistoryNode = target.get(node.id()).unwrap();
            assert_eq!(serde_cbor::to_vec(&recovered).unwrap(), wire);
            assert_eq!(target.checkpoint_bytes().unwrap(), before);
            assert!(crate::controlled_authority::formal::physical::pin(&directory, true).is_err());
            drop(retained);
            drop(node);
            drop(guard);
            assert_eq!(memory.used(), 0);
            // Independent OLD writer only after NEW staged+retained bytes exist.
            assert_eq!(target.put(&original).unwrap(), expected);
            assert!(crate::controlled_authority::formal::physical::pin(&directory, true).is_ok());
        });
    });
}
#[test]
fn history_retention_requires_original_same_work_and_pool_and_cancel_releases_every_fee() {
    with_history_read_fixture(|source, ids, _| {
        let original: HistoryNode = source.get(&ids[0]).unwrap();
        with_store_for_bounded_index_fixture(|target, _| {
            let memory = memory();
            let work = work();
            let other = MemoryBudget::new(256 << 20, 0).unwrap();
            let foreign = super::tests::work();
            let before = target.checkpoint_bytes().unwrap();
            let directory = target.path.join("formal-objects-v1");
            let guard =
                super::super::stage_io::guard_initialized(&directory, &memory, &work, &mut || {
                    false
                })
                .unwrap();
            assert!(
                super::super::stage_io::stage_history_node_bounded_v1(
                    &guard,
                    &original,
                    &other,
                    &work,
                    &mut || false
                )
                .is_err()
            );
            assert_eq!(other.used(), 0);
            assert!(
                super::super::stage_io::stage_history_node_bounded_v1(
                    &guard,
                    &original,
                    &memory,
                    &work,
                    &mut || true
                )
                .is_err()
            );
            let node = super::super::stage_io::stage_history_node_bounded_v1(
                &guard,
                &original,
                &memory,
                &work,
                &mut || false,
            )
            .unwrap();
            assert!(crate::controlled_authority::formal::physical::roots::append_staged_history_bounded_v1(&node,&memory,&foreign,&mut ||false).is_err());
            drop(node);
            drop(guard);
            assert_eq!(memory.used(), 0);
            assert_eq!(target.checkpoint_bytes().unwrap(), before);
        });
    });
}
