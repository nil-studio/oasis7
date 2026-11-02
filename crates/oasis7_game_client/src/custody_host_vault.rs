//! Host-owned encrypted state and irreversible local send reservations.
use super::state::*;
use cap_std::fs::Dir;
use oasis7_client_storage::private_storage as storage;
use oasis7_custody_core::{
    DecimalU64,
    http_pop::{CanonicalHttpTarget, HttpPopProfileV1},
    provisioning::SessionPurposeV1,
};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;
const MAGIC: &[u8; 8] = b"O7HVT001";
const AAD: &[u8] = b"oasis7/custody-host-private-vault/v1";
const LIMIT: usize = 256 * 1024;
const RECORD: &str = "host.state";
pub(super) struct Reservation<'a> {
    pub original_id: &'a str,
    pub uri: &'a str,
    pub body: &'a [u8],
    pub profile: Option<HttpPopProfileV1>,
    pub purpose: Option<SessionPurposeV1>,
    pub generation: Option<DecimalU64>,
    pub bootstrap: bool,
}
pub(super) struct Vault {
    pub root: PathBuf,
    directory: Dir,
    key: Zeroizing<[u8; 32]>,
}
pub(super) fn now() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| denied())?
        .as_millis()
        .try_into()
        .map_err(|_| denied())
}
pub(super) fn random_id() -> Result<String, String> {
    let mut b = [0; 32];
    getrandom::fill(&mut b).map_err(|_| denied())?;
    Ok(hex::encode(b))
}
pub(super) fn random_secret() -> Result<Zeroizing<[u8; 32]>, String> {
    let mut bytes = Zeroizing::new([0; 32]);
    getrandom::fill(&mut *bytes).map_err(|_| denied())?;
    Ok(bytes)
}
pub(super) fn secret32(v: &str) -> Result<Zeroizing<[u8; 32]>, String> {
    let bytes = Zeroizing::new(hex::decode(v).map_err(|_| denied())?);
    if v.len() != 64 || hex::encode(&*bytes) != v {
        return Err(denied());
    }
    Ok(Zeroizing::new(
        bytes.as_slice().try_into().map_err(|_| denied())?,
    ))
}
pub(super) fn actual_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and returns this process's kernel identity.
    unsafe { libc::geteuid() }
}
impl Vault {
    pub fn initialize(
        root: &Path,
        key: Zeroizing<[u8; 32]>,
        mut identity: IdentityBinding,
        request_id: String,
    ) -> Result<Self, String> {
        identity.isolation.validate()?;
        if identity.isolation.host_uid != actual_uid()
            || !id(&request_id)
            || !identity.descriptor.client_public_key.is_empty()
        {
            return Err(denied());
        }
        let seed = random_secret()?;
        identity.descriptor.client_public_key = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&seed)
                .verifying_key()
                .to_bytes(),
        );
        super::ipc::fill_runtime_pins(&mut identity)?;
        identity.validate()?;
        let directory = storage::open_root(root)?;
        // Initialization never adopts an interrupted marker or missing history.
        if directory.entries().map_err(|_| denied())?.next().is_some() {
            return Err(denied());
        }
        storage::initialize_key(&directory, &key)?;
        let state = State {
            api_version: 1,
            signing_seed: hex::encode(seed.as_slice()),
            enrollment: Enrollment {
                original: oasis7_custody_core::client_enrollment::ClientEnrollmentCreateRequestV1 {
                    target: identity.target.clone(),
                    api_version: 1,
                    request_id,
                    descriptor: identity.descriptor.clone(),
                },
                response: None,
                proof: None,
                proved: false,
            },
            identity,
            session: None,
            attempts: Vec::new(),
            time_floor: now()?,
        };
        state.validate()?;
        let bytes = Zeroizing::new(serde_json::to_vec(&state).map_err(|_| denied())?);
        if !storage::publish_bounded_record(&directory, &key, RECORD, MAGIC, AAD, &bytes, LIMIT)? {
            return Err(denied());
        }
        let vault = Self {
            root: root.into(),
            directory,
            key,
        };
        {
            let (_lock, _) = storage::lock_record(&vault.directory, &vault.key, "host.lock")?;
        }
        vault.read(|_| Ok(()))?;
        Ok(vault)
    }
    pub fn open(root: &Path, key: Zeroizing<[u8; 32]>) -> Result<Self, String> {
        if !std::fs::symlink_metadata(root)
            .map_err(|_| denied())?
            .is_dir()
        {
            return Err(denied());
        }
        let directory = storage::open_root(root)?;
        storage::verify_key(&directory, &key)?;
        storage::read_private(&directory, "host.lock", 1)?;
        let vault = Self {
            root: root.into(),
            directory,
            key,
        };
        vault.read(|_| Ok(()))?;
        vault.verify_history()?;
        Ok(vault)
    }
    fn load(&self) -> Result<State, String> {
        let bytes =
            storage::read_bounded_record(&self.directory, &self.key, RECORD, MAGIC, AAD, LIMIT)?
                .ok_or_else(denied)?;
        let s: State = serde_json::from_slice(&bytes).map_err(|_| denied())?;
        s.validate()?;
        if s.identity.isolation.host_uid != actual_uid() || now()? < s.time_floor {
            return Err(denied());
        }
        Ok(s)
    }
    pub fn read<T>(&self, f: impl FnOnce(&State) -> Result<T, String>) -> Result<T, String> {
        let (_lock, _) = storage::lock_record(&self.directory, &self.key, "host.lock")?;
        f(&self.load()?)
    }
    pub fn update<T>(&self, f: impl FnOnce(&mut State) -> Result<T, String>) -> Result<T, String> {
        let (_lock, _) = storage::lock_record(&self.directory, &self.key, "host.lock")?;
        let mut s = self.load()?;
        let value = f(&mut s)?;
        s.time_floor = s.time_floor.max(now()?);
        s.validate()?;
        let bytes = Zeroizing::new(serde_json::to_vec(&s).map_err(|_| denied())?);
        storage::replace_bounded_record(
            &self.directory,
            &self.key,
            RECORD,
            MAGIC,
            AAD,
            &bytes,
            LIMIT,
        )?;
        Ok(value)
    }
    pub fn bind_session(&self, binding: SessionBinding) -> Result<(), String> {
        self.update(|s| {
            binding.validate(
                &s.identity,
                s.enrollment.response.as_ref().ok_or_else(denied)?,
            )?;
            if !s.enrollment.proved {
                return Err(denied());
            }
            if let Some(old) = &s.session {
                return if old.binding == binding {
                    Ok(())
                } else {
                    Err(denied())
                };
            }
            s.session = Some(Session {
                binding,
                inbox_key: hex::encode(random_secret()?.as_slice()),
                credentials: Vec::new(),
                retired_execution: 0,
                retired_read: 0,
                logicals: Default::default(),
                claims: Default::default(),
                closing: false,
                inbox_ready: false,
            });
            Ok(())
        })?;
        self.provision_inbox()
    }
    fn inbox_parts(
        &self,
    ) -> Result<(Zeroizing<[u8; 32]>, crate::custody_inbox::HostInboxScope), String> {
        self.read(|s| {
            let session = s.session.as_ref().ok_or_else(denied)?;
            let e = s.enrollment.response.as_ref().ok_or_else(denied)?;
            Ok((
                secret32(&session.inbox_key)?,
                crate::custody_inbox::HostInboxScope {
                    environment: s.identity.descriptor.environment.clone(),
                    host_instance_id: s.identity.descriptor.host_instance_id.clone(),
                    client_public_key: s.identity.descriptor.client_public_key.clone(),
                    session_id: session.binding.session_id.clone(),
                    identity_digest: s.identity.digest()?,
                    session_digest: session.binding.digest()?,
                    enrollment_id: e.enrollment_id.clone(),
                    key_id: e.key_id.clone(),
                    service_origin: s.identity.service_origin.clone(),
                    provision_id: s.identity.target.provision_id.clone(),
                    custody_account_id: s.identity.target.custody_account_id.clone(),
                },
            ))
        })
    }
    fn provision_inbox(&self) -> Result<(), String> {
        if self.read(|s| Ok(s.session.as_ref().ok_or_else(denied)?.inbox_ready))? {
            self.inbox()?;
            return Ok(());
        }
        let (key, scope) = self.inbox_parts()?;
        let path = self.root.join("session-inbox");
        // Explicit bind creates this namespace once. An existing namespace must
        // authenticate its complete original scope and history.
        if path.exists() {
            crate::custody_inbox::CustodyInbox::open_existing(&path, key, scope)?;
        } else {
            crate::custody_inbox::CustodyInbox::initialize(&path, key, scope)?;
        }
        self.update(|s| {
            s.session.as_mut().ok_or_else(denied)?.inbox_ready = true;
            Ok(())
        })
    }
    pub fn inbox(&self) -> Result<crate::custody_inbox::CustodyInbox, String> {
        if !self.read(|s| Ok(s.session.as_ref().ok_or_else(denied)?.inbox_ready))? {
            return Err(denied());
        }
        let (key, scope) = self.inbox_parts()?;
        crate::custody_inbox::CustodyInbox::open_existing(
            &self.root.join("session-inbox"),
            key,
            scope,
        )
    }
    pub fn retain(
        &self,
        id: &str,
        target: &CanonicalHttpTarget,
        body: &[u8],
    ) -> Result<(), String> {
        let expected = self.read(|s| {
            Ok(s.session
                .as_ref()
                .and_then(|s| s.logicals.get(id))
                .is_some_and(|x| x.retained)
                || s.attempts
                    .iter()
                    .any(|a| a.original_id == id && a.retained && !a.bootstrap))
        })?;
        let inbox = self.inbox()?;
        if expected && inbox.lookup(id)?.is_none() {
            return Err(denied());
        }
        let original = crate::custody_inbox::HostBusinessOriginal::new(
            inbox.scope().clone(),
            id,
            target,
            body,
        )?;
        inbox.retain(&original)?;
        self.update(|s| {
            if let Some(l) = s.session.as_mut().and_then(|s| s.logicals.get_mut(id)) {
                l.retained = true;
            }
            Ok(())
        })
    }
    pub fn verify_history(&self) -> Result<(), String> {
        let (live, logicals) = self.read(|s| {
            Ok((
                s.attempts
                    .iter()
                    .filter(|a| !a.bootstrap && a.retained)
                    .map(|a| {
                        (
                            a.original_id.clone(),
                            a.target_digest.clone(),
                            a.body_digest.clone(),
                        )
                    })
                    .collect::<Vec<_>>(),
                s.session
                    .as_ref()
                    .map(|s| {
                        s.logicals
                            .iter()
                            .filter(|(_, x)| x.retained)
                            .map(|(id, _)| id.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            ))
        })?;
        if live.is_empty() && logicals.is_empty() {
            return Ok(());
        }
        let inbox = self.inbox()?;
        for id in logicals {
            if inbox.lookup(&id)?.is_none() {
                return Err(denied());
            }
        }
        for (id, target, body) in live {
            let saved = inbox.lookup(&id)?.ok_or_else(denied)?;
            if saved.transport_digests() != (target, body) {
                return Err(denied());
            }
        }
        Ok(())
    }
    pub fn mark_retained(&self, attempt_id: &str) -> Result<(), String> {
        self.update(|s| {
            s.attempts
                .iter_mut()
                .find(|a| a.id == attempt_id)
                .ok_or_else(denied)?
                .retained = true;
            Ok(())
        })
    }
    pub fn reserve_create(&self, original_id: &str, body: &[u8]) -> Result<String, String> {
        let uri = self.read(|s| {
            Ok(format!(
                "{}/v1/client-enrollments",
                s.identity.service_origin
            ))
        })?;
        self.reserve(Reservation {
            original_id,
            uri: &uri,
            body,
            profile: None,
            purpose: None,
            generation: None,
            bootstrap: true,
        })
    }
    pub fn reserve(&self, q: Reservation<'_>) -> Result<String, String> {
        use sha2::{Digest, Sha256};
        let target_digest = hex::encode(Sha256::digest(q.uri.as_bytes()));
        let body_digest = hex::encode(Sha256::digest(q.body));
        self.update(|s| {
            let at = now()?;
            if at < s.time_floor {
                return Err(denied());
            }
            if !q.bootstrap {
                let session = s.session.as_ref().ok_or_else(denied)?;
                if at >= session.binding.deadline(q.purpose.ok_or_else(denied)?)? {
                    return Err(denied());
                }
            }
            let id = random_id()?;
            s.attempts.push(Attempt {
                id: id.clone(),
                original_id: q.original_id.into(),
                at,
                bootstrap: q.bootstrap,
                retained: q.bootstrap,
                target_digest,
                body_digest,
                profile: q.profile,
                purpose: q.purpose,
                generation: q.generation,
            });
            Ok(id)
        })
    }
}
