#![cfg(unix)]
use std::{os::unix::fs::PermissionsExt, process::Command};
#[test]
fn operator_cli_rejects_public_directory_and_lists_without_credential_references() {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("catalog-cli-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_oasis7_game_client"))
            .args([
                "dev-test-catalog-list",
                "--directory",
                root.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    assert!(!invoke().status.success());
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let data = serde_json::json!([{"account_ref":"ref-a","account_id":"account-a","environment":"isolated","public_key":"public-key","alias":"A","credential_ref":"/private/secret-config","login_operation":"private-operation"}]);
    std::fs::write(root.join("catalog.json"), data.to_string()).unwrap();
    let result = invoke();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("account-a"));
    assert!(!output.contains("secret-config"));
    assert!(!output.contains("private-operation"));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn trusted_tls_registration_is_idempotent_and_rejects_subject_replacement() {
    use oasis7_client_storage::identity_store::IdentityStore;
    use std::{io::Write, process::Stdio, time::Duration};
    let directory = tls_fixture::Directory::new("catalog-tls");
    let root = &directory.0;
    let cert = root.join("cert.pem");
    let key = root.join("key.pem");
    assert!(
        Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=127.0.0.1",
                "-addext",
                "subjectAltName=IP:127.0.0.1",
                "-keyout",
                key.to_str().unwrap(),
                "-out",
                cert.to_str().unwrap()
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    let ca = root.join("ca.pem");
    let ca_key = root.join("ca-key.pem");
    std::fs::rename(&cert, &ca).unwrap();
    std::fs::rename(&key, &ca_key).unwrap();
    let csr = root.join("leaf.csr");
    let extensions = root.join("extensions.cnf");
    std::fs::write(&extensions, "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n").unwrap();
    assert!(
        Command::new("openssl")
            .args([
                "req",
                "-new",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-subj",
                "/CN=127.0.0.1",
                "-keyout",
                key.to_str().unwrap(),
                "-out",
                csr.to_str().unwrap()
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        Command::new("openssl")
            .args([
                "x509",
                "-req",
                "-in",
                csr.to_str().unwrap(),
                "-CA",
                ca.to_str().unwrap(),
                "-CAkey",
                ca_key.to_str().unwrap(),
                "-CAcreateserial",
                "-days",
                "1",
                "-extfile",
                extensions.to_str().unwrap(),
                "-out",
                cert.to_str().unwrap()
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
    let ready = root.join("ready");
    let subject = root.join("subject");
    std::fs::write(&subject, "account-a").unwrap();
    let mut child = Command::new("python3")
        .args([
            "-c",
            tls_fixture::SCRIPT,
            cert.to_str().unwrap(),
            key.to_str().unwrap(),
            ready.to_str().unwrap(),
            subject.to_str().unwrap(),
        ])
        .spawn()
        .unwrap();
    let mut server = tls_fixture::Server(&mut child);
    let port = server
        .wait_for_port(&ready, Duration::from_secs(2))
        .unwrap();
    let endpoint = format!("https://127.0.0.1:{port}");
    let vault = root.join("vault");
    let password = b"synthetic catalog fixture unlock";
    let store = IdentityStore::create(&vault, password).unwrap();
    let scope = blake3::hash(
        serde_json::to_string(&(&endpoint, "identity:fixture", "isolated", &endpoint))
            .unwrap()
            .as_bytes(),
    )
    .to_hex()
    .to_string();
    let _: serde_json::Value = store
        .put_original(
            &scope,
            "login-op",
            "login-session",
            &serde_json::json!({"account_id":"account-a","cookie":"fixture-private-cookie"}),
        )
        .unwrap();
    let config = root.join("identity.json");
    std::fs::write(&config,serde_json::json!({"endpoint":endpoint,"identity_service_id":"identity:fixture","environment_id":"isolated","audience":endpoint,"state_directory":vault}).to_string()).unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
    let invoke = |config: &std::path::Path, operation: &str, alias: &str, unlock: &[u8]| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_oasis7_game_client"))
            .args([
                "dev-test-catalog-register",
                "--directory",
                root.to_str().unwrap(),
                "--identity-config",
                config.to_str().unwrap(),
                "--operation",
                operation,
                "--alias",
                alias,
            ])
            .env("SSL_CERT_FILE", &ca)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(unlock).unwrap();
        child.wait_with_output().unwrap()
    };
    let first = invoke(&config, "login-op", "A", password);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let again = invoke(&config, "login-op", "Renamed", password);
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert_eq!(first.stdout, again.stdout);
    assert!(
        !invoke(&config, "login-op", "Wrong", b"wrong synthetic unlock")
            .status
            .success()
    );
    let _: serde_json::Value = store
        .put_original(
            &scope,
            "fresh-login",
            "login-session",
            &serde_json::json!({"account_id":"account-a","cookie":"fixture-fresh-cookie"}),
        )
        .unwrap();
    let renewed = invoke(&config, "fresh-login", "Renamed", password);
    assert!(
        renewed.status.success(),
        "{}",
        String::from_utf8_lossy(&renewed.stderr)
    );
    assert_eq!(first.stdout, renewed.stdout);
    std::fs::write(&subject, "account-b").unwrap();
    assert!(
        !invoke(&config, "login-op", "Replacement", password)
            .status
            .success()
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("catalog.json")).unwrap()).unwrap();
    assert_eq!(catalog.as_array().unwrap().len(), 1);
    assert_eq!(catalog[0]["account_id"], "account-a");
    assert_eq!(catalog[0]["alias"], "Renamed");
    assert_eq!(catalog[0]["login_operation"], "fresh-login");
    std::fs::write(&subject, "account-a").unwrap();
    let alternate_scope = blake3::hash(
        serde_json::to_string(&(&endpoint, "identity", "fixture:isolated", &endpoint))
            .unwrap()
            .as_bytes(),
    )
    .to_hex()
    .to_string();
    let _: serde_json::Value = store
        .put_original(
            &alternate_scope,
            "login-op",
            "login-session",
            &serde_json::json!({"account_id":"account-a","cookie":"fixture-private-cookie"}),
        )
        .unwrap();
    let alternate = root.join("alternate.json");
    std::fs::write(&alternate,serde_json::json!({"endpoint":endpoint,"identity_service_id":"identity","environment_id":"fixture:isolated","audience":endpoint,"state_directory":vault}).to_string()).unwrap();
    std::fs::set_permissions(&alternate, std::fs::Permissions::from_mode(0o600)).unwrap();
    let distinct = invoke(&alternate, "login-op", "Other environment", password);
    assert!(
        distinct.status.success(),
        "{}",
        String::from_utf8_lossy(&distinct.stderr)
    );
    assert_ne!(first.stdout, distinct.stdout);
    let catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("catalog.json")).unwrap()).unwrap();
    assert_eq!(catalog.as_array().unwrap().len(), 2);
    server.stop().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "linux")]
mod tls_fixture {
    use std::{
        fs, io,
        os::unix::fs::DirBuilderExt,
        path::{Path, PathBuf},
        process::{Child, Command},
        time::{Duration, Instant},
    };

    pub(super) const SCRIPT: &str = r#"import http.server,ssl,sys,pathlib,json
def publish_port(ready, port):
 ready=pathlib.Path(ready)
 pending=ready.with_name(ready.name+'.pending')
 pending.write_text(str(port))
 pending.replace(ready)
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  assert self.path.endswith('/session')
  body=json.dumps({'api_version':2,'ok':True,'account':{'account_id':pathlib.Path(sys.argv[4]).read_text()}}).encode()
  self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
 def log_message(self,*args): pass
if __name__ == '__main__':
 server=http.server.HTTPServer(('127.0.0.1',0),Handler)
 ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(sys.argv[1],sys.argv[2]);server.socket=ctx.wrap_socket(server.socket,server_side=True)
 publish_port(sys.argv[3],server.server_port)
 server.serve_forever()
"#;

    pub(super) struct Directory(pub(super) PathBuf);

    impl Directory {
        pub(super) fn new(prefix: &str) -> Self {
            let mut id = [0; 12];
            getrandom::fill(&mut id).unwrap();
            let root = std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("{prefix}-{}", hex::encode(id)));
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
            Self(root)
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    pub(super) struct Server<'a>(pub(super) &'a mut Child);

    impl Server<'_> {
        pub(super) fn wait_for_port(&mut self, ready: &Path, timeout: Duration) -> io::Result<u16> {
            let deadline = Instant::now() + timeout;
            loop {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "TLS fixture did not publish a port before its deadline",
                    ));
                }
                if let Some(status) = self.0.try_wait()? {
                    return Err(io::Error::other(format!(
                        "TLS fixture exited before readiness: {status}"
                    )));
                }
                match fs::read_to_string(ready) {
                    Ok(value) => {
                        return value
                            .parse::<u16>()
                            .ok()
                            .filter(|port| *port != 0)
                            .ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "TLS fixture published an invalid port",
                                )
                            });
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
                std::thread::sleep(
                    Duration::from_millis(20)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
        }

        pub(super) fn stop(&mut self) -> io::Result<()> {
            if self.0.try_wait()?.is_none() {
                self.0.kill()?;
                self.0.wait()?;
            }
            Ok(())
        }
    }

    impl Drop for Server<'_> {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }

    fn sleeping_child() -> Child {
        Command::new("python3")
            .args(["-c", "import time; time.sleep(60)"])
            .spawn()
            .unwrap()
    }

    #[test]
    fn port_is_published_only_after_the_complete_write() {
        let directory = Directory::new("catalog-port-publication");
        let ready = directory.0.join("ready");
        let check = r#"import pathlib,sys,unittest.mock
fixture={'__name__':'catalog_tls_fixture'}
exec(sys.argv[1],fixture)
ready=pathlib.Path(sys.argv[2])
def partial_write(path,text,*args,**kwargs):
 with path.open('w') as output:
  assert not ready.exists(), 'ready was visible before any bytes were written'
  output.write(text[:1]);output.flush()
  assert not ready.exists(), 'ready was visible with only part of the port'
  output.write(text[1:]);output.flush()
  assert not ready.exists(), 'ready was visible before the file was closed'
 return len(text)
with unittest.mock.patch.object(pathlib.Path,'write_text',partial_write):
 fixture['publish_port'](ready,45678)
assert ready.read_text() == '45678'
"#;
        let result = Command::new("python3")
            .args(["-c", check, SCRIPT, ready.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    fn readiness_rejects_empty_malformed_zero_and_out_of_range_ports() {
        let directory = Directory::new("catalog-invalid-port");
        let ready = directory.0.join("ready");
        let mut child = sleeping_child();
        let mut server = Server(&mut child);
        for invalid in ["", "12x", "0", "65536"] {
            fs::write(&ready, invalid).unwrap();
            assert_eq!(
                server
                    .wait_for_port(&ready, Duration::from_secs(2))
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData,
                "{invalid:?}"
            );
        }
        fs::write(&ready, "45678").unwrap();
        assert_eq!(
            server
                .wait_for_port(&ready, Duration::from_secs(2))
                .unwrap(),
            45678
        );
        server.stop().unwrap();
    }

    #[test]
    fn readiness_reports_early_exit_and_has_a_deadline() {
        let directory = Directory::new("catalog-readiness");
        let ready = directory.0.join("ready");
        let mut exited = Command::new("python3")
            .args(["-c", "raise SystemExit(23)"])
            .spawn()
            .unwrap();
        let mut server = Server(&mut exited);
        let error = server
            .wait_for_port(&ready, Duration::from_secs(2))
            .unwrap_err();
        assert!(error.to_string().contains("exited before readiness"));
        assert!(error.to_string().contains("23"));
        drop(server);

        let mut child = sleeping_child();
        let mut server = Server(&mut child);
        assert_eq!(
            server
                .wait_for_port(&ready, Duration::from_millis(50))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        server.stop().unwrap();
    }

    #[test]
    fn guards_reap_the_child_and_remove_private_files_on_return_and_panic() {
        for should_panic in [false, true] {
            let directory = Directory::new("catalog-cleanup");
            let fixture = directory.0.clone();
            let private = fixture.join("private");
            fs::create_dir(&private).unwrap();
            fs::write(private.join("fixture.key"), b"synthetic private fixture").unwrap();
            let mut child = sleeping_child();
            let process = PathBuf::from(format!("/proc/{}", child.id()));
            let observed_child = process.exists();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _directory = directory;
                let _server = Server(&mut child);
                if should_panic {
                    panic!("intentional fixture failure after spawn");
                }
            }));
            let child_remained = process.exists();
            let directory_remained = fixture.exists();
            // Clean up even when a regression makes one of the observations fail.
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_dir_all(fixture);
            assert!(
                observed_child,
                "the process observer must see the owned child"
            );
            assert_eq!(result.is_err(), should_panic);
            assert!(
                !child_remained,
                "the owned child was not terminated and reaped"
            );
            assert!(
                !directory_remained,
                "private fixture files were left behind"
            );
        }
    }
}
