use oasis7_command_center_core::CommandCenter;
use std::{path::Path, process::Command};
fn git(root: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success()
    );
}
#[test]
fn section_reading_history_survives_insert_reorder_rename_and_ambiguity() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    git(&root, &["init"]);
    git(&root, &["config", "user.name", "test"]);
    git(&root, &["config", "user.email", "test@example.org"]);
    std::fs::create_dir(root.join("doc")).unwrap();
    let document = root.join("doc/prd.md");
    std::fs::write(
        &document,
        "# Product\n## Goal\nAcceptance A\n## Other\nOther condition\n",
    )
    .unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "fixture"]);
    let db = tmp.path().join("state.db");
    let mut core = CommandCenter::open(&db).unwrap();
    let initial = core.open_project(&root).unwrap();
    let goal = initial
        .sections
        .iter()
        .find(|s| s.title == "Goal")
        .unwrap()
        .clone();
    core.record_reading(&goal.id, &goal.revision, goal.line)
        .unwrap();
    std::fs::write(
        &document,
        "# Product\n## New\nNew condition\n## Other\nOther condition\n## Goal\nAcceptance A\n",
    )
    .unwrap();
    let moved = core.refresh().unwrap();
    assert_eq!(
        moved
            .sections
            .iter()
            .find(|s| s.title == "Goal")
            .unwrap()
            .id,
        goal.id
    );
    assert_eq!(core.search(&goal.id, 20).unwrap().len(), 1);
    std::fs::write(
        &document,
        "# Product\n## Renamed\nAcceptance A\n## Other\nOther condition\n",
    )
    .unwrap();
    let renamed = core.refresh().unwrap();
    assert!(!renamed.sections.iter().any(|s| s.id == goal.id));
    assert_eq!(
        core.read_version(&goal.id, &goal.revision)
            .unwrap()
            .unwrap()
            .content,
        goal.content
    );
    std::fs::write(&document, "# Product\n## Goal\nFirst\n## Goal\nSecond\n").unwrap();
    let ambiguous = core.refresh().unwrap();
    let duplicates: Vec<_> = ambiguous
        .sections
        .iter()
        .filter(|s| s.title == "Goal")
        .collect();
    assert_eq!(duplicates.len(), 2);
    assert_ne!(duplicates[0].id, duplicates[1].id);
    assert!(duplicates.iter().all(|s| s.id != goal.id));
    let ambiguous_id = duplicates[0].id.clone();
    std::fs::write(&document, "# Product\n## Goal\nSecond\n## Goal\nFirst\n").unwrap();
    assert!(
        !core
            .refresh()
            .unwrap()
            .sections
            .iter()
            .any(|s| s.id == ambiguous_id)
    );
    drop(core);
    let reopened = CommandCenter::open(&db).unwrap();
    assert!(
        reopened
            .read_version(&goal.id, &goal.revision)
            .unwrap()
            .is_some()
    );
}
#[test]
fn explicit_source_ids_survive_title_changes_and_are_searchable() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    git(root, &["init"]);
    git(root, &["config", "user.name", "test"]);
    git(root, &["config", "user.email", "test@example.org"]);
    std::fs::create_dir(root.join("doc")).unwrap();
    let file = root.join("doc/rules.md");
    std::fs::write(&file,"<a id=\"REQ-CC-123\"></a>\n\n## Original title\nRule\n## REQ-CC-456: Explicit clause\nClause\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "fixture"]);
    let mut core = CommandCenter::open(":memory:").unwrap();
    let snap = core.open_project(root).unwrap();
    assert!(
        snap.sections
            .iter()
            .any(|s| s.id == "doc/rules.md#REQ-CC-123")
    );
    assert_eq!(core.search("REQ-CC-456", 20).unwrap().len(), 1);
    std::fs::write(
        &file,
        "<a id=\"REQ-CC-123\"></a>\n\n## Changed title\nRule\n",
    )
    .unwrap();
    let changed = core.refresh().unwrap();
    assert_eq!(
        changed
            .sections
            .iter()
            .find(|s| s.title == "Changed title")
            .unwrap()
            .id,
        "doc/rules.md#REQ-CC-123"
    );
}
