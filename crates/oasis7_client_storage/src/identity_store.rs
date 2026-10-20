//! Password-unlocked native login key and separate encrypted request originals.
//! Reuses the project's fixed Argon2id/XChaCha envelope and no-link private FS.
use crate::private_storage as storage;
use cap_std::fs::Dir;
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;
use zeroize::Zeroizing;

const KEY_FILE: &str = "login-identity.encrypted-v1";
const KEY_KIND: &str = "login-identity-v1";
const RECORD_MAGIC: &[u8; 8] = b"O7ENTRY1";
const LIMIT: usize = 65536;

pub struct IdentityStore {
    directory: Dir,
    material: Zeroizing<Vec<u8>>,
}
impl IdentityStore {
    /// Explicit user action; never overwrites an existing or interrupted vault.
    pub fn create(root: &Path, passphrase: &[u8]) -> Result<Self, String> {
        let directory = storage::open_root(root)?;
        let mut material = Zeroizing::new(vec![0u8; 64]);
        getrandom::fill(&mut material).map_err(|_| "identity randomness unavailable")?;
        let envelope = oasis7_secure_envelope::seal(KEY_KIND, &material, passphrase)
            .map_err(|_| "identity encryption failed")?;
        let key: &[u8; 32] = material[32..]
            .try_into()
            .map_err(|_| "invalid identity material")?;
        storage::initialize_key(&directory, key)?;
        if !storage::publish_immutable(&directory, KEY_FILE, &envelope)? {
            return Err("identity already exists; unlock the original".into());
        }
        Ok(Self {
            directory,
            material,
        })
    }
    /// Never generates replacement keys when originals are missing/corrupt.
    pub fn open(root: &Path, passphrase: &[u8]) -> Result<Self, String> {
        let directory = storage::open_root(root)?;
        let envelope = storage::read_private(&directory, KEY_FILE, 4096)?;
        let material = oasis7_secure_envelope::open(KEY_KIND, &envelope, passphrase)
            .map_err(|_| "identity unlock failed")?;
        if material.len() != 64 {
            return Err("invalid identity material".into());
        }
        storage::verify_key(
            &directory,
            material[32..]
                .try_into()
                .map_err(|_| "invalid identity material")?,
        )?;
        Ok(Self {
            directory,
            material,
        })
    }
    /// For the trusted local identity adapter only. Never export to model tools,
    /// request originals, diagnostics or world/executor configuration.
    pub fn login_seed(&self) -> &[u8; 32] {
        self.material[..32]
            .try_into()
            .expect("validated identity seed size")
    }
    fn key(&self) -> &[u8; 32] {
        self.material[32..]
            .try_into()
            .expect("validated outbox key size")
    }
    fn binding(
        &self,
        scope: &str,
        operation: &str,
        stage: &str,
    ) -> Result<(String, Vec<u8>), String> {
        if scope.is_empty()
            || scope.len() > 256
            || operation.is_empty()
            || operation.len() > 256
            || stage.is_empty()
            || stage.len() > 128
        {
            return Err("invalid identity journal binding".into());
        }
        let aad = oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7/identity-outbox/v1",
            scope,
            operation,
            stage,
        ))
        .map_err(|_| "identity journal encoding failed")?;
        let name = format!("original-{}", blake3::keyed_hash(self.key(), &aad));
        Ok((name, aad))
    }
    pub fn read_original<T: DeserializeOwned>(
        &self,
        scope: &str,
        operation: &str,
        stage: &str,
    ) -> Result<Option<T>, String> {
        let (name, aad) = self.binding(scope, operation, stage)?;
        storage::read_bounded_record(
            &self.directory,
            self.key(),
            &name,
            RECORD_MAGIC,
            &aad,
            LIMIT,
        )?
        .map(|v| serde_json::from_slice(&v).map_err(|_| "identity journal corrupt".into()))
        .transpose()
    }
    /// Durable before network I/O. Returns the existing immutable winner; caller
    /// must compare it to its original intent before sending or replaying.
    pub fn put_original<T: Serialize + DeserializeOwned>(
        &self,
        scope: &str,
        operation: &str,
        stage: &str,
        value: &T,
    ) -> Result<T, String> {
        let (name, aad) = self.binding(scope, operation, stage)?;
        let bytes = Zeroizing::new(
            serde_json::to_vec(value).map_err(|_| "identity journal encoding failed")?,
        );
        storage::publish_bounded_record(
            &self.directory,
            self.key(),
            &name,
            RECORD_MAGIC,
            &aad,
            &bytes,
            LIMIT,
        )?;
        self.read_original(scope, operation, stage)?
            .ok_or_else(|| "identity original missing after publication".into())
    }
}
