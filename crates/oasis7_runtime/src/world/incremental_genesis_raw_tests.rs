use super::*;
use oasis7_formal_protocol::incremental_value::ValueIoBudget;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default)]
struct Pages(BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>);
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        _: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let row = self
            .0
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("GEN test missing page"))?;
        if row.len() > max || !row.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("GEN test page admission"));
        }
        Ok(row)
    }
}
impl SchedulePageWriter for Pages {}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}
fn work() -> RuntimeRawGenesisWorkV1 {
    RuntimeRawGenesisWorkV1 {
        writes_remaining: 100,
        agents_remaining: 100,
        instances_remaining: 100,
        instance_count_rows_remaining: 100,
        schedule_entries_remaining: 100,
        module_states: ModuleStateContentBudgetV1 {
            states_remaining: 100,
            logical_bytes_remaining: 1 << 20,
            max_state_bytes: 1 << 20,
        },
        max_scalar_bytes: 1 << 20,
    }
}
#[test]
fn runtime_raw_genesis_composes_original_rows_and_consumes_shared_quotas() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let mut world = World::new();
    world.state.agents.clear();
    world.state.module_instances.clear();
    world.state.module_states.clear();
    world.module_tick_schedule.clear();
    world
        .state
        .module_states
        .insert("module-original".into(), vec![0, 255, 0]);
    world
        .module_tick_schedule
        .insert("instance-original".into(), u64::MAX);
    let mut pages = Pages::default();
    let mut quota = io();
    let mut limits = work();
    let mut rows = BTreeMap::new();
    world
        .prepare_iws_runtime_raw_genesis_v1(
            &mut BodyMutationContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut quota,
                cancelled: || false,
            },
            &mut limits,
            &mut |_, _, _| panic!("empty mailbox"),
            &mut |row| {
                assert!(row.uses_budget(&memory));
                assert_eq!(row.mutation.before, None);
                assert!(
                    rows.insert(
                        (row.namespace as u8, row.mutation.key.clone()),
                        row.mutation.after
                    )
                    .is_none()
                );
                Ok(())
            },
        )
        .unwrap();
    // Four allocator pairs, one opaque module, wake+index, and instance total.
    assert_eq!(rows.len(), 8);
    assert_eq!(limits.writes_remaining, 92);
    assert_eq!(limits.module_states.states_remaining, 99);
    assert_eq!(limits.module_states.logical_bytes_remaining, (1 << 20) - 3);
    assert_eq!(limits.schedule_entries_remaining, 99);
    assert!(quota.write_bytes_remaining < 64 << 20);
}
#[test]
fn runtime_raw_genesis_stops_on_sink_error_and_shared_row_exhaustion() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let world = World::new();
    for fail_sink in [false, true] {
        let mut pages = Pages::default();
        let mut quota = io();
        let mut limits = work();
        if !fail_sink {
            limits.writes_remaining = 1;
        }
        let mut emitted = 0;
        let result = world.prepare_iws_runtime_raw_genesis_v1(
            &mut BodyMutationContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut quota,
                cancelled: || false,
            },
            &mut limits,
            &mut |_, _, _| panic!("no mailbox before allocator failure"),
            &mut |_| {
                emitted += 1;
                if fail_sink {
                    Err(ValueTreeError::Invalid("injected sink failure"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(emitted, 1);
        assert_eq!(limits.module_states.states_remaining, 100);
        assert_eq!(limits.schedule_entries_remaining, 100);
    }
}
