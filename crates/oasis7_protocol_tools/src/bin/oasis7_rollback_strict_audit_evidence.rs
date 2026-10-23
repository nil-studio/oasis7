use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use oasis7_client_api::rollback_audit_evidence::{
    RollbackStrictAuditEvidenceInput, build_unsigned_strict_audit_evidence,
};

#[derive(Debug)]
struct Options {
    audit_report: PathBuf,
    manifest: PathBuf,
    output: Option<PathBuf>,
    canonical_payload_output: Option<PathBuf>,
    authority_id: String,
    rollback_ticket: String,
    receipt_id: String,
    canonical_intent_digest: String,
    recovery_snapshot_hash: String,
    candidate_state_root: String,
    reorg_epoch: u64,
    issued_at_ms: u64,
    expires_at_ms: u64,
    nonce: String,
    private_key_file: Option<PathBuf>,
    signature_file: Option<PathBuf>,
    signer_public_key_file: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run(std::env::args().skip(1)) {
        eprintln!("error: {error}");
        std::process::exit(2);
    }
}

fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let options = parse_options(args)?;
    let report = std::fs::read(&options.audit_report)
        .map_err(|error| format!("read audit report failed: {error}"))?;
    let manifest = std::fs::read(&options.manifest)
        .map_err(|error| format!("read manifest failed: {error}"))?;
    verify_audited_manifest_binding(&report, &manifest)?;
    let mut evidence = build_unsigned_strict_audit_evidence(RollbackStrictAuditEvidenceInput {
        authority_id: options.authority_id,
        rollback_ticket: options.rollback_ticket,
        receipt_id: options.receipt_id,
        canonical_intent_digest: options.canonical_intent_digest,
        recovery_snapshot_hash: options.recovery_snapshot_hash,
        reorg_epoch: options.reorg_epoch,
        candidate_state_root: options.candidate_state_root,
        audit_report_bytes: report,
        manifest_bytes: manifest,
        issued_at_ms: options.issued_at_ms,
        expires_at_ms: options.expires_at_ms,
        nonce: options.nonce,
    });
    let payload = evidence
        .canonical_signing_payload()
        .map_err(|error| format!("construct canonical payload failed: {error}"))?;
    if let Some(path) = options.canonical_payload_output.as_deref() {
        write_private(path, &payload)?;
    }

    match (
        options.private_key_file.as_deref(),
        options.signature_file.as_deref(),
    ) {
        (Some(key_path), None) => {
            let signing_key = read_signing_key(key_path)?;
            evidence.signature_hex = hex::encode(signing_key.sign(&payload).to_bytes());
        }
        (None, Some(signature_path)) => {
            let public_path = options.signer_public_key_file.as_deref().ok_or_else(|| {
                "--signer-public-key-file is required with --signature-file".to_string()
            })?;
            let signature = read_signature(signature_path)?;
            let public_key = read_verifying_key(public_path)?;
            public_key
                .verify(&payload, &signature)
                .map_err(|_| "external signer signature does not verify".to_string())?;
            evidence.signature_hex = hex::encode(signature.to_bytes());
        }
        (None, None) if options.canonical_payload_output.is_some() && options.output.is_none() => {
            return Ok(());
        }
        (Some(_), Some(_)) => {
            return Err("choose exactly one of --private-key-file or --signature-file".to_string());
        }
        _ => {
            return Err(
                "signed output requires --private-key-file or --signature-file".to_string(),
            );
        }
    }
    let output = options
        .output
        .as_deref()
        .ok_or_else(|| "--output is required for signed evidence".to_string())?;
    let encoded = serde_json::to_vec_pretty(&evidence)
        .map_err(|error| format!("encode evidence failed: {error}"))?;
    write_private(output, &encoded)
}

fn verify_audited_manifest_binding(report: &[u8], manifest: &[u8]) -> Result<(), String> {
    let report: serde_json::Value = serde_json::from_slice(report)
        .map_err(|_| "audit report must be valid JSON".to_string())?;
    let digest = report
        .get("audited_manifest_digest")
        .and_then(serde_json::Value::as_str)
        .filter(|digest| digest.len() == 64 && hex::decode(digest).is_ok())
        .ok_or_else(|| "audit report manifest digest is missing or malformed".to_string())?;
    if oasis7_client_api::rollback_audit_evidence::strict_audit_manifest_digest(manifest) != digest
    {
        return Err("audit report does not bind the supplied manifest".to_string());
    }
    Ok(())
}

fn parse_options(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut values = BTreeMap::new();
    let mut args = args.peekable();
    while let Some(flag) = args.next() {
        if !flag.starts_with("--") {
            return Err(format!("unexpected argument {flag}"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        if values.insert(flag.clone(), value).is_some() {
            return Err(format!("duplicate option {flag}"));
        }
    }
    let required = |flag: &str| {
        values
            .get(flag)
            .cloned()
            .ok_or_else(|| format!("missing required option {flag}"))
    };
    let parse_u64 = |flag: &str| -> Result<u64, String> {
        required(flag)?
            .parse()
            .map_err(|_| format!("{flag} must be an unsigned integer"))
    };
    Ok(Options {
        audit_report: required("--audit-report")?.into(),
        manifest: required("--manifest")?.into(),
        output: values.get("--output").map(PathBuf::from),
        canonical_payload_output: values.get("--canonical-payload-output").map(PathBuf::from),
        authority_id: required("--authority-id")?,
        rollback_ticket: required("--rollback-ticket")?,
        receipt_id: required("--receipt-id")?,
        canonical_intent_digest: required("--canonical-intent-digest")?,
        recovery_snapshot_hash: required("--recovery-snapshot-hash")?,
        candidate_state_root: required("--candidate-state-root")?,
        reorg_epoch: parse_u64("--reorg-epoch")?,
        issued_at_ms: parse_u64("--issued-at-ms")?,
        expires_at_ms: parse_u64("--expires-at-ms")?,
        nonce: required("--nonce")?,
        private_key_file: values.get("--private-key-file").map(PathBuf::from),
        signature_file: values.get("--signature-file").map(PathBuf::from),
        signer_public_key_file: values.get("--signer-public-key-file").map(PathBuf::from),
    })
}

fn read_hex<const N: usize>(path: &Path, label: &str) -> Result<[u8; N], String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("read {label} failed: {error}"))?;
    hex::decode(text.trim())
        .map_err(|_| format!("{label} must be hex"))?
        .try_into()
        .map_err(|_| format!("{label} must contain exactly {N} bytes"))
}

fn read_signing_key(path: &Path) -> Result<SigningKey, String> {
    Ok(SigningKey::from_bytes(&read_hex(path, "private key")?))
}

fn read_signature(path: &Path) -> Result<Signature, String> {
    Ok(Signature::from_bytes(&read_hex(path, "signature")?))
}

fn read_verifying_key(path: &Path) -> Result<VerifyingKey, String> {
    VerifyingKey::from_bytes(&read_hex(path, "public key")?)
        .map_err(|_| "public key is invalid".to_string())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_private_for_platform(path, bytes, cfg!(unix))
}

fn write_private_for_platform(
    path: &Path,
    bytes: &[u8],
    private_creation_supported: bool,
) -> Result<(), String> {
    if !private_creation_supported {
        return Err(
            "private output creation is unsupported on this platform; no artifact was published"
                .to_string(),
        );
    }

    #[cfg(not(unix))]
    {
        let _ = (path, bytes);
        return Err(
            "private output creation is unsupported on this platform; no artifact was published"
                .to_string(),
        );
    }

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .ok_or_else(|| "output path must name a file".to_string())?;
        let temp_name = format!(
            ".{}.{}.{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        );
        let temp = parent.join(temp_name);
        let result = (|| {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
            let mut file = options
                .open(&temp)
                .map_err(|error| format!("create private output failed: {error}"))?;
            file.write_all(bytes)
                .map_err(|error| format!("write private output failed: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("sync private output failed: {error}"))?;
            std::fs::hard_link(&temp, path)
                .map_err(|error| format!("publish private output failed: {error}"))?;
            Ok(())
        })();
        let _ = std::fs::remove_file(&temp);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fn passing_report(manifest: &[u8]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "overall_status": "ready_for_ops_drill",
            "overall_single_failure_tolerance_pass": true,
            "manifest_match_pass": true,
            "rollback_blockers": [],
            "audited_manifest_digest": oasis7_client_api::rollback_audit_evidence::strict_audit_manifest_digest(manifest)
        }))
        .expect("audit report")
    }

    #[test]
    fn local_signer_preserves_exact_artifacts_without_emitting_private_key() {
        let root = std::env::temp_dir().join(format!(
            "oasis7-strict-audit-evidence-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let report = root.join("audit.json");
        let manifest = root.join("manifest.json");
        let key = root.join("governance.key");
        let output = root.join("evidence.json");
        let manifest_bytes = br#"{"entries":[{"slot_id":"governance.rollback.v1"}]}"#;
        let report_bytes = passing_report(manifest_bytes);
        let signing_key = SigningKey::from_bytes(&[73; 32]);
        std::fs::write(&report, &report_bytes).expect("write report");
        std::fs::write(&manifest, manifest_bytes).expect("write manifest");
        std::fs::write(&key, hex::encode(signing_key.to_bytes())).expect("write key");
        run([
            "--audit-report",
            report.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--authority-id",
            "governance-bob",
            "--rollback-ticket",
            "ROLLBACK-2313",
            "--receipt-id",
            "receipt-1",
            "--canonical-intent-digest",
            "intent-1",
            "--recovery-snapshot-hash",
            "snapshot-1",
            "--candidate-state-root",
            "root-1",
            "--reorg-epoch",
            "4",
            "--issued-at-ms",
            "100",
            "--expires-at-ms",
            "200",
            "--nonce",
            "audit-1",
            "--private-key-file",
            key.to_str().unwrap(),
        ]
        .into_iter()
        .map(str::to_string))
        .expect("construct evidence");
        let encoded = std::fs::read(&output).expect("read evidence");
        let evidence: oasis7_proto::viewer::RollbackStrictAuditEvidence =
            serde_json::from_slice(&encoded).expect("decode evidence");
        assert_eq!(evidence.audit_report_bytes, report_bytes);
        assert_eq!(evidence.manifest_bytes, manifest_bytes);
        assert!(!String::from_utf8_lossy(&encoded).contains(&hex::encode(signing_key.to_bytes())));
        signing_key
            .verifying_key()
            .verify(
                &evidence.canonical_signing_payload().expect("payload"),
                &Signature::from_slice(&hex::decode(&evidence.signature_hex).unwrap()).unwrap(),
            )
            .expect("valid signature");
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn outputs_are_private_non_destructive_and_symlink_safe() {
        let root = std::env::temp_dir().join(format!(
            "oasis7-strict-audit-safe-output-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let output = root.join("payload.bin");
        write_private(&output, b"payload").expect("first publication");
        assert_eq!(std::fs::metadata(&output).unwrap().mode() & 0o777, 0o600);
        assert!(write_private(&output, b"replacement").is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"payload");

        let target = root.join("target");
        std::fs::write(&target, b"preserve").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        let link = root.join("output-link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(write_private(&link, b"overwrite").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"preserve");
        assert_eq!(std::fs::metadata(&target).unwrap().mode() & 0o777, 0o640);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unsupported_private_output_platform_fails_closed_without_publication() {
        let root = std::env::temp_dir().join(format!(
            "oasis7-strict-audit-unsupported-output-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp dir");
        let output = root.join("evidence.json");
        let error = write_private_for_platform(&output, b"sensitive", false)
            .expect_err("unsupported private creation must fail closed");
        assert!(error.contains("unsupported"));
        assert!(
            !output.exists(),
            "unsupported host must publish no artifact"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn external_hsm_payload_and_assemble_round_trip() {
        let root =
            std::env::temp_dir().join(format!("oasis7-strict-audit-hsm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let report = root.join("audit.json");
        let manifest = root.join("manifest.json");
        let payload = root.join("payload.bin");
        let signature = root.join("signature.hex");
        let public_key = root.join("public-key.hex");
        let output = root.join("evidence.json");
        let manifest_bytes = br#"{"entries":[]}"#;
        std::fs::write(&manifest, manifest_bytes).unwrap();
        std::fs::write(&report, passing_report(manifest_bytes)).unwrap();
        let common = vec![
            "--audit-report",
            report.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
            "--authority-id",
            "governance-bob",
            "--rollback-ticket",
            "ROLLBACK-HSM",
            "--receipt-id",
            "receipt-hsm",
            "--canonical-intent-digest",
            "intent-hsm",
            "--recovery-snapshot-hash",
            "snapshot-hsm",
            "--candidate-state-root",
            "root-hsm",
            "--reorg-epoch",
            "7",
            "--issued-at-ms",
            "100",
            "--expires-at-ms",
            "200",
            "--nonce",
            "audit-hsm",
        ];
        let mut prepare = common.clone();
        prepare.extend(["--canonical-payload-output", payload.to_str().unwrap()]);
        run(prepare.into_iter().map(str::to_string)).expect("prepare payload");
        let signing_key = SigningKey::from_bytes(&[91; 32]);
        std::fs::write(
            &signature,
            hex::encode(
                signing_key
                    .sign(&std::fs::read(&payload).unwrap())
                    .to_bytes(),
            ),
        )
        .unwrap();
        std::fs::write(
            &public_key,
            hex::encode(signing_key.verifying_key().to_bytes()),
        )
        .unwrap();
        let mut assemble = common;
        assemble.extend([
            "--signature-file",
            signature.to_str().unwrap(),
            "--signer-public-key-file",
            public_key.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ]);
        run(assemble.into_iter().map(str::to_string)).expect("assemble evidence");
        let evidence: oasis7_proto::viewer::RollbackStrictAuditEvidence =
            serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        signing_key
            .verifying_key()
            .verify(
                &evidence.canonical_signing_payload().unwrap(),
                &Signature::from_slice(&hex::decode(evidence.signature_hex).unwrap()).unwrap(),
            )
            .expect("assembled signature verifies");
        let _ = std::fs::remove_dir_all(root);
    }
}
