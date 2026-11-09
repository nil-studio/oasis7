use super::*;
use std::cell::Cell;
#[test]
fn main_token_policy_genesis_limits_and_final_emit_cancellation_never_complete() {
    for mode in 0..7 {
        let directory = Directory::new();
        let memory = MemoryBudget::new(if mode == 4 { 8192 } else { 64 << 20 }, 0).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("producer"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let mut backend = store.value_session(physical());
        let mut io = logical();
        if mode == 3 {
            io.nodes_remaining = 0;
        }
        let mut keys = SemanticKeyBudget {
            bytes_remaining: if mode == 2 { 0 } else { 1 << 20 },
        };
        let emitted = Cell::new(0usize);
        let stop = Cell::new(mode == 0);
        let mut work = MainTokenPolicyWorkV1 {
            cells_remaining: if mode == 1 { 0 } else { 101 },
            max_pending_node_bytes: 2 << 20,
        };
        let result = prepare_main_token_policy_genesis_v1(
            &source(),
            &mut MemberRecordWriteContext {
                sink: &mut backend,
                memory: &memory,
                io: &mut io,
                key_work: &mut keys,
                limits: limits(),
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
                if mode == 6 && emitted.get() == 16 {
                    stop.set(true);
                }
                Ok(())
            },
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
            } else if mode == 6 {
                16
            } else {
                0
            }
        );
    }
}
