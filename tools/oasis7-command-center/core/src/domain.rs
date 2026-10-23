use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub kind: String,
    pub state: String,
    pub last_attempt: Option<String>,
    pub last_success: Option<String>,
    pub source_revision: Option<String>,
    pub generation: String,
    pub coverage: String,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub path: String,
    pub git_common_dir: String,
    pub branch: Option<String>,
    pub head: String,
    pub environment: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSection {
    pub id: String,
    pub path: String,
    pub title: String,
    pub content: String,
    pub line: usize,
    pub revision: String,
    pub committed_revision: String,
    pub dirty: bool,
    pub fact_kind: String,
    #[serde(default)]
    pub lifecycle: Option<DocumentLifecycle>,
}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLifecycle {
    /// Original declaration, including code delimiters and notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub raw_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
    /// Compatibility projection used by older clients; not a normalized category.
    pub status: Option<String>,
    pub successor_declaration: Option<String>,
    pub successor_targets: Vec<String>,
    pub successor_reason: Option<String>,
    pub issue: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub project_path: Option<String>,
    pub view: String,
    pub query: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReadingPosition {
    pub section_id: String,
    pub revision: String,
    pub line: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitSnapshot {
    pub status: String,
    pub worktrees: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub project: Option<Project>,
    pub sources: Vec<SourceStatus>,
    pub sections: Vec<DocumentSection>,
    pub preferences: Preferences,
    pub reading_positions: Vec<ReadingPosition>,
    pub git: GitSnapshot,
}
