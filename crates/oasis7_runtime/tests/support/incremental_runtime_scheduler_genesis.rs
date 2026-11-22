use super::*;
use std::cell::Cell;
#[test]
fn scheduler_genesis_budgets_sink_failure_and_last_emit_cancel_remain_provisional() {
    for mode in 0..6 {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("genesis"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let denied = MemoryBudget::new(1 << 20, 1 << 20).unwrap();
        let mut writer = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let stopped = Cell::new(false);
        let emitted = Cell::new(0);
        let mut routing = routing_source();
        if mode == 5 {
            routing.oldest_overdue_ticks = None;
        }
        let source = RuntimeSchedulerControlsRefV1 {
            cursor: if mode == 5 { None } else { Some("") },
            limits: &limits_source(),
            backpressure: &stats_source(),
            routing: &routing,
        };
        let mut work = RuntimeSchedulerControlsWorkV1 {
            cells_remaining: match mode {
                0 => 0,
                1 => 23,
                _ => 24,
            },
            max_pending_node_bytes: 2 << 20,
        };
        let result = prepare_runtime_scheduler_controls_genesis_v1(
            &source,
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory: if mode == 4 { &denied } else { &memory },
                io: &mut io,
                key_work: &mut keys,
                limits: limits(),
                cancelled: || stopped.get(),
            },
            &mut work,
            &mut |_| {
                emitted.set(emitted.get() + 1);
                assert!(emitted.get() <= 16);
                if mode == 2 && emitted.get() == 8 {
                    return Err(ValueTreeError::Io(std::io::Error::other(
                        "scheduler genesis sink prefix",
                    )));
                }
                if mode == 3 && emitted.get() == 16 {
                    stopped.set(true);
                }
                Ok(())
            },
        );
        match mode {
            0 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(_)))
                ));
                assert_eq!(emitted.get(), 0);
            }
            1 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(_)))
                ));
                assert_eq!(emitted.get(), 15);
            }
            2 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::Io(_)))
                ));
                assert_eq!(emitted.get(), 8);
            }
            3 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::Cancelled))
                ));
                assert_eq!(emitted.get(), 16);
            }
            4 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::Memory(_)))
                ));
                assert_eq!(emitted.get(), 0);
            }
            5 => {
                result.unwrap();
                assert_eq!(emitted.get(), 16);
                assert_eq!(work.cells_remaining, 0);
            }
            _ => unreachable!(),
        }
    }
}
