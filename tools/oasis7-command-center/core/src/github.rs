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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub metadata: Option<PullMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PullMetadata {
    pub observed_at: String,
    pub merged_at: Option<String>,
    pub updated_at: Option<String>,
    pub body: String,
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
    pub fn append_page(
        &mut self,
        root: &Path,
        explicit: Option<&str>,
        page: u32,
    ) -> Result<GitHubSnapshot> {
        if !(2..=10).contains(&page) {
            return Err(Error::Invalid(
                "PR 继续读取每次一页，总窗口最多十页；请按已知身份补读".into(),
            ));
        }
        let repo = self
            .current
            .repository
            .clone()
            .ok_or_else(|| Error::Invalid("请先读取仓库身份".into()))?;
        let path = program(explicit)?;
        let raw = api(
            &path,
            &format!(
                "repos/{repo}/pulls?state=all&sort=updated&direction=desc&per_page=100&page={page}"
            ),
            root,
        )?;
        let pulls = decode_pulls(&raw)?;
        let count = pulls.len();
        for pull in pulls {
            self.current.pulls.retain(|old| old.id != pull.id);
            self.current.pulls.push(pull);
        }
        self.current.coverage = format!(
            "REST 已显式读取至第 {page} 页（各页为独立时刻）；本页 {count}/100，保留 {} 个不同 PR；{}。详情及评审请在 GitHub 核对。",
            self.current.pulls.len(),
            if count == 100 {
                "可能仍有后续页"
            } else {
                "本页未满，列表可在读取中变化"
            }
        );
        self.current.state = "partial".into();
        self.current.error = None;
        Ok(self.snapshot())
    }
    pub fn read_known_pull(
        &mut self,
        root: &Path,
        explicit: Option<&str>,
        number: u64,
    ) -> Result<GitHubSnapshot> {
        if number == 0 {
            return Err(Error::Invalid("PR 编号无效".into()));
        }
        let repo = self
            .current
            .repository
            .clone()
            .ok_or_else(|| Error::Invalid("请先读取仓库身份".into()))?;
        let raw = api(
            &program(explicit)?,
            &format!("repos/{repo}/pulls/{number}"),
            root,
        )?;
        let mut pulls = decode_pulls(&serde_json::json!([raw]))?;
        let pull = pulls.remove(0);
        if pull.id != number.to_string()
            || pull.url != format!("https://github.com/{repo}/pull/{number}")
        {
            return Err(Error::Invalid("PR 补读来源身份不匹配".into()));
        }
        self.current.pulls.retain(|p| p.id != pull.id);
        self.current.pulls.push(pull);
        self.current.state = "partial".into();
        self.current.error = None;
        Ok(self.snapshot())
    }
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
        snapshot.coverage = format!(
            "缓存 PR 窗口，保留 {} 项最近成功读取结果；非全仓库清单。",
            snapshot.pulls.len()
        );
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
        let retry = message
            .split("retry-after-seconds:")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<u32>().ok());
        match retry {
            Some(seconds)=>format!("GitHub REST 读取触及限额；来源要求至少等待 {seconds} 秒，下次可重读 Unix 时间 {}。未自动重试或回退 GraphQL，保留最近成功事实。", crate::now().parse::<u64>().unwrap_or_default()+u64::from(seconds)),
            None=>"GitHub REST 读取触及限额；来源未提供可用重读时间。未自动重试或回退 GraphQL，请稍后显式刷新，保留最近成功事实。".into()
        }
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
    api_before(program, endpoint, root, std::time::Instant::now())
}
fn api_before(
    program: &str,
    endpoint: &str,
    root: &Path,
    start: std::time::Instant,
) -> Result<Value> {
    let bytes = process::run_for(
        program,
        &[
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            endpoint,
            "--include",
        ],
        root,
        std::time::Duration::from_secs(30).saturating_sub(start.elapsed()),
    )?;
    // gh --include carries HTTP metadata; fixture/older wrappers may return JSON only.
    let body = if bytes.starts_with(b"HTTP/") {
        bytes
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| &bytes[i + 4..])
            .ok_or_else(|| Error::Invalid("HTTP headers incomplete".into()))?
    } else {
        &bytes
    };
    Ok(serde_json::from_slice(body)?)
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
    let deadline = std::time::Instant::now();
    let path = program(explicit)?;
    let origin = process::run("git", &["remote", "get-url", "origin"], root)?;
    let selected = origin_repository(&String::from_utf8_lossy(&origin))?;
    let repo = api_before(&path, &format!("repos/{selected}"), root, deadline)?;
    let repository = field(&repo, "full_name")?;
    if !valid_repository(&repository)
        || repo["html_url"].as_str() != Some(format!("https://github.com/{repository}").as_str())
    {
        return Err(Error::Invalid("invalid REST repository identity".into()));
    }
    let account = api_before(&path, "user", root, deadline)?["login"]
        .as_str()
        .map(str::to_owned);
    let raw = api_before(
        &path,
        &format!(
            "repos/{repository}/pulls?state=all&sort=updated&direction=desc&per_page=100&page=1"
        ),
        root,
        deadline,
    )?;
    let pulls = decode_pulls(&raw)?;
    Ok(GitHubSnapshot {
        state: if pulls.is_empty() { "empty" } else { "partial" }.into(),
        repository: Some(repository),
        account,
        observed_at: None,
        last_attempt: None,
        coverage: format!(
            "REST PR 第 1 页，已读取 {} 项，最多 100 项；{}。详情及评审请在 GitHub 核对。",
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
                metadata: Some(PullMetadata {
                    observed_at: crate::now(),
                    merged_at: pr["merged_at"].as_str().map(str::to_owned),
                    updated_at: pr["updated_at"].as_str().map(str::to_owned),
                    body: pr["body"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(32768)
                        .collect(),
                }),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "github_tests.rs"]
mod tests;
