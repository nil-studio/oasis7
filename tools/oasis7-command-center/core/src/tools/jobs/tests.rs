use super::*;

fn original_file_input() -> super::super::FileSignInput {
    super::super::FileSignInput {
        job_id: "OriginalJob".into(),
        file_path: "/selected/original".into(),
        signer_id: "signer".into(),
        grant_id: "grant".into(),
        context: serde_json::json!({"intent":"original"}),
    }
}
#[test]
fn file_input_survives_restart_atomically_and_cannot_be_replaced() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("jobs.sqlite");
    let original = original_file_input();
    let jobs = ToolJobs::open(&path).unwrap();
    jobs.admit_file_prepare("op", "fp", "signer:installation:OriginalJob", &original)
        .unwrap();
    assert!(
        !serde_json::to_string(&jobs.list().unwrap())
            .unwrap()
            .contains("/selected/original")
    );
    drop(jobs);
    let jobs = ToolJobs::open(&path).unwrap();
    assert_eq!(jobs.list().unwrap()[0].state, "unknown");
    assert_eq!(
        serde_json::to_value(jobs.file_prepare_input("op").unwrap().unwrap()).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    let mut changed = original.clone();
    changed.context = serde_json::json!({"intent":"replacement"});
    assert!(
        jobs.admit_file_prepare("op", "fp", "signer:installation:OriginalJob", &changed)
            .is_err()
    );
    jobs.finish("op", "succeeded", None).unwrap();
    assert!(jobs.file_prepare_input("op").unwrap().is_some());
    jobs.admit("legacy", "fp", "legacy", "file_prepare")
        .unwrap();
    assert!(jobs.file_prepare_input("legacy").unwrap().is_none());
}
#[test]
fn frozen_new_file_admission_has_no_effect_but_existing_and_sql_errors_do_not() {
    let jobs = ToolJobs::open(":memory:").unwrap();
    let input = original_file_input();
    jobs.admit_file_prepare("old", "oldfp", "signer:install:old", &input)
        .unwrap();
    jobs.freeze().unwrap();
    let mut dispatches = 0;
    let run = |id: &str,
               dispatches: &mut usize|
     -> std::result::Result<(), super::super::FilePrepareError> {
        let (_, fresh) = jobs.admit_file_prepare(id, "fp", "signer:install:new", &input)?;
        assert!(fresh);
        *dispatches += 1;
        Ok(())
    };
    assert!(matches!(
        run("new", &mut dispatches).unwrap_err().effect,
        super::super::FilePrepareEffect::None
    ));
    assert_eq!(dispatches, 0);
    assert!(jobs.file_prepare_input("new").unwrap().is_none());
    assert!(matches!(
        run("old", &mut dispatches).unwrap_err().effect,
        super::super::FilePrepareEffect::Unknown
    ));
    let same_resource = jobs
        .admit_file_prepare("alias", "fp", "signer:INSTALL:OLD", &input)
        .unwrap_err();
    assert!(matches!(
        same_resource.effect,
        super::super::FilePrepareEffect::Unknown
    ));
    assert!(jobs.file_prepare_input("alias").unwrap().is_none());
    jobs.unfreeze().unwrap();
    run("new", &mut dispatches).unwrap();
    assert_eq!(dispatches, 1);
    jobs.freeze().unwrap();
    jobs.inner
        .lock()
        .unwrap()
        .db
        .execute_batch("DROP TABLE tool_jobs")
        .unwrap();
    assert!(matches!(
        run("sql", &mut dispatches).unwrap_err().effect,
        super::super::FilePrepareEffect::Unknown
    ));
    assert_eq!(dispatches, 1);
}
#[test]
fn signer_job_case_aliases_preserve_existing_running_and_unknown_reservations() {
    let jobs = ToolJobs::open(":memory:").unwrap();
    jobs.admit(
        "original",
        "original-input",
        "signer:install:JobOne",
        "file_prepare",
    )
    .unwrap();
    assert!(
        jobs.admit(
            "another",
            "other-input",
            "signer:install:jobone",
            "file_prepare"
        )
        .is_err()
    );
    jobs.finish("original", "unknown", None).unwrap();
    assert!(
        jobs.admit(
            "another",
            "other-input",
            "signer:install:JOBONE",
            "file_prepare"
        )
        .is_err()
    );
    assert!(
        jobs.admit(
            "original",
            "original-input",
            "signer:install:jobone",
            "file_prepare"
        )
        .is_err()
    );
    jobs.finish("original", "succeeded", None).unwrap();
    assert!(
        jobs.admit(
            "another",
            "other-input",
            "signer:install:jobone",
            "file_prepare"
        )
        .is_ok()
    );
    // Only signer Jobs have this conservative filesystem-alias guard.
    jobs.admit("cli-upper", "upper", "cli:Release", "install")
        .unwrap();
    assert!(
        jobs.admit("cli-lower", "lower", "cli:release", "install")
            .is_ok()
    );
}
#[test]
fn secret_admission_does_not_block_job_queries_or_receipts() {
    let jobs = std::sync::Arc::new(ToolJobs::open(":memory:").unwrap());
    jobs.admit("save", "hash", "store", "save").unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker = jobs.clone();
    let thread = std::thread::spawn(move || {
        worker
            .with_open(|| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
            .unwrap()
    });
    started_rx.recv().unwrap();
    assert_eq!(jobs.list().unwrap().len(), 1);
    jobs.finish("save", "succeeded", None).unwrap();
    let (frozen_tx, frozen_rx) = std::sync::mpsc::channel();
    let freeze_jobs = jobs.clone();
    let freeze = std::thread::spawn(move || {
        freeze_jobs.freeze().unwrap();
        frozen_tx.send(()).unwrap();
    });
    assert!(
        frozen_rx
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err()
    );
    release_tx.send(()).unwrap();
    thread.join().unwrap();
    freeze.join().unwrap();
    assert!(jobs.with_open(|| Ok(())).is_err());
}
#[test]
fn idempotent_admission_freeze_and_terminal_monotonicity() {
    let jobs = ToolJobs::open(":memory:").unwrap();
    jobs.admit("one", "hash", "root", "save").unwrap();
    assert!(jobs.admit("one", "different", "root", "save").is_err());
    assert!(jobs.admit("two", "hash", "root", "save").is_err());
    jobs.finish("one", "succeeded", None).unwrap();
    assert_eq!(
        jobs.finish("one", "unknown", None).unwrap().state,
        "succeeded"
    );
    jobs.freeze().unwrap();
    assert!(jobs.admit("two", "hash", "other", "save").is_err());
    jobs.unfreeze().unwrap();
    jobs.admit("two", "hash", "other", "save").unwrap();
    assert!(jobs.quit_pending().unwrap());
}
