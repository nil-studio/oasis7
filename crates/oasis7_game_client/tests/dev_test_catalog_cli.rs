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
    let root = std::env::temp_dir().join(format!("catalog-tls-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
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
    let script = r#"import http.server,ssl,sys,pathlib,json
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  assert self.path.endswith('/session')
  body=json.dumps({'api_version':2,'ok':True,'account':{'account_id':pathlib.Path(sys.argv[4]).read_text()}}).encode()
  self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
 def log_message(self,*args): pass
server=http.server.HTTPServer(('127.0.0.1',0),Handler)
ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(sys.argv[1],sys.argv[2]);server.socket=ctx.wrap_socket(server.socket,server_side=True)
pathlib.Path(sys.argv[3]).write_text(str(server.server_port));server.serve_forever()
"#;
    let mut server = Command::new("python3")
        .args([
            "-c",
            script,
            cert.to_str().unwrap(),
            key.to_str().unwrap(),
            ready.to_str().unwrap(),
            subject.to_str().unwrap(),
        ])
        .spawn()
        .unwrap();
    for _ in 0..100 {
        if ready.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let endpoint = format!(
        "https://127.0.0.1:{}",
        std::fs::read_to_string(&ready).unwrap()
    );
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
    server.kill().unwrap();
    server.wait().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
