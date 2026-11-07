#![cfg(unix)]

use serde_json::{Value, json};
use std::{
    cell::Cell,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, FileTypeExt, OpenOptionsExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const TOKEN: &str = "synthetic-dev-test-service-cli-credential-0001";
const WAIT: Duration = Duration::from_secs(10);

struct Process {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl Process {
    fn wait(mut self) -> Output {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return Output {
                    status,
                    stdout: fs::read(&self.stdout).unwrap(),
                    stderr: fs::read(&self.stderr).unwrap(),
                };
            }
            assert!(Instant::now() < deadline, "owned CLI exceeded its deadline");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

struct Fixture {
    root: PathBuf,
    next: Cell<usize>,
}

impl Fixture {
    fn new() -> Self {
        let mut id = [0; 12];
        getrandom::fill(&mut id).unwrap();
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("o7-service-cli-{}", hex::encode(id)));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join("usage"))
            .unwrap();
        let fixture = Self {
            root,
            next: Cell::new(0),
        };
        private_file(
            &fixture.config(),
            &serde_json::to_vec(&json!({
                "profile": "dev-test-accounts-v1",
                "socket": fixture.socket(),
                "usage_directory": fixture.root.join("usage"),
                "grant_id": "cli-contract",
                "max_request_bytes": 65536,
                "timeout_ms": 1000,
                "max_calls": 2,
                "credentials": [{
                    "caller_id": "cli-caller",
                    "token": TOKEN,
                    "expires_at_unix": u64::MAX,
                    "revoked": false
                }]
            }))
            .unwrap(),
        );
        fixture
    }

    fn config(&self) -> PathBuf {
        self.root.join("service.json")
    }

    fn socket(&self) -> PathBuf {
        self.root.join("service.sock")
    }

    fn spawn(&self, args: &[&str], input: &[u8]) -> Process {
        let id = self.next.get();
        self.next.set(id + 1);
        let stdin = self.root.join(format!("{id}.stdin"));
        let stdout = self.root.join(format!("{id}.stdout"));
        let stderr = self.root.join(format!("{id}.stderr"));
        private_file(&stdin, input);
        // Files avoid blocking the parent on a full stdin/stdout pipe when a
        // rejection or a broken child stops consuming input. The deadline owns
        // and reaps the actual CLI even when an assertion unwinds the test.
        Process {
            child: Command::new(env!("CARGO_BIN_EXE_oasis7_game_client"))
                .args(args)
                .stdin(Stdio::from(File::open(stdin).unwrap()))
                .stdout(Stdio::from(File::create(&stdout).unwrap()))
                .stderr(Stdio::from(File::create(&stderr).unwrap()))
                .spawn()
                .unwrap(),
            stdout,
            stderr,
        }
    }

    fn invoke(&self, args: &[&str], input: &[u8]) -> Output {
        let output = self.spawn(args, input).wait();
        for bytes in [&output.stdout, &output.stderr] {
            assert!(!String::from_utf8_lossy(bytes).contains(TOKEN));
        }
        output
    }

    fn serve(&self) -> Process {
        let mut process = self.spawn(
            &[
                "dev-test-serve",
                "--profile",
                self.config().to_str().unwrap(),
            ],
            b"",
        );
        let deadline = Instant::now() + WAIT;
        loop {
            assert!(
                process.child.try_wait().unwrap().is_none(),
                "service exited before readiness"
            );
            if fs::symlink_metadata(self.socket())
                .is_ok_and(|m| m.file_type().is_socket() && m.permissions().mode() & 0o777 == 0o600)
            {
                return process;
            }
            assert!(Instant::now() < deadline, "service did not become ready");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn call(&self, input: &[u8]) -> Output {
        self.invoke(
            &["dev-test-call", "--socket", self.socket().to_str().unwrap()],
            input,
        )
    }

    fn used(&self) -> u64 {
        let ledgers: Vec<_> = fs::read_dir(self.root.join("usage"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect();
        assert_eq!(ledgers.len(), 1);
        serde_json::from_slice::<Value>(&fs::read(&ledgers[0]).unwrap()).unwrap()["used"]
            .as_u64()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn private_file(path: &Path, bytes: &[u8]) {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

fn request(method: &str, credential: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "profile": "dev-test-accounts-v1",
        "audience": "oasis7-dev-test-tools",
        "credential": credential,
        "method": method
    }))
    .unwrap()
}

fn response(output: Output, status: &str) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["profile"], "dev-test-accounts-v1");
    assert_eq!(value["status"], status);
    assert_eq!(value["accounts_available"], false);
    assert_eq!(value["writes_available"], false);
    if status != "ok" {
        assert_eq!(value["caller_id"], Value::Null);
        assert_eq!(value["methods"], json!([]));
    }
    value
}

fn rejection(output: Output, error: &str) {
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), error);
}

#[test]
fn real_service_cli_preserves_credentials_closed_methods_and_usage() {
    let fixture = Fixture::new();
    let service = fixture.serve();
    let accepted = response(fixture.call(&request("capabilities", TOKEN)), "ok");
    assert_eq!(accepted["caller_id"], "cli-caller");
    assert_eq!(accepted["methods"], json!(["capabilities"]));
    assert_eq!(fixture.used(), 1);
    response(
        fixture.call(&request("capabilities", "wrong-credential")),
        "unauthorized",
    );
    response(fixture.call(&request("write", TOKEN)), "method_unavailable");
    assert_eq!(fixture.used(), 1);
    response(fixture.call(&request("capabilities", TOKEN)), "ok");
    response(
        fixture.call(&request("capabilities", TOKEN)),
        "call_limit_exhausted",
    );
    assert_eq!(fixture.used(), 2);
    drop(service);
    rejection(
        fixture.call(&request("capabilities", TOKEN)),
        "service_unavailable",
    );
}

#[test]
fn cli_rejects_public_symlink_and_invalid_configurations() {
    let fixture = Fixture::new();
    let config = fixture.config();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o644)).unwrap();
    rejection(
        fixture.invoke(
            &["dev-test-serve", "--profile", config.to_str().unwrap()],
            b"",
        ),
        "private_configuration_required",
    );
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let link = fixture.root.join("config-link");
    symlink(&config, &link).unwrap();
    for path in [&link, &fixture.root] {
        rejection(
            fixture.invoke(
                &["dev-test-serve", "--profile", path.to_str().unwrap()],
                b"",
            ),
            "private_configuration_required",
        );
    }
    fs::write(&config, b"invalid json").unwrap();
    rejection(
        fixture.invoke(
            &["dev-test-serve", "--profile", config.to_str().unwrap()],
            b"",
        ),
        "invalid_service_configuration",
    );
    fs::remove_file(&config).unwrap();
    rejection(
        fixture.invoke(
            &["dev-test-serve", "--profile", config.to_str().unwrap()],
            b"",
        ),
        "configuration_unavailable",
    );
    assert!(!fixture.socket().exists());
}

#[test]
fn cli_enforces_exact_input_boundaries_and_argument_errors() {
    let fixture = Fixture::new();
    let _service = fixture.serve();
    let mut boundary = request("capabilities", TOKEN);
    boundary.resize(65536, b' ');
    response(fixture.call(&boundary), "ok");
    boundary.push(b' ');
    rejection(fixture.call(&boundary), "request_too_large");
    for body in [b"".as_slice(), b"{", b"{\"unexpected\":true}"] {
        rejection(fixture.call(body), "invalid_request");
    }
    rejection(
        fixture.invoke(&["dev-test-call", "--profile", "unused"], b""),
        "unsupported_service_command",
    );
    rejection(
        fixture.invoke(&["dev-test-delete", "--socket", "unused"], b""),
        "unsupported_service_command",
    );
    rejection(
        fixture.invoke(&["dev-test-call"], b""),
        "dev-test-serve --profile <private-config> | dev-test-call --socket <approved-socket>",
    );
    assert_eq!(fixture.used(), 1);
}
