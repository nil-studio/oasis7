use oasis7_command_center_core::CommandCenter;

#[test]
fn old_runtime_records_are_preserved_without_loading_or_replaying_them() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("command-center.sqlite");
    drop(CommandCenter::open(&path).unwrap());
    let db = rusqlite::Connection::open(&path).unwrap();
    // Deliberately not valid current DTOs: retired records must not be interpreted.
    db.execute_batch("CREATE TABLE cc_runtime_state(scope TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE cc_runtime_operations(id TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO cc_runtime_state VALUES('p:local','{old active enabled session}'); INSERT INTO cc_runtime_operations VALUES('pending','{unknown request, original input}'); INSERT INTO cc_runtime_private_inputs VALUES('p:local','private-thread','{old marker}');").unwrap();
    drop(db);
    let mut core = CommandCenter::open(&path).unwrap();
    assert!(
        !core
            .snapshot()
            .sources
            .iter()
            .any(|source| source.kind == "codex")
    );
    // Cache maintenance does not reinterpret retired execution state either.
    core.reload_device().unwrap();
    drop(core);
    let db = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        db.query_row("SELECT value FROM cc_runtime_state", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "{old active enabled session}"
    );
    assert_eq!(
        db.query_row("SELECT value FROM cc_runtime_operations", [], |row| row
            .get::<_, String>(
            0
        ))
        .unwrap(),
        "{unknown request, original input}"
    );
    assert_eq!(
        db.query_row(
            "SELECT reference FROM cc_runtime_private_inputs",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "{old marker}"
    );
}

#[test]
fn legacy_personal_records_are_opaque_and_device_preferences_are_independent() {
    use oasis7_command_center_core::domain::Preferences;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.sqlite");
    drop(CommandCenter::open(&path).unwrap());
    let db = rusqlite::Connection::open(&path).unwrap();
    for key in [
        "preferences",
        "none:decisions",
        "none:associations",
        "document_tree_reading_v1:old",
    ] {
        db.execute(
            "INSERT INTO personal(key,value) VALUES(?1,'opaque legacy bytes')",
            [key],
        )
        .unwrap();
    }
    db.execute(
        "INSERT INTO history(key,value) VALUES('old','opaque history')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO cache(key,value) VALUES('snapshot','opaque legacy snapshot')",
        [],
    )
    .unwrap();
    let mut core = CommandCenter::open(&path).unwrap();
    assert!(core.snapshot().preferences.project_path.is_none());
    core.save_preferences(Preferences {
        project_path: None,
        view: "tools".into(),
        query: "device".into(),
    })
    .unwrap();
    core.reload_device().unwrap();
    assert_eq!(core.snapshot().preferences.query, "device");
    let snapshot = serde_json::to_value(core.snapshot()).unwrap();
    assert!(snapshot.get("decisions").is_none());
    assert!(snapshot.get("associations").is_none());
    assert!(core.snapshot().sources.is_empty());
    let untouched: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM personal WHERE value='opaque legacy bytes'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(untouched, 4);
    assert_eq!(
        db.query_row("SELECT value FROM cache WHERE key='snapshot'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "opaque legacy snapshot"
    );
    assert_eq!(
        db.query_row("SELECT value FROM history WHERE key='old'", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "opaque history"
    );
    drop(core);
    assert_eq!(
        CommandCenter::open(&path)
            .unwrap()
            .snapshot()
            .preferences
            .query,
        "device"
    );
}

#[test]
fn legacy_search_cannot_be_read_without_a_selected_project() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state.sqlite");
    drop(CommandCenter::open(&path).unwrap());
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(r#"INSERT INTO search(id,body,value) VALUES('private','private secret','{"id":"private","path":"private.md","title":"private","content":"private secret","line":1,"revision":"old","committedRevision":"old","dirty":false,"factKind":"local"}' )"#, []).unwrap();
    db.execute(
        "INSERT INTO search_fts(id,body) VALUES('private','private secret')",
        [],
    )
    .unwrap();
    // The legacy source cache has no active device project identity.
    db.execute(
        "INSERT INTO cache(key,value) VALUES('snapshot','opaque old private snapshot')",
        [],
    )
    .unwrap();
    let private = temp.path().join("private");
    oasis7_context::store::Store::init(&private, "private", "Private").unwrap();
    oasis7_command_center_core::context::ContextService::open(&path)
        .unwrap()
        .bind(&private, "private", "Private", false)
        .unwrap();
    let core = CommandCenter::open(&path).unwrap();
    assert!(core.snapshot().project.is_none());
    assert!(core.search("private", 20).is_err());
    assert!(core.search("x", 20).is_err());
}
