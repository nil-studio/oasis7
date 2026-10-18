use super::*;

/// Derive the legacy bridge key from the target Harness' domain-separated
/// digest contract without treating the legacy DTO as cognition proof.
/// Timeout changes and transport retries do not create a new invocation.
pub(super) fn legacy_provider_invocation_key(request: &DecisionRequest) -> String {
    let mut value = serde_json::to_value(request).expect("legacy request is serializable");
    if let Some(object) = value.as_object_mut() {
        object.remove("timeout_budget_ms");
    }
    let canonical = oasis7_wasm_abi::encode_canonical_cbor(&value)
        .expect("legacy request is canonical-CBOR encodable");
    let request_digest = h_v1("oasis7.cognition.request.v1", &canonical);
    h_v1("oasis7.cognition.provider-invocation.v1", &request_digest).to_string()
}

pub(super) fn should_fallback_to_local_agent(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("gateway timeout")
        || normalized.contains("read operation timed out")
        || normalized.contains("operation timed out")
}

pub(super) fn local_session_id_from_session_key(session_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(session_key.as_bytes());
    format!("ws-{}", hex::encode(hasher.finalize()))
}

pub(super) fn agent_output_from_json(
    prompt: String,
    payload: &str,
    route_note: Option<String>,
) -> Result<AgentInvocationOutput, String> {
    let parsed = parse_provider_agent_command_output(payload)?;
    let text = parsed
        .payloads
        .into_iter()
        .find_map(|entry| entry.text)
        .ok_or_else(|| "provider agent json did not contain payloads[].text".to_string())?;
    Ok(AgentInvocationOutput {
        prompt,
        text,
        provider_version: parsed
            .meta
            .as_ref()
            .and_then(|meta| meta.agent_meta.as_ref())
            .map(
                |agent_meta| match (&agent_meta.provider, &agent_meta.model) {
                    (Some(provider), Some(model)) => format!("{provider}/{model}"),
                    (Some(provider), None) => provider.clone(),
                    (None, Some(model)) => model.clone(),
                    (None, None) => DEFAULT_PROVIDER_ID.to_string(),
                },
            ),
        duration_ms: parsed.meta.as_ref().and_then(|meta| meta.duration_ms),
        prompt_tokens: parsed
            .meta
            .as_ref()
            .and_then(|meta| meta.agent_meta.as_ref())
            .and_then(|agent_meta| agent_meta.prompt_tokens),
        completion_tokens: parsed
            .meta
            .as_ref()
            .and_then(|meta| meta.agent_meta.as_ref())
            .and_then(|agent_meta| agent_meta.usage.as_ref())
            .and_then(|usage| usage.output),
        total_tokens: parsed
            .meta
            .as_ref()
            .and_then(|meta| meta.agent_meta.as_ref())
            .and_then(|agent_meta| agent_meta.usage.as_ref())
            .and_then(|usage| usage.total),
        route_note,
        upstream_trace: None,
    })
}

#[derive(Debug, Deserialize)]
struct ProviderAgentCliOutput {
    result: ProviderAgentCliResult,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ProviderAgentCommandOutput {
    Gateway(ProviderAgentCliOutput),
    Local(ProviderAgentCliResult),
}

fn parse_provider_agent_command_output(payload: &str) -> Result<ProviderAgentCliResult, String> {
    match serde_json::from_str::<ProviderAgentCommandOutput>(payload) {
        Ok(ProviderAgentCommandOutput::Gateway(output)) => Ok(output.result),
        Ok(ProviderAgentCommandOutput::Local(result)) => Ok(result),
        Err(err) => Err(format!("parse provider agent json failed: {err}")),
    }
}

#[derive(Debug, Deserialize)]
struct ProviderAgentCliResult {
    payloads: Vec<ProviderAgentPayload>,
    #[serde(default)]
    meta: Option<ProviderAgentMeta>,
}

#[derive(Debug, Deserialize)]
struct ProviderAgentPayload {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProviderAgentMeta {
    #[serde(rename = "durationMs", default)]
    duration_ms: Option<u64>,
    #[serde(rename = "agentMeta", default)]
    agent_meta: Option<ProviderAgentMetaDetails>,
}

#[derive(Debug, Deserialize)]
struct ProviderAgentMetaDetails {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(rename = "promptTokens", default)]
    prompt_tokens: Option<u64>,
    #[serde(default)]
    usage: Option<ProviderAgentUsage>,
}

#[derive(Debug, Deserialize)]
struct ProviderAgentUsage {
    #[serde(default)]
    output: Option<u64>,
    #[serde(default)]
    total: Option<u64>,
}

pub(super) fn estimated_current_location_id(
    observation: &oasis7_agent_api::ProviderObservation,
) -> Option<&str> {
    if observation.self_state.location_ref.trim().is_empty() {
        None
    } else {
        Some(observation.self_state.location_ref.as_str())
    }
}

pub(super) fn nearest_reachable_non_current_location_id(
    observation: &oasis7_agent_api::ProviderObservation,
) -> Option<String> {
    let current_location_id = estimated_current_location_id(observation);
    if !observation.local_navigation_graph.is_empty() {
        return observation
            .local_navigation_graph
            .iter()
            .find(|node| {
                node.traversable
                    && node.relation == "reachable_location"
                    && Some(node.node_ref.as_str()) != current_location_id
            })
            .map(|node| node.node_ref.clone());
    }
    observation
        .nearby_entities
        .iter()
        .filter(|entity| {
            entity.kind == "location"
                && entity.relation == "reachable_location"
                && Some(entity.entity_ref.as_str()) != current_location_id
        })
        .map(|entity| entity.entity_ref.clone())
        .next()
}
