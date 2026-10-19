use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderAgentChatRequest {
    pub agent_id: String,
    pub player_id: String,
    pub message: String,
    pub world_time: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<Value>,
    #[serde(default)]
    pub recent_feedback: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderAgentChatResponse {
    pub agent_id: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
}

pub fn provider_agent_chat_log_key(request: &ProviderAgentChatRequest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(request.world_time.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(request.agent_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(request.player_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(request.message.as_bytes());
    let digest = hasher.finalize();
    format!("chat-{}", hex::encode(&digest[..8]))
}
