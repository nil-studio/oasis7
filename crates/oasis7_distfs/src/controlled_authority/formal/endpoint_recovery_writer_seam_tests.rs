//! Executable controlled-file writer custody. This is a software isolation
//! fixture; it does not attest physical key custody or independent fault domains.
use super::*;
use crate::controlled_authority::formal::issuer::IssuerAnchorV1;
use std::io::{BufRead, Read};

pub(in super::super) struct WriterProcess {
    child: EndpointChild,
    binary: std::ffi::OsString,
    file: PathBuf,
    config: serde_json::Value,
    authority: VerifiedAuthorityV1,
    pub(in super::super) normal: RemoteWriterCustodyClientV1,
    pub(in super::super) recovery: RemoteWriterCustodyClientV1,
}

fn start_writer(binary: &std::ffi::OsStr, config: &Path) -> (EndpointChild, std::net::SocketAddr) {
    let mut child = EndpointChild(
        Command::new(binary)
            .arg("--writer-custody-config")
            .arg(config)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        let result = std::io::BufReader::new(stdout)
            .take(1024)
            .read_line(&mut line)
            .map(|bytes| (bytes, line));
        let _ = tx.send(result);
    });
    let result = rx.recv_timeout(Duration::from_secs(10));
    let (bytes, line) = match result {
        Ok(Ok(value)) => value,
        error => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            panic!("writer custody readiness failed: {error:?}");
        }
    };
    reader.join().unwrap();
    assert!(bytes > 0 && bytes < 1024, "writer readiness bounded line");
    let address = line
        .trim()
        .strip_prefix("writer_custody_listening=")
        .unwrap_or_else(|| panic!("writer custody did not announce readiness: {line:?}"))
        .parse()
        .unwrap();
    (child, address)
}

impl WriterProcess {
    pub(in super::super) fn start(
        fixture: &Fixture,
        binary: &std::ffi::OsStr,
        authority: &VerifiedAuthorityV1,
        writer_key: u8,
        permission: Option<(RecoveryPermissionBootstrapV1, IssuerAnchorV1)>,
    ) -> Self {
        let epoch = authority.definition().epoch;
        let key_file = fixture.path.join(format!("writer-custody-key-{epoch}"));
        let mut key_bytes = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&key_file)
            .unwrap();
        key_bytes
            .write_all(hex::encode([writer_key; 32]).as_bytes())
            .unwrap();
        key_bytes.sync_all().unwrap();
        let initial_body = fixture.authority.initial_body().unwrap().clone();
        let initial = SignedInitialActivationV1 {
            signature_hex: signed(initial_activation_signing_bytes(&initial_body).unwrap(), 9),
            body: initial_body,
        };
        let trust = InitialActivationTrustV1 {
            issuer_public_key: public(9),
            world: initial.body.authority.world.clone(),
            initial_state_root: initial.body.initial_state_root.clone(),
            governing_manifest_digest: initial.body.governing_manifest_digest.clone(),
            recovery_policy_digest: fixture.authority.policy().digest().unwrap(),
        };
        let (permission, minimum) = permission.unzip();
        let config = serde_json::json!({
            "schema_version": 1,
            "storage_mode": "initialize",
            "initial_trust": trust,
            "initial_certificate": initial,
            "recovery_permission": permission,
            "minimum_issuer_anchor": minimum,
            "data_dir": fixture.path.join(format!("writer-custody-{epoch}")),
            "writer_key_file": key_file,
            "normal_credential_public_key": public(27),
            "recovery_credential_public_key": public(28),
            "minimum_revision": 0,
            "listen": "127.0.0.1:0",
            "timeout_ms": 3000,
        });
        let file = fixture.path.join(format!("writer-custody-{epoch}.json"));
        fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
        let (child, address) = start_writer(binary, &file);
        let client = |credential| {
            RemoteWriterCustodyClientV1::new(
                address,
                Duration::from_secs(3),
                authority.authority_id().unwrap(),
                authority.definition().writer_public_key.clone(),
                key(credential),
                0,
            )
            .unwrap()
        };
        Self {
            child,
            binary: binary.to_owned(),
            file,
            config,
            authority: authority.clone(),
            normal: client(27),
            recovery: client(28),
        }
    }

    pub(in super::super) fn cold_reopen(&mut self, minimum_revision: u64) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
        self.config["storage_mode"] = serde_json::json!("open_existing");
        self.config["minimum_revision"] = serde_json::json!(minimum_revision);
        fs::write(&self.file, serde_json::to_vec(&self.config).unwrap()).unwrap();
        let (child, address) = start_writer(&self.binary, &self.file);
        self.child = child;
        let client = |credential| {
            RemoteWriterCustodyClientV1::new(
                address,
                Duration::from_secs(3),
                self.authority.authority_id().unwrap(),
                self.authority.definition().writer_public_key.clone(),
                key(credential),
                minimum_revision,
            )
            .unwrap()
        };
        self.normal = client(27);
        self.recovery = client(28);
    }

    pub(in super::super) fn persist_plan(
        &mut self,
        plan: &SignedRecoveryPlanV1,
        fixed: &RecoveryTailWitnessV1,
        fixed_material: Option<&ClosedRecord>,
        pending: &RecoveryPendingHandoffV1,
    ) -> SignedWriterRecoveryPlanAckV1 {
        let ack = self
            .recovery
            .prepare_recovery_plan(plan, fixed, fixed_material, pending, &self.authority)
            .unwrap();
        self.cold_reopen(ack.body.revision);
        assert_eq!(
            self.recovery.query().unwrap().plan_digest.as_ref(),
            Some(&plan.digest().unwrap())
        );
        assert_eq!(
            self.recovery
                .prepare_recovery_plan(plan, fixed, fixed_material, pending, &self.authority)
                .unwrap(),
            ack
        );
        ack
    }

    pub(in super::super) fn persist_seal(
        &mut self,
        plan: &SignedRecoveryPlanV1,
        drained: &RecoveryDrainedProofV1,
    ) -> SignedWriterEpochSealV1 {
        assert_eq!(
            self.recovery
                .complete_drain(plan, drained, &self.authority)
                .unwrap(),
            *drained
        );
        let snapshot = self.recovery.query().unwrap();
        self.cold_reopen(snapshot.revision);
        assert_eq!(
            self.recovery.query().unwrap().drained_digest.as_ref(),
            Some(&drained.digest().unwrap())
        );
        assert_eq!(
            self.recovery
                .complete_drain(plan, drained, &self.authority)
                .unwrap(),
            *drained
        );
        let seal = self
            .recovery
            .seal_epoch(plan, drained, &self.authority)
            .unwrap();
        self.cold_reopen(seal.body.revision);
        assert_eq!(
            self.recovery.query().unwrap().seal_digest.as_ref(),
            Some(&seal.digest().unwrap())
        );
        assert_eq!(
            self.recovery
                .seal_epoch(plan, drained, &self.authority)
                .unwrap(),
            seal
        );
        seal
    }

    pub(in super::super) fn plan_authorization(&self) -> RecoveryPlanAuthorityV1 {
        let snapshot = self.normal.query().unwrap();
        assert_eq!(
            snapshot.writer_public_key,
            self.authority.definition().writer_public_key
        );
        RecoveryPlanAuthorityV1 {
            recovery_credential_public_key: public(28),
            isolation_evidence_digest: hash(
                "oasis7.controlled_software_writer_custody_isolation.v1",
                &(
                    self.authority.authority_id().unwrap(),
                    snapshot,
                    public(27),
                    public(28),
                ),
            )
            .unwrap(),
        }
    }

    pub(in super::super) fn normal_endpoint(
        &self,
        address: std::net::SocketAddr,
        role: EndpointRole,
        minimum: FormalHeadRefV1,
    ) -> RemoteFormalEndpoint {
        RemoteFormalEndpoint::new(
            address,
            Duration::from_secs(3),
            self.authority.clone(),
            role,
            WriterSigningCapabilityV1::new(self.normal.clone()),
            minimum,
        )
        .unwrap()
    }

    pub(in super::super) fn endpoint(
        &self,
        address: std::net::SocketAddr,
        role: EndpointRole,
        minimum: FormalHeadRefV1,
    ) -> RemoteFormalEndpoint {
        RemoteFormalEndpoint::new(
            address,
            Duration::from_secs(3),
            self.authority.clone(),
            role,
            WriterSigningCapabilityV1::new(self.recovery.clone()),
            minimum,
        )
        .unwrap()
    }
}
