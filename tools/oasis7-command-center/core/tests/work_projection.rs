use oasis7_command_center_core::{
    domain::Snapshot,
    github::GitHubSnapshot,
    work_projection::{WorkRef, project_work},
};
use serde_json::json;
fn snapshot() -> Snapshot {
    serde_json::from_value(json!({
    "project":{"id":"p","path":"/repo","gitCommonDir":"/repo/.git","branch":"same","head":"h","environment":"local"},
    "sources":[],"sections":[],"decisions":[],"readingPositions":[],"preferences":{"projectPath":null,"view":"direction","query":""},"git":{"status":"","worktrees":""},
    "associations":[{"workUrl":"https://github.com/o/r/pull/1","goalSectionIds":["goal-a","goal-b"],"confirmedAt":"10"}]
})).unwrap()
}
fn github() -> GitHubSnapshot {
    serde_json::from_value(json!({
    "state":"partial","repository":"o/r","account":"reader","observedAt":"20","lastAttempt":"21","coverage":"recent 100 only","error":null,
    "pulls":[{"id":"1","title":"No issue required","url":"https://github.com/o/r/pull/1","state":"MERGED","headOid":"h","reviewDecision":null,"checks":[{"name":"test","state":"SUCCESS","testedOid":null,"url":null}]}]
})).unwrap()
}
#[test]
fn pr_without_issue_preserves_two_goals_and_unknown_test_and_event_times() {
    let projection = project_work(&snapshot(), &github(), "30").unwrap();
    let item = &projection.items[0];
    assert!(matches!(item.reference, WorkRef::PullRequest { .. }));
    assert_eq!(item.goals.len(), 2);
    assert!(item.goals.iter().all(|g| g.title.is_none()));
    assert_eq!(item.result.value, "PR MERGED");
    assert!(item.remaining.contains("尚未核实"));
    assert!(item.checks[0].tested_oid.is_none());
    assert_eq!(item.result.evidence.verified_at.as_deref(), Some("20"));
    assert!(item.result.evidence.event_at.is_none());
    let later = project_work(&snapshot(), &github(), "40").unwrap();
    assert_eq!(
        later.items[0].result.evidence.verified_at,
        item.result.evidence.verified_at
    );
    assert_eq!(later.read_at, "40");
}
#[test]
fn source_failure_retains_prs_without_a_session_source() {
    let mut github = github();
    github.state = "error".into();
    github.error = Some("offline".into());
    let projection = project_work(&snapshot(), &github, "30").unwrap();
    assert_eq!(projection.items.len(), 1);
    assert_eq!(projection.sources[0].state, "error");
    assert_eq!(projection.sources[0].verified_at.as_deref(), Some("20"));
    assert_eq!(projection.sources.len(), 1);
    github.pulls[0].url = "https://github.com/another/repo/pull/1".into();
    assert!(
        project_work(&snapshot(), &github, "30")
            .unwrap()
            .items
            .is_empty()
    );
}
