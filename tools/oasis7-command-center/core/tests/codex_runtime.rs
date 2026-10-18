#![cfg(unix)]
use oasis7_command_center_core::{CommandCenter, codex::*};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
struct Fixture {
    tmp: tempfile::TempDir,
    project: oasis7_command_center_core::domain::Project,
    db: PathBuf,
    binary: PathBuf,
    trace: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init"]);
        git(&root, &["config", "user.name", "test"]);
        git(&root, &["config", "user.email", "test@example.org"]);
        std::fs::write(root.join("README.md"), "fixture").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "fixture"]);
        let db = tmp.path().join("runtime.db");
        let mut core = CommandCenter::open(&db).unwrap();
        let project = core.open_project(&root).unwrap().project.unwrap();
        let binary = tmp.path().join("fake-codex");
        let trace = tmp.path().join("trace");
        let script=r##"#!/usr/bin/env python3
import sys,json,subprocess,time,os
TRACE=__TRACE__
if '--version' in sys.argv: print('codex-cli 0.137.0');sys.exit(0)
threads={};n=0;resumed=False;helper=None

def send(v): print(json.dumps(v),flush=True)
def reply(m,v): send({'id':m['id'],'result':v})
def thread(t): return {'thread':{'id':t,'status':{'type':threads.get(t,'idle')}},'model':'test','cwd':__ROOT__}
def completed(t,turn,status='completed'):
 threads[t]='idle';send({'method':'turn/completed','params':{'threadId':t,'turn':{'id':turn,'status':status,'items':[],'error':None}}});send({'method':'thread/status/changed','params':{'threadId':t,'status':{'type':'idle'}}})
for line in sys.stdin:
 m=json.loads(line)
 with open(TRACE,'a') as f:f.write(json.dumps({'method':m.get('method','REPLY'),'id':m.get('id')})+'\n')
 if 'method' not in m:
  send({'method':'serverRequest/resolved','params':{'threadId':'t1','requestId':m['id']}});completed('t1','turn-t1');continue
 method=m['method'];p=m.get('params',{})
 if method=='initialize':reply(m,{'userAgent':'codex/0.137.0'})
 elif method=='thread/start':
  n+=1;t='t'+str(n);threads[t]='idle'
  if n==1:helper=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'],start_new_session=True)
  reply(m,thread(t))
 elif method=='thread/resume':resumed=True;reply(m,thread(p['threadId']))
 elif method=='thread/read':
  reply(m,thread(p['threadId']))
  if os.path.exists(TRACE+'.exit'):sys.exit(0)
 elif method=='turn/start':
  t=p['threadId'];turn='turn-'+t;mode=p['input'][0]['text'];threads[t]='active'
  send({'method':'turn/started','params':{'threadId':t,'turn':{'id':turn,'status':'inProgress','items':[],'error':None}}})
  if mode=='malformed':reply(m,{});continue
  if mode=='lost':threads[t]='idle';continue
  reply(m,{'turn':{'id':turn,'status':'inProgress','items':[],'error':None}})
  if mode in ['request','burst']:
   if mode=='burst':
    time.sleep(0.15)
    for i in range(100):send({'method':'item/started','params':{'threadId':t,'turnId':turn,'item':{'id':'event'+str(i),'type':'commandExecution'}}})
   req={'id':9007199254740993,'method':'item/tool/requestUserInput','params':{'itemId':'q1','threadId':t,'turnId':turn,'questions':[{'id':'choice','header':'Choice','question':'Choose','isSecret':True,'options':[{'label':'A','description':'A'}]}]}}
   send(req);send(req)
  elif mode=='exit':
   if helper:helper.terminate();helper.wait()
   sys.exit(0)
  elif mode=='old':send({'method':'turn/completed','params':{'threadId':t,'turn':{'id':'OLD','status':'completed','items':[],'error':None}}})
  elif mode=='background':subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)']);completed(t,turn)
  elif mode=='overflow':
   for i in range(150):send({'id':'q'+str(i),'method':'item/commandExecution/requestApproval','params':{'itemId':'i'+str(i),'startedAtMs':1,'threadId':t,'turnId':turn,'command':'echo x'}})
  elif mode=='failed':completed(t,turn,'failed')
  elif mode=='complete':
   if resumed:time.sleep(0.25)
   completed(t,turn)
 elif method=='turn/interrupt':reply(m,{});completed(p['threadId'],p['turnId'],'interrupted')
 elif method=='turn/steer':reply(m,{'turnId':p['expectedTurnId']})
"##.replace("__TRACE__",&serde_json::to_string(&trace).unwrap()).replace("__ROOT__",&serde_json::to_string(&root).unwrap());
        std::fs::write(&binary, script).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            tmp,
            project,
            db,
            binary,
            trace,
        }
    }
    fn runtime(&self) -> Runtime {
        let mut r = Runtime::open(&self.db).unwrap();
        assert!(!r.snapshot(&self.project).unwrap().enabled);
        r.set_enabled(&self.project, true).unwrap();
        r.connect(&self.project, self.binary.to_str().unwrap())
            .unwrap();
        r
    }
    fn session(&self, r: &mut Runtime) -> String {
        let s = r
            .create_session(
                &self.project,
                SessionConfig {
                    purpose: "test work".into(),
                    scope: "fixture".into(),
                    model: None,
                    basis_revision: self.project.head.clone(),
                },
            )
            .unwrap();
        tick();
        r.snapshot(&self.project).unwrap();
        s.sessions.last().unwrap().thread_id.clone()
    }
}
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
fn wait_idle(r: &mut Runtime) {
    let end = std::time::Instant::now() + Duration::from_secs(3);
    while !r.quit_status().safe && std::time::Instant::now() < end {
        tick();
    }
}
fn tick() {
    std::thread::sleep(Duration::from_millis(100));
}
#[test]
fn genuine_requests_preserve_big_id_target_and_receipt_with_independent_persistence() {
    let f = Fixture::new();
    let mut r = f.runtime();
    let t = f.session(&mut r);
    r.start_turn(&f.project, &t, "request").unwrap();
    tick();
    let s = r.snapshot(&f.project).unwrap();
    assert_eq!(s.requests.len(), 1);
    assert!(s.operations.iter().any(|o| o.method == "thread/start"
        && o.state == "confirmed"
        && o.thread_id.as_deref() == Some(t.as_str())));
    assert!(s.operations.iter().any(|o| o.method == "turn/start"
        && o.state == "confirmed"
        && o.thread_id.as_deref() == Some(t.as_str())
        && o.turn_id.as_deref() == Some("turn-t1")));
    let req = &s.requests[0];
    assert_eq!(req.request_id, "9007199254740993");
    let target = RuntimeTarget {
        instance_id: req.instance_id.clone(),
        thread_id: req.thread_id.clone(),
        turn_id: req.turn_id.clone(),
    };
    let answer = RequestAnswer {
        request_id: req.request_id.clone(),
        target: target.clone(),
        decision: None,
        answers: BTreeMap::from([("choice".into(), vec!["TOP-SECRET-ANSWER".into()])]),
    };
    let mut wrong = answer.clone();
    wrong.target.instance_id = "other".into();
    assert!(r.answer_request(&f.project, wrong).is_err());
    let conn = rusqlite::Connection::open(&f.db).unwrap();
    let source: String = conn
        .query_row("SELECT value FROM cc_runtime_state", [], |row| row.get(0))
        .unwrap();
    assert!(source.contains("9007199254740993"));
    // An unrelated missing receipt must not prevent management of a genuine request.
    let phantom = serde_json::json!({"id":"unknown-steer","instanceId":target.instance_id,"threadId":target.thread_id,"turnId":target.turn_id,"method":"turn/steer","state":"unknown","fingerprint":"test","createdAt":"test","error":null});
    conn.execute("INSERT INTO cc_runtime_operations(id,scope,target,state,value) VALUES('unknown-steer',?1,?2,'unknown',?3)",rusqlite::params![format!("{}:local",f.project.id),target.thread_id,phantom.to_string()]).unwrap();
    r.answer_request(&f.project, answer.clone()).unwrap();
    conn.execute(
        "UPDATE cc_runtime_operations SET state='sourceResolved' WHERE id='unknown-steer'",
        [],
    )
    .unwrap();
    tick();
    let final_s = r.snapshot(&f.project).unwrap();
    assert_eq!(final_s.requests[0].state, "resolved");
    assert!(r.answer_request(&f.project, answer).is_err());
    wait_idle(&mut r);
    assert!(
        r.quit_status().safe,
        "{:?} {:?}",
        r.quit_status(),
        r.snapshot(&f.project).unwrap()
    );
    let trace = std::fs::read_to_string(&f.trace).unwrap();
    assert!(trace.contains("9007199254740993"));
    let stored: String = conn
        .query_row(
            "SELECT group_concat(value) FROM cc_runtime_operations",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!stored.contains("TOP-SECRET-ANSWER"));
    assert!(r.shutdown_if_idle().unwrap().safe);
}
#[test]
fn lost_receipt_persists_unknown_never_replays_and_readonly_reconciliation_is_explicit() {
    let f = Fixture::new();
    let mut r = f.runtime();
    let t = f.session(&mut r);
    let s = r.start_turn(&f.project, &t, "lost").unwrap();
    let op = s
        .operations
        .iter()
        .find(|o| o.method == "turn/start")
        .unwrap();
    assert_eq!(op.state, "unknown");
    let id = op.id.clone();
    assert!(!r.quit_status().safe);
    assert!(r.start_turn(&f.project, &t, "lost").is_err());
    let reconciled = r.reconcile_operation(&f.project, &id).unwrap();
    assert!(
        reconciled
            .operations
            .iter()
            .any(|o| o.id == id && o.state == "sourceResolved")
    );
    assert!(r.quit_status().safe);
    let count = std::fs::read_to_string(&f.trace)
        .unwrap()
        .matches("turn/start")
        .count();
    assert_eq!(count, 1);
    drop(r);
    let mut reopened = Runtime::open(&f.db).unwrap();
    assert!(
        reopened
            .snapshot(&f.project)
            .unwrap()
            .operations
            .iter()
            .any(|o| o.id == id && o.state == "sourceResolved")
    );
    assert_eq!(
        std::fs::read_to_string(&f.trace)
            .unwrap()
            .matches("turn/start")
            .count(),
        1
    );
}
#[test]
fn two_owned_sessions_same_branch_old_turn_and_disable_keep_original_management() {
    let f = Fixture::new();
    let mut r = f.runtime();
    let first = f.session(&mut r);
    let second = f.session(&mut r);
    assert_ne!(first, second);
    r.start_turn(&f.project, &first, "old").unwrap();
    tick();
    let s = r.set_enabled(&f.project, false).unwrap();
    let active = s.sessions.iter().find(|s| s.thread_id == first).unwrap();
    assert_eq!(active.state, "active");
    assert!(r.start_turn(&f.project, &second, "complete").is_err());
    let target = RuntimeTarget {
        instance_id: active.instance_id.clone(),
        thread_id: first,
        turn_id: active.turn_id.clone().unwrap(),
    };
    let mut wrong = target.clone();
    wrong.turn_id = "OLD".into();
    assert!(r.interrupt_turn(&f.project, wrong).is_err());
    r.interrupt_turn(&f.project, target).unwrap();
    tick();
    assert!(r.quit_status().safe);
    assert!(r.shutdown_if_idle().unwrap().safe);
}
#[test]
fn completed_turn_with_background_process_does_not_allow_exit() {
    let f = Fixture::new();
    let mut r = f.runtime();
    let t = f.session(&mut r);
    r.start_turn(&f.project, &t, "background").unwrap();
    tick();
    let s = r.snapshot(&f.project).unwrap();
    assert_eq!(s.sessions[0].state, "completed");
    assert_eq!(s.sessions[0].background_state, "unknown");
    assert!(!r.shutdown_if_idle().unwrap().safe);
    assert!(r.start_turn(&f.project, &t, "complete").is_err());
}
#[test]
fn storage_failure_and_head_drift_prevent_wire_sideeffects() {
    let f = Fixture::new();
    let mut r = f.runtime();
    let t = f.session(&mut r);
    let conn = rusqlite::Connection::open(&f.db).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_ops BEFORE INSERT ON cc_runtime_operations BEGIN SELECT RAISE(FAIL,'no storage'); END;").unwrap();
    assert!(r.start_turn(&f.project, &t, "complete").is_err());
    assert!(
        !std::fs::read_to_string(&f.trace)
            .unwrap()
            .contains("turn/start")
    );
    conn.execute_batch("DROP TRIGGER reject_ops;").unwrap();
    std::fs::write(Path::new(&f.project.path).join("new"), "new").unwrap();
    git(Path::new(&f.project.path), &["add", "."]);
    git(Path::new(&f.project.path), &["commit", "-m", "new head"]);
    assert!(r.start_turn(&f.project, &t, "complete").is_err());
    assert!(
        !std::fs::read_to_string(&f.trace)
            .unwrap()
            .contains("turn/start")
    );
    let _ = &f.tmp;
}
#[test]
fn malformed_success_and_critical_overflow_are_unknown_and_disable_new_work() {
    for mode in ["malformed", "overflow"] {
        let f = Fixture::new();
        let mut r = f.runtime();
        let t = f.session(&mut r);
        r.start_turn(&f.project, &t, mode).unwrap();
        tick();
        let s = r.snapshot(&f.project).unwrap();
        assert_eq!(s.state, "degraded");
        assert!(s.requests.len() <= 128);
        assert!(!r.quit_status().safe);
        assert!(
            r.create_session(
                &f.project,
                SessionConfig {
                    purpose: "x".into(),
                    scope: "x".into(),
                    model: None,
                    basis_revision: f.project.head.clone()
                }
            )
            .is_err()
        );
    }
}
#[test]
fn schema_two_migration_preserves_personal_history_and_recovers_pending_operations() {
    let f = Fixture::new();
    let conn = rusqlite::Connection::open(&f.db).unwrap();
    conn.execute_batch("PRAGMA user_version=1;INSERT INTO personal(key,value) VALUES('sentinel','{}');INSERT INTO history(key,value) VALUES('sentinel','{}');").unwrap();
    let mut r = Runtime::open(&f.db).unwrap();
    assert!(!r.snapshot(&f.project).unwrap().enabled);
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap(),
        2
    );
    for table in ["personal", "history"] {
        assert_eq!(
            conn.query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE key='sentinel'"),
                [],
                |row| row.get::<_, u32>(0)
            )
            .unwrap(),
            1
        );
    }
}

#[test]
fn clean_history_requires_explicit_resume_and_future_database_clock_is_preserved() {
    let f = Fixture::new();
    let mut r = f.runtime();
    let t = f.session(&mut r);
    r.start_turn(&f.project, &t, "complete").unwrap();
    tick();
    r.snapshot(&f.project).unwrap();
    wait_idle(&mut r);
    assert!(r.shutdown_if_idle().unwrap().safe);
    drop(r);
    let conn = rusqlite::Connection::open(&f.db).unwrap();
    conn.execute(
        "UPDATE cc_runtime_state SET sequence=sequence+100000000000000",
        [],
    )
    .unwrap();
    let mut r = Runtime::open(&f.db).unwrap();
    assert!(r.snapshot(&f.project).unwrap().sessions[0].restorable_history);
    r.connect(&f.project, f.binary.to_str().unwrap()).unwrap();
    assert!(r.start_turn(&f.project, &t, "complete").is_err());
    r.continue_session(&f.project, &t).unwrap();
    r.start_turn(&f.project, &t, "complete").unwrap();
    // A receipt confirms turn creation, not asynchronous completion or background-idle proof.
    wait_idle(&mut r);
    let status = r.shutdown_if_idle().unwrap();
    assert!(
        status.safe,
        "resumed turn never reached verified idle: {status:?}"
    );
}

#[test]
fn slow_event_store_coalesces_burst_without_losing_original_pending_request() {
    let f = Fixture::new();
    let mut r = f.runtime();
    let t = f.session(&mut r);
    r.start_turn(&f.project, &t, "burst").unwrap();
    let db = rusqlite::Connection::open(&f.db).unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    std::thread::sleep(Duration::from_millis(500));
    db.execute_batch("COMMIT").unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let source: String = db
            .query_row("SELECT value FROM cc_runtime_state", [], |row| row.get(0))
            .unwrap();
        if source.contains("9007199254740993") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "reader request never persisted independently"
        );
        tick();
    }
    let snapshot = r.snapshot(&f.project).unwrap();
    assert_eq!(snapshot.state, "ready");
    assert_eq!(snapshot.requests.len(), 1);
    assert_eq!(snapshot.requests[0].request_id, "9007199254740993");
    assert_eq!(snapshot.requests[0].state, "pending");
    assert!(!r.quit_status().safe);
}
#[test]
fn event_writer_must_be_ready_before_app_server_process_starts() {
    let f = Fixture::new();
    let mut r = Runtime::open(&f.db).unwrap();
    r.set_enabled(&f.project, true).unwrap();
    let db = rusqlite::Connection::open(&f.db).unwrap();
    db.execute_batch("PRAGMA user_version=1").unwrap();
    assert!(r.connect(&f.project, f.binary.to_str().unwrap()).is_err());
    assert!(
        !f.trace.exists(),
        "failed writer readiness must not initialize owned process"
    );
}

#[test]
fn empty_exited_instance_retires_without_automatic_reconnect_but_work_stays_unknown() {
    let f = Fixture::new();
    seed_history(&f, false);
    std::fs::write(format!("{}.exit", f.trace.display()), "").unwrap();
    let mut r = Runtime::open(&f.db).unwrap();
    r.connect(&f.project, f.binary.to_str().unwrap()).unwrap();
    r.read_thread(&f.project, "old-1").unwrap();
    let end = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let snapshot = r.snapshot(&f.project).unwrap();
        if snapshot.state == "disconnected" {
            break;
        }
        assert!(std::time::Instant::now() < end);
        tick();
    }
    let historical = r.snapshot(&f.project).unwrap();
    assert_eq!(historical.sessions.len(), 128);
    assert!(
        historical.sessions.iter().all(|s| s.state == "completed"
            && s.background_state == "clear"
            && s.restorable_history)
    );
    let before = std::fs::read_to_string(&f.trace).unwrap();
    assert!(r.quit_status().safe);
    assert_eq!(std::fs::read_to_string(&f.trace).unwrap(), before);
    std::fs::remove_file(format!("{}.exit", f.trace.display())).unwrap();
    assert_eq!(
        r.connect(&f.project, f.binary.to_str().unwrap())
            .unwrap()
            .state,
        "ready"
    );
    let thread = f.session(&mut r);
    let snapshot = r.start_turn(&f.project, &thread, "exit").unwrap();
    let original = snapshot.instance_id.clone();
    tick();
    let snapshot = r.snapshot(&f.project).unwrap();
    assert_eq!(snapshot.state, "degraded");
    assert!(!r.quit_status().safe);
    assert!(r.connect(&f.project, f.binary.to_str().unwrap()).is_err());
    assert_eq!(r.snapshot(&f.project).unwrap().instance_id, original);
    assert!(
        snapshot
            .operations
            .iter()
            .any(|o| o.thread_id.as_deref() == Some(&thread))
    );
}
fn seed_history(f: &Fixture, protected: bool) {
    let mut r = f.runtime();
    let mut saved = r.snapshot(&f.project).unwrap();
    assert!(r.shutdown_if_idle().unwrap().safe);
    drop(r);
    saved.state = "disconnected".into();
    saved.sessions = (0..128)
        .map(|i| oasis7_command_center_core::codex::RuntimeSession {
            purpose: "old purpose".into(),
            scope: "old scope".into(),
            model: None,
            basis_revision: f.project.head.clone(),
            thread_id: format!("old-{i}"),
            instance_id: "historic".into(),
            turn_id: Some(format!("past-{i}")),
            state: if protected { "unknown" } else { "completed" }.into(),
            owned: true,
            background_state: if protected { "unknown" } else { "clear" }.into(),
            restorable_history: !protected,
            private_sources: vec![],
        })
        .collect();
    let db = rusqlite::Connection::open(&f.db).unwrap();
    db.execute(
        "UPDATE cc_runtime_state SET value=?1",
        [serde_json::to_string(&saved).unwrap()],
    )
    .unwrap();
    let op = serde_json::json!({"id":"durable-old","instanceId":"historic","threadId":"old-0","turnId":"past-0","method":"turn/start","state":"confirmed","fingerprint":"old","createdAt":"old","error":null});
    db.execute("INSERT INTO cc_runtime_operations(id,scope,target,state,value) VALUES('durable-old',?1,'old-0','confirmed',?2)",rusqlite::params![format!("{}:local",f.project.id),op.to_string()]).unwrap();
}
#[test]
fn session_window_prunes_only_clean_terminal_summaries_and_retains_operation_history() {
    let f = Fixture::new();
    seed_history(&f, false);
    let mut r = Runtime::open(&f.db).unwrap();
    r.connect(&f.project, f.binary.to_str().unwrap()).unwrap();
    let thread = f.session(&mut r);
    let snapshot = r.snapshot(&f.project).unwrap();
    assert_eq!(snapshot.sessions.len(), 128);
    assert!(!snapshot.sessions.iter().any(|s| s.thread_id == "old-0"));
    assert!(snapshot.sessions.iter().any(|s| s.thread_id == thread));
    let db = rusqlite::Connection::open(&f.db).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM cc_runtime_operations WHERE id='durable-old'",
            [],
            |row| row.get::<_, u32>(0)
        )
        .unwrap(),
        1
    );
    let f = Fixture::new();
    seed_history(&f, true);
    let mut r = Runtime::open(&f.db).unwrap();
    r.connect(&f.project, f.binary.to_str().unwrap()).unwrap();
    assert!(
        r.create_session(
            &f.project,
            SessionConfig {
                purpose: "new".into(),
                scope: "new".into(),
                model: None,
                basis_revision: f.project.head.clone()
            }
        )
        .is_err()
    );
    let trace = std::fs::read_to_string(&f.trace).unwrap();
    assert!(
        !trace.contains("thread/start"),
        "historical unknown must block new side effects"
    );
    assert_eq!(r.snapshot(&f.project).unwrap().sessions.len(), 128);
    assert!(!r.quit_status().safe);
}

#[test]
fn private_input_is_registered_before_send_and_filters_later_public_projections() {
    let f = Fixture::new();
    let mut r = f.runtime();
    let thread = f.session(&mut r);
    let source = PrivateInputRef {
        binding_id: "b".into(),
        binding_version: 1,
        entry_id: "machine".into(),
        revision: "rev".into(),
        scope: "whole_entry".into(),
    };
    r.start_private_turn(&f.project, &thread, "request", vec![source.clone()])
        .unwrap();
    tick();
    let snapshot = r.snapshot(&f.project).unwrap();
    assert_eq!(snapshot.sessions[0].private_sources, vec![source]);
    assert_eq!(snapshot.requests.len(), 1); // approval detail remains available to its owner
    let public = r.public_snapshot(&f.project).unwrap();
    assert!(public.sessions.is_empty());
    assert!(public.requests.is_empty());
    let expected_operations: Vec<_> = snapshot
        .operations
        .iter()
        .filter(|op| op.thread_id.as_deref() != Some(thread.as_str()))
        .map(|op| op.id.as_str())
        .collect();
    let public_operations: Vec<_> = public.operations.iter().map(|op| op.id.as_str()).collect();
    assert_eq!(public_operations, expected_operations);
    let req = &snapshot.requests[0];
    r.answer_request(
        &f.project,
        RequestAnswer {
            request_id: req.request_id.clone(),
            target: RuntimeTarget {
                instance_id: req.instance_id.clone(),
                thread_id: thread.clone(),
                turn_id: req.turn_id.clone(),
            },
            decision: None,
            answers: BTreeMap::from([("choice".into(), vec!["A".into()])]),
        },
    )
    .unwrap();
    tick();
    r.start_turn(&f.project, &thread, "ordinary later turn")
        .unwrap();
    tick();
    assert_eq!(
        r.snapshot(&f.project).unwrap().sessions[0]
            .private_sources
            .len(),
        1
    );
    assert!(r.public_snapshot(&f.project).unwrap().sessions.is_empty());
    r.stop_for_exit().unwrap();
    wait_idle(&mut r);
    let _ = r.shutdown_if_idle();
    drop(r);
    let mut reopened = Runtime::open(&f.db).unwrap();
    assert_eq!(
        reopened.snapshot(&f.project).unwrap().sessions[0]
            .private_sources
            .len(),
        1
    );
    assert!(
        reopened
            .public_snapshot(&f.project)
            .unwrap()
            .requests
            .is_empty()
    );
}
#[test]
fn failed_private_registration_prevents_any_turn_start_frame() {
    let f = Fixture::new();
    let mut r = f.runtime();
    let thread = f.session(&mut r);
    let db = rusqlite::Connection::open(&f.db).unwrap();
    db.execute_batch("DROP TABLE cc_runtime_private_inputs")
        .unwrap();
    let source = PrivateInputRef {
        binding_id: "b".into(),
        binding_version: 1,
        entry_id: "machine".into(),
        revision: "rev".into(),
        scope: "whole_entry".into(),
    };
    assert!(
        r.start_private_turn(&f.project, &thread, "private data", vec![source])
            .is_err()
    );
    assert!(
        !std::fs::read_to_string(&f.trace)
            .unwrap()
            .contains("turn/start")
    );
}

#[test]
fn composed_start_preserves_child_operations_and_hides_intents_after_thread_becomes_private() {
    let f = Fixture::new();
    let mut runtime = f.runtime();
    let config = SessionConfig {
        purpose: "explicit work".into(),
        scope: "fixture only".into(),
        model: None,
        basis_revision: f.project.head.clone(),
    };
    runtime
        .start_work(&f.project, "intent-original", config.clone(), "complete")
        .unwrap();
    tick();
    let records = runtime.start_work_records(&f.project).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, "inputAcknowledged");
    assert_eq!(records[0].operation_ids.len(), 2);
    let thread = records[0].thread_id.clone().unwrap();
    assert!(
        runtime
            .start_work(&f.project, "intent-original", config, "complete")
            .is_err()
    );
    runtime
        .start_private_turn(
            &f.project,
            &thread,
            "request",
            vec![PrivateInputRef {
                binding_id: "b".into(),
                binding_version: 1,
                entry_id: "private".into(),
                revision: "r".into(),
                scope: "whole_entry".into(),
            }],
        )
        .unwrap();
    tick();
    assert!(
        runtime
            .public_snapshot(&f.project)
            .unwrap()
            .requests
            .is_empty()
    );
    assert!(
        runtime
            .public_start_work_records(&f.project)
            .unwrap()
            .is_empty()
    );
    assert_eq!(runtime.start_work_records(&f.project).unwrap().len(), 1);
}
#[test]
fn unknown_composed_start_never_replays_under_a_new_id_or_after_restart() {
    for lose_creation in [false, true] {
        let f = Fixture::new();
        if lose_creation {
            let code = std::fs::read_to_string(&f.binary).unwrap().replacen(
                "  reply(m,thread(t))",
                "  continue",
                1,
            );
            std::fs::write(&f.binary, code).unwrap();
        }
        let mut runtime = f.runtime();
        let config = SessionConfig {
            purpose: "explicit work".into(),
            scope: "fixture only".into(),
            model: None,
            basis_revision: f.project.head.clone(),
        };
        runtime
            .start_work(&f.project, "intent-lost", config.clone(), "lost")
            .unwrap();
        assert_eq!(
            runtime.start_work_records(&f.project).unwrap()[0].state,
            "unknown"
        );
        assert!(
            runtime
                .start_work(&f.project, "new-id", config.clone(), "lost")
                .is_err()
        );
        drop(runtime);
        let mut restored = Runtime::open(&f.db).unwrap();
        assert_eq!(
            restored.start_work_records(&f.project).unwrap()[0].state,
            "unknown"
        );
        assert!(
            restored
                .start_work(&f.project, "another-id", config, "lost")
                .is_err()
        );
        let trace = std::fs::read_to_string(&f.trace).unwrap();
        assert_eq!(trace.matches("thread/start").count(), 1);
        assert_eq!(
            trace.matches("turn/start").count(),
            usize::from(!lose_creation)
        );
    }
}

#[test]
fn composed_intent_recovery_uses_original_ledger_and_never_replays_it() {
    let f = Fixture::new();
    let mut runtime = f.runtime();
    let config = SessionConfig {
        purpose: "explicit work".into(),
        scope: "fixture only".into(),
        model: None,
        basis_revision: f.project.head.clone(),
    };
    runtime
        .start_work(&f.project, "intent-original", config.clone(), "complete")
        .unwrap();
    tick();
    let record = runtime.start_work_records(&f.project).unwrap().remove(0);
    let db = rusqlite::Connection::open(&f.db).unwrap();
    let key = format!("start_intent_v1:{}:local:intent-original", f.project.id);
    let mut value = serde_json::to_value(&record).unwrap();
    value["state"] = serde_json::json!("unknown");
    db.execute(
        "UPDATE personal SET value=?1 WHERE key=?2",
        rusqlite::params![value.to_string(), key],
    )
    .unwrap();
    // Settled children can be older than the ordinary 200-operation snapshot.
    for i in 0..201 {
        let op = serde_json::json!({"id":format!("later-{i}"),"instanceId":"unrelated","threadId":"other","turnId":null,"method":"turn/start","state":"confirmed","fingerprint":"fixture","createdAt":"now","error":null});
        db.execute("INSERT INTO cc_runtime_operations(id,scope,target,state,value) VALUES(?1,?2,'other','confirmed',?3)",rusqlite::params![format!("later-{i}"),format!("{}:local",f.project.id),op.to_string()]).unwrap();
    }
    assert_eq!(
        runtime.start_work_records(&f.project).unwrap()[0].state,
        "inputAcknowledged"
    );
    assert!(
        runtime
            .start_work(&f.project, "intent-original", config.clone(), "complete")
            .is_err()
    );
    runtime
        .start_work(&f.project, "genuinely-new", config, "complete")
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&f.trace)
            .unwrap()
            .matches("thread/start")
            .count(),
        2
    );
}

#[test]
fn unresolved_intents_beyond_display_window_still_block_new_effects() {
    let f = Fixture::new();
    let mut runtime = f.runtime();
    let db = rusqlite::Connection::open(&f.db).unwrap();
    for i in 0..=1000 {
        let id = if i == 1000 {
            "z-unresolved".to_owned()
        } else {
            format!("a-{i:04}")
        };
        let record = serde_json::json!({"schemaVersion":1,"intentId":id,"projectId":f.project.id,"environment":"local","basisRevision":f.project.head,"fingerprint":"fixture","state":if i==1000{"unknown"}else{"inputAcknowledged"},"threadId":null,"operationIds":[],"updatedAt":"now"});
        db.execute(
            "INSERT INTO personal(key,value) VALUES(?1,?2)",
            rusqlite::params![
                format!("start_intent_v1:{}:local:{id}", f.project.id),
                record.to_string()
            ],
        )
        .unwrap();
    }
    let config = SessionConfig {
        purpose: "explicit work".into(),
        scope: "fixture only".into(),
        model: None,
        basis_revision: f.project.head.clone(),
    };
    assert!(
        runtime
            .start_work(&f.project, "new-intent", config, "complete")
            .is_err()
    );
    assert!(
        !std::fs::read_to_string(&f.trace)
            .unwrap()
            .contains("thread/start")
    );
}

#[test]
fn source_audited_intent_releases_new_work_without_claiming_input_acknowledged() {
    let f = Fixture::new();
    let mut runtime = f.runtime();
    let config = SessionConfig {
        purpose: "explicit work".into(),
        scope: "fixture only".into(),
        model: None,
        basis_revision: f.project.head.clone(),
    };
    runtime
        .start_work(&f.project, "original-audited", config.clone(), "complete")
        .unwrap();
    tick();
    let record = runtime.start_work_records(&f.project).unwrap().remove(0);
    let db = rusqlite::Connection::open(&f.db).unwrap();
    let mut value = serde_json::to_value(&record).unwrap();
    value["state"] = serde_json::json!("unknown");
    db.execute(
        "UPDATE personal SET value=?1 WHERE key=?2",
        rusqlite::params![
            value.to_string(),
            format!("start_intent_v1:{}:local:original-audited", f.project.id)
        ],
    )
    .unwrap();
    let turn = runtime
        .snapshot(&f.project)
        .unwrap()
        .operations
        .into_iter()
        .find(|op| record.operation_ids.contains(&op.id) && op.method == "turn/start")
        .unwrap();
    let mut audited = serde_json::to_value(&turn).unwrap();
    audited["state"] = serde_json::json!("sourceResolved");
    db.execute(
        "UPDATE cc_runtime_operations SET state='sourceResolved',value=?1 WHERE id=?2",
        rusqlite::params![audited.to_string(), turn.id],
    )
    .unwrap();
    assert_eq!(
        runtime.start_work_records(&f.project).unwrap()[0].state,
        "sourceResolved"
    );
    assert!(
        runtime
            .start_work(&f.project, "original-audited", config.clone(), "complete")
            .is_err()
    );
    runtime
        .start_work(&f.project, "separate-new", config, "complete")
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&f.trace)
            .unwrap()
            .matches("thread/start")
            .count(),
        2
    );
}
