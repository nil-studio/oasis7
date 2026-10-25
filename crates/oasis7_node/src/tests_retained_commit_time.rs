use super::*;

#[derive(Default)]
struct DurableBeforeAckHook {
    original: Option<NodeExecutionCommitContext>,
    recovered: usize,
    executions: usize,
}
impl NodeExecutionHook for DurableBeforeAckHook {
    fn retained_local_commit_time(
        &mut self,
        context: &NodeExecutionCommitContext,
    ) -> Result<Option<i64>, String> {
        let Some(original) = &self.original else {
            return Ok(None);
        };
        if context.height != original.height {
            return Ok(None);
        }
        assert_eq!(context.node_block_hash, original.node_block_hash);
        assert_eq!(context.slot, original.slot);
        self.recovered += 1;
        Ok(Some(original.committed_at_unix_ms))
    }
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        if self.original.is_none() {
            self.original = Some(context);
            self.executions += 1;
            return Err("injected response loss after durable execution".into());
        }
        let original = self.original.as_ref().unwrap();
        if context.height == original.height {
            assert_eq!(context.committed_at_unix_ms, original.committed_at_unix_ms);
        } else {
            self.executions += 1;
        }
        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: format!("execution-{}", context.height),
            execution_state_root: format!("state-{}", context.height),
        })
    }
}

#[test]
fn local_tick_recovers_durable_commit_time_before_node_ack_and_advances() {
    let config = NodeConfig::new("node-a", "retained-local-time", NodeRole::Sequencer).unwrap();
    let mut engine = PosNodeEngine::new(&config).unwrap();
    let mut hook = DurableBeforeAckHook::default();
    let first = engine.tick(
        &config.node_id,
        &config.world_id,
        1_000,
        None,
        None,
        None,
        None,
        vec![],
        Some(&mut hook),
    );
    assert!(
        first
            .unwrap_err()
            .to_string()
            .contains("injected response loss")
    );
    assert_eq!(engine.committed_height, 0);
    engine
        .tick(
            &config.node_id,
            &config.world_id,
            61_000,
            None,
            None,
            None,
            None,
            vec![],
            Some(&mut hook),
        )
        .expect("retry original at later wall time");
    assert_eq!(engine.committed_height, 1);
    assert_eq!(engine.last_committed_at_ms, Some(1_000));
    assert_eq!(hook.executions, 1);
    assert_eq!(hook.recovered, 1);
    engine
        .tick(
            &config.node_id,
            &config.world_id,
            62_000,
            None,
            None,
            None,
            None,
            vec![],
            Some(&mut hook),
        )
        .expect("successor progresses");
    assert_eq!(engine.committed_height, 2);
    assert_eq!(hook.executions, 2);
}
