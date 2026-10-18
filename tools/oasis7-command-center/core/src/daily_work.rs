//! Versioned personal records, separate from source snapshots and runtime effects.
use crate::{
    CommandCenter, Error, Result, context, index, load, now, save, work_projection::WorkRef,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[path = "daily_work_types.rs"]
mod types;
pub use types::*;

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn legacy(core: &CommandCenter) -> Result<String> {
    let associations: Vec<crate::domain::WorkAssociation> =
        load(&core.db, "personal", &core.personal_key("associations"))?
            .unwrap_or_else(|| core.current.associations.clone());
    let decisions: Vec<crate::domain::DecisionRecord> =
        load(&core.db, "personal", &core.personal_key("decisions"))?
            .unwrap_or_else(|| core.current.decisions.clone());
    Ok(index::digest(&serde_json::to_vec(&(
        associations,
        decisions,
    ))?))
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Family<T> {
    schema_version: u32,
    revision: u64,
    value: T,
}
fn read_family<T: serde::de::DeserializeOwned>(
    core: &CommandCenter,
    kind: &str,
    revision: u64,
) -> Result<T> {
    let value: Family<T> = load(&core.db, "personal", &core.personal_key(kind))?
        .ok_or_else(|| invalid("个人记录分组缺失，保留原值，请修复或使用匹配备份"))?;
    if value.schema_version != 1 || value.revision != revision {
        return Err(invalid("个人记录分组版本或修订不兼容，未以空值覆盖"));
    }
    Ok(value.value)
}
fn persist(core: &CommandCenter, db: &rusqlite::Connection, state: &DailyWorkState) -> Result<()> {
    let mut header = state.clone();
    header.relations.clear();
    header.external.clear();
    header.worktrees.clear();
    header.decisions.clear();
    header.attention.clear();
    save(
        db,
        "personal",
        &core.personal_key("work_navigation_v1"),
        &header,
    )?;
    save(
        db,
        "personal",
        &core.personal_key("work_relations_v1"),
        &Family {
            schema_version: 1,
            revision: state.revision,
            value: (&state.relations, &state.external, &state.worktrees),
        },
    )?;
    save(
        db,
        "personal",
        &core.personal_key("decision_followthrough_v1"),
        &Family {
            schema_version: 1,
            revision: state.revision,
            value: &state.decisions,
        },
    )?;
    save(
        db,
        "personal",
        &core.personal_key("work_attention_v1"),
        &Family {
            schema_version: 1,
            revision: state.revision,
            value: &state.attention,
        },
    )?;
    Ok(())
}
pub fn decision_digest(value: &crate::domain::DecisionRecord) -> Result<String> {
    Ok(index::digest(&serde_json::to_vec(value)?))
}
pub(crate) fn record_owned_legacy_edit(
    core: &CommandCenter,
    db: &rusqlite::Connection,
    decisions: &[crate::domain::DecisionRecord],
) -> Result<()> {
    let key = core.personal_key("work_navigation_v1");
    let Some(mut value) = load::<serde_json::Value>(db, "personal", &key)? else {
        return Ok(());
    };
    if value["schemaVersion"] == 1 && value["legacyDigest"].as_str() == Some(legacy(core)?.as_str())
    {
        value["legacyDigest"] = serde_json::json!(index::digest(&serde_json::to_vec(&(
            &core.current.associations,
            decisions
        ))?));
        save(db, "personal", &key, &value)?;
    }
    Ok(())
}
pub fn valid_work(work: &WorkRef, project_id: &str, environment: &str) -> bool {
    match work {
        WorkRef::PullRequest {
            host,
            repository,
            number,
        } => {
            host == "github.com"
                && repository.split('/').count() == 2
                && repository.split('/').all(|s| {
                    !s.is_empty()
                        && s != "."
                        && s != ".."
                        && s.chars()
                            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
                })
                && number.parse::<u64>().is_ok_and(|n| n > 0)
        }
        WorkRef::Session {
            namespace,
            project_id: id,
            environment: env,
            thread_id,
        } => {
            matches!(namespace.as_str(), "owned-stdio" | "external-locator")
                && id == project_id
                && env == environment
                && !thread_id.is_empty()
                && thread_id.len() < 512
        }
    }
}
fn valid_scoped_work(work: &WorkRef, state: &DailyWorkState) -> bool {
    valid_work(work, &state.project_id, &state.environment)
        || state
            .worktrees
            .iter()
            .any(|r| valid_work(work, &r.project_id, &r.environment))
}
impl CommandCenter {
    fn private_work(&self, work: &WorkRef) -> Result<bool> {
        let WorkRef::Session {
            namespace,
            project_id,
            environment,
            thread_id,
        } = work
        else {
            return Ok(false);
        };
        if namespace != "owned-stdio" {
            return Ok(false);
        }
        Ok(self.runtime_privacy.as_ref().unwrap_or(&self.db).query_row("SELECT EXISTS(SELECT 1 FROM cc_runtime_private_inputs WHERE scope=?1 AND thread=?2)",rusqlite::params![format!("{project_id}:{environment}"),thread_id],|r|r.get(0))?)
    }
    pub fn public_daily_work(&self) -> Result<DailyWorkState> {
        let mut state = self.daily_work()?;
        if state
            .navigation
            .selected
            .as_ref()
            .is_some_and(|w| self.private_work(w).unwrap_or(true))
        {
            state.navigation.selected = None;
            state.navigation.draft.clear();
            state.navigation.draft_purpose.clear();
            state.navigation.draft_scope.clear();
            state.navigation.start_intent_id = None;
        }
        let mut retained = vec![];
        for item in state.navigation.pinned {
            if !self.private_work(&item)? {
                retained.push(item);
            }
        }
        state.navigation.pinned = retained;
        state.relations = state
            .relations
            .into_iter()
            .map(|item| {
                self.private_work(&item.work)
                    .map(|private| (!private).then_some(item))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        state.external = state
            .external
            .into_iter()
            .map(|item| {
                self.private_work(&item.work)
                    .map(|private| (!private).then_some(item))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        state.attention = state
            .attention
            .into_iter()
            .map(|item| {
                self.private_work(&item.work)
                    .map(|private| (!private).then_some(item))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(state)
    }
    pub fn inspect_worktree(path: &Path) -> Result<crate::domain::Project> {
        index::project(path)
    }
    pub fn remember_worktree(
        &mut self,
        reference: crate::domain::Project,
        expected_revision: u64,
    ) -> Result<DailyWorkState> {
        let tx = self.db.unchecked_transaction()?;
        let mut state = self.daily_work()?;
        let project = self
            .current
            .project
            .as_ref()
            .ok_or_else(|| invalid("请先选择项目"))?;
        if state.revision != expected_revision
            || project.git_common_dir != reference.git_common_dir
            || state.worktrees.len() >= 8
        {
            return Err(invalid(
                "阅读范围已变化、不是同一仓库或超过八个显式工作树预算",
            ));
        }
        context::check_project(&self.db, Path::new(&reference.path))?;
        tx.execute(
            "INSERT OR IGNORE INTO ordinary_projects(path) VALUES (?1)",
            [&reference.path],
        )?;
        state.worktrees.retain(|r| r.project_id != reference.id);
        state.worktrees.push(WorktreeReference {
            project_id: reference.id,
            path: reference.path,
            environment: reference.environment,
            head: reference.head,
            verified_at: now(),
        });
        state.revision += 1;
        persist(self, &tx, &state)?;
        tx.commit()?;
        Ok(state)
    }
    pub fn daily_work(&self) -> Result<DailyWorkState> {
        let project = self
            .current
            .project
            .as_ref()
            .ok_or_else(|| invalid("请先选择项目"))?;
        context::check_project(&self.db, Path::new(&project.path))?;
        let key = self.personal_key("work_navigation_v1");
        // A future or corrupt personal record is an error, never a default overwriting it.
        let mut state =
            load::<DailyWorkState>(&self.db, "personal", &key)?.unwrap_or(DailyWorkState {
                schema_version: 1,
                project_id: project.id.clone(),
                environment: project.environment.clone(),
                revision: 0,
                navigation: WorkNavigation::default(),
                relations: vec![],
                decisions: vec![],
                attention: vec![],
                external: vec![],
                worktrees: vec![],
                legacy_digest: legacy(self)?,
                legacy_changed: false,
            });
        if state.schema_version != 1
            || state.project_id != project.id
            || state.environment != project.environment
        {
            return Err(invalid(
                "个人工作记录版本或范围不兼容，已保留原值；请使用匹配版本修复",
            ));
        }
        if state.revision > 0 {
            (state.relations, state.external, state.worktrees) =
                read_family(self, "work_relations_v1", state.revision)?;
            state.decisions = read_family(self, "decision_followthrough_v1", state.revision)?;
            state.attention = read_family(self, "work_attention_v1", state.revision)?;
        }
        if state.revision == 0 {
            for relation in &self.current.associations {
                if let Some((repository, number)) = relation
                    .work_url
                    .strip_prefix("https://github.com/")
                    .and_then(|s| s.split_once("/pull/"))
                {
                    let work = WorkRef::PullRequest {
                        host: "github.com".into(),
                        repository: repository.into(),
                        number: number.into(),
                    };
                    if !valid_work(&work, &project.id, &project.environment) {
                        continue;
                    }
                    for section_id in &relation.goal_section_ids {
                        state.relations.push(WorkRelation {
                            id: index::digest(&serde_json::to_vec(&(
                                &work,
                                section_id,
                                &state.environment,
                            ))?),
                            work: work.clone(),
                            section_id: section_id.clone(),
                            source_revision: self
                                .current
                                .sections
                                .iter()
                                .find(|s| &s.id == section_id)
                                .map(|s| s.revision.clone())
                                .unwrap_or_default(),
                            confirmed_at: relation.confirmed_at.clone(),
                            removed_at: None,
                            previous: vec![],
                        });
                    }
                }
            }
        }
        state.legacy_changed = state.legacy_digest != legacy(self)?;
        for record in &mut state.decisions {
            let decision = self
                .current
                .decisions
                .iter()
                .find(|d| d.id == record.decision_id);
            for evidence in [&mut record.formal, &mut record.implementation]
                .into_iter()
                .flatten()
            {
                let changed = decision.is_none_or(|d| {
                    decision_digest(d).ok().as_ref() != Some(&evidence.decision_digest)
                });
                let source_changed = if evidence.state == "verified" {
                    let local = evidence.source.as_ref().is_some_and(|target| {
                        self.current.sections.iter().any(|s| {
                            &s.id == target && evidence.revision.as_ref() == Some(&s.revision)
                        })
                    });
                    let remote = self.load_github_cache()?.is_some_and(|g| {
                        g.error.is_none()
                            && evidence.source.as_ref().is_some_and(|target| {
                                g.pulls.iter().any(|p| {
                                    &p.url == target
                                        && evidence.revision.as_ref() == Some(&p.head_oid)
                                })
                            })
                    });
                    !local && !remote
                } else {
                    false
                };
                if (changed || source_changed) && evidence.state != "needsReview" {
                    record.history.push(evidence.clone());
                    evidence.state = "needsReview".into();
                }
            }
        }
        Ok(state)
    }
    pub fn save_daily_work(&mut self, request: DailyWorkRequest) -> Result<DailyWorkState> {
        self.reload_personal()?;
        let tx = self.db.unchecked_transaction()?;
        let mut state = self.daily_work()?;
        let mut references = vec![];
        match &request.action {
            DailyWorkAction::Navigate { navigation } => {
                references.extend(navigation.selected.iter());
                references.extend(&navigation.pinned);
            }
            DailyWorkAction::Relate { work, .. } => references.push(work),
            DailyWorkAction::Attention { item } => references.push(&item.work),
            DailyWorkAction::External { item } => references.push(&item.work),
            _ => {}
        }
        for work in references {
            if self.private_work(work)? {
                return Err(invalid("私有线程及其派生记录不能进入普通工作记录"));
            }
        }
        if request.project_id != state.project_id
            || request.environment != state.environment
            || request.expected_revision != state.revision
        {
            return Err(invalid("个人工作记录已变化，未保存；请重读后核对差异"));
        }
        if state.legacy_changed
            && !matches!(
                request.action,
                DailyWorkAction::AcceptLegacy {} | DailyWorkAction::Navigate { .. }
            )
        {
            return Err(invalid(
                "旧版个人记录有新修改，请先核对并接纳旧记录变化；新关系和证据不会被覆盖",
            ));
        }
        match request.action {
            DailyWorkAction::Navigate { mut navigation } => {
                if navigation.draft.len() > 32768
                    || navigation.draft_purpose.len() > 4096
                    || navigation.draft_scope.len() > 4096
                    || navigation.filter.len() > 4096
                    || navigation.pinned.len() > 100
                    || !navigation.detail_scroll_top.is_finite()
                    || !(0.0..=1e9).contains(&navigation.detail_scroll_top)
                    || !navigation.scroll_top.is_finite()
                    || !(0.0..=1e9).contains(&navigation.scroll_top)
                    || !["", "direction", "execution", "attention", "rules", "tools"]
                        .contains(&navigation.return_view.as_str())
                    || navigation
                        .selected
                        .as_ref()
                        .is_some_and(|w| !valid_scoped_work(w, &state))
                    || navigation
                        .pinned
                        .iter()
                        .any(|w| !valid_scoped_work(w, &state))
                {
                    return Err(invalid("工作位置或普通草稿超过范围/预算"));
                }
                if !navigation.retain_draft {
                    navigation.draft.clear();
                    navigation.draft_purpose.clear();
                    navigation.draft_scope.clear();
                }
                state.navigation = navigation;
            }
            DailyWorkAction::Relate {
                work,
                section_id,
                replace_id,
            } => {
                if !valid_scoped_work(&work, &state) {
                    return Err(invalid("工作身份与当前范围不匹配"));
                }
                let section = self
                    .current
                    .sections
                    .iter()
                    .find(|s| s.id == section_id)
                    .ok_or_else(|| invalid("条款未读取；请选择当前可核实的条款"))?;
                let id = index::digest(&serde_json::to_vec(&(
                    &work,
                    &section_id,
                    &state.environment,
                ))?);
                let mut previous = vec![];
                if let Some(old_id) = replace_id {
                    let old = state
                        .relations
                        .iter_mut()
                        .find(|r| r.id == old_id)
                        .ok_or_else(|| invalid("原关系不存在，请重新读取"))?;
                    previous.push(serde_json::to_string(old)?);
                    old.removed_at = Some(now());
                }
                if let Some(old) = state.relations.iter_mut().find(|r| r.id == id) {
                    previous.push(serde_json::to_string(old)?);
                    previous.extend(old.previous.clone());
                }
                state.relations.retain(|r| r.id != id);
                state.relations.push(WorkRelation {
                    id,
                    work,
                    section_id,
                    source_revision: section.revision.clone(),
                    confirmed_at: now(),
                    removed_at: None,
                    previous,
                });
            }
            DailyWorkAction::RemoveRelation { id } => {
                let old = state
                    .relations
                    .iter_mut()
                    .find(|r| r.id == id)
                    .ok_or_else(|| invalid("原关系不存在"))?;
                old.previous.push(serde_json::to_string(&(
                    &old.section_id,
                    &old.source_revision,
                    &old.confirmed_at,
                ))?);
                old.removed_at = Some(now());
            }
            DailyWorkAction::SyncDecision {
                decision_id,
                track,
                mut evidence,
            } => {
                let decision = self
                    .current
                    .decisions
                    .iter()
                    .find(|d| d.id == decision_id && d.confirmed_at.is_some())
                    .ok_or_else(|| invalid("请先保存明确决定"))?;
                if !["formal", "implementation"].contains(&track.as_str())
                    || ![
                        "unknown",
                        "notRequired",
                        "pending",
                        "evidenceLinked",
                        "verified",
                        "needsReview",
                    ]
                    .contains(&evidence.state.as_str())
                    || evidence.scope.trim().is_empty()
                    || evidence.conclusion.trim().is_empty()
                {
                    return Err(invalid("请选择同步轨道、状态并填写适用范围和核实结论"));
                }
                if matches!(evidence.state.as_str(), "verified" | "evidenceLinked") {
                    let target = evidence
                        .source
                        .as_deref()
                        .ok_or_else(|| invalid("核实需要真实来源"))?;
                    let revision = evidence
                        .revision
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| invalid("核实需要具体来源版本"))?;
                    let local = self
                        .current
                        .sections
                        .iter()
                        .any(|s| s.id == target && s.revision == revision);
                    let github = self.load_github_cache()?.is_some_and(|g| {
                        g.error.is_none()
                            && g.pulls
                                .iter()
                                .any(|p| p.url == target && p.head_oid == revision)
                    });
                    if !local && !github {
                        return Err(invalid(
                            "该来源版本未读取或已变化；请先回读来源，不能仅凭陈述核实",
                        ));
                    }
                }
                if evidence.remaining.len() > 100
                    || evidence
                        .remaining
                        .iter()
                        .any(|s| s.trim().is_empty() || s.len() > 4096)
                {
                    return Err(invalid("剩余同步项无效"));
                }
                evidence.checked_at = now();
                evidence.decision_digest = decision_digest(decision)?;
                let entry = match state
                    .decisions
                    .iter()
                    .position(|d| d.decision_id == decision_id)
                {
                    Some(i) => &mut state.decisions[i],
                    None => {
                        state.decisions.push(DecisionFollowthrough {
                            decision_id,
                            formal: None,
                            implementation: None,
                            history: vec![],
                            following: true,
                        });
                        state.decisions.last_mut().unwrap()
                    }
                };
                let slot = if track == "formal" {
                    &mut entry.formal
                } else {
                    &mut entry.implementation
                };
                if let Some(previous) = slot.take() {
                    entry.history.push(previous);
                }
                *slot = Some(evidence);
            }
            DailyWorkAction::FollowDecision {
                decision_id,
                following,
            } => {
                if !self.current.decisions.iter().any(|d| d.id == decision_id) {
                    return Err(invalid("决定不属于当前项目"));
                }
                if let Some(entry) = state
                    .decisions
                    .iter_mut()
                    .find(|d| d.decision_id == decision_id)
                {
                    entry.following = following;
                } else {
                    state.decisions.push(DecisionFollowthrough {
                        decision_id,
                        formal: None,
                        implementation: None,
                        history: vec![],
                        following,
                    });
                }
            }
            DailyWorkAction::Attention { mut item } => {
                if !valid_scoped_work(&item.work, &state)
                    || item.id.is_empty()
                    || item.question.trim().is_empty()
                    || !["open", "awaitingEvidence", "resolved", "superseded"]
                        .contains(&item.state.as_str())
                {
                    return Err(invalid("个人问题身份或状态无效"));
                }
                if let Some(old) = state.attention.iter().find(|a| a.id == item.id) {
                    if old.work != item.work
                        || old.question != item.question
                        || old.source_revision != item.source_revision
                    {
                        return Err(invalid("实质新问题须使用新身份，不能覆盖原处理历史"));
                    }
                    item.history = old.history.clone();
                    item.history.push(serde_json::to_string(old)?);
                    if matches!(old.state.as_str(), "resolved" | "superseded")
                        && item.state == "open"
                    {
                        return Err(invalid("已处理问题不能因刷新重新打开；请记录新的问题"));
                    }
                }
                item.updated_at = now();
                state.attention.retain(|a| a.id != item.id);
                state.attention.push(item);
            }
            DailyWorkAction::External { mut item } => {
                if !valid_scoped_work(&item.work, &state)
                    || item.title.trim().is_empty()
                    || item.locator.trim().is_empty()
                    || item.locator.len() > 4096
                    || item
                        .source_url
                        .as_deref()
                        .is_some_and(|s| !s.starts_with("https://github.com/"))
                {
                    return Err(invalid(
                        "外部现场需要明确身份、标题及原工具定位；仅接受 GitHub HTTPS 导航",
                    ));
                }
                item.recorded_at = now();
                state.external.retain(|e| e.id != item.id);
                state.external.push(item);
            }
            DailyWorkAction::RemoveExternal { id } => state.external.retain(|e| e.id != id),
            DailyWorkAction::RemoveWorktree { project_id } => {
                state.worktrees.retain(|w| w.project_id != project_id)
            }
            DailyWorkAction::AcceptLegacy {} => {
                state.legacy_digest = legacy(self)?;
                state.legacy_changed = false;
            }
        }
        if state.relations.len() > 1000
            || state.attention.len() > 1000
            || state.external.len() > 100
            || serde_json::to_vec(&state)?.len() > 1024 * 1024
        {
            return Err(invalid("个人工作记录达到容量预算，请整理历史后重试"));
        }
        state.revision += 1;
        persist(self, &tx, &state)?;
        tx.commit()?;
        Ok(state)
    }
}
