use oasis7_command_center_core::{
    CommandCenter, document_inventory::DocumentInventoryQuery,
    document_semantic_tree::DocumentSemanticTreeQuery, document_tree_reading::*,
};
use std::{fs, path::Path, process::Command};
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
fn fixture(
    files: &[(&str, &str)],
) -> (tempfile::TempDir, CommandCenter, DocumentSemanticTreeQuery) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init"]);
    git(&root, &["config", "user.name", "test"]);
    git(&root, &["config", "user.email", "test@example.org"]);
    for (path, content) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "fixture"]);
    let mut core = CommandCenter::open(tmp.path().join("db")).unwrap();
    let snapshot = core.open_project(&root).unwrap();
    let query = DocumentSemanticTreeQuery {
        parent_occurrence: String::new(),
        parent_route: None,
        view: "professional".into(),
        lifecycle_filter: None,
        kind_filter: None,
        pairing_filter: None,
        inventory: DocumentInventoryQuery {
            schema_version: 1,
            project_id: snapshot.project.unwrap().id,
            snapshot_id: None,
            query: String::new(),
            read_filter: "all".into(),
            cursor: None,
        },
    };
    (tmp, core, query)
}
fn set_parent(core: &CommandCenter, query: &mut DocumentSemanticTreeQuery, route: &str) {
    let mut root = query.clone();
    root.parent_occurrence.clear();
    root.inventory.cursor = None;
    let snapshot = core.document_semantic_tree(root).unwrap().snapshot_id;
    query.parent_occurrence = format!("{snapshot}-{}:{route}", query.view);
}
fn tree_fixture() -> (tempfile::TempDir, CommandCenter, DocumentSemanticTreeQuery) {
    fixture(&[
        (
            "doc/README.md",
            "# Docs\n## 工程模块入口矩阵\n| 模块 | PRD 主文档 | 设计主文档 |\n| --- | --- | --- |\n| engineering | `doc/engineering/prd.md` | `doc/engineering/design.md` |\n",
        ),
        ("doc/engineering/prd.md", "# Engineering\n"),
        (
            "doc/engineering/design.md",
            "# Design\n## 阅读顺序\n[landing](README.md)\n",
        ),
        (
            "doc/engineering/README.md",
            "# Landing\n## 按主题进入\n[topic](topic.design.md)\n",
        ),
        (
            "doc/engineering/topic.design.md",
            "# Topic\n## Explicit chapter\nBody\n",
        ),
        ("doc/engineering/other.design.md", "# Topic\n"),
    ])
}
fn key(core: &CommandCenter, query: &DocumentSemanticTreeQuery) -> DocumentTreeReadingKey {
    let page = core.document_inventory(query.inventory.clone()).unwrap();
    DocumentTreeReadingKey {
        schema_version: 1,
        project_id: page.project_id,
        baseline_mode: "tracked-working-tree".into(),
        baseline_oid: page.baseline_oid,
        view: query.view.clone(),
    }
}
fn state() -> DocumentTreeReadingState {
    DocumentTreeReadingState {
        filters: DocumentTreeReadingFilters {
            query: String::new(),
            read_filter: "all".into(),
            lifecycle_filter: "current".into(),
            kind_filter: "all".into(),
            pairing_filter: "all".into(),
        },
        expanded_routes: vec![],
        parent_route: vec![],
        selection: None,
        tree_scroll_top: 420.0,
        detail_scroll_top: 12.0,
    }
}
fn saved_raw(db: &rusqlite::Connection, key: &DocumentTreeReadingKey) -> (String, String) {
    db.query_row("SELECT key,value FROM device WHERE key LIKE 'document_tree_reading_v1:%' AND json_extract(value,'$.key.view')=?1", [&key.view], |r| Ok((r.get(0)?,r.get(1)?))).unwrap()
}
#[test]
fn reading_record_survives_restart_new_head_and_source_line_drift_without_snapshot_changes() {
    let (tmp, mut core, mut query) = tree_fixture();
    set_parent(&core, &mut query, "0/1/0");
    let leaf = core.document_semantic_tree(query.clone()).unwrap().entries[0].clone();
    let mut reading = state();
    reading.parent_route = leaf.stable_route[..3].to_vec();
    reading.expanded_routes = vec![reading.parent_route.clone()];
    let chapter = core
        .snapshot()
        .sections
        .iter()
        .find(|s| s.path == "doc/engineering/topic.design.md" && s.title == "Explicit chapter")
        .unwrap()
        .clone();
    reading.selection = Some(DocumentTreeReadingSelection {
        route: leaf.stable_route.clone(),
        path: leaf.path.clone().unwrap(),
        section_path: Some(chapter.path),
        section_id: Some(chapter.id.clone()),
        section_revision: Some(chapter.revision),
    });
    let key = key(&core, &query);
    let db = rusqlite::Connection::open(tmp.path().join("db")).unwrap();
    let before: String = db
        .query_row(
            "SELECT value FROM cache WHERE key='device_snapshot_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.execute(
        "INSERT INTO device(key,value) VALUES('unrelated-personal-record','opaque')",
        [],
    )
    .unwrap();
    let saved = core
        .save_document_tree_reading(DocumentTreeReadingSaveRequest {
            key: key.clone(),
            state: reading,
        })
        .unwrap();
    assert_eq!(
        saved.resolved_section_id.as_deref(),
        Some(chapter.id.as_str())
    );
    let after: String = db
        .query_row(
            "SELECT value FROM cache WHERE key='device_snapshot_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
    drop(core);
    let mut core = CommandCenter::open(tmp.path().join("db")).unwrap();
    let loaded = core.load_document_tree_reading(key.clone()).unwrap();
    assert_eq!(loaded.status, "ready");
    assert_eq!(loaded.state.unwrap().tree_scroll_top, 420.0);
    let root = tmp.path().join("repo");
    fs::write(root.join("doc/engineering/README.md"), "<!-- shifted -->\n\n# Renamed landing title\n## 按主题进入\n[changed label](topic.design.md)\n").unwrap();
    fs::write(
        root.join("doc/engineering/topic.design.md"),
        "# New Topic\n## Explicit chapter\nChanged body\n",
    )
    .unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "new baseline"]);
    core.refresh().unwrap();
    let new_key = key_for(&core, &query);
    assert_ne!(key.baseline_oid, new_key.baseline_oid);
    let loaded = core.load_document_tree_reading(new_key.clone()).unwrap();
    assert_eq!(loaded.status, "ready");
    assert!(loaded.resolved_section_id.is_none());
    assert!(loaded.notice.unwrap().contains("chapter"));
    query.parent_occurrence.clear();
    query.parent_route = Some(leaf.stable_route);
    let projected = core.document_semantic_tree(query).unwrap();
    assert_eq!(projected.parent_route.len(), 4);
    assert!(projected.route_notice.is_none());
    let pointer = core
        .load_document_tree_last_view(DocumentTreeReadingScope {
            schema_version: 1,
            project_id: new_key.project_id,
            baseline_mode: new_key.baseline_mode,
            baseline_oid: new_key.baseline_oid,
        })
        .unwrap();
    assert_eq!(pointer.view.as_deref(), Some("professional"));
    let untouched: String = db
        .query_row(
            "SELECT value FROM device WHERE key='unrelated-personal-record'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(untouched, "opaque");
}
fn key_for(core: &CommandCenter, query: &DocumentSemanticTreeQuery) -> DocumentTreeReadingKey {
    key(core, query)
}
#[test]
fn stable_route_missing_duplicate_and_filtered_targets_have_distinct_honest_outcomes() {
    let (tmp, mut core, mut query) = tree_fixture();
    set_parent(&core, &mut query, "0/1/0");
    let leaf = core.document_semantic_tree(query.clone()).unwrap().entries[0].clone();
    query.parent_occurrence.clear();
    query.parent_route = Some(leaf.stable_route.clone());
    query.inventory.query = "no such match".into();
    let filtered = core.document_semantic_tree(query.clone()).unwrap();
    assert_eq!(filtered.parent_route.len(), 4);
    assert!(filtered.route_notice.unwrap().contains("hidden by"));
    query.inventory.query.clear();
    fs::remove_file(tmp.path().join("repo/doc/engineering/topic.design.md")).unwrap();
    core.refresh().unwrap();
    let deleted = core.document_semantic_tree(query.clone()).unwrap();
    assert_eq!(deleted.parent_route.len(), 3);
    assert!(deleted.route_notice.unwrap().contains("deleted"));
    fs::write(
        tmp.path().join("repo/doc/engineering/topic.design.md"),
        "# Topic\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("repo/doc/engineering/README.md"),
        "# Landing\n## 按主题进入\n[one](topic.design.md)\n[two](topic.design.md)\n",
    )
    .unwrap();
    core.refresh().unwrap();
    let changed = core.document_semantic_tree(query.clone()).unwrap();
    assert_eq!(changed.parent_route.len(), 3);
    query.parent_route = Some(leaf.stable_route[..3].to_vec());
    let duplicate = core.document_semantic_tree(query.clone()).unwrap().entries[0]
        .stable_route
        .clone();
    assert_eq!(duplicate.last().unwrap().duplicate_count, 2);
    query.parent_route = Some(duplicate);
    let ambiguous = core.document_semantic_tree(query).unwrap();
    assert_eq!(ambiguous.parent_route.len(), 3);
    assert!(ambiguous.route_notice.unwrap().contains("ambiguous"));
}
#[test]
fn corrupt_future_and_failed_transactions_preserve_other_records_and_can_be_reset() {
    let (tmp, mut core, query) = tree_fixture();
    let key = key(&core, &query);
    core.save_document_tree_reading(DocumentTreeReadingSaveRequest {
        key: key.clone(),
        state: state(),
    })
    .unwrap();
    let db = rusqlite::Connection::open(tmp.path().join("db")).unwrap();
    let (stored_key, _) = saved_raw(&db, &key);
    db.execute("INSERT INTO device(key,value) VALUES('other','keep')", [])
        .unwrap();
    let before: String = db
        .query_row(
            "SELECT value FROM cache WHERE key='device_snapshot_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for (raw, status) in [
        ("{broken", "corrupt"),
        ("{\"schemaVersion\":2}", "unsupported"),
    ] {
        db.execute(
            "UPDATE device SET value=?1 WHERE key=?2",
            [raw, &stored_key],
        )
        .unwrap();
        assert_eq!(
            core.load_document_tree_reading(key.clone()).unwrap().status,
            status
        );
        let untouched: String = db
            .query_row(
                "SELECT value FROM device WHERE key=?1",
                [&stored_key],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(untouched, raw);
        if status == "unsupported" {
            assert!(
                core.save_document_tree_reading(DocumentTreeReadingSaveRequest {
                    key: key.clone(),
                    state: state()
                })
                .is_err()
            );
        }
    }
    assert_eq!(
        core.reset_document_tree_reading(key.clone())
            .unwrap()
            .status,
        "missing"
    );
    core.save_document_tree_reading(DocumentTreeReadingSaveRequest {
        key: key.clone(),
        state: state(),
    })
    .unwrap();
    let (stored_key, saved) = saved_raw(&db, &key);
    let pointer: String = db
        .query_row(
            "SELECT value FROM device WHERE key LIKE 'document_tree_last_view_v1:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.execute_batch("CREATE TRIGGER reject_pointer BEFORE INSERT ON device WHEN new.key LIKE 'document_tree_last_view_v1:%' BEGIN SELECT RAISE(ABORT,'injected pointer failure'); END;").unwrap();
    let mut changed = state();
    changed.tree_scroll_top = 999.0;
    assert!(
        core.save_document_tree_reading(DocumentTreeReadingSaveRequest {
            key: key.clone(),
            state: changed
        })
        .is_err()
    );
    let rolled_back: String = db
        .query_row(
            "SELECT value FROM device WHERE key=?1",
            [&stored_key],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(saved, rolled_back);
    let old_pointer: String = db
        .query_row(
            "SELECT value FROM device WHERE key LIKE 'document_tree_last_view_v1:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pointer, old_pointer);
    let after: String = db
        .query_row(
            "SELECT value FROM cache WHERE key='device_snapshot_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
    let other: String = db
        .query_row("SELECT value FROM device WHERE key='other'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(other, "keep");
    let mut wrong = key;
    wrong.project_id = "other-project".into();
    assert!(core.load_document_tree_reading(wrong).is_err());
}
#[test]
fn view_isolation_validation_and_future_pointer_recovery_are_bounded() {
    let (tmp, mut core, query) = tree_fixture();
    let professional = key(&core, &query);
    let mut product = professional.clone();
    product.view = "product".into();
    core.save_document_tree_reading(DocumentTreeReadingSaveRequest {
        key: professional.clone(),
        state: state(),
    })
    .unwrap();
    assert_eq!(
        core.load_document_tree_reading(product.clone())
            .unwrap()
            .status,
        "missing"
    );
    let mut product_state = state();
    product_state.tree_scroll_top = 1.0;
    core.save_document_tree_reading(DocumentTreeReadingSaveRequest {
        key: product.clone(),
        state: product_state,
    })
    .unwrap();
    assert_eq!(
        core.load_document_tree_reading(professional.clone())
            .unwrap()
            .state
            .unwrap()
            .tree_scroll_top,
        420.0
    );
    let mut invalid = state();
    invalid.expanded_routes = vec![vec![]; 129];
    assert!(
        core.save_document_tree_reading(DocumentTreeReadingSaveRequest {
            key: product.clone(),
            state: invalid
        })
        .is_err()
    );
    let mut invalid = state();
    invalid.tree_scroll_top = f64::NAN;
    assert!(
        core.save_document_tree_reading(DocumentTreeReadingSaveRequest {
            key: product.clone(),
            state: invalid
        })
        .is_err()
    );
    let db = rusqlite::Connection::open(tmp.path().join("db")).unwrap();
    db.execute("UPDATE device SET value='{\"schemaVersion\":99}' WHERE key LIKE 'document_tree_last_view_v1:%'", []).unwrap();
    let scope = DocumentTreeReadingScope {
        schema_version: 1,
        project_id: product.project_id.clone(),
        baseline_mode: product.baseline_mode.clone(),
        baseline_oid: product.baseline_oid.clone(),
    };
    assert_eq!(
        core.load_document_tree_last_view(scope.clone())
            .unwrap()
            .status,
        "unsupported"
    );
    assert!(
        core.save_document_tree_reading(DocumentTreeReadingSaveRequest {
            key: product.clone(),
            state: state()
        })
        .is_err()
    );
    core.reset_document_tree_reading(product.clone()).unwrap();
    assert_eq!(
        core.load_document_tree_last_view(scope).unwrap().status,
        "missing"
    );
    assert_eq!(
        core.load_document_tree_reading(professional)
            .unwrap()
            .status,
        "ready"
    );
}
#[test]
fn stable_route_reprojects_beyond_page_boundary_without_numeric_offsets() {
    let mut files = vec![("doc/README.md".to_owned(), "# Docs\n".to_owned())];
    for i in 0..105 {
        files.push((
            format!("doc/engineering/{i:03}.design.md"),
            format!("# Same title {i}\n"),
        ));
    }
    let refs: Vec<_> = files
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();
    let (tmp, mut core, mut query) = fixture(&refs);
    set_parent(&core, &mut query, "0");
    let first = core.document_semantic_tree(query.clone()).unwrap();
    query.inventory.cursor = first.next_cursor;
    let last = core
        .document_semantic_tree(query.clone())
        .unwrap()
        .entries
        .last()
        .unwrap()
        .clone();
    assert_eq!(last.path.as_deref(), Some("doc/engineering/104.design.md"));
    fs::write(
        tmp.path().join("repo/doc/engineering/000.design.md"),
        "# Changed title\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("repo/doc/engineering/000-new.design.md"),
        "# Newly inserted sibling\n",
    )
    .unwrap();
    git(&tmp.path().join("repo"), &["add", "."]);
    core.refresh().unwrap();
    query.inventory.cursor = None;
    query.parent_occurrence.clear();
    query.parent_route = Some(last.stable_route);
    let restored = core.document_semantic_tree(query.clone()).unwrap();
    assert_eq!(restored.parent_route.len(), 2);
    assert!(restored.route_notice.is_none());
    assert_eq!(
        restored.parent_route.last().unwrap().path.as_deref(),
        Some("doc/engineering/104.design.md")
    );
    let mut invalid = restored.parent_route.clone();
    invalid.extend(vec![invalid.last().unwrap().clone(); 64]);
    query.parent_route = Some(invalid);
    assert!(core.document_semantic_tree(query).is_err());
}
#[test]
fn complete_record_budget_is_checked_before_any_personal_write() {
    let (_tmp, mut core, query) = tree_fixture();
    let key = key(&core, &query);
    let mut reading = state();
    let step = DocumentTreeRouteStep {
        path: None,
        source_path: None,
        destination: Some(String::new()),
        group: Some("explicit-group".into()),
        duplicate_index: 0,
        duplicate_count: 1,
    };
    reading.expanded_routes = vec![vec![step]; 128];
    let target = 256 * 1024 - 100;
    let mut low: usize = 0;
    let mut high: usize = 4096;
    while low < high {
        let mid = (low + high).div_ceil(2);
        for route in &mut reading.expanded_routes {
            route[0].destination = Some("x".repeat(mid));
        }
        if serde_json::to_vec(&reading).unwrap().len() <= target {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    for route in &mut reading.expanded_routes {
        route[0].destination = Some("x".repeat(low));
    }
    let remaining = target - serde_json::to_vec(&reading).unwrap().len();
    reading.expanded_routes[0][0]
        .destination
        .as_mut()
        .unwrap()
        .push_str(&"x".repeat(remaining));
    assert_eq!(serde_json::to_vec(&reading).unwrap().len(), target);
    assert!(
        core.save_document_tree_reading(DocumentTreeReadingSaveRequest {
            key: key.clone(),
            state: reading
        })
        .is_err()
    );
    assert_eq!(
        core.load_document_tree_reading(key).unwrap().status,
        "missing"
    );
}
#[test]
fn visible_cross_domain_reference_is_not_mislabeled_as_a_filtered_document() {
    let (tmp, mut core, mut query) = tree_fixture();
    let root = tmp.path().join("repo");
    fs::create_dir_all(root.join("doc/product/world-infrastructure")).unwrap();
    fs::write(
        root.join("doc/product/world-infrastructure/prd.md"),
        "# Infrastructure\n",
    )
    .unwrap();
    fs::write(
        root.join("doc/engineering/README.md"),
        "# Landing\n## 按主题进入\n[cross](../product/world-infrastructure/prd.md)\n",
    )
    .unwrap();
    git(&root, &["add", "."]);
    core.refresh().unwrap();
    set_parent(&core, &mut query, "0/1/0");
    let boundary = core.document_semantic_tree(query.clone()).unwrap().entries[0].clone();
    assert!(boundary.boundary.is_some());
    query.parent_occurrence.clear();
    query.parent_route = Some(boundary.stable_route);
    let projected = core.document_semantic_tree(query).unwrap();
    assert_eq!(projected.parent_route.len(), 4);
    assert!(projected.route_notice.is_none());
}
