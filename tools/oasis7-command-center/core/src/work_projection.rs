//! Read-only projection of source identities. No task table or inferred completion.
use crate::{
    Error, Result,
    domain::*,
    github::{Check, GitHubSnapshot, PullChecksSource},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WorkRef {
    PullRequest {
        host: String,
        repository: String,
        number: String,
    },
    Session {
        namespace: String,
        project_id: String,
        environment: String,
        thread_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourceFact {
    pub source: WorkRef,
    pub revision: Option<String>,
    pub verified_at: Option<String>,
    pub event_at: Option<String>,
    pub read_at: String,
    pub coverage: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FactCell {
    pub value: String,
    pub evidence: SourceFact,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkGoal {
    pub section_id: String,
    pub title: Option<String>,
    pub revision: Option<String>,
    pub confirmed_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub reference: WorkRef,
    pub purpose: FactCell,
    pub result: FactCell,
    pub remaining: String,
    pub source_url: Option<String>,
    pub goals: Vec<WorkGoal>,
    pub checks: Vec<Check>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub checks_source: Option<PullChecksSource>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkSource {
    pub kind: String,
    pub state: String,
    pub coverage: String,
    pub verified_at: Option<String>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkProjection {
    #[serde(default)]
    #[ts(optional)]
    pub view_revision: Option<String>,
    #[serde(default)]
    #[ts(optional, type = "number | null")]
    pub personal_revision: Option<u64>,
    pub schema_version: u32,
    pub project_id: String,
    pub environment: String,
    pub read_at: String,
    pub sources: Vec<WorkSource>,
    pub items: Vec<WorkItem>,
}

pub fn project_work(
    snapshot: &Snapshot,
    github: &GitHubSnapshot,
    read_at: &str,
) -> Result<WorkProjection> {
    let project = snapshot
        .project
        .as_ref()
        .ok_or_else(|| Error::Invalid("select a project first".into()))?;
    let mut items = Vec::new();
    if let Some(repository) = &github.repository {
        for pull in &github.pulls {
            // The connector supports github.com only. Reject mismatched object identities.
            if pull.url != format!("https://github.com/{repository}/pull/{}", pull.id)
                || pull.id.parse::<u64>().ok().filter(|id| *id > 0).is_none()
            {
                continue;
            }
            let reference = WorkRef::PullRequest {
                host: "github.com".into(),
                repository: repository.clone(),
                number: pull.id.clone(),
            };
            if items
                .iter()
                .any(|item: &WorkItem| item.reference == reference)
            {
                continue;
            }
            let evidence = SourceFact {
                source: reference.clone(),
                revision: Some(pull.head_oid.clone()),
                verified_at: pull
                    .metadata
                    .as_ref()
                    .map(|m| m.observed_at.clone())
                    .or_else(|| github.observed_at.clone()),
                event_at: pull.metadata.as_ref().and_then(|m| m.merged_at.clone()),
                read_at: read_at.into(),
                coverage: github.coverage.clone(),
            };
            let goals = snapshot
                .associations
                .iter()
                .filter(|link| link.work_url == pull.url)
                .flat_map(|link| {
                    link.goal_section_ids.iter().map(|id| {
                        let section = snapshot.sections.iter().find(|s| &s.id == id);
                        WorkGoal {
                            section_id: id.clone(),
                            title: section.map(|s| s.title.clone()),
                            revision: section.map(|s| s.revision.clone()),
                            confirmed_at: link.confirmed_at.clone(),
                        }
                    })
                })
                .collect();
            items.push(WorkItem {
                reference,
                purpose: FactCell {
                    value: pull.title.clone(),
                    evidence: evidence.clone(),
                },
                result: FactCell {
                    value: format!("PR {}", pull.state),
                    evidence,
                },
                remaining: "部署、目标验收及实现语义尚未核实；检查结果仅覆盖各自测试对象。".into(),
                source_url: Some(pull.url.clone()),
                goals,
                checks: pull.checks.clone(),
                checks_source: pull.checks_source.clone(),
            });
        }
    }
    Ok(WorkProjection {
        view_revision: None,
        personal_revision: None,
        schema_version: 1,
        project_id: project.id.clone(),
        environment: project.environment.clone(),
        read_at: read_at.into(),
        sources: vec![WorkSource {
            kind: "github".into(),
            state: github.state.clone(),
            coverage: github.coverage.clone(),
            verified_at: github.observed_at.clone(),
            error: github.error.clone(),
        }],
        items,
    })
}
