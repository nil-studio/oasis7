//! Real Runner + session adapter + protocol encoder; transport is unpaid scripted I/O.
use super::*;
use oasis7_agent_api::ModelProviderHistory;
use oasis7_agent_harness::{
    NativeTaskContext, NativeTaskControl, NativeTaskHost, NativeTaskRecovery, NativeTaskRunConfig,
    NativeTaskRunner, NativeTaskStopReason, NativeTaskTool, NativeTaskToolResult,
};
use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};

struct HistoryHost {
    spent: AtomicU32,
    reads: AtomicU32,
    effects: AtomicU32,
}
impl HistoryHost {
    fn fence() -> oasis7_agent_api::game::GameTaskFenceV1 {
        serde_json::from_value(json!({"world_id":"world","genesis_digest":"g","branch_id":"main","reorg_epoch":"0",
            "agent_id":"agent","owner_binding":"owner","owner_generation":"1","owner_public_key":"a".repeat(64),
            "task_id":"task","task_revision":"1","task_status":"active","latest_goal_revision":"0","active_goal_revision":"0",
            "active_goal_reference":null,"executor_epoch":"1","executor_binding":{"executor_id":"executor","executor_public_key":"b".repeat(64),"delegation_generation":"1","executor_epoch":"1"}})).unwrap()
    }
}
impl NativeTaskHost for HistoryHost {
    fn initialize_run(&self, _: &NativeTaskRunConfig) -> Result<u32, String> {
        Ok(self.spent.load(Ordering::SeqCst))
    }
    fn reserve_model_call(&self, _: &NativeTaskRunConfig, _: &str) -> Result<u32, String> {
        Ok(self.spent.fetch_add(1, Ordering::SeqCst) + 1)
    }
    fn recover(&self, _: &str) -> Result<NativeTaskRecovery, String> {
        Ok(NativeTaskRecovery::Vacant)
    }
    fn settle_exhausted_context(&self, _: &str) -> Result<NativeTaskRecovery, String> {
        panic!("four calls suffice")
    }
    fn prepare(&self, operation: &str) -> Result<NativeTaskContext, String> {
        Ok(NativeTaskContext {
            operation: operation.into(),
            fence: Self::fence(),
            valid_until_tick: 100,
            model_context: json!({"goal":"AUTHORIZED_GOAL_ONCE".to_owned()+&"g".repeat(7000),"soft_constraints":["retain all constraints"],"observation":{"fresh":true}}),
        })
    }
    fn current_control(&self) -> Result<NativeTaskControl, String> {
        Ok(NativeTaskControl {
            fence: Self::fence(),
            logical_tick: 1,
        })
    }
    fn invoke(
        &self,
        _: &NativeTaskContext,
        t: &NativeTaskTool,
    ) -> Result<NativeTaskToolResult, String> {
        let effect = t.side_effect();
        let value = if effect {
            self.effects.fetch_add(1, Ordering::SeqCst);
            json!({"committed":true})
        } else {
            let i = self.reads.fetch_add(1, Ordering::SeqCst);
            json!({"observation":format!("RESULT_ONCE_{i}:{}","x".repeat(15000))})
        };
        Ok(NativeTaskToolResult {
            value,
            effect: effect.then_some(NativeTaskRecovery::Settled),
            decision_required: false,
        })
    }
}
struct EncodedSession {
    inner: SessionModelProvider,
    encoded: Arc<Mutex<Vec<String>>>,
    snapshot_regression: bool,
}
impl ModelProvider for EncodedSession {
    fn provider_id(&self) -> &str {
        self.inner.provider_id()
    }
    fn history_mode(&self) -> ModelProviderHistory {
        if self.snapshot_regression {
            ModelProviderHistory::CallerSnapshot
        } else {
            self.inner.history_mode()
        }
    }
    fn preflight(&self, r: &ModelProviderRequest) -> Result<(), ModelProviderError> {
        self.inner.preflight(r)
    }
    fn check_execution(&self) -> Result<(), ModelProviderError> {
        self.inner.check_execution()
    }
    fn pending_tool_calls(&self) -> Vec<ModelProviderToolCall> {
        self.inner.pending_tool_calls()
    }
    fn record_tool_results(
        &mut self,
        r: &[ModelProviderToolResult],
    ) -> Result<(), ModelProviderError> {
        self.inner.record_tool_results(r)
    }
    fn complete(
        &mut self,
        r: &ModelProviderRequest,
    ) -> Result<ModelProviderResponse, ModelProviderError> {
        let encoded = self.encoded.clone();
        self.inner.complete_using(r, move |permit, conversation| {
            let bytes = crate::protocol::encode_request(permit.preset(), conversation).unwrap();
            let mut all = encoded.lock().unwrap();
            let index = all.len();
            all.push(String::from_utf8(bytes).unwrap());
            permit.before_send().unwrap();
            let mut reply = response();
            let mut call = reply.tool_calls[0].clone();
            call.id = format!("supplier-{index}");
            call.name = if index == 3 { "move" } else { "observe" }.into();
            call.arguments = if index == 3 {
                json!({"to":[1,2,3]})
            } else {
                json!({})
            };
            reply.tool_calls = vec![call.clone()];
            reply.replay = vec![Message::Assistant {
                content: None,
                tool_calls: vec![call],
                reasoning_content: Some(format!("REQUIRED_REASONING_{index}")),
                reasoning_details: None,
            }];
            Ok(reply)
        })
    }
}
#[test]
fn native_runner_session_history_stays_linear_and_complete_under_real_input_cap() {
    // The second mode intentionally models the old erroneous composition: each
    // request carries a snapshot into an adapter already retaining full history.
    for snapshot_regression in [false, true] {
        let (inner, _) = fixture_limits(
            Limits {
                max_calls: 12,
                max_tokens: 1_000_000,
                max_input_tokens: 131328,
                ..Limits::default()
            },
            RequestLimits {
                max_input_bytes: 131072,
                ..RequestLimits::default()
            },
        );
        let manager = inner.manager.clone();
        let principal = inner.principal.clone();
        let id = inner.connection_id.clone();
        let encoded = Arc::new(Mutex::new(vec![]));
        let p = EncodedSession {
            inner,
            encoded: encoded.clone(),
            snapshot_regression,
        };
        let host = Arc::new(HistoryHost {
            spent: AtomicU32::new(0),
            reads: AtomicU32::new(0),
            effects: AtomicU32::new(0),
        });
        let config = NativeTaskRunConfig {
            run_id: "history-growth".into(),
            model: "fixture-model".into(),
            max_turns: 1,
            max_model_calls: 4,
            ..NativeTaskRunConfig::default()
        };
        let report = NativeTaskRunner::new(host.clone(), Box::new(p), config)
            .unwrap()
            .run()
            .unwrap();
        let all = encoded.lock().unwrap();
        if snapshot_regression {
            assert_eq!(report.stop_reason, NativeTaskStopReason::InputLimit);
            assert_eq!(host.effects.load(Ordering::SeqCst), 0);
            assert!(all.len() < 4);
            assert_eq!(report.model_calls_spent, all.len() as u32);
        } else {
            assert_eq!(report.stop_reason, NativeTaskStopReason::Completed);
            assert_eq!(host.effects.load(Ordering::SeqCst), 1);
            assert_eq!(all.len(), 4);
            for (i, body) in all.iter().enumerate() {
                assert_eq!(body.matches("AUTHORIZED_GOAL_ONCE").count(), 1);
                for previous in 0..i {
                    assert_eq!(body.matches(&format!("RESULT_ONCE_{previous}:")).count(), 1);
                    assert_eq!(
                        body.matches(&format!("REQUIRED_REASONING_{previous}"))
                            .count(),
                        1
                    );
                }
                if i > 0 {
                    assert!(body.len() - all[i - 1].len() < 17000);
                }
                assert!(body.len() < 70000);
            }
        }
        let budget = manager.snapshot(&principal, &id).unwrap().budget;
        assert_eq!(budget.calls, 2 + all.len() as u64);
        assert_eq!(budget.in_flight, 0);
    }
}
