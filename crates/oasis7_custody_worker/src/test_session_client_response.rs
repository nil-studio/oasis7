//! Credential transport is distinct from management Progress and model output.
use oasis7_custody_core::{DecimalU64, provisioning::SessionPurposeV1};
use serde::{Serialize, Serializer};
use zeroize::Zeroizing;

/// Only the once-only winner may construct a token transport value.
pub(crate) struct PrivateClientToken(Zeroizing<String>);
impl PrivateClientToken {
    pub(crate) fn new(value: Zeroizing<String>) -> Self {
        Self(value)
    }
}
impl Serialize for PrivateClientToken {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.as_str())
    }
}
impl core::fmt::Debug for PrivateClientToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PrivateClientToken(<redacted>)")
    }
}

#[derive(Serialize)]
pub(crate) struct ClaimedClientCredential {
    pub purpose: SessionPurposeV1,
    pub generation: DecimalU64,
    pub token: PrivateClientToken,
}

#[derive(Serialize)]
pub(crate) struct HistoricalClientCredential {
    pub purpose: SessionPurposeV1,
    pub generation: DecimalU64,
}

/// The historical variant structurally cannot return a fresh or old token.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum ClientCredentialReply {
    Claimed {
        api_version: u8,
        session_id: String,
        request_id: String,
        credentials: Vec<ClaimedClientCredential>,
    },
    Historical {
        api_version: u8,
        session_id: String,
        request_id: String,
        credentials: Vec<HistoricalClientCredential>,
    },
}
