//! Player-safe summaries for in-world Agent Intents.
//!
//! Provider output, chat text, prompt material, and internal diagnostics are
//! intentionally not inputs to the rendered copy.  A runtime/viewer boundary
//! can use this contract to turn an authoritative `(kind, source, status)`
//! tuple into a bounded, deterministic summary without ever forwarding raw
//! model or player text.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Version of the player-facing summary contract.
pub const AGENT_INTENT_SUMMARY_SCHEMA_VERSION: u32 = 1;

/// Maximum length of any rendered summary, measured in Unicode scalar values.
pub const AGENT_INTENT_SUMMARY_MAX_CHARS: usize = 160;

/// Only these sources may produce a canonical runtime intent summary.
pub const AUTHORITATIVE_AGENT_INTENT_SOURCES: &[&str] = &["player", "agent"];

/// Source classes that are permitted to own a canonical runtime Intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentIntentSourceV1 {
    Player,
    Agent,
}

/// A bounded catalog of intent classes used by the player-facing contract.
///
/// Unknown runtime kinds intentionally collapse to `other`; the raw kind is
/// never copied into player-visible summary text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentIntentKindV1 {
    AgentChat,
    StartRecipe,
    MoveAgent,
    InspectTarget,
    SimpleInteract,
    Wait,
    WaitTicks,
    Other,
}

impl AgentIntentKindV1 {
    fn from_raw(kind: &str) -> Self {
        match kind.trim().to_ascii_lowercase().as_str() {
            "agent_chat" => Self::AgentChat,
            "start_recipe" => Self::StartRecipe,
            "move_agent" => Self::MoveAgent,
            "inspect_target" => Self::InspectTarget,
            "simple_interact" => Self::SimpleInteract,
            "wait" => Self::Wait,
            "wait_ticks" => Self::WaitTicks,
            _ => Self::Other,
        }
    }
}

impl AgentIntentSourceV1 {
    fn from_raw(source: &str) -> Result<Self, AgentIntentSummaryError> {
        match source.trim() {
            "player" => Ok(Self::Player),
            "agent" => Ok(Self::Agent),
            "provider_advisory" => {
                Err(AgentIntentSummaryError::ProviderAdvisoryCannotBeAuthoritative)
            }
            other => Err(AgentIntentSummaryError::UnsupportedSource(
                other.to_string(),
            )),
        }
    }
}

/// Runtime lifecycle states allowed in the summary contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentIntentStatusV1 {
    Proposed,
    Submitted,
    Accepted,
    Blocked,
    Completed,
    Rejected,
    Expired,
    Cancelled,
    Superseded,
}

impl AgentIntentStatusV1 {
    fn from_raw(status: &str) -> Option<Self> {
        Some(match status.trim().to_ascii_lowercase().as_str() {
            "proposed" => Self::Proposed,
            "submitted" => Self::Submitted,
            "accepted" | "accepted_new" | "unchanged" | "reprioritized" => Self::Accepted,
            "blocked" => Self::Blocked,
            "completed" => Self::Completed,
            "rejected" => Self::Rejected,
            "expired" => Self::Expired,
            "cancelled" => Self::Cancelled,
            "superseded" => Self::Superseded,
            _ => return None,
        })
    }
}

/// The fixed summary template selected for an authoritative status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentIntentSummaryTemplateV1 {
    Proposed,
    Submitted,
    Accepted,
    Blocked,
    Completed,
    Rejected,
    Expired,
    Cancelled,
    Superseded,
}

impl AgentIntentSummaryTemplateV1 {
    fn for_status(status: AgentIntentStatusV1) -> Self {
        match status {
            AgentIntentStatusV1::Proposed => Self::Proposed,
            AgentIntentStatusV1::Submitted => Self::Submitted,
            AgentIntentStatusV1::Accepted => Self::Accepted,
            AgentIntentStatusV1::Blocked => Self::Blocked,
            AgentIntentStatusV1::Completed => Self::Completed,
            AgentIntentStatusV1::Rejected => Self::Rejected,
            AgentIntentStatusV1::Expired => Self::Expired,
            AgentIntentStatusV1::Cancelled => Self::Cancelled,
            AgentIntentStatusV1::Superseded => Self::Superseded,
        }
    }

    fn render(self) -> &'static str {
        match self {
            Self::Proposed => "Agent guidance is proposed and not yet accepted.",
            Self::Submitted => "Agent guidance was submitted and awaits runtime acceptance.",
            Self::Accepted => {
                "Agent guidance accepted; the Agent will evaluate its next world action."
            }
            Self::Blocked => "Agent guidance is blocked pending a runtime recheck.",
            Self::Completed => "Agent guidance completed with a confirmed world receipt.",
            Self::Rejected => "Agent guidance was rejected by runtime authority.",
            Self::Expired => "Agent guidance expired before execution.",
            Self::Cancelled => "Agent guidance was cancelled before completion.",
            Self::Superseded => "Agent guidance was replaced by newer guidance.",
        }
    }
}

/// Versioned, deterministic, player-safe Agent Intent summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentIntentSummaryV1 {
    pub schema_version: u32,
    pub source: AgentIntentSourceV1,
    pub kind: AgentIntentKindV1,
    pub status: AgentIntentStatusV1,
    pub template: AgentIntentSummaryTemplateV1,
    pub text: String,
}

/// Why a candidate could not be turned into an authoritative summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentIntentSummaryError {
    EmptyKind,
    EmptySource,
    ProviderAdvisoryCannotBeAuthoritative,
    UnsupportedSource(String),
    UnsupportedStatus(String),
}

impl fmt::Display for AgentIntentSummaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyKind => formatter.write_str("intent kind cannot be empty"),
            Self::EmptySource => formatter.write_str("intent source cannot be empty"),
            Self::ProviderAdvisoryCannotBeAuthoritative => formatter.write_str(
                "provider_advisory may remain a proposed suggestion but cannot become an authoritative intent",
            ),
            Self::UnsupportedSource(source) => {
                write!(formatter, "unsupported intent source `{source}`")
            }
            Self::UnsupportedStatus(status) => {
                write!(formatter, "unsupported intent status `{status}`")
            }
        }
    }
}

impl std::error::Error for AgentIntentSummaryError {}

/// Build canonical player-safe copy from runtime authority fields.
///
/// `raw_kind` is reduced to a closed catalog and is never interpolated into
/// the result. `raw_source == "provider_advisory"` is always rejected here;
/// callers that want to display a provider suggestion must use a separate
/// advisory-only surface and keep its status `proposed`.
pub fn canonical_agent_intent_summary(
    raw_kind: &str,
    raw_source: &str,
    raw_status: &str,
) -> Result<AgentIntentSummaryV1, AgentIntentSummaryError> {
    if raw_kind.trim().is_empty() {
        return Err(AgentIntentSummaryError::EmptyKind);
    }
    let source = raw_source.trim();
    if source.is_empty() {
        return Err(AgentIntentSummaryError::EmptySource);
    }
    let source = AgentIntentSourceV1::from_raw(source)?;
    let status = AgentIntentStatusV1::from_raw(raw_status)
        .ok_or_else(|| AgentIntentSummaryError::UnsupportedStatus(raw_status.trim().to_string()))?;
    let template = AgentIntentSummaryTemplateV1::for_status(status);
    let text = template.render().to_string();
    debug_assert!(text.chars().count() <= AGENT_INTENT_SUMMARY_MAX_CHARS);
    Ok(AgentIntentSummaryV1 {
        schema_version: AGENT_INTENT_SUMMARY_SCHEMA_VERSION,
        source,
        kind: AgentIntentKindV1::from_raw(raw_kind),
        status,
        template,
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEAK_PROBES: [&str; 9] = [
        "system prompt",
        "provider rationale",
        "chain of thought",
        "memory:",
        "trace:",
        "debug:",
        "auth token",
        "cost_cents",
        "secret-player-message",
    ];

    #[test]
    fn authoritative_summary_is_versioned_bounded_and_deterministic() {
        let first = canonical_agent_intent_summary("agent_chat", "player", "accepted")
            .expect("accepted player intent summary");
        let second = canonical_agent_intent_summary("agent_chat", "player", "accepted_new")
            .expect("accepted compatibility status summary");

        assert_eq!(first, second);
        assert_eq!(first.schema_version, AGENT_INTENT_SUMMARY_SCHEMA_VERSION);
        assert_eq!(first.source, AgentIntentSourceV1::Player);
        assert_eq!(first.template, AgentIntentSummaryTemplateV1::Accepted);
        assert!(first.text.chars().count() <= AGENT_INTENT_SUMMARY_MAX_CHARS);
        assert!(!first.text.is_empty());
    }

    #[test]
    fn raw_kind_and_status_content_never_enters_player_copy() {
        let summary = canonical_agent_intent_summary(
            "provider-controlled-kind\nsecret-player-message",
            "agent",
            "blocked",
        )
        .expect("unknown kind should use the safe other bucket");

        assert_eq!(summary.kind, AgentIntentKindV1::Other);
        assert!(!summary.text.contains("provider-controlled-kind"));
        assert!(!summary.text.contains("secret-player-message"));
        for probe in LEAK_PROBES {
            assert!(
                !summary.text.to_ascii_lowercase().contains(probe),
                "summary leaked sensitive probe `{probe}`"
            );
        }
    }

    #[test]
    fn provider_advisory_never_gets_an_authoritative_summary() {
        for status in ["proposed", "submitted", "accepted", "completed"] {
            assert_eq!(
                canonical_agent_intent_summary("agent_chat", "provider_advisory", status),
                Err(AgentIntentSummaryError::ProviderAdvisoryCannotBeAuthoritative)
            );
        }
    }

    #[test]
    fn only_allowlisted_sources_and_statuses_are_accepted() {
        assert_eq!(
            canonical_agent_intent_summary("agent_chat", "", "accepted"),
            Err(AgentIntentSummaryError::EmptySource)
        );
        assert!(matches!(
            canonical_agent_intent_summary("agent_chat", "tool", "accepted"),
            Err(AgentIntentSummaryError::UnsupportedSource(_))
        ));
        assert!(matches!(
            canonical_agent_intent_summary("agent_chat", "player", "provider_rationale"),
            Err(AgentIntentSummaryError::UnsupportedStatus(_))
        ));
        assert_eq!(
            canonical_agent_intent_summary("", "player", "accepted"),
            Err(AgentIntentSummaryError::EmptyKind)
        );
    }

    #[test]
    fn every_runtime_status_has_a_fixed_template() {
        for status in [
            "proposed",
            "submitted",
            "accepted",
            "blocked",
            "completed",
            "rejected",
            "expired",
            "cancelled",
            "superseded",
        ] {
            let summary = canonical_agent_intent_summary("start_recipe", "agent", status)
                .expect("known lifecycle status");
            assert_eq!(
                summary.status,
                AgentIntentStatusV1::from_raw(status).unwrap()
            );
            assert_eq!(summary.template.render(), summary.text);
        }
    }

    #[test]
    fn summary_serialization_is_stable_and_contains_no_raw_input() {
        let summary =
            canonical_agent_intent_summary("unexpected-provider-kind", "player", "rejected")
                .expect("safe fallback summary");
        let encoded = serde_json::to_value(summary).expect("serialize summary contract");
        assert_eq!(
            encoded["schema_version"],
            AGENT_INTENT_SUMMARY_SCHEMA_VERSION
        );
        assert_eq!(encoded["source"], "player");
        assert_eq!(encoded["kind"], "other");
        assert_eq!(encoded["template"], "rejected");
        assert!(encoded["text"].as_str().unwrap().contains("rejected"));
        assert!(!encoded.to_string().contains("unexpected-provider-kind"));
    }
}
