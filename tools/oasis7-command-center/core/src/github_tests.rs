use super::*;
use serde_json::json;
#[cfg(unix)]
#[test]
fn read_only_rest_window_and_offline_restore() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["remote", "add", "origin", "git@github.com:old/r.git"],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(tmp.path())
                .status()
                .unwrap()
                .success()
        );
    }
    let program = tmp.path().join("gh-fixture");
    let calls = tmp.path().join("calls");
    std::fs::write(&program, format!(r##"#!/bin/sh
printf '%s\n' "$*" >> '{}'
[ "$1" = api ] && [ "$2" = --hostname ] && [ "$3" = github.com ] && [ "$4" = --method ] && [ "$5" = GET ] || exit 2
case "$6" in
repos/old/r) echo '{{"full_name":"o/r","html_url":"https://github.com/o/r"}}' ;;
user) echo '{{"login":"reader"}}' ;;
'repos/o/r/pulls?state=all&sort=updated&direction=desc&per_page=100&page=1') echo '[{{"number":1,"title":"purpose","html_url":"https://github.com/o/r/pull/1","state":"open","head":{{"sha":"abc"}},"merged_at":null}}]' ;;
*) exit 2 ;;
esac
"##, calls.display())).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut client = GitHubClient::new();
    let read = client.refresh(tmp.path(), program.to_str());
    assert_eq!(read.state, "partial");
    assert_eq!(read.repository.as_deref(), Some("o/r"));
    assert_eq!(read.account.as_deref(), Some("reader"));
    assert_eq!(read.pulls.len(), 1);
    assert!(read.pulls[0].checks.is_empty());
    assert!(read.pulls[0].checks_source.is_none());
    let calls = std::fs::read_to_string(calls).unwrap();
    assert_eq!(calls.lines().count(), 3);
    assert!(!calls.contains("graphql"));
    assert!(!calls.contains("check-runs"));
    client.current.pulls[0].checks_source = Some(PullChecksSource {
        schema_version: 1,
        state: "partial".into(),
        observed_at: Some("old-ci-time".into()),
        coverage: "first page".into(),
        error: None,
    });
    client.current.pulls[0].checks = vec![Check {
        name: "test".into(),
        state: "SUCCESS".into(),
        tested_oid: None,
        source_oid: Some("abc".into()),
        url: None,
    }];
    let same = client.refresh(tmp.path(), program.to_str());
    assert_eq!(
        same.pulls[0]
            .checks_source
            .as_ref()
            .unwrap()
            .observed_at
            .as_deref(),
        Some("old-ci-time")
    );
    let source = std::fs::read_to_string(&program).unwrap();
    std::fs::write(&program, source.replace("abc", "def")).unwrap();
    let changed = client.refresh(tmp.path(), program.to_str());
    assert!(changed.pulls[0].checks.is_empty());
    assert!(changed.pulls[0].checks_source.is_none());
    let mut restored = GitHubClient::restore(read);
    assert_eq!(restored.snapshot().state, "stale");
    std::fs::remove_file(&program).unwrap();
    let failed = restored.refresh(tmp.path(), program.to_str());
    assert_eq!(failed.state, "error");
    assert_eq!(failed.pulls.len(), 1);
    assert!(failed.observed_at.is_some());
}
#[test]
fn rest_pr_merge_fact_and_unknown_reviews_are_independent() {
    let rows = decode_pulls(&json!([{"number":42,"title":"without issue","html_url":"https://github.com/o/r/pull/42","state":"closed","head":{"sha":"abc"},"merged_at":"2026-10-17T00:00:00Z"}])).unwrap();
    assert_eq!(rows[0].state, "MERGED");
    assert!(rows[0].review_decision.is_none());
    assert!(rows[0].checks_source.is_none());
    assert!(decode_pulls(&json!({"error":"rate limit"})).is_err());
    assert!(decode_pulls(&json!([])).unwrap().is_empty());
}
#[test]
fn associated_sha_does_not_prove_checkout_and_pages_stay_distinct() {
    let runs = json!({"total_count":101,"check_runs":[{"name":"test","status":"completed","conclusion":"success","head_sha":"abc"},{"name":"legacy","status":"in_progress","conclusion":null}]});
    let statuses = json!({"sha":"abc","total_count":0,"statuses":[]});
    let (checks, coverage) = decode_checks(&runs, &statuses, "abc").unwrap();
    assert!(checks[0].tested_oid.is_none());
    assert_eq!(checks[0].source_oid.as_deref(), Some("abc"));
    assert!(checks[1].tested_oid.is_none());
    assert!(coverage.contains("2/101"));
    assert_eq!(checks[1].state, "IN_PROGRESS");
    assert!(decode_checks(&runs, &statuses, "other").is_err());
    assert!(decode_checks(&json!({}), &statuses, "abc").is_err());
    assert!(
        decode_checks(&json!({"total_count":0,"check_runs":[]}), &statuses, "abc")
            .unwrap()
            .0
            .is_empty()
    );
}
#[test]
fn rate_limit_errors_are_sanitized_and_origin_identity_is_strict() {
    let message = public_error(&Error::Invalid("HTTP 429 rate limit SECRET".into()));
    assert!(message.contains("限额"));
    assert!(!message.contains("SECRET"));
    for origin in [
        "git@github.com:o/r.git",
        "https://github.com/o/r.git",
        "ssh://git@github.com/o/r",
    ] {
        assert_eq!(origin_repository(origin).unwrap(), "o/r");
    }
    for origin in [
        "https://token@github.com/o/r",
        "https://evil.test/o/r",
        "git@github.com:o/r?x=y",
        "git@github.com:o/../r",
    ] {
        assert!(origin_repository(origin).is_err());
    }
}

#[cfg(unix)]
#[test]
fn explicit_ci_uses_only_two_fixed_rest_get_pages() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let program = tmp.path().join("gh");
    let calls = tmp.path().join("calls");
    let head = "a".repeat(40);
    std::fs::write(&program,format!(r##"#!/bin/sh
printf '%s\n' "$*" >> '{}'
[ "$1" = api ] && [ "$4" = --method ] && [ "$5" = GET ] || exit 2
case "$6" in
'repos/o/r/commits/{head}/check-runs?filter=latest&per_page=100&page=1') echo '{{"total_count":0,"check_runs":[]}}' ;;
'repos/o/r/commits/{head}/status?per_page=100&page=1') echo '{{"sha":"{head}","total_count":0,"statuses":[]}}' ;;
*) exit 2 ;;
esac
"##,calls.display())).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        read_pull_checks(tmp.path(), program.to_str(), "o/r", &head)
            .unwrap()
            .0
            .is_empty()
    );
    let calls = std::fs::read_to_string(calls).unwrap();
    assert_eq!(calls.lines().count(), 2);
    assert!(!calls.contains("graphql"));
}

#[test]
fn unsupported_ci_cache_metadata_cannot_relabel_associated_sha_as_tested() {
    let snapshot: GitHubSnapshot=serde_json::from_value(json!({"state":"partial","repository":"o/r","account":"reader","observedAt":"20","lastAttempt":"20","coverage":"old","error":null,"pulls":[{"id":"1","title":"old","url":"https://github.com/o/r/pull/1","state":"OPEN","headOid":"abc","reviewDecision":null,"checks":[{"name":"old","state":"SUCCESS","testedOid":"abc","url":null}],"checksSource":{"state":"partial","observedAt":"20","coverage":"old","error":null}}]})).unwrap();
    let restored = GitHubClient::restore(snapshot).snapshot();
    assert!(restored.pulls[0].checks.is_empty());
    assert!(restored.pulls[0].checks_source.is_none());
}
