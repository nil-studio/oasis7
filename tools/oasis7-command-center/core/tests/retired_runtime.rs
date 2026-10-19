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
    core.reload_personal().unwrap();
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
