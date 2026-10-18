use oasis7_command_center_core::{
    CommandCenter, daily_work::*, domain::DecisionDraft, work_projection::WorkRef,
};
use std::{path::Path, process::Command};
fn git(path: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}
fn project(root: &Path) {
    std::fs::create_dir_all(root.join("doc")).unwrap();
    git(root, &["init", "-q"]);
    std::fs::write(root.join("doc/prd.md"), "# Stage\n\nA real requirement\n").unwrap();
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=t@e.test",
            "commit",
            "-qm",
            "initial",
        ],
    );
}
fn save(core: &mut CommandCenter, action: DailyWorkAction) -> DailyWorkState {
    let state = core.daily_work().unwrap();
    core.save_daily_work(DailyWorkRequest {
        project_id: state.project_id,
        environment: state.environment,
        expected_revision: state.revision,
        action,
    })
    .unwrap()
}
fn work() -> WorkRef {
    WorkRef::PullRequest {
        host: "github.com".into(),
        repository: "o/r".into(),
        number: "42".into(),
    }
}
fn draft(id: Option<String>, text: &str) -> DecisionDraft {
    DecisionDraft {
        id,
        content: text.into(),
        reason: "explicit reason".into(),
        scope: "local workflow".into(),
        related_section: None,
        confirmed: true,
    }
}
#[test]
fn cas_and_future_versions_preserve_original_records_and_legacy_changes_are_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    project(&root);
    let db = temp.path().join("host.db");
    let mut core = CommandCenter::open(&db).unwrap();
    core.open_project(&root).unwrap();
    let initial = core.daily_work().unwrap();
    let mut navigation = initial.navigation.clone();
    navigation.draft = "ordinary draft".into();
    navigation.retain_draft = true;
    navigation.start_intent_id = Some("original-intent".into());
    let saved = save(&mut core, DailyWorkAction::Navigate { navigation });
    assert!(
        core.save_daily_work(DailyWorkRequest {
            project_id: initial.project_id,
            environment: initial.environment,
            expected_revision: 0,
            action: DailyWorkAction::AcceptLegacy {}
        })
        .is_err()
    );
    assert_eq!(
        core.daily_work().unwrap().navigation.draft,
        "ordinary draft"
    );
    let decision = core.save_decision(draft(None, "Decide")).unwrap();
    assert!(!core.daily_work().unwrap().legacy_changed);
    let external = rusqlite::Connection::open(&db).unwrap();
    let decision_key = format!("{}:decisions", saved.project_id);
    let mut legacy = vec![decision.clone()];
    legacy[0].content = "old App changed this".into();
    external
        .execute(
            "UPDATE personal SET value=?1 WHERE key=?2",
            rusqlite::params![serde_json::to_string(&legacy).unwrap(), decision_key],
        )
        .unwrap();
    assert!(core.daily_work().unwrap().legacy_changed);
    assert!(
        core.save_daily_work(DailyWorkRequest {
            project_id: saved.project_id.clone(),
            environment: saved.environment.clone(),
            expected_revision: saved.revision,
            action: DailyWorkAction::FollowDecision {
                decision_id: decision.id,
                following: false
            }
        })
        .is_err()
    );
    save(&mut core, DailyWorkAction::AcceptLegacy {});
    let database = rusqlite::Connection::open(&db).unwrap();
    let key = format!("{}:work_relations_v1", saved.project_id);
    let original: String = database
        .query_row("SELECT value FROM personal WHERE key=?1", [&key], |r| {
            r.get(0)
        })
        .unwrap();
    let mut future: serde_json::Value = serde_json::from_str(&original).unwrap();
    future["schemaVersion"] = serde_json::json!(99);
    database
        .execute(
            "UPDATE personal SET value=?1 WHERE key=?2",
            rusqlite::params![future.to_string(), key],
        )
        .unwrap();
    assert!(core.daily_work().is_err());
    assert_eq!(
        database
            .query_row("SELECT value FROM personal WHERE key=?1", [&key], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
        future.to_string()
    );
}
#[test]
fn relationships_repair_remove_and_cache_rebuild_do_not_revive_old_relations() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    project(&root);
    let mut core = CommandCenter::open(temp.path().join("host.db")).unwrap();
    let snapshot = core.open_project(&root).unwrap();
    let section = snapshot.sections[0].id.clone();
    let first = save(
        &mut core,
        DailyWorkAction::Relate {
            work: work(),
            section_id: section.clone(),
            replace_id: None,
        },
    );
    let id = first.relations[0].id.clone();
    let repaired = save(
        &mut core,
        DailyWorkAction::Relate {
            work: work(),
            section_id: section,
            replace_id: Some(id.clone()),
        },
    );
    assert!(!repaired.relations[0].previous.is_empty());
    save(&mut core, DailyWorkAction::RemoveRelation { id });
    core.rebuild_cache().unwrap();
    assert!(core.daily_work().unwrap().relations[0].removed_at.is_some());
}
#[test]
fn sync_evidence_is_per_track_and_changed_sources_keep_historical_verification() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    project(&root);
    let mut core = CommandCenter::open(temp.path().join("host.db")).unwrap();
    let snapshot = core.open_project(&root).unwrap();
    let section = snapshot.sections[0].clone();
    let decision = core.save_decision(draft(None, "Decision A")).unwrap();
    save(&mut core, DailyWorkAction::AcceptLegacy {});
    let evidence = SyncEvidence {
        state: "verified".into(),
        source: Some(section.id.clone()),
        revision: Some(section.revision),
        scope: "one clause only".into(),
        conclusion: "read and compared".into(),
        covers_full_scope: false,
        remaining: vec!["other obligations remain".into()],
        checked_at: "forged time".into(),
        decision_digest: "forged".into(),
    };
    let state = save(
        &mut core,
        DailyWorkAction::SyncDecision {
            decision_id: decision.id.clone(),
            track: "formal".into(),
            evidence,
        },
    );
    assert_eq!(
        state.decisions[0].formal.as_ref().unwrap().state,
        "verified"
    );
    assert!(state.decisions[0].implementation.is_none());
    std::fs::write(root.join("doc/prd.md"), "# Stage\n\nChanged requirement\n").unwrap();
    core.refresh().unwrap();
    let changed = core.daily_work().unwrap();
    assert_eq!(
        changed.decisions[0].formal.as_ref().unwrap().state,
        "needsReview"
    );
    assert_eq!(changed.decisions[0].history[0].state, "verified");
    let edited = core
        .save_decision(draft(Some(decision.id), "Decision with minor correction"))
        .unwrap();
    assert_eq!(edited.confirmed_at, decision.confirmed_at);
}
#[test]
fn foreign_clones_and_closed_questions_cannot_cross_scope_or_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    let foreign = temp.path().join("foreign");
    project(&root);
    project(&foreign);
    let mut core = CommandCenter::open(temp.path().join("host.db")).unwrap();
    core.open_project(&root).unwrap();
    let other = CommandCenter::inspect_worktree(&foreign).unwrap();
    assert!(core.remember_worktree(other, 0).is_err());
    let item = WorkAttention {
        id: "q1".into(),
        work: work(),
        question: "Need a choice".into(),
        state: "resolved".into(),
        source_revision: "head".into(),
        statement: "user answered, not source completion".into(),
        updated_at: "".into(),
        history: vec![],
    };
    save(&mut core, DailyWorkAction::Attention { item: item.clone() });
    core.rebuild_cache().unwrap();
    let mut reopened = item;
    reopened.state = "open".into();
    let state = core.daily_work().unwrap();
    assert!(
        core.save_daily_work(DailyWorkRequest {
            project_id: state.project_id,
            environment: state.environment,
            expected_revision: state.revision,
            action: DailyWorkAction::Attention { item: reopened }
        })
        .is_err()
    );
}
#[test]
fn later_private_mark_hides_saved_derived_records_without_deleting_them() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    project(&root);
    let db = temp.path().join("host.db");
    let mut core = CommandCenter::open(&db).unwrap();
    let p = core.open_project(&root).unwrap().project.unwrap();
    let reference = WorkRef::Session {
        namespace: "owned-stdio".into(),
        project_id: p.id.clone(),
        environment: "local".into(),
        thread_id: "later-private".into(),
    };
    let mut nav = core.daily_work().unwrap().navigation;
    nav.selected = Some(reference.clone());
    nav.draft = "derived body".into();
    nav.retain_draft = true;
    save(&mut core, DailyWorkAction::Navigate { navigation: nav });
    save(
        &mut core,
        DailyWorkAction::Attention {
            item: WorkAttention {
                id: "question".into(),
                work: reference.clone(),
                question: "derived private question".into(),
                state: "open".into(),
                source_revision: "h".into(),
                statement: "".into(),
                updated_at: "".into(),
                history: vec![],
            },
        },
    );
    let runtime_db = temp.path().join("runtime.db");
    let _runtime = oasis7_command_center_core::codex::Runtime::open(&runtime_db).unwrap();
    core.use_runtime_privacy_database(&runtime_db).unwrap();
    let database = rusqlite::Connection::open(&runtime_db).unwrap();
    database
        .execute(
            "INSERT INTO cc_runtime_private_inputs(scope,thread,reference) VALUES (?1,?2,?3)",
            rusqlite::params![format!("{}:local", p.id), "later-private", "{}"],
        )
        .unwrap();
    let public = serde_json::to_string(&core.public_daily_work().unwrap()).unwrap();
    assert!(!public.contains("derived body"));
    assert!(!public.contains("derived private question"));
    assert!(!public.contains("later-private"));
    assert_eq!(core.daily_work().unwrap().attention.len(), 1);
    let state = core.daily_work().unwrap();
    assert!(
        core.save_daily_work(DailyWorkRequest {
            project_id: p.id,
            environment: "local".into(),
            expected_revision: state.revision,
            action: DailyWorkAction::External {
                item: ExternalWork {
                    id: "bad".into(),
                    title: "bad".into(),
                    work: reference,
                    locator: "bad".into(),
                    source_url: None,
                    recorded_at: "".into()
                }
            }
        })
        .is_err()
    );
}
#[test]
fn decision_replacement_keeps_original_and_minor_history_without_revival() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    project(&root);
    let mut core = CommandCenter::open(temp.path().join("db")).unwrap();
    core.open_project(&root).unwrap();
    let original = core.save_decision(draft(None, "Original")).unwrap();
    let minor = core
        .save_decision(draft(Some(original.id.clone()), "Spelling correction"))
        .unwrap();
    assert_eq!(minor.confirmed_at, original.confirmed_at);
    assert_eq!(
        core.decision_history(&original.id).unwrap()[0].content,
        "Original"
    );
    let replacement = core
        .replace_decision(draft(None, "New policy"), original.id.clone())
        .unwrap();
    assert_eq!(
        replacement.supersedes.as_deref(),
        Some(original.id.as_str())
    );
    assert_eq!(
        core.snapshot()
            .decisions
            .iter()
            .find(|d| d.id == original.id)
            .unwrap()
            .state,
        "superseded"
    );
    assert!(
        core.save_decision(draft(Some(original.id.clone()), "Revive"))
            .is_err()
    );
    core.rebuild_cache().unwrap();
    assert_eq!(
        core.snapshot()
            .decisions
            .iter()
            .find(|d| d.id == original.id)
            .unwrap()
            .state,
        "superseded"
    );
}
