//! Dispatch to the original three agent key codecs; no inferred qualification.
use super::*;
use crate::{
    incremental_agent_body::{AGENT_BODY_KEY_DOMAIN_V1, AgentBodyKeyV1},
    incremental_event_fields::{MAILBOX_KEY_DOMAIN_V1, MailboxKeyV1},
};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSubtreeKeyV1<'a> {
    Field {
        owner: [u8; 32],
        field: AgentFieldV1,
        ordinal: u64,
    },
    Body {
        agent: &'a str,
    },
    Mailbox {
        agent: &'a str,
        ordinal: Option<u64>,
    },
}
/// Reject malformed, unknown-version and wrong-namespace agent keys. Other
/// owners remain explicitly unclassified; matching a name cannot grant coverage.
pub fn parse_agent_subtree_key_v1(
    namespace: StateNamespaceV1,
    bytes: &[u8],
) -> Result<Option<AgentSubtreeKeyV1<'_>>, ValueTreeError> {
    let invalid = || ValueTreeError::Invalid("agent subtree key profile");
    let value = if bytes.starts_with(AGENT_FIELD_KEY_DOMAIN_V1) {
        let (owner, field, ordinal) =
            AgentFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
        if namespace != field.namespace() {
            return Err(invalid());
        }
        Some(AgentSubtreeKeyV1::Field {
            owner,
            field,
            ordinal,
        })
    } else if bytes.starts_with(AGENT_BODY_KEY_DOMAIN_V1) {
        if namespace != StateNamespaceV1::Business {
            return Err(invalid());
        }
        Some(AgentSubtreeKeyV1::Body {
            agent: AgentBodyKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?,
        })
    } else if bytes.starts_with(MAILBOX_KEY_DOMAIN_V1) {
        if namespace != StateNamespaceV1::Scheduler {
            return Err(invalid());
        }
        let (agent, ordinal) = MailboxKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
        Some(AgentSubtreeKeyV1::Mailbox { agent, ordinal })
    } else {
        for domain in [
            AGENT_FIELD_KEY_DOMAIN_V1,
            AGENT_BODY_KEY_DOMAIN_V1,
            MAILBOX_KEY_DOMAIN_V1,
        ] {
            if bytes.starts_with(&domain[..domain.len() - 3]) {
                return Err(invalid());
            }
        }
        None
    };
    Ok(value)
}
