//! Real executable writer service used by the native Runtime acceptance paths.
//! The fixture provisions software custody; Runtime receives only its client key.
use super::*;
use formal::{
    InitialActivationTrustV1, RecoveryPermissionBootstrapV1, SignedInitialActivationV1,
    issuer::IssuerAnchorV1, writer_signing::RemoteWriterCustodyClientV1,
};

pub(crate) struct Authorization<'a> {
    pub authority: &'a VerifiedAuthorityV1,
    pub trust: &'a InitialActivationTrustV1,
    pub certificate: &'a SignedInitialActivationV1,
    pub recovery: Option<(&'a RecoveryPermissionBootstrapV1, &'a IssuerAnchorV1)>,
}

pub(crate) struct WriterCustodyFixture {
    child: EndpointChild,
    binary: std::ffi::OsString,
    config: PathBuf,
    authority: VerifiedAuthorityV1,
    credentials: [u8; 2],
    pub address: std::net::SocketAddr,
    pub credential_file: PathBuf,
    recovery_credential_file: PathBuf,
    pub normal: RemoteWriterCustodyClientV1,
    pub recovery: RemoteWriterCustodyClientV1,
}

impl WriterCustodyFixture {
    pub(crate) fn initialize(
        directory: &Path,
        binary: &std::ffi::OsStr,
        authorization: Authorization<'_>,
        writer: &SigningKey,
        credentials: [u8; 2],
    ) -> Result<Self, String> {
        private_directory(directory)?;
        let config = directory.join("service.json");
        let writer_file = directory.join("provider-writer-key");
        write_private(&writer_file, hex::encode(writer.to_bytes()).as_bytes())?;
        let credential_file = directory.join("runtime-client-key");
        write_private(
            &credential_file,
            hex::encode(key(credentials[0]).to_bytes()).as_bytes(),
        )?;
        let recovery_credential_file = directory.join("recovery-client-key");
        write_private(
            &recovery_credential_file,
            hex::encode(key(credentials[1]).to_bytes()).as_bytes(),
        )?;
        let (permission, minimum) = authorization.recovery.unzip();
        let value = serde_json::json!({
            "schema_version": 1, "storage_mode": "initialize",
            "initial_trust": authorization.trust,
            "initial_certificate": authorization.certificate,
            "recovery_permission": permission,
            "minimum_issuer_anchor": minimum,
            "data_dir": directory.join("ledger"),
            "writer_key_file": writer_file,
            "normal_credential_public_key": public(credentials[0]),
            "recovery_credential_public_key": public(credentials[1]),
            "minimum_revision": 0, "listen": "127.0.0.1:0", "timeout_ms": 30000,
        });
        write_private(
            &config,
            &serde_json::to_vec(&value).map_err(|e| e.to_string())?,
        )?;
        let (child, address) = start_service(
            binary,
            &config,
            "--writer-custody-config",
            "writer_custody_listening=",
            "writer custody",
        )?;
        let normal = client(address, authorization.authority, credentials[0], 0)?;
        let recovery = client(address, authorization.authority, credentials[1], 0)?;
        let state = normal.query().map_err(|e| e.to_string())?;
        if state.revision != 0
            || state.authority_epoch != authorization.authority.definition().epoch
            || state.plan_digest.is_some()
            || state.drained_digest.is_some()
            || state.seal_digest.is_some()
        {
            return Err("fresh writer service did not report its exact empty epoch".into());
        }
        Ok(Self {
            child,
            binary: binary.to_owned(),
            config,
            authority: authorization.authority.clone(),
            credentials,
            address,
            credential_file,
            recovery_credential_file,
            normal,
            recovery,
        })
    }

    pub(crate) fn runtime_config(&self) -> serde_json::Value {
        serde_json::json!({"endpoint": self.address, "credential_file": self.credential_file, "minimum_revision": 0})
    }

    pub(crate) fn recovery_credential_public_key(&self) -> String {
        public(self.credentials[1])
    }

    pub(crate) fn recovery_runtime_config(&self, minimum_revision: u64) -> serde_json::Value {
        serde_json::json!({
            "endpoint": self.address,
            "credential_file": self.recovery_credential_file,
            "minimum_revision": minimum_revision,
        })
    }

    pub(crate) fn reopen_after_sigkill(&mut self, minimum_revision: u64) -> Result<(), String> {
        self.child.kill_sigkill_and_wait()?;
        let mut config: serde_json::Value =
            serde_json::from_slice(&fs::read(&self.config).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        config["storage_mode"] = "open_existing".into();
        config["minimum_revision"] = minimum_revision.into();
        let staging = self.config.with_extension("cold.tmp");
        write_private(
            &staging,
            &serde_json::to_vec(&config).map_err(|e| e.to_string())?,
        )?;
        fs::rename(&staging, &self.config).map_err(|e| e.to_string())?;
        fs::File::open(self.config.parent().ok_or("writer config parent")?)
            .and_then(|file| file.sync_all())
            .map_err(|e| e.to_string())?;
        let (child, address) = start_service(
            &self.binary,
            &self.config,
            "--writer-custody-config",
            "writer_custody_listening=",
            "writer custody",
        )?;
        self.child = child;
        self.address = address;
        self.normal = client(
            address,
            &self.authority,
            self.credentials[0],
            minimum_revision,
        )?;
        self.recovery = client(
            address,
            &self.authority,
            self.credentials[1],
            minimum_revision,
        )?;
        Ok(())
    }
}

fn client(
    address: std::net::SocketAddr,
    authority: &VerifiedAuthorityV1,
    credential: u8,
    minimum: u64,
) -> Result<RemoteWriterCustodyClientV1, String> {
    RemoteWriterCustodyClientV1::new(
        address,
        Duration::from_secs(30),
        authority.authority_id().map_err(|e| e.to_string())?,
        authority.definition().writer_public_key.clone(),
        key(credential),
        minimum,
    )
    .map_err(|e| e.to_string())
}
