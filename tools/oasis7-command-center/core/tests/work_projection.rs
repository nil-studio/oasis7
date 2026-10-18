use oasis7_command_center_core::{
    codex::RuntimeSnapshot,
    domain::Snapshot,
    github::GitHubSnapshot,
    work_projection::{WorkRef, project_work},
};
use serde_json::{Value, json};
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
fn session(thread: &str, instance: &str, state: &str) -> Value {
    json!({"purpose":"same purpose","scope":"same branch","model":null,"basisRevision":"h","threadId":thread,"instanceId":instance,"turnId":"turn","state":state,"owned":true,"backgroundState":"active","privateSources":[]})
}
fn runtime(sessions: Vec<Value>) -> RuntimeSnapshot {
    serde_json::from_value(json!({"projectId":"p","environment":"local","enabled":true,"experimental":true,"state":"connected","version":null,"instanceId":"i","capabilities":[],"coverage":"owned sessions only","error":null,"sessions":sessions,"requests":[],"operations":[],"logs":[],"logsTruncated":false,"exitWaiting":false})).unwrap()
}
#[test]
fn pr_without_issue_preserves_two_goals_and_unknown_test_and_event_times() {
    let projection = project_work(&snapshot(), &github(), None, "30").unwrap();
    let item = &projection.items[0];
    assert!(matches!(item.reference, WorkRef::PullRequest { .. }));
    assert_eq!(item.goals.len(), 2);
    assert!(item.goals.iter().all(|g| g.title.is_none()));
    assert_eq!(item.result.value, "PR MERGED");
    assert!(item.remaining.contains("尚未核实"));
    assert!(item.checks[0].tested_oid.is_none());
    assert_eq!(item.result.evidence.verified_at.as_deref(), Some("20"));
    assert!(item.result.evidence.event_at.is_none());
    let later = project_work(&snapshot(), &github(), None, "40").unwrap();
    assert_eq!(
        later.items[0].result.evidence.verified_at,
        item.result.evidence.verified_at
    );
    assert_eq!(later.read_at, "40");
}
#[test]
fn same_branch_sessions_and_multiple_run_bindings_do_not_overwrite() {
    let runtime = runtime(vec![
        session("t1", "i1", "active"),
        session("t2", "i1", "idle"),
        session("t1", "i2", "idle"),
    ]);
    let projection = project_work(&snapshot(), &github(), Some(&runtime), "30").unwrap();
    assert_eq!(projection.items.len(), 3);
    assert_eq!(projection.items[1].runs.len(), 2);
    assert_eq!(projection.items[1].runs[0].state, "active");
    assert_eq!(projection.items[1].runs[1].instance_id, "i2");
    assert_ne!(projection.items[1].reference, projection.items[2].reference);
}
#[test]
fn private_thread_exclusion_covers_all_bindings_before_serialization() {
    let mut private = session("secret", "i2", "idle");
    private["purpose"] = json!("secret derived purpose");
    private["privateSources"] = json!([{"bindingId":"b","bindingVersion":1,"entryId":"e","revision":"r","scope":"selected"}]);
    let runtime = runtime(vec![
        session("secret", "i1", "active"),
        private,
        session("public", "i1", "idle"),
    ]);
    let projection = project_work(&snapshot(), &github(), Some(&runtime), "30").unwrap();
    let encoded = serde_json::to_string(&projection).unwrap();
    assert!(!encoded.contains("secret"));
    assert!(!encoded.contains("privateSources"));
    assert_eq!(projection.items.len(), 2);
}
#[test]
fn source_failure_retains_prs_and_cross_scope_runtime_is_excluded() {
    let mut github = github();
    github.state = "error".into();
    github.error = Some("offline".into());
    let mut runtime = runtime(vec![session("wrong", "i", "active")]);
    runtime.environment = "other".into();
    let projection = project_work(&snapshot(), &github, Some(&runtime), "30").unwrap();
    assert_eq!(projection.items.len(), 1);
    assert_eq!(projection.sources[0].state, "error");
    assert_eq!(projection.sources[0].verified_at.as_deref(), Some("20"));
    assert_eq!(projection.sources[1].state, "unconnected");
    github.pulls[0].url = "https://github.com/another/repo/pull/1".into();
    assert!(
        project_work(&snapshot(), &github, None, "30")
            .unwrap()
            .items
            .is_empty()
    );
}
