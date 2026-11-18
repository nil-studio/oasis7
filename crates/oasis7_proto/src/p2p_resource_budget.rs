//! Explicit resource limits for one development libp2p instance.
use std::{
    num::{NonZeroU32, NonZeroU64},
    time::Duration,
};

use serde::{Deserialize, Serialize};

pub const P2P_RESOURCE_BUDGET_V1: &str = "v1-discovery-resource-budget-1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "profile", deny_unknown_fields)]
pub enum P2pResourceBudget {
    #[serde(rename = "legacy")]
    Legacy {},
    #[serde(rename = "v1-discovery-resource-budget-1")]
    SubstreamV1(P2pResourceBudgetLimits),
}

impl Default for P2pResourceBudget {
    fn default() -> Self {
        Self::Legacy {}
    }
}

impl P2pResourceBudget {
    pub const fn is_legacy(&self) -> bool {
        matches!(self, Self::Legacy {})
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct P2pResourceBudgetLimits {
    pub connections_total: NonZeroU32,
    pub pending_connections_total: NonZeroU32,
    pub pending_inbound_unknown: NonZeroU32,
    pub pending_outbound: NonZeroU32,
    pub connections_per_peer: NonZeroU32,
    pub handshake_timeout_ms: NonZeroU64,
    pub bytes_in_per_window: NonZeroU64,
    pub bytes_out_per_window: NonZeroU64,
    pub bytes_total_per_window: NonZeroU64,
}

impl P2pResourceBudgetLimits {
    pub fn validate_connection_limits(self) -> Result<(), &'static str> {
        if self.pending_connections_total > self.connections_total {
            return Err("pending_connections_total exceeds connections_total");
        }
        if self.pending_inbound_unknown > self.pending_connections_total {
            return Err("pending_inbound_unknown exceeds pending_connections_total");
        }
        if self.pending_outbound > self.pending_connections_total {
            return Err("pending_outbound exceeds pending_connections_total");
        }
        if self.connections_per_peer > self.connections_total {
            return Err("connections_per_peer exceeds connections_total");
        }
        Ok(())
    }

    pub const fn handshake_timeout(self) -> Duration {
        Duration::from_millis(self.handshake_timeout_ms.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{"profile":"v1-discovery-resource-budget-1",
        "connections_total":4,"pending_connections_total":2,"pending_inbound_unknown":1,
        "pending_outbound":2,"connections_per_peer":1,"handshake_timeout_ms":1000,
        "bytes_in_per_window":4,"bytes_out_per_window":5,"bytes_total_per_window":6}"#;

    #[test]
    fn resource_budget_profile_is_strict_and_roundtrips_explicit_limits() {
        let parsed: P2pResourceBudget = serde_json::from_str(VALID).unwrap();
        let P2pResourceBudget::SubstreamV1(limits) = parsed else {
            panic!("expected explicit limits");
        };
        assert_eq!(limits.handshake_timeout(), Duration::from_secs(1));
        assert!(limits.validate_connection_limits().is_ok());
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            serde_json::from_str::<serde_json::Value>(VALID).unwrap()
        );

        let legacy = serde_json::from_str::<P2pResourceBudget>(r#"{"profile":"legacy"}"#).unwrap();
        assert!(legacy.is_legacy());
        for invalid in [
            "null".to_owned(),
            r#"{"profile":"legacy","connections_total":4}"#.to_owned(),
            r#"{"profile":"legacy","profile":"legacy"}"#.to_owned(),
            VALID.replace("v1-discovery-resource-budget-1", "future-version"),
            VALID.replace(r#""connections_total":4,"#, ""),
            VALID.replace(
                r#""connections_total":4"#,
                r#""connections_total":4,"connections_total":4"#,
            ),
            VALID.replace(
                r#""connections_total":4"#,
                r#""connections_total":4,"extra":1"#,
            ),
            VALID.replace(r#""connections_total":4"#, r#""connections_total":0"#),
            VALID.replace(
                r#""connections_total":4"#,
                r#""connections_total":4294967296"#,
            ),
            VALID.replace(r#""connections_total":4"#, r#""connections_total":"4""#),
            VALID.replace(r#""connections_total":4"#, r#""connections_total":null"#),
            VALID.replace(
                r#""handshake_timeout_ms":1000"#,
                r#""handshake_timeout_ms":18446744073709551616"#,
            ),
        ] {
            assert!(
                serde_json::from_str::<P2pResourceBudget>(&invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }
}
