//! Private process fixture: real governance, node execution, HTTP and signed canonical receipts.
use super::*;
use std::{io::Write,path::PathBuf};
struct FixtureDirectory(PathBuf);
impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "started explicitly by the custody workerd integration harness"]
fn serve_custody_world_http_fixture() {
    let ready = std::env::var_os("OASIS7_CUSTODY_WORLD_HTTP_READY").expect("isolated ready path");
    let stop_file = std::env::var_os("OASIS7_CUSTODY_WORLD_HTTP_STOP").expect("isolated stop path");
    let (dir, mut driver, identity, _, _) = canonical_agent_chat::driver_fixture_without_provider();
    let _directory = FixtureDirectory(dir.clone());
    // Match the real native test authority, not the unrelated historical HTTP fixture namespace.
    let ns = GameIdentityNamespaceV1 {
        service_id: "model-contract".into(),
        environment_id: "test".into(),
        instance_epoch: DecimalU64(4),
    };
    governance::install_for_identity(
        &mut driver.execution_world,
        &identity,
        1,
        true,
        ns.clone(),
        95,
    );
    empty_commit(&mut driver);
    let root = dir.canonicalize().unwrap();
    let pinned = pin(&root, &identity);
    let binding = pinned.world.current_cognition_runtime_binding().unwrap();
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        public_key_hex: sign_read_request("fixture", (), &hex::encode([9; 32]))
            .unwrap()
            .subject_public_key,
        private_key_hex: hex::encode([9; 32]),
    };
    let node = Arc::new(Mutex::new(runtime(&root, driver)));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = AtomicBool::new(false);
    // The existing transport's optional first-submit loss is disabled for this happy fixture.
    let dropped = AtomicBool::new(true);
    std::thread::scope(|scope| {
        let _guard = Stop {
            stop: &stop,
            node: node.clone(),
        };
        let server =
            scope.spawn(|| transport::serve(listener, &node, &root, &signer, &stop, &dropped));
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&ready)
            .unwrap();
        serde_json::to_writer(
            &mut output,
            &serde_json::json!({"endpoint":endpoint,"world":{
            "world_id":identity.world_id,"genesis_digest":identity.genesis_digest,
            "branch_id":binding.branch_id,"reorg_epoch":binding.reorg_epoch.to_string(),
            "service_public_key":signer.public_key_hex},"identity_namespace":ns}),
        )
        .unwrap();
        output.write_all(b"\n").unwrap();
        output.sync_all().unwrap();
        let deadline = Instant::now() + Duration::from_secs(180);
        while !std::path::Path::new(&stop_file).exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
    });
}
