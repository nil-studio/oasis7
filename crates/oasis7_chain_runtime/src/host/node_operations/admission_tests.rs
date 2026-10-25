//! A forked, not-yet-execed child can retain a short-lived admission descriptor.
use super::super::node_lifecycle::{actions, tests::operation_fixture_with_stop_signal};
use super::{
    executor,
    model::{Kind, Request},
    planner,
    store::Store,
};
use std::{
    net::Shutdown,
    os::{fd::AsRawFd, unix::net::UnixStream},
};

pub(crate) struct InheritedDescriptors {
    pid: libc::pid_t,
    release: UnixStream,
}
impl InheritedDescriptors {
    pub(crate) fn fork() -> Self {
        let (release, wait) = UnixStream::pair().unwrap();
        // SAFETY: Only async-signal-safe libc calls run in the forked child. Holding
        // before exec models another test's concurrent Command::spawn without
        // relying on scheduler timing or changing the normal test concurrency.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork failed");
        if pid == 0 {
            // SAFETY: The socket and stack buffer are valid; read and _exit
            // are async-signal-safe and no Rust destructors run in the child.
            unsafe {
                let mut byte = 0_u8;
                libc::read(wait.as_raw_fd(), (&mut byte as *mut u8).cast(), 1);
                libc::_exit(0);
            }
        }
        Self { pid, release }
    }

    pub(crate) fn is_waiting(&self) -> bool {
        // SAFETY: This is our unreaped child; WNOHANG permits a null status pointer.
        unsafe { libc::waitpid(self.pid, std::ptr::null_mut(), libc::WNOHANG) == 0 }
    }
}
impl Drop for InheritedDescriptors {
    fn drop(&mut self) {
        // shutdown also wakes the child if an unrelated concurrent fork has
        // temporarily inherited another copy of this endpoint.
        let _ = self.release.shutdown(Shutdown::Both);
        loop {
            // SAFETY: Reap only our child; a null status pointer is permitted.
            let result = unsafe { libc::waitpid(self.pid, std::ptr::null_mut(), 0) };
            if result >= 0
                || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                break;
            }
        }
    }
}

#[test]
fn operation_store_releases_writer_while_forked_child_retains_descriptor() {
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "oasis7-operation-store-fork-{}",
            super::super::node_lifecycle::local_io::random_id().unwrap()
        ));
    let store = Store::open(&root).unwrap();
    assert!(Store::open(&root).is_err());
    let inherited = InheritedDescriptors::fork();
    drop(store);
    let child_retained_descriptors = inherited.is_waiting();
    let reopened = Store::open(&root);
    let reopened_ok = reopened.is_ok();
    let exclusive_before_child_exit = Store::open(&root).is_err();
    drop(inherited);
    let exclusive_after_child_exit = Store::open(&root).is_err();
    drop(reopened);
    std::fs::remove_dir_all(&root).unwrap();
    assert!(child_retained_descriptors);
    assert!(
        reopened_ok,
        "finished store writer must release its lock before another child execs"
    );
    assert!(exclusive_before_child_exit && exclusive_after_child_exit);
}

#[test]
fn stop_admission_releases_writer_even_while_forked_child_retains_descriptor() {
    let (_fixture, config, receiver, current) = operation_fixture_with_stop_signal();
    let binding = actions::binding(&config).unwrap();
    let request = Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "stop-after-inherited-admission".into(),
        world_id: binding.world_id,
        chain_id: binding.chain_id,
        kind: Kind::Stop,
        config,
        expected_generation: current.generation,
        expected_config_digest: binding.config_digest,
        expected_instance: Some(current.clone()),
        managed_target: None,
        migration: None,
        authorization: None,
        package_upgrade: None,
    };
    let mut store = Store::open(&binding.data_dir.join("management")).unwrap();
    let record = store.admit(planner::plan(request).unwrap()).unwrap();
    let context = super::super::node_lifecycle::management::ManagementRequestBinding {
        api_version: "oasis7.local-management-request/v1".into(),
        operation_id: record.request.operation_id.clone(),
        step_id: "stop".into(),
        action_id: record.action_id("stop"),
        request_digest: record.request_digest.clone(),
        request_json: serde_json::to_string(&record.request).unwrap(),
    };
    super::super::node_lifecycle::management::bind(&record.request.config, &context).unwrap();
    let admission = actions::test_admission_lock(&binding.data_dir).unwrap();
    assert!(actions::test_admission_lock(&binding.data_dir).is_err());
    let inherited = InheritedDescriptors::fork();
    drop(admission);
    let result = actions::stop(&binding.data_dir, &record.action_id("stop"), &current, 1000);
    let stop_completed = result
        .as_ref()
        .is_ok_and(|record| matches!(record.target.state.as_str(), "stopped" | "failed"));
    let cleanup = receiver.finish(!stop_completed);
    let child_retained_descriptors =
        unsafe { libc::waitpid(inherited.pid, std::ptr::null_mut(), libc::WNOHANG) == 0 };
    drop(inherited);
    assert!(
        child_retained_descriptors,
        "child must retain descriptors through the native Stop attempt"
    );
    assert!(cleanup.is_ok(), "receiver cleanup failed");
    let result =
        result.expect("completed writer must release admission before another child execs");
    assert_eq!(result.target.state, "stopped");
    assert_eq!(result.target.incarnation, current.incarnation);
    let completed = executor::resume(
        &mut store,
        "stop-after-inherited-admission",
        record.revision,
    )
    .unwrap();
    assert_eq!(completed.state, "succeeded");
    assert_eq!(completed.results[0].action_id, result.action_id);
}
