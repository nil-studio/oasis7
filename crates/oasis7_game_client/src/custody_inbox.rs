//! Private durable business originals. HTTP challenge/credential replacement
//! never changes these bytes or the stable business identity.
use base64::{Engine, engine::general_purpose::STANDARD};
use cap_std::fs::Dir;
use oasis7_client_storage::private_storage as storage;
use oasis7_custody_core::http_pop::{CanonicalHttpTarget, MAX_HTTP_BODY_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"O7HIN001";
const AAD: &[u8] = b"oasis7/custody-host-business-original/v1";
const MAX_RECORD: usize = 64 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostInboxScope {
    pub environment: String,
    pub host_instance_id: String,
    pub client_public_key: String,
    pub session_id: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostBusinessOriginal {
    scope: HostInboxScope,
    request_id: String,
    method: String,
    target_uri: String,
    body_base64: String,
    body_digest: String,
}
impl core::fmt::Debug for HostBusinessOriginal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("HostBusinessOriginal(<private>)")
    }
}
pub(crate) struct CustodyInbox {
    directory: Dir,
    key: Zeroizing<[u8; 32]>,
    scope: HostInboxScope,
}
fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
}
impl HostInboxScope {
    fn validate(&self) -> Result<(), String> {
        if ![&self.environment, &self.host_instance_id, &self.session_id]
            .iter()
            .all(|s| id(s))
            || self.client_public_key.len() != 64
            || hex::decode(&self.client_public_key).is_err()
            || self
                .client_public_key
                .bytes()
                .any(|b| b.is_ascii_uppercase())
        {
            return Err("invalid custody inbox binding".into());
        }
        Ok(())
    }
}
impl HostBusinessOriginal {
    pub(crate) fn new(
        scope: HostInboxScope,
        request_id: &str,
        target: &CanonicalHttpTarget,
        body: &[u8],
    ) -> Result<Self, String> {
        scope.validate()?;
        if !id(request_id)
            || body.len() > MAX_HTTP_BODY_BYTES
            || target.method() == "GET" && !body.is_empty()
        {
            return Err("invalid custody business original".into());
        }
        Ok(Self {
            scope,
            request_id: request_id.into(),
            method: target.method().into(),
            target_uri: target.uri().into(),
            body_base64: STANDARD.encode(body),
            body_digest: hex::encode(Sha256::digest(body)),
        })
    }
    fn body(&self) -> Result<Zeroizing<Vec<u8>>, String> {
        let body = Zeroizing::new(
            STANDARD
                .decode(&self.body_base64)
                .map_err(|_| "corrupt custody original encoding")?,
        );
        if body.len() > MAX_HTTP_BODY_BYTES
            || STANDARD.encode(&body) != self.body_base64
            || hex::encode(Sha256::digest(&body)) != self.body_digest
        {
            return Err("corrupt custody original digest".into());
        }
        Ok(body)
    }
    pub(crate) fn exact_body(
        &self,
        target: &CanonicalHttpTarget,
    ) -> Result<Zeroizing<Vec<u8>>, String> {
        if self.method != target.method() || self.target_uri != target.uri() {
            return Err("custody original target changed".into());
        }
        self.body()
    }
    pub(crate) fn request_id(&self) -> &str {
        &self.request_id
    }
}
impl CustodyInbox {
    /// Only a new trusted enrollment may initialize a namespace. Restart callers
    /// must use open_existing; missing history is never repaired into empty state.
    pub(crate) fn initialize(
        root: &Path,
        key: [u8; 32],
        scope: HostInboxScope,
    ) -> Result<Self, String> {
        scope.validate()?;
        let directory = storage::open_root(root)?;
        storage::initialize_key(&directory, &key)?;
        let scope_bytes =
            Zeroizing::new(serde_json::to_vec(&scope).map_err(|_| "invalid inbox scope")?);
        storage::publish_bounded_record(
            &directory,
            &key,
            "scope.original",
            MAGIC,
            AAD,
            &scope_bytes,
            MAX_RECORD,
        )?;
        verify_scope(&directory, &key, &scope)?;
        Ok(Self {
            directory,
            key: Zeroizing::new(key),
            scope,
        })
    }
    pub(crate) fn open_existing(
        root: &Path,
        key: [u8; 32],
        scope: HostInboxScope,
    ) -> Result<Self, String> {
        scope.validate()?;
        if !std::fs::symlink_metadata(root)
            .map_err(|_| "custody inbox history missing")?
            .is_dir()
        {
            return Err("custody inbox history unavailable".into());
        }
        let directory = storage::open_root(root)?;
        storage::verify_key(&directory, &key)?;
        verify_scope(&directory, &key, &scope)?;
        Ok(Self {
            directory,
            key: Zeroizing::new(key),
            scope,
        })
    }
    /// Restore the original by its stable business ID without accepting new model bytes.
    pub(crate) fn lookup(&self, request_id: &str) -> Result<Option<HostBusinessOriginal>, String> {
        if !id(request_id) {
            return Err("invalid custody business identity".into());
        }
        let name = format!(
            "{}.original",
            hex::encode(Sha256::digest(request_id.as_bytes()))
        );
        let Some(bytes) = storage::read_bounded_record(
            &self.directory,
            &self.key,
            &name,
            MAGIC,
            AAD,
            MAX_RECORD,
        )?
        else {
            return Ok(None);
        };
        let saved: HostBusinessOriginal =
            serde_json::from_slice(&bytes).map_err(|_| "invalid custody original record")?;
        if saved.scope != self.scope || saved.request_id != request_id {
            return Err("custody retained original binding changed".into());
        }
        saved.body()?;
        Ok(Some(saved))
    }
    /// The encrypted immutable winner is durable before this returns. No HTTP
    /// signature, token or challenge is retained as a reusable business request.
    pub(crate) fn retain(
        &self,
        original: &HostBusinessOriginal,
    ) -> Result<HostBusinessOriginal, String> {
        if original.scope != self.scope {
            return Err("custody inbox scope mismatch".into());
        }
        original.body()?;
        let name = format!(
            "{}.original",
            hex::encode(Sha256::digest(original.request_id.as_bytes()))
        );
        let bytes = Zeroizing::new(
            serde_json::to_vec(original).map_err(|_| "custody original encoding failed")?,
        );
        storage::publish_bounded_record(
            &self.directory,
            &self.key,
            &name,
            MAGIC,
            AAD,
            &bytes,
            MAX_RECORD,
        )?;
        let winner = storage::read_bounded_record(
            &self.directory,
            &self.key,
            &name,
            MAGIC,
            AAD,
            MAX_RECORD,
        )?
        .ok_or("custody original unavailable after publication")?;
        let saved: HostBusinessOriginal =
            serde_json::from_slice(&winner).map_err(|_| "invalid custody original record")?;
        saved.body()?;
        if saved != *original {
            return Err("custody request ID conflicts with retained original".into());
        }
        Ok(saved)
    }
}

fn verify_scope(directory: &Dir, key: &[u8; 32], scope: &HostInboxScope) -> Result<(), String> {
    let bytes =
        storage::read_bounded_record(directory, key, "scope.original", MAGIC, AAD, MAX_RECORD)?
            .ok_or("custody inbox scope history missing")?;
    let saved: HostInboxScope =
        serde_json::from_slice(&bytes).map_err(|_| "custody inbox scope record invalid")?;
    if &saved != scope {
        return Err("custody inbox scope changed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_custody_core::http_pop::HttpPopRoute;
    #[test]
    fn durable_original_survives_restart_and_rejects_changed_body_scope_key_or_missing_history() {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-custody-inbox-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let scope = HostInboxScope {
            environment: "fixture".into(),
            host_instance_id: "host-one".into(),
            client_public_key: "71".repeat(32),
            session_id: "session-one".into(),
        };
        let target =
            CanonicalHttpTarget::new("https://custody.example", HttpPopRoute::ProofChallenges)
                .unwrap();
        let original =
            HostBusinessOriginal::new(scope.clone(), "request-one", &target, b"{\"fixture\":1}")
                .unwrap();
        let inbox = CustodyInbox::initialize(&root, [7; 32], scope.clone()).unwrap();
        let kept = inbox.retain(&original).unwrap();
        assert_eq!(
            kept.exact_body(&target).unwrap().as_slice(),
            b"{\"fixture\":1}"
        );
        assert!(!format!("{kept:?}").contains("fixture"));
        drop(inbox);
        let reopened = CustodyInbox::open_existing(&root, [7; 32], scope.clone()).unwrap();
        assert_eq!(
            reopened
                .lookup("request-one")
                .unwrap()
                .unwrap()
                .exact_body(&target)
                .unwrap()
                .as_slice(),
            b"{\"fixture\":1}"
        );
        assert!(reopened.lookup("unknown-request").unwrap().is_none());
        assert_eq!(
            reopened.retain(&original).unwrap().request_id(),
            "request-one"
        );
        let different =
            HostBusinessOriginal::new(scope.clone(), "request-one", &target, b"{}").unwrap();
        assert!(reopened.retain(&different).is_err());
        assert!(CustodyInbox::open_existing(&root, [8; 32], scope.clone()).is_err());
        let mut other = scope.clone();
        other.session_id = "session-two".into();
        assert!(CustodyInbox::open_existing(&root, [7; 32], other.clone()).is_err());
        assert!(CustodyInbox::initialize(&root, [7; 32], other).is_err());
        drop(reopened);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(CustodyInbox::open_existing(&root, [7; 32], scope).is_err());
        assert!(!root.exists());
    }
}
