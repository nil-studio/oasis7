//! Private collaboration goal codec. These values are not public ledger payloads
//! and do not prove ownership, activation or permission to start a decision.
use super::DecimalU64;
use crate::{Digest32, h_v1};
use serde::{Deserialize, Serialize};

pub const MAX_GOAL_CONTENT_BYTES: usize = 4 * 1024;
pub const GAME_GOAL_VERSION_DOMAIN: &str = "game_goal_version/v1";

/// Complete content, preserved without trimming, truncation or reordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoalContentV1 {
    pub goal_text: String,
    pub soft_constraints: Vec<String>,
}

impl GoalContentV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.goal_text.trim().is_empty() {
            return Err("goal text must not be empty");
        }
        let bytes = serde_json::to_vec(self).map_err(|_| "goal content encoding failed")?;
        if bytes.len() > MAX_GOAL_CONTENT_BYTES {
            return Err("complete goal content exceeds byte limit");
        }
        Ok(())
    }
}

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}

/// Domain-separated canonical CBOR digest of the versioned private content.
/// This is distinct from the Harness GoalSnapshot/context digest. Neither the
/// content nor this enumerable fingerprint belongs in public world streams.
pub fn game_goal_version_digest(
    task_id: &str,
    goal_revision: DecimalU64,
    content: &GoalContentV1,
) -> Result<Digest32, &'static str> {
    if !valid_identity(task_id) || goal_revision.0 == 0 {
        return Err("invalid goal version identity");
    }
    content.validate()?;
    #[derive(Serialize)]
    struct Version<'a> {
        schema_revision: u32,
        task_id: &'a str,
        goal_revision: DecimalU64,
        content: &'a GoalContentV1,
    }
    Ok(h_v1(
        GAME_GOAL_VERSION_DOMAIN,
        &Version {
            schema_revision: 1,
            task_id,
            goal_revision,
            content,
        },
    ))
}

/// Authorized full-body projection at a committed activation cut. Parsing is
/// only structural; callers must validate against trusted current TaskControl
/// at the same WorldBinding, and recheck audience before delivering the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalVersionProjectionV1 {
    pub schema_revision: u32,
    pub task_id: String,
    pub goal_revision: DecimalU64,
    pub goal_digest: Digest32,
    pub activation_commit_ref: String,
    pub content: GoalContentV1,
}

impl GoalVersionProjectionV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_revision != 1 || !valid_identity(&self.activation_commit_ref) {
            return Err("invalid goal projection revision or activation reference");
        }
        if self.goal_digest
            != game_goal_version_digest(&self.task_id, self.goal_revision, &self.content)?
        {
            return Err("goal version digest does not match complete content");
        }
        Ok(())
    }

    /// Expected values must come from authorized canonical control, not from
    /// this response or a caller-supplied claim. No world authority is granted.
    pub fn validate_against(
        &self,
        task_id: &str,
        active_goal_revision: DecimalU64,
        active_goal_digest: &Digest32,
        activation_commit_ref: &str,
    ) -> Result<(), &'static str> {
        self.validate()?;
        if self.task_id != task_id
            || self.goal_revision != active_goal_revision
            || &self.goal_digest != active_goal_digest
            || self.activation_commit_ref != activation_commit_ref
        {
            return Err("goal projection differs from canonical active control");
        }
        Ok(())
    }
}
