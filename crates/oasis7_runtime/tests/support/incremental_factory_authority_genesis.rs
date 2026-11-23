use super::*;
use std::cell::Cell;
#[test]
fn factory_authority_genesis_limits_and_final_emit_cancellation_never_complete() {
    for mode in 0..8 {
        let directory = Directory::new();
        let memory = MemoryBudget::new(if mode == 4 { 8192 } else { 64 << 20 }, 0).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("producer"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        // Retain the original100k physical-read pressure case separately.
        // The final-emit case uses this module's explicit complete-operation scope.
        let mut backend = store.value_session(if mode == 7 {
            physical()
        } else {
            physical_scope()
        });
        let mut io = logical();
        if mode == 3 {
            io.nodes_remaining = 0;
        }
        let mut keys = SemanticKeyBudget {
            bytes_remaining: if mode == 2 { 0 } else { 1 << 20 },
        };
        let emitted = Cell::new(0usize);
        let stop = Cell::new(mode == 0);
        let mut work = FactoryAuthorityWorkV1 {
            cells_remaining: if mode == 1 { 0 } else { 101 },
            max_pending_node_bytes: 2 << 20,
        };
        let result = prepare_factory_authority_genesis_v1(
            &source(),
            &mut MemberRecordWriteContext {
                sink: &mut backend,
                memory: &memory,
                io: &mut io,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 1024,
                    record_bytes: 4096,
                },
                cancelled: || stop.get(),
            },
            &mut work,
            &mut |_| {
                emitted.set(emitted.get() + 1);
                if mode == 5 {
                    return Err(ValueTreeError::Io(std::io::Error::other(
                        "provisional emit",
                    )));
                }
                if mode == 6 && emitted.get() == 8 {
                    stop.set(true);
                }
                Ok(())
            },
        );
        eprintln!(
            "factory GEN mode={mode} emitted={} result={result:?}",
            emitted.get()
        );
        match mode {
            0 | 6 => assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::Cancelled))
            )),
            1 | 3 => assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(_)))
            )),
            2 => assert!(matches!(
                result,
                Err(MemberRecordError::Key(SemanticKeyError::ResourceLimited))
            )),
            4 => assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::Memory(_)))
            )),
            7 => assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(
                    "physical read allowance"
                )))
            )),
            5 => assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::Io(_)))
            )),
            _ => unreachable!(),
        }
        assert_eq!(
            emitted.get(),
            if mode == 5 || mode == 2 {
                1
            } else if mode == 7 {
                7
            } else if mode == 6 {
                8
            } else {
                0
            }
        );
    }
}
