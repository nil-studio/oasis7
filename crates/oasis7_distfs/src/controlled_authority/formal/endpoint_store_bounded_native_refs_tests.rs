use super::super::super::tests::{
    NativeDraftFixtureV1, with_history_read_fixture, with_native_draft_fixture,
};
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(8_000_000, 512 << 20).unwrap()
}
#[test]
fn genuine_qualified_history_index_and_entry_match_original_refs_without_publication() {
    with_history_read_fixture(|store, ids, _| {
        assert!(!ids.is_empty());
        let before = store.checkpoint_bytes().unwrap();
        let memory = memory();
        let meter = work();
        let mut no = || false;
        let draft = store
            .prepare_native_checkpoint_draft_bounded(&mut meter.bounded_context(&memory, &mut no))
            .unwrap();
        store.validate_refs(draft.body()).unwrap();
        let paid =
            validate_native_checkpoint_refs_bounded_v1(draft, &memory, &meter, &mut no).unwrap();
        assert!(paid.uses_budget(&memory));
        assert_eq!(paid.reserved_bytes(), memory.used());
        assert_eq!(store.checkpoint_bytes().unwrap(), before);
        drop(paid);
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn genuine_initial_empty_refs_are_metadata_only_and_leave_hot_unchanged() {
    with_native_draft_fixture(NativeDraftFixtureV1::Plain, |store, _| {
        let memory = memory();
        let meter = work();
        let mut no = || false;
        let before = store.checkpoint_bytes().unwrap();
        let draft = store
            .prepare_native_checkpoint_draft_bounded(&mut meter.bounded_context(&memory, &mut no))
            .unwrap();
        store.validate_refs(draft.body()).unwrap();
        let paid =
            validate_native_checkpoint_refs_bounded_v1(draft, &memory, &meter, &mut no).unwrap();
        assert_eq!(paid.reserved_bytes(), memory.used());
        drop(paid);
        assert_eq!(memory.used(), 0);
        assert_eq!(store.checkpoint_bytes().unwrap(), before);
    });
}
#[test]
fn foreign_pool_core_cancellation_and_exhaustion_consume_real_draft() {
    for mode in 0..4 {
        with_history_read_fixture(|store, _, _| {
            let memory = memory();
            let meter = work();
            let mut no = || false;
            let draft = store
                .prepare_native_checkpoint_draft_bounded(
                    &mut meter.bounded_context(&memory, &mut no),
                )
                .unwrap();
            let other = MemoryBudget::new(256 << 20, 0).unwrap();
            let other_work = work();
            if mode == 3 {
                meter
                    .consume_steps(meter.remaining_steps(), &mut no)
                    .unwrap();
            }
            let error = validate_native_checkpoint_refs_bounded_v1(
                draft,
                if mode == 0 { &other } else { &memory },
                if mode == 1 { &other_work } else { &meter },
                &mut || mode == 2,
            )
            .unwrap_err();
            if mode == 2 {
                assert!(error.to_string().contains("cancelled"));
            }
            if mode == 3 {
                assert!(error.to_string().contains("steps"));
            }
            assert_eq!(memory.used(), 0);
            assert_eq!(other.used(), 0);
        });
    }
}

#[test]
fn original_kernel_imported_position_short_circuits_record_hash_then_keeps_read_order() {
    use super::super::native_refs::{NativeReferenceValidationOps, validate_refs_with};
    type Local = super::super::super::incremental::local_closure::LocalDataClosureRecordV1;
    struct Trace<'a> {
        store: &'a Store,
        events: Vec<&'static str>,
    }
    impl NativeReferenceValidationOps for Trace<'_> {
        type Error = FormalError;
        type History = HistoryNode;
        type Closure = Local;
        type Request = FormalRequestStateV1;
        type RecordId = String;
        fn history(&mut self, id: &str) -> Result<HistoryNode, FormalError> {
            self.events.push("history");
            self.store.get(id)
        }
        fn history_node(v: &HistoryNode) -> &HistoryNode {
            v
        }
        fn closure(&mut self, id: &str) -> Result<Local, FormalError> {
            self.events.push("closure");
            self.store.get(id)
        }
        fn closure_record(v: &Local) -> &Local {
            v
        }
        fn request(&mut self, id: &str) -> Result<FormalRequestStateV1, FormalError> {
            self.events.push("request");
            self.store.get(id)
        }
        fn request_state(v: &FormalRequestStateV1) -> &FormalRequestStateV1 {
            v
        }
        fn index(&mut self, id: &str) -> Result<(), FormalError> {
            self.events.push("index");
            let _: Index = self.store.get(id)?;
            Ok(())
        }
        fn record_id(&mut self, r: &CanonicalRecordV1) -> Result<String, FormalError> {
            self.events.push("record_id");
            r.record_id()
        }
        fn id(v: &String) -> &str {
            v
        }
        fn before_hash_reference(&mut self, _id: &str) -> Result<(), FormalError> {
            Ok(())
        }
        fn before_scope(&mut self, _r: &Local, _b: &HotBody) -> Result<(), FormalError> {
            Ok(())
        }
        fn compare(&mut self, _: usize) -> Result<(), FormalError> {
            Ok(())
        }
    }
    with_history_read_fixture(|store, ids, _| {
        let node: HistoryNode = store.get(&ids[0]).unwrap();
        let entry: FormalRequestStateV1 = store.get(&node.entry).unwrap();
        let FormalRequestStateV1::Qualified { proof, .. } = entry else {
            panic!("genuine Qualified source")
        };
        let mut body = store.body.clone();
        body.imported_head_node = Some(ids[0].clone());
        body.base_parent = Some(CheckpointParent {
            record: proof.decision.record.clone(),
            finalizing_authority_id: proof.decision.record.original_authority_id.clone(),
        });
        body.pending = Some(node.entry.clone());
        let mut wrong = body.clone();
        wrong.base_parent.as_mut().unwrap().record.position += 1;
        let mut trace = Trace {
            store,
            events: vec![],
        };
        let error = validate_refs_with(&wrong, &mut trace).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("formal imported archive canonical cut")
        );
        assert_eq!(trace.events, ["history"]);
        let mut trace = Trace {
            store,
            events: vec![],
        };
        validate_refs_with(&body, &mut trace).unwrap();
        assert_eq!(
            trace.events,
            [
                "history",
                "record_id",
                "index",
                "request",
                "history",
                "request"
            ]
        );
        // This cloned body tests the shared decision core only. It is never
        // signed/installed and cannot construct the private validated owner.
    });
}
