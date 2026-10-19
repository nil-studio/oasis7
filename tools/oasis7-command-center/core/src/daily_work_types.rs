use crate::work_projection::WorkRef;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkNavigation {
    pub selected: Option<WorkRef>,
    pub filter: String,
    pub expanded: bool,
    pub scroll_top: f64,
    pub return_view: String,
    pub draft: String,
    #[serde(default)]
    pub draft_purpose: String,
    #[serde(default)]
    pub draft_scope: String,
    #[serde(default)]
    pub detail_scroll_top: f64,
    pub retain_draft: bool,
    #[serde(default)]
    pub start_intent_id: Option<String>,
    pub last_read_at: Option<String>,
    pub pinned: Vec<WorkRef>,
    #[serde(default)]
    #[ts(optional)]
    pub recent_work: Option<Vec<WorkRef>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkRelation {
    pub id: String,
    pub work: WorkRef,
    pub section_id: String,
    pub source_revision: String,
    pub confirmed_at: String,
    pub removed_at: Option<String>,
    pub previous: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkAttention {
    pub id: String,
    pub work: WorkRef,
    pub question: String,
    pub state: String,
    pub source_revision: String,
    pub statement: String,
    pub updated_at: String,
    pub history: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExternalWork {
    pub id: String,
    pub title: String,
    pub work: WorkRef,
    pub locator: String,
    pub source_url: Option<String>,
    pub recorded_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeReference {
    pub project_id: String,
    pub path: String,
    pub environment: String,
    pub head: String,
    pub verified_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DailyWorkState {
    pub schema_version: u32,
    pub project_id: String,
    pub environment: String,
    #[ts(type = "number")]
    pub revision: u64,
    pub navigation: WorkNavigation,
    pub relations: Vec<WorkRelation>,
    pub attention: Vec<WorkAttention>,
    pub external: Vec<ExternalWork>,
    pub worktrees: Vec<WorktreeReference>,
    pub legacy_digest: String,
    pub legacy_changed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DailyWorkAction {
    Navigate {
        navigation: WorkNavigation,
    },
    Relate {
        work: WorkRef,
        section_id: String,
        replace_id: Option<String>,
    },
    RemoveRelation {
        id: String,
    },
    Attention {
        item: WorkAttention,
    },
    External {
        item: ExternalWork,
    },
    RemoveExternal {
        id: String,
    },
    RemoveWorktree {
        project_id: String,
    },
    AcceptLegacy {},
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DailyWorkRequest {
    pub project_id: String,
    pub environment: String,
    #[ts(type = "number")]
    pub expected_revision: u64,
    pub action: DailyWorkAction,
}
