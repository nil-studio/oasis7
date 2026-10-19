use crate::viewer::protocol::AgentChatRequest;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::viewer::runtime_live) struct RuntimePrimaryIntent {
    pub(in crate::viewer::runtime_live) message: Option<String>,
    pub(in crate::viewer::runtime_live) status: String,
    pub(in crate::viewer::runtime_live) resume_required: bool,
}

impl RuntimePrimaryIntent {
    fn accepted(message: String, status: &str) -> Self {
        Self {
            message: Some(message),
            status: status.to_string(),
            resume_required: false,
        }
    }
}

pub(super) fn apply_accepted_primary_intent(
    current: Option<&RuntimePrimaryIntent>,
    message: &str,
) -> RuntimePrimaryIntent {
    let message = message.trim().to_string();
    let status = match current {
        None => "accepted_new",
        Some(existing) if existing.message.as_deref() == Some(message.as_str()) => "unchanged",
        Some(_) => "reprioritized",
    };
    RuntimePrimaryIntent::accepted(message, status)
}

pub(super) fn apply_short_term_goal_primary_intent(
    current: Option<&RuntimePrimaryIntent>,
    short_term_goal: Option<&str>,
) -> RuntimePrimaryIntent {
    match short_term_goal
        .map(str::trim)
        .filter(|goal| !goal.is_empty())
    {
        Some(goal) => apply_accepted_primary_intent(current, goal),
        None => RuntimePrimaryIntent {
            message: current.and_then(|existing| existing.message.clone()),
            status: "resume_required".to_string(),
            resume_required: true,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ResolvedAgentChatIntent {
    pub(super) intent_tick: Option<u64>,
    pub(super) intent_seq: u64,
}

pub(super) fn resolve_agent_chat_intent(
    request: &AgentChatRequest,
    verified_nonce: u64,
) -> Result<ResolvedAgentChatIntent, String> {
    let intent_seq = match request.intent_seq {
        Some(0) => {
            return Err("intent_seq must be greater than zero".to_string());
        }
        Some(seq) if seq != verified_nonce => {
            return Err(format!(
                "intent_seq {} must match auth nonce {}",
                seq, verified_nonce
            ));
        }
        Some(seq) => seq,
        None => verified_nonce,
    };
    Ok(ResolvedAgentChatIntent {
        intent_tick: request.intent_tick,
        intent_seq,
    })
}
