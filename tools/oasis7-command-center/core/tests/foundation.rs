use oasis7_command_center_core::CommandCenter;
use std::process::Command;
fn git(root: &std::path::Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
}
#[test]
fn chinese_dirty_index_survives_restart_and_cache_rebuild() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo with spaces");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "test"]);
    git(&repo, &["config", "user.email", "test@example.org"]);
    std::fs::create_dir(repo.join("doc")).unwrap();
    std::fs::write(
        repo.join("doc/prd.md"),
        "# 当前阶段交付目标与全局 P0\n规范 合入\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "fixture"]);
    let db = tmp.path().join("state.sqlite");
    let mut core = CommandCenter::open(&db).unwrap();
    core.open_project(&repo).unwrap();
    let section = core.search("规范", 20).unwrap().pop().unwrap();
    assert!(!section.dirty);
    core.record_reading(&section.id, &section.revision, 2)
        .unwrap();
    let historical_id = section.id.clone();
    let historical_revision = section.revision.clone();
    std::fs::write(
        repo.join("doc/prd.md"),
        "# 当前阶段交付目标与全局 P0\n规范 合入 更改\n",
    )
    .unwrap();
    assert!(core.refresh().unwrap().sections[0].dirty);
    drop(core);
    let mut core = CommandCenter::open(&db).unwrap();
    let snap = core.rebuild_cache().unwrap();
    assert_eq!(
        core.read_version(&historical_id, &historical_revision)
            .unwrap()
            .unwrap()
            .content,
        "# 当前阶段交付目标与全局 P0\n规范 合入\n"
    );
    assert_eq!(core.search("交付目标", 20).unwrap().len(), 1);
    assert!(core.search("%", 20).unwrap().is_empty());
    assert_eq!(snap.reading_positions.len(), 1);
    assert_eq!(core.search("合入", 20).unwrap().len(), 1);
    assert!(core.document_path("../../etc/passwd").is_err());
    let other = tmp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    git(&other, &["init"]);
    git(&other, &["config", "user.name", "test"]);
    git(&other, &["config", "user.email", "test@example.org"]);
    std::fs::create_dir(other.join("doc")).unwrap();
    std::fs::write(other.join("doc/prd.md"), "# Other\nother\n").unwrap();
    git(&other, &["add", "."]);
    git(&other, &["commit", "-m", "other"]);
    let other_snap = core.open_project(&other).unwrap();
    assert!(other_snap.reading_positions.is_empty());
    let faults = rusqlite::Connection::open(&db).unwrap();
    faults.execute_batch("CREATE TRIGGER fail_index BEFORE INSERT ON search BEGIN SELECT RAISE(FAIL,'storage rejected'); END;").unwrap();
    assert!(core.open_project(&repo).is_err());
    assert_eq!(
        core.snapshot().project.unwrap().path,
        other.canonicalize().unwrap().to_string_lossy()
    );
    faults.execute_batch("DROP TRIGGER fail_index;").unwrap();
    core.open_project(&repo).unwrap();
    std::fs::rename(&repo, tmp.path().join("moved")).unwrap();
    let stale = core.refresh().unwrap();
    assert_eq!(stale.sections.len(), 1);
    assert!(
        stale
            .sources
            .iter()
            .any(|s| s.kind == "local" && s.state == "error" && s.last_success.is_some())
    );
}
#[cfg(unix)]
#[test]
fn external_symlink_document_is_not_readable() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "test"]);
    git(&repo, &["config", "user.email", "test@example.org"]);
    std::fs::create_dir(repo.join("doc")).unwrap();
    std::fs::write(tmp.path().join("secret.md"), "secret").unwrap();
    std::os::unix::fs::symlink(tmp.path().join("secret.md"), repo.join("doc/escape.md")).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "fixture"]);
    let mut core = CommandCenter::open(tmp.path().join("state.db")).unwrap();
    core.open_project(&repo).unwrap();
    assert!(core.search("secret", 20).unwrap().is_empty());
    assert!(core.document_path("doc/escape.md").is_err());
}

#[test]
fn future_schema_is_not_opened() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("future.sqlite");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("PRAGMA user_version=99").unwrap();
    drop(conn);
    assert!(CommandCenter::open(&db).is_err());
}

#[test]
fn local_coverage_reports_deleted_and_oversized_documents_after_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "test"]);
    git(&repo, &["config", "user.email", "test@example.org"]);
    std::fs::create_dir(repo.join("doc")).unwrap();
    for path in ["read.md", "deleted.md", "large.md", "empty.md"] {
        std::fs::write(repo.join("doc").join(path), "# Document\n").unwrap();
    }
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "fixture"]);
    std::fs::remove_file(repo.join("doc/deleted.md")).unwrap();
    std::fs::write(repo.join("doc/large.md"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
    std::fs::write(repo.join("doc/empty.md"), "").unwrap();
    std::fs::write(repo.join("doc/untracked.md"), "# Not included\n").unwrap();
    let db = tmp.path().join("state.sqlite");
    let mut core = CommandCenter::open(&db).unwrap();
    let snapshot = core.open_project(&repo).unwrap();
    let source = snapshot.sources.iter().find(|s| s.kind == "local").unwrap();
    assert_eq!(source.state, "partial");
    assert!(source.error.is_none());
    assert!(source.coverage.contains("候选 4"), "{}", source.coverage);
    assert!(source.coverage.contains("已读 2"), "{}", source.coverage);
    assert!(
        source.coverage.contains("deleted: doc/deleted.md"),
        "{}",
        source.coverage
    );
    assert!(
        source.coverage.contains("file_budget: doc/large.md"),
        "{}",
        source.coverage
    );
    assert!(
        !snapshot
            .sections
            .iter()
            .any(|s| s.path == "doc/untracked.md")
    );
    drop(core);
    let restored = CommandCenter::open(&db).unwrap().snapshot();
    assert_eq!(
        restored
            .sources
            .iter()
            .find(|s| s.kind == "local")
            .unwrap()
            .coverage,
        source.coverage
    );
}

#[test]
fn wholly_omitted_documents_are_partial_instead_of_successfully_empty() {
    let tmp = tempfile::tempdir().unwrap();
    git(tmp.path(), &["init"]);
    git(tmp.path(), &["config", "user.name", "test"]);
    git(tmp.path(), &["config", "user.email", "test@example.org"]);
    std::fs::create_dir(tmp.path().join("doc")).unwrap();
    let path = tmp.path().join("doc/prd.md");
    std::fs::write(&path, "# Product\n").unwrap();
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-m", "fixture"]);
    std::fs::write(&path, [0xff]).unwrap();
    let mut core = CommandCenter::open(tmp.path().join("state.sqlite")).unwrap();
    let snapshot = core.open_project(tmp.path()).unwrap();
    assert!(snapshot.sections.is_empty());
    let source = snapshot.sources.iter().find(|s| s.kind == "local").unwrap();
    assert_eq!(source.state, "partial");
    assert!(source.coverage.contains("invalid_utf8: doc/prd.md"));
    std::fs::write(path, "").unwrap();
    let snapshot = core.refresh().unwrap();
    let source = snapshot.sources.iter().find(|s| s.kind == "local").unwrap();
    assert_eq!(source.state, "empty");
    assert!(source.coverage.contains("候选 1，已读 1，遗漏 0"));
}

#[test]
fn markdown_titles_use_visible_text_and_keep_reading_identity() {
    let tmp = tempfile::tempdir().unwrap();
    git(tmp.path(), &["init"]);
    git(tmp.path(), &["config", "user.name", "test"]);
    git(tmp.path(), &["config", "user.email", "test@example.org"]);
    std::fs::create_dir(tmp.path().join("doc")).unwrap();
    let path = tmp.path().join("doc/design.md");
    std::fs::write(&path, "# 项目\n\n## **恢复**[策略](other.md) `API` ##\n正文\n\n多行\n标题\n====\n内容\n\n```md\n## 伪标题\n```\n").unwrap();
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-m", "fixture"]);
    let db = tmp.path().join("state.sqlite");
    let mut core = CommandCenter::open(&db).unwrap();
    core.open_project(tmp.path()).unwrap();
    let section = core
        .search("恢复策略", 20)
        .unwrap()
        .pop()
        .expect("visible title is searchable");
    assert_eq!(section.title, "恢复策略 API");
    use sha2::{Digest, Sha256};
    let old_context = serde_json::to_vec(&["项目", "**恢复**[策略](other.md) `API` ##"]).unwrap();
    assert_eq!(
        section.id,
        format!("doc/design.md#heading-{:x}", Sha256::digest(old_context))
    );

    assert_eq!(section.line, 3);
    assert!(section.content.starts_with("## **恢复**"));
    assert_eq!(core.search("多行 标题", 20).unwrap()[0].title, "多行 标题");
    assert!(
        !core
            .snapshot()
            .sections
            .iter()
            .any(|s| s.title.contains("伪标题"))
    );
    core.record_reading(&section.id, &section.revision, 0)
        .unwrap();
    // Normalizing display text preserves the existing source-based heading key.
    std::fs::write(
        &path,
        "# 项目\n\n## **恢复**[策略](other.md) `API` ##\n更新正文\n",
    )
    .unwrap();
    core.open_project(tmp.path()).unwrap();
    assert_eq!(core.search("恢复策略", 20).unwrap()[0].id, section.id);
    assert_eq!(core.snapshot().reading_positions.len(), 1);
}
