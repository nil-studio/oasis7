//! Narrow observation values supplied by Node hosts to reward accounting.
use serde::{Deserialize, Serialize};

/// Deterministic reward classification; host Node roles are explicitly adapted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodePointsRole {
    Sequencer,
    Storage,
    Observer,
}
impl NodePointsRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sequencer => "sequencer",
            Self::Storage => "storage",
            Self::Observer => "observer",
        }
    }
}
impl std::str::FromStr for NodePointsRole {
    type Err = ();
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "sequencer" => Ok(Self::Sequencer),
            "storage" => Ok(Self::Storage),
            "observer" => Ok(Self::Observer),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistFsChallengeSampleSource {
    LocalStoreIndex,
    ReplicationCommit,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistFsChallengeFailureReason {
    MissingSample,
    HashMismatch,
    Timeout,
    ReadIoError,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistFsChallengeProofHint {
    pub sample_source: DistFsChallengeSampleSource,
    pub sample_reference: String,
    #[serde(default)]
    pub failure_reason: Option<DistFsChallengeFailureReason>,
    #[serde(default)]
    pub proof_kind_hint: String,
    #[serde(default)]
    pub vrf_seed_hint: Option<String>,
    #[serde(default)]
    pub post_commitment_hint: Option<String>,
}
