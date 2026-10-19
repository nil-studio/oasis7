use super::*;
use oasis7_agent_api::{ProviderAgentChatRequest, provider_agent_chat_log_key};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct RecordingInvoker {
    response: Result<AgentInvocationOutput, String>,
    invocations: Arc<Mutex<Vec<AgentInvocation>>>,
}

impl AgentInvoker for RecordingInvoker {
    fn invoke(&self, invocation: AgentInvocation) -> Result<AgentInvocationOutput, String> {
        self.invocations
            .lock()
            .expect("recording invoker lock")
            .push(invocation);
        self.response.clone()
    }
}

#[test]
fn handle_agent_chat_invocation_carries_loggable_request_key() {
    let state = ProviderState::new(CliOptions::default()).expect("provider state");
    let invocations = Arc::new(Mutex::new(Vec::new()));
    let invoker = RecordingInvoker {
        response: Ok(AgentInvocationOutput {
            prompt: "prompt".to_string(),
            text: "hello from provider".to_string(),
            provider_version: Some("provider/model".to_string()),
            duration_ms: Some(42),
            prompt_tokens: Some(11),
            completion_tokens: Some(7),
            total_tokens: Some(18),
            route_note: None,
            upstream_trace: None,
        }),
        invocations: invocations.clone(),
    };
    let request = ProviderAgentChatRequest {
        agent_id: "agent-0".to_string(),
        player_id: "player-a".to_string(),
        message: "where are you?".to_string(),
        world_time: 7,
        location_id: Some("loc-1".to_string()),
        resources: None,
        recent_feedback: Vec::new(),
    };
    let expected_key = provider_agent_chat_log_key(&request);

    let response = state
        .handle_agent_chat(request, Some("test-route"), &invoker)
        .expect("agent chat response");

    assert_eq!(response.message, "hello from provider");
    let captured = invocations.lock().expect("recording invoker lock");
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].chat_request_key.as_deref(),
        Some(expected_key.as_str())
    );
}
