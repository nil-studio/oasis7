//! Explicit, read-only GitHub access using the user's existing gh credential store.
use crate::{Error, Result, process};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSnapshot {
    pub state: String,
    pub repository: Option<String>,
    pub account: Option<String>,
    pub observed_at: Option<String>,
    pub last_attempt: Option<String>,
    pub coverage: String,
    pub error: Option<String>,
    pub pulls: Vec<PullRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub id: String,
    pub title: String,
    pub url: String,
    pub state: String,
    pub head_oid: String,
    pub review_decision: Option<String>,
    pub checks: Vec<Check>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub checks_source: Option<PullChecksSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    pub state: String,
    pub tested_oid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_oid: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PullChecksSource {
    #[serde(default)]
    pub schema_version: u32,
    pub state: String,
    pub observed_at: Option<String>,
    pub coverage: String,
    pub error: Option<String>,
}

pub struct GitHubClient {
    current: GitHubSnapshot,
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHubClient {
    pub fn new() -> Self {
        Self {
            current: GitHubSnapshot {
                state: "unconnected".into(),
                repository: None,
                account: None,
                observed_at: None,
                last_attempt: None,
                coverage: "未接入 GitHub".into(),
                error: None,
                pulls: vec![],
            },
        }
    }
    /// Publish an already read observation without changing its source health.
    pub fn from_observation(snapshot: GitHubSnapshot) -> Self {
        Self { current: snapshot }
    }
    pub fn snapshot(&self) -> GitHubSnapshot {
        self.current.clone()
    }
    pub fn restore(mut snapshot: GitHubSnapshot) -> Self {
        for pull in &mut snapshot.pulls {
            if pull
                .checks_source
                .as_ref()
                .is_some_and(|source| source.schema_version != 1)
            {
                pull.checks.clear();
                pull.checks_source = None;
            }
        }
        if snapshot.observed_at.is_some() {
            snapshot.state = "stale".into();
        }
        Self { current: snapshot }
    }
    /// A project switch must never display another repository's retained facts.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
    pub fn refresh(&mut self, root: &Path, gh_path: Option<&str>) -> GitHubSnapshot {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();
        self.current.last_attempt = Some(now.clone());
        let result = read(root, gh_path);
        match result {
            Ok(mut candidate) => {
                candidate.observed_at = Some(now.clone());
                candidate.last_attempt = Some(now);
                if candidate.repository == self.current.repository
                    && candidate.account == self.current.account
                {
                    for pull in &mut candidate.pulls {
                        if let Some(previous) = self.current.pulls.iter().find(|old| {
                            old.id == pull.id
                                && old.head_oid == pull.head_oid
                                && old
                                    .checks_source
                                    .as_ref()
                                    .is_some_and(|source| source.schema_version == 1)
                        }) {
                            pull.checks = previous.checks.clone();
                            pull.checks_source = previous.checks_source.clone();
                        }
                    }
                }
                self.current = candidate;
            }
            Err(error) => {
                self.current.state = "error".into();
                self.current.error = Some(public_error(&error));
            }
        }
        self.snapshot()
    }
}

pub fn public_error(error: &Error) -> String {
    // Classify, but never forward stderr, which may include credentials/proxy details.
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("rate limit")
        || message.contains("rate_limit")
        || message.contains("http 429")
    {
        "GitHub REST 读取触及限额；未自动重试或回退 GraphQL。请稍后显式刷新，保留最近成功事实。"
            .into()
    } else {
        "GitHub REST 读取失败；请检查 gh 路径、登录、仓库权限或网络。保留最近成功读取的事实。"
            .into()
    }
}
fn program(explicit: Option<&str>) -> Result<String> {
    if let Some(path) = explicit {
        if !Path::new(path).is_absolute() || !Path::new(path).is_file() {
            return Err(Error::Invalid(
                "gh path must be an absolute file path".into(),
            ));
        }
        return Ok(path.to_owned());
    }
    ["/opt/homebrew/bin/gh", "/usr/local/bin/gh", "/usr/bin/gh"]
        .into_iter()
        .find(|p| Path::new(p).is_file())
        .map(str::to_owned)
        .ok_or_else(|| Error::Invalid("gh is not installed; select its absolute path".into()))
}
fn api(program: &str, endpoint: &str, root: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&process::run(
        program,
        &[
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            endpoint,
        ],
        root,
    )?)?)
}
fn valid_repository(repository: &str) -> bool {
    let parts: Vec<_> = repository.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
}
fn origin_repository(origin: &str) -> Result<String> {
    let repository = [
        "git@github.com:",
        "https://github.com/",
        "ssh://git@github.com/",
    ]
    .into_iter()
    .find_map(|prefix| origin.trim().strip_prefix(prefix))
    .ok_or_else(|| Error::Invalid("select a GitHub origin remote".into()))?;
    let repository = repository.strip_suffix(".git").unwrap_or(repository);
    if !valid_repository(repository) {
        return Err(Error::Invalid("invalid GitHub origin identity".into()));
    }
    Ok(repository.into())
}
fn read(root: &Path, explicit: Option<&str>) -> Result<GitHubSnapshot> {
    let path = program(explicit)?;
    let origin = process::run("git", &["remote", "get-url", "origin"], root)?;
    let selected = origin_repository(&String::from_utf8_lossy(&origin))?;
    let repo = api(&path, &format!("repos/{selected}"), root)?;
    let repository = field(&repo, "full_name")?;
    if !valid_repository(&repository)
        || repo["html_url"].as_str() != Some(format!("https://github.com/{repository}").as_str())
    {
        return Err(Error::Invalid("invalid REST repository identity".into()));
    }
    let account = api(&path, "user", root)?["login"]
        .as_str()
        .map(str::to_owned);
    let raw = api(
        &path,
        &format!(
            "repos/{repository}/pulls?state=all&sort=updated&direction=desc&per_page=100&page=1"
        ),
        root,
    )?;
    let pulls = decode_pulls(&raw)?;
    Ok(GitHubSnapshot {
        state: if pulls.is_empty() { "empty" } else { "partial" }.into(),
        repository: Some(repository),
        account,
        observed_at: None,
        last_attempt: None,
        coverage: format!(
            "REST PR 第 1 页，已读取 {} 项，最多 100 项；{}。CI 按单个 PR 显式读取，评审状态未读取。",
            pulls.len(),
            if pulls.len() == 100 {
                "可能仍有后续页，非全仓库清单"
            } else {
                "仅本次列表窗口"
            }
        ),
        error: None,
        pulls,
    })
}

/// Two bounded REST reads for one explicitly selected, already known PR HEAD.
pub fn read_pull_checks(
    root: &Path,
    explicit: Option<&str>,
    repository: &str,
    head: &str,
) -> Result<(Vec<Check>, String)> {
    if !valid_repository(repository)
        || !matches!(head.len(), 40 | 64)
        || !head.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(Error::Invalid("invalid CI source identity".into()));
    }
    let path = program(explicit)?;
    let runs = api(
        &path,
        &format!("repos/{repository}/commits/{head}/check-runs?filter=latest&per_page=100&page=1"),
        root,
    )?;
    let statuses = api(
        &path,
        &format!("repos/{repository}/commits/{head}/status?per_page=100&page=1"),
        root,
    )?;
    decode_checks(&runs, &statuses, head)
}
fn decode_checks(runs: &Value, statuses: &Value, head: &str) -> Result<(Vec<Check>, String)> {
    let run_list = runs["check_runs"]
        .as_array()
        .ok_or_else(|| Error::Invalid("invalid check-runs page".into()))?;
    let status_list = statuses["statuses"]
        .as_array()
        .ok_or_else(|| Error::Invalid("invalid combined status page".into()))?;
    let mut checks = Vec::new();
    for check in run_list {
        let tested = check["head_sha"].as_str();
        if tested.is_some_and(|sha| sha != head) {
            return Err(Error::Invalid("check source SHA mismatch".into()));
        }
        checks.push(Check {
            name: field(check, "name")?,
            state: check["conclusion"]
                .as_str()
                .or(check["status"].as_str())
                .unwrap_or("UNKNOWN")
                .to_ascii_uppercase(),
            // REST associates this check/status with a commit; it does not
            // prove a workflow's checkout SHA or tested merge tree.
            tested_oid: None,
            source_oid: tested.map(str::to_owned),
            url: check["html_url"].as_str().map(str::to_owned),
        });
    }
    let tested = statuses["sha"].as_str();
    if tested.is_some_and(|sha| sha != head) {
        return Err(Error::Invalid("status source SHA mismatch".into()));
    }
    for status in status_list {
        checks.push(Check {
            name: field(status, "context")?,
            state: field(status, "state")?.to_ascii_uppercase(),
            // REST associates this check/status with a commit; it does not
            // prove a workflow's checkout SHA or tested merge tree.
            tested_oid: None,
            source_oid: tested.map(str::to_owned),
            url: status["target_url"].as_str().map(str::to_owned),
        });
    }
    let coverage = format!(
        "REST check-runs 第 1 页 {}/{} 项；commit statuses 第 1 页 {}/{} 项；每来源最多 100 项，未读后续页及 required 定义；关联提交不证明实际 checkout 的测试对象，不能据此认定全部检查通过。",
        run_list.len(),
        runs["total_count"],
        status_list.len(),
        statuses["total_count"]
    );
    Ok((checks, coverage))
}

fn field(value: &Value, key: &str) -> Result<String> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::Invalid(format!("missing GitHub field {key}")))
}

pub fn decode_pulls(raw: &Value) -> Result<Vec<PullRequest>> {
    raw.as_array()
        .ok_or_else(|| Error::Invalid("invalid GitHub PR list".into()))?
        .iter()
        .map(|pr| {
            let id = pr["number"]
                .as_u64()
                .ok_or_else(|| Error::Invalid("missing PR number".into()))?
                .to_string();
            Ok(PullRequest {
                id,
                title: field(pr, "title")?,
                url: field(pr, "html_url")?,
                state: if pr["merged_at"].is_string() {
                    "MERGED".into()
                } else {
                    field(pr, "state")?.to_ascii_uppercase()
                },
                head_oid: field(&pr["head"], "sha")?,
                review_decision: None,
                checks: Vec::new(),
                checks_source: None,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "github_tests.rs"]
mod tests;
