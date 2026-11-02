//! World HTTP-local permanent consumption. This is not Runtime/consensus state.
//! The separately configured guard directory must be outside the primary backup
//! unit. A full rollback including that guard is not qualified by this store.
use cap_fs_ext::DirExt;
use cap_std::fs::Dir;
use oasis7_client_api::world_service::{
    participant_discovery::ParticipantOwnerViewRequestV1, resource_basis::RESOURCE_GOAL_PATH_V2,
};
use oasis7_client_storage::private_storage as storage;
pub use oasis7_custody_core::provisioning::{
    OwnerSetupResourceWorldBodyV2, PaidResourceReadIssuerFloorV1, PaidResourceReadIssuerPinV1,
    PaidResourceReadPayloadV1, PaidResourceReadProofV1, PaidResourceReadStageV1,
    RESOURCE_READ_PROOF_HEADER,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::File,
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"O7WRP001";
const AAD: &[u8] = b"oasis7.world.resource-paid-ledger.v1";
const HEAD: &str = "resource-paid-head.v1";
const GUARD: &str = "resource-paid-guard.v1";
const HEAD_BYTES: usize = 4096;
const EVENT_BYTES: usize = 192 * 1024;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Head {
    api_version: u8,
    namespace: String,
    sequence: u64,
    digest: String,
}
fn required_next<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<Head>, D::Error> {
    Option::<Head>::deserialize(d)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Guard {
    stable: Head,
    #[serde(deserialize_with = "required_next")]
    prepared: Option<Head>,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Substep {
    OwnerPreflight,
    ResourceFinal,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Body {
    OwnerPreflight {
        request: Box<ParticipantOwnerViewRequestV1>,
    },
    ResourceFinal {
        request: Box<OwnerSetupResourceWorldBodyV2>,
    },
}
impl Body {
    fn substep(&self) -> Substep {
        match self {
            Self::OwnerPreflight { .. } => Substep::OwnerPreflight,
            Self::ResourceFinal { .. } => Substep::ResourceFinal,
        }
    }
    fn validate(&self, proof: &PaidResourceReadProofV1) -> Result<()> {
        match self {
            Self::OwnerPreflight { request } => {
                OwnerSetupResourceWorldBodyV2::validate_preflight_bindings(request, &proof.payload)
            }
            Self::ResourceFinal { request } => request.validate_bindings(&proof.payload),
        }
        .map_err(|_| "World resource paid request binding".into())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum EventKind {
    ConfiguredIssuer {
        pin: Box<PaidResourceReadIssuerPinV1>,
    },
    PhysicalAttempt {
        at_unix_ms: u64,
        proof: Box<PaidResourceReadProofV1>,
        body: Body,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    api_version: u8,
    sequence: u64,
    previous_digest: String,
    event: EventKind,
}

/// No Clone/Serde/Debug or public constructor. Only a committed physical claim
/// returns this exact request. The HTTP consumer still checks local participant,
/// full Current, historical activation and delivery at the original deadline.
pub struct ConsumedWorldResourceAttemptV1 {
    proof: PaidResourceReadProofV1,
    body: Body,
}
impl ConsumedWorldResourceAttemptV1 {
    pub fn ensure_live(&self) -> Result<()> {
        let at = now()?;
        if at < self.proof.payload.issued_at.0 || at >= self.proof.payload.expires_at.0 {
            return Err("World resource original expired".into());
        }
        Ok(())
    }
    /// Metadata equality only: the actual handler supplies its opaque qualified
    /// cut. This never constructs Current or consumes/returns another claim.
    pub fn ensure_current_scope(
        &self,
        commit: &oasis7_client_api::world_service::CommitRef,
    ) -> Result<()> {
        self.ensure_live()?;
        let approved = &self.proof.payload.world_audience;
        if commit.world.world_id != approved.world_id
            || commit.world.genesis_digest != approved.genesis_digest
            || commit.binding.branch_id != approved.branch_id
            || commit.binding.reorg_generation != approved.reorg_epoch.0
        {
            return Err("World resource paid original/current scope mismatch".into());
        }
        Ok(())
    }
    pub fn original_binding_digest(&self) -> Result<String> {
        digest(
            "oasis7.world.resource-consumed-original.v1",
            &(&self.proof, &self.body),
        )
    }
    pub fn paid_reply_binding(
        &self,
    ) -> Result<oasis7_client_api::world_service::resource_basis::ResourcePaidWorldBindingV1> {
        use oasis7_client_api::world_service::{self as ws, resource_basis as resource};
        if self.proof.payload.stage != PaidResourceReadStageV1::OwnerSetupFinal {
            return Err("World Resource paid reply stage".into());
        }
        let (substep, body_digest) = match &self.body {
            Body::OwnerPreflight { request } => (
                resource::ResourcePaidWorldSubstepV1::OwnerPreflight,
                ws::request_digest(
                    resource::ResourcePaidWorldSubstepV1::OwnerPreflight.world_path(),
                    request.as_ref(),
                )?,
            ),
            Body::ResourceFinal { request } => (
                resource::ResourcePaidWorldSubstepV1::ResourceFinal,
                ws::request_digest(resource::RESOURCE_GOAL_PATH_V2, request.as_ref())?,
            ),
        };
        let binding = resource::ResourcePaidWorldBindingV1 {
            api_version: 1,
            proof_digest: ws::request_digest(
                resource::RESOURCE_PAID_PROOF_DIGEST_DOMAIN,
                &self.proof,
            )?,
            stage: resource::ResourcePaidWorldStageV1::OwnerSetupFinal,
            substep,
            world_path: substep.world_path().into(),
            body_digest,
        };
        binding.validate()?;
        Ok(binding)
    }
    pub fn original_window(&self) -> (u64, u64) {
        (
            self.proof.payload.issued_at.0,
            self.proof.payload.expires_at.0,
        )
    }
    pub fn owner_preflight(&self) -> Option<&ParticipantOwnerViewRequestV1> {
        if let Body::OwnerPreflight { request } = &self.body {
            Some(request)
        } else {
            None
        }
    }
    pub fn resource_final(&self) -> Option<&OwnerSetupResourceWorldBodyV2> {
        if let Body::ResourceFinal { request } = &self.body {
            Some(request)
        } else {
            None
        }
    }
    pub fn resource_path(&self) -> Option<&'static str> {
        self.resource_final().map(|_| RESOURCE_GOAL_PATH_V2)
    }
}

/// Caller owns a dedicated service-local Mutex; never acquire it under a Node,
/// Viewer, publication or Runtime lock. The OS guard lock is held for this
/// process lifetime, so a second process cannot consume through an old backup.
pub struct WorldResourcePaidLedgerV1 {
    primary: Dir,
    guard: Dir,
    key: Zeroizing<[u8; 32]>,
    _lock: File,
    head: Head,
    pin: PaidResourceReadIssuerPinV1,
    floor: PaidResourceReadIssuerFloorV1,
    attempts: BTreeSet<String>,
    healthy: bool,
    maximum_events: u64,
    #[cfg(test)]
    test_fault: Option<TestFault>,
}
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum TestFault {
    AfterPrepared,
    AfterEvent,
    AfterHead,
}

fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    oasis7_wasm_abi::encode_canonical_cbor(value).map_err(|e| e.to_string())
}
fn digest<T: Serialize>(domain: &str, value: &T) -> Result<String> {
    Ok(blake3::hash(&canonical(&(domain, value))?)
        .to_hex()
        .to_string())
}
fn decode<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    let value: T = serde_cbor::from_slice(bytes).map_err(|_| "World resource ledger codec")?;
    if canonical(&value)? != bytes {
        return Err("World resource ledger noncanonical bytes".into());
    }
    Ok(value)
}
fn now() -> Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "World UTC unavailable")?
            .as_millis(),
    )
    .map_err(|_| "World UTC overflow".into())
}
pub(crate) fn existing_dir(path: &Path) -> Result<Dir> {
    let parts = path.components().collect::<Vec<_>>();
    if !path.is_absolute()
        || parts.len() < 2
        || parts
            .iter()
            .skip(1)
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("World resource ledger requires absolute private directories".into());
    }
    let mut dir =
        Dir::open_ambient_dir("/", cap_std::ambient_authority()).map_err(|e| e.to_string())?;
    for part in parts.iter().skip(1) {
        dir = dir
            .open_dir_nofollow(part.as_os_str())
            .map_err(|e| e.to_string())?;
    }
    storage::private_directory(&dir)?;
    Ok(dir)
}
fn record_aad(name: &str) -> Result<Vec<u8>> {
    canonical(&(AAD, name))
}
fn read<T: serde::de::DeserializeOwned + Serialize>(
    dir: &Dir,
    key: &[u8; 32],
    name: &str,
    max: usize,
) -> Result<T> {
    let bytes = storage::read_bounded_record(dir, key, name, MAGIC, &record_aad(name)?, max)?
        .ok_or("World resource ledger missing required record")?;
    decode(&bytes)
}
fn replace<T: Serialize>(dir: &Dir, key: &[u8; 32], name: &str, value: &T) -> Result<()> {
    storage::replace_bounded_record(
        dir,
        key,
        name,
        MAGIC,
        &record_aad(name)?,
        &canonical(value)?,
        HEAD_BYTES,
    )
}
fn issuer_namespace(pin: &PaidResourceReadIssuerPinV1) -> Result<String> {
    // Deliberately excludes rotatable issuer/Node public keys, key IDs and
    // issuer epochs/config generations. World ID/genesis remain permanent.
    digest(
        "oasis7.world.resource-paid-namespace.v1",
        &(
            &pin.namespace,
            &pin.service_id,
            &pin.environment,
            &pin.world_audience.world_id,
            &pin.world_audience.genesis_digest,
        ),
    )
}
fn attempt_key(proof: &PaidResourceReadProofV1, substep: Substep) -> Result<String> {
    let p = &proof.payload;
    digest(
        "oasis7.world.resource-paid-physical-attempt.v1",
        &(
            &p.issuer_namespace,
            &p.issuer_service_id,
            &p.issuer_environment,
            &p.world_audience.world_id,
            &p.world_audience.genesis_digest,
            &p.paid_attempt_id,
            substep,
        ),
    )
}
fn event_name(sequence: u64) -> String {
    format!("resource-paid-{sequence:016x}.v1")
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Origin {
    head: Head,
    pin: PaidResourceReadIssuerPinV1,
}
impl WorldResourcePaidLedgerV1 {
    /// Explicit operator setup only. Neither HTTP nor ordinary open calls this.
    /// Both existing private directories must be new for this ledger lineage.
    pub fn initialize(
        primary: &Path,
        external_guard: &Path,
        key: [u8; 32],
        pin: PaidResourceReadIssuerPinV1,
        maximum_events: u64,
    ) -> Result<Self> {
        if maximum_events == 0 {
            return Err("World resource ledger requires explicit storage bound".into());
        }
        if primary.starts_with(external_guard) || external_guard.starts_with(primary) {
            return Err("World resource guard must be outside the primary restore tree".into());
        }
        let primary = existing_dir(primary)?;
        let guard = existing_dir(external_guard)?;
        separate(&primary, &guard)?;
        storage::initialize_key(&primary, &key)?;
        storage::initialize_key(&guard, &key)?;
        let (_lock, _) = storage::try_lock_record(&guard, &key, "resource-paid-process.lock")?;
        for (dir, name) in [
            (&primary, "resource-paid-origin.v1"),
            (&primary, HEAD),
            (&guard, GUARD),
        ] {
            if storage::read_bounded_record(dir, &key, name, MAGIC, &record_aad(name)?, HEAD_BYTES)?
                .is_some()
            {
                return Err(
                    "World resource ledger already initialized; never reset or backfill".into(),
                );
            }
        }
        validate_pin(&pin)?;
        let namespace = issuer_namespace(&pin)?;
        let head = Head {
            api_version: 1,
            namespace,
            sequence: 0,
            digest: digest("oasis7.world.resource-paid-origin.v1", &pin)?,
        };
        let origin = Origin {
            head: head.clone(),
            pin: pin.clone(),
        };
        if !storage::publish_bounded_record(
            &primary,
            &key,
            "resource-paid-origin.v1",
            MAGIC,
            &record_aad("resource-paid-origin.v1")?,
            &canonical(&origin)?,
            HEAD_BYTES,
        )? {
            return Err("World resource origin already exists".into());
        }
        // An interrupted explicit setup remains blocked, never implicitly reset.
        replace(
            &guard,
            &key,
            GUARD,
            &Guard {
                stable: head.clone(),
                prepared: None,
            },
        )?;
        replace(&primary, &key, HEAD, &head)?;
        Ok(Self {
            primary,
            guard,
            key: Zeroizing::new(key),
            _lock,
            head,
            floor: PaidResourceReadIssuerFloorV1 {
                issuer_epoch: pin.issuer_epoch,
                config_generation: pin.issuer_config_generation,
            },
            pin,
            attempts: BTreeSet::new(),
            healthy: true,
            maximum_events,
            #[cfg(test)]
            test_fault: None,
        })
    }
    pub fn open_existing(
        primary: &Path,
        external_guard: &Path,
        key: [u8; 32],
        configured: PaidResourceReadIssuerPinV1,
        maximum_events: u64,
    ) -> Result<Self> {
        if maximum_events == 0 {
            return Err("World resource ledger requires explicit storage bound".into());
        }
        if primary.starts_with(external_guard) || external_guard.starts_with(primary) {
            return Err("World resource guard must be outside the primary restore tree".into());
        }
        let primary = existing_dir(primary)?;
        let guard = existing_dir(external_guard)?;
        separate(&primary, &guard)?;
        storage::verify_key(&primary, &key)?;
        storage::verify_key(&guard, &key)?;
        let (_lock, _) = storage::try_lock_record(&guard, &key, "resource-paid-process.lock")?;
        let origin: Origin = read(&primary, &key, "resource-paid-origin.v1", HEAD_BYTES)?;
        let head: Head = read(&primary, &key, HEAD, HEAD_BYTES)?;
        let journal: Guard = read(&guard, &key, GUARD, HEAD_BYTES)?;
        validate_pin(&origin.pin)?;
        validate_pin(&configured)?;
        if origin.head.api_version != 1
            || origin.head.sequence != 0
            || origin.head.namespace != issuer_namespace(&origin.pin)?
            || origin.head.digest != digest("oasis7.world.resource-paid-origin.v1", &origin.pin)?
            || head.namespace != origin.head.namespace
            || configured_namespace(&configured, &origin.head)? != head.namespace
        {
            return Err("World resource ledger origin/configuration".into());
        }
        let finalize = match journal.prepared.as_ref() {
            Some(next)
                if next == &head
                    && journal.stable.namespace == head.namespace
                    && journal.stable.sequence.checked_add(1) == Some(head.sequence) =>
            {
                true
            }
            Some(_) => return Err(
                "World resource prepared guard with old/mismatching primary; rollback ambiguous"
                    .into(),
            ),
            None if journal.stable == head => false,
            None => {
                return Err("World resource primary rolled back beyond independent guard".into());
            }
        };
        let mut value = Self {
            primary,
            guard,
            key: Zeroizing::new(key),
            _lock,
            head: origin.head,
            floor: PaidResourceReadIssuerFloorV1 {
                issuer_epoch: origin.pin.issuer_epoch,
                config_generation: origin.pin.issuer_config_generation,
            },
            pin: origin.pin,
            attempts: BTreeSet::new(),
            healthy: true,
            maximum_events,
            #[cfg(test)]
            test_fault: None,
        };
        for sequence in 1..=head.sequence {
            let event: Event = read(
                &value.primary,
                &value.key,
                &event_name(sequence),
                EVENT_BYTES,
            )?;
            if finalize && sequence == head.sequence && value.head != journal.stable {
                return Err(
                    "World resource prepared predecessor differs from permanent chain".into(),
                );
            }
            value.apply_replayed(&event)?;
            value.head = next_head(&value.head, &event)?;
        }
        if value.head != head {
            return Err("World resource permanent history differs from protected head".into());
        }
        if finalize {
            replace(
                &value.guard,
                &value.key,
                GUARD,
                &Guard {
                    stable: head,
                    prepared: None,
                },
            )?;
        }
        if configured != value.pin {
            value.append(EventKind::ConfiguredIssuer {
                pin: Box::new(configured),
            })?;
        }
        Ok(value)
    }
    /// Actual trusted configuration update only, never request JSON. The same
    /// durable event validator used by cold replay enforces both floors and
    /// stable namespace, while preserving every consumed physical attempt.
    pub fn advance_configured_issuer(&mut self, pin: PaidResourceReadIssuerPinV1) -> Result<()> {
        if !self.healthy {
            return Err("World resource ledger requires recovery".into());
        }
        if pin == self.pin {
            return Ok(());
        }
        self.append(EventKind::ConfiguredIssuer { pin: Box::new(pin) })?;
        Ok(())
    }
    pub fn validate_static_service_identity(
        &self,
        world: &oasis7_client_api::world_service::WorldIdentity,
        node_public_key: &str,
    ) -> Result<()> {
        if !self.healthy
            || world.world_id != self.pin.world_audience.world_id
            || world.genesis_digest != self.pin.world_audience.genesis_digest
            || node_public_key != self.pin.world_audience.service_public_key
            || self.pin.public_key == node_public_key
        {
            return Err("World resource fixed service identity".into());
        }
        Ok(())
    }
    /// Fixed server configuration binding, checked before a claim and again
    /// before delivery. A valid Account proof for a different World/Node is
    /// not permission for this service to read or sign any observation.
    pub fn validate_service_binding(
        &self,
        commit: &oasis7_client_api::world_service::CommitRef,
        node_public_key: &str,
    ) -> Result<()> {
        let world = &self.pin.world_audience;
        if !self.healthy
            || commit.world.world_id != world.world_id
            || commit.world.genesis_digest != world.genesis_digest
            || commit.binding.branch_id != world.branch_id
            || commit.binding.reorg_generation != world.reorg_epoch.0
            || node_public_key != world.service_public_key
        {
            return Err("World resource fixed service binding".into());
        }
        Ok(())
    }
    pub fn claim_owner_preflight(
        &mut self,
        proof: PaidResourceReadProofV1,
        request: ParticipantOwnerViewRequestV1,
    ) -> Result<ConsumedWorldResourceAttemptV1> {
        self.claim(
            proof,
            Body::OwnerPreflight {
                request: Box::new(request),
            },
        )
    }
    pub fn claim_resource_final(
        &mut self,
        proof: PaidResourceReadProofV1,
        request: OwnerSetupResourceWorldBodyV2,
    ) -> Result<ConsumedWorldResourceAttemptV1> {
        self.claim(
            proof,
            Body::ResourceFinal {
                request: Box::new(request),
            },
        )
    }
    /// Reenter after actual Current/archive work, before delivery. Historical
    /// claim bytes never bypass a live issuer/floor change or expiry.
    pub fn validate_delivery(&self, attempt: &ConsumedWorldResourceAttemptV1) -> Result<()> {
        if !self.healthy {
            return Err("World resource ledger requires recovery".into());
        }
        attempt
            .proof
            .verify_issuer(&self.pin, &self.floor, now()?)?;
        attempt.body.validate(&attempt.proof)?;
        if !self
            .attempts
            .contains(&attempt_key(&attempt.proof, attempt.body.substep())?)
        {
            return Err("World resource delivery has no permanent physical claim".into());
        }
        Ok(())
    }
    fn claim(
        &mut self,
        proof: PaidResourceReadProofV1,
        body: Body,
    ) -> Result<ConsumedWorldResourceAttemptV1> {
        let at = now()?;
        proof.verify_issuer(&self.pin, &self.floor, at)?;
        body.validate(&proof)?;
        let identity = attempt_key(&proof, body.substep())?;
        if self.attempts.contains(&identity) {
            return Err("World resource physical attempt already consumed".into());
        }
        // Move the exact originals into permanent storage before returning any
        // pin/quote/history capability. Unknown I/O is spent and not retryable.
        let event = EventKind::PhysicalAttempt {
            at_unix_ms: at,
            proof: Box::new(proof),
            body,
        };
        let stored = self.append(event)?;
        let EventKind::PhysicalAttempt { proof, body, .. } = stored.event else {
            unreachable!()
        };
        Ok(ConsumedWorldResourceAttemptV1 {
            proof: *proof,
            body,
        })
    }
    fn append(&mut self, event: EventKind) -> Result<Event> {
        if !self.healthy {
            return Err("World resource ledger requires explicit recovery".into());
        }
        let value = Event {
            api_version: 1,
            sequence: self
                .head
                .sequence
                .checked_add(1)
                .ok_or("World resource sequence overflow")?,
            previous_digest: self.head.digest.clone(),
            event,
        };
        // Validate all predicates without changing floor/once state first.
        self.check_event(&value)?;
        let next = next_head(&self.head, &value)?;
        let bytes = canonical(&value)?;
        if bytes.len() > EVENT_BYTES {
            return Err("World resource permanent event exceeds bound".into());
        }
        let committed = (|| {
            replace(
                &self.guard,
                &self.key,
                GUARD,
                &Guard {
                    stable: self.head.clone(),
                    prepared: Some(next.clone()),
                },
            )?;
            #[cfg(test)]
            self.test_fail(TestFault::AfterPrepared)?;
            if !storage::publish_bounded_record(
                &self.primary,
                &self.key,
                &event_name(value.sequence),
                MAGIC,
                &record_aad(&event_name(value.sequence))?,
                &bytes,
                EVENT_BYTES,
            )? {
                return Err("World resource sequence already occupied".into());
            }
            #[cfg(test)]
            self.test_fail(TestFault::AfterEvent)?;
            replace(&self.primary, &self.key, HEAD, &next)?;
            #[cfg(test)]
            self.test_fail(TestFault::AfterHead)?;
            replace(
                &self.guard,
                &self.key,
                GUARD,
                &Guard {
                    stable: next.clone(),
                    prepared: None,
                },
            )
        })();
        if let Err(error) = committed {
            self.healthy = false;
            return Err(error);
        }
        self.apply_replayed(&value)?;
        self.head = next;
        Ok(value)
    }
    #[cfg(test)]
    fn test_fail(&self, phase: TestFault) -> Result<()> {
        if self.test_fault == Some(phase) {
            Err("fixture durable I/O interruption".into())
        } else {
            Ok(())
        }
    }
    fn check_event(&self, event: &Event) -> Result<()> {
        if event.sequence > self.maximum_events {
            return Err("World resource permanent ledger storage capacity exhausted".into());
        }
        if event.api_version != 1
            || event.sequence
                != self
                    .head
                    .sequence
                    .checked_add(1)
                    .ok_or("World resource sequence overflow")?
            || event.previous_digest != self.head.digest
        {
            return Err("World resource event chain".into());
        }
        match &event.event {
            EventKind::ConfiguredIssuer { pin } => {
                validate_pin(pin)?;
                if issuer_namespace(pin)? != self.head.namespace
                    || pin.issuer_epoch.0 < self.floor.issuer_epoch.0
                    || pin.issuer_config_generation.0 < self.floor.config_generation.0
                    || (pin.issuer_epoch == self.floor.issuer_epoch
                        && pin.issuer_config_generation == self.floor.config_generation)
                {
                    return Err(
                        "World resource configured floor rollback or unversioned change".into(),
                    );
                }
            }
            EventKind::PhysicalAttempt {
                at_unix_ms,
                proof,
                body,
            } => {
                proof.verify_issuer(&self.pin, &self.floor, *at_unix_ms)?;
                body.validate(proof)?;
                if self.attempts.contains(&attempt_key(proof, body.substep())?) {
                    return Err("World resource permanent physical replay".into());
                }
            }
        }
        Ok(())
    }
    fn apply_replayed(&mut self, event: &Event) -> Result<()> {
        self.check_event(event)?;
        match &event.event {
            EventKind::ConfiguredIssuer { pin } => {
                self.floor = PaidResourceReadIssuerFloorV1 {
                    issuer_epoch: pin.issuer_epoch,
                    config_generation: pin.issuer_config_generation,
                };
                self.pin = (**pin).clone();
            }
            EventKind::PhysicalAttempt { proof, body, .. } => {
                self.attempts.insert(attempt_key(proof, body.substep())?);
            }
        }
        Ok(())
    }
}
fn configured_namespace(pin: &PaidResourceReadIssuerPinV1, origin: &Head) -> Result<String> {
    let namespace = issuer_namespace(pin)?;
    if namespace != origin.namespace {
        return Err("World resource configured namespace mismatch".into());
    }
    Ok(namespace)
}
fn next_head(previous: &Head, event: &Event) -> Result<Head> {
    Ok(Head {
        api_version: 1,
        namespace: previous.namespace.clone(),
        sequence: event.sequence,
        digest: digest("oasis7.world.resource-paid-event.v1", event)?,
    })
}
fn validate_pin(pin: &PaidResourceReadIssuerPinV1) -> Result<()> {
    if pin.issuer_epoch.0 == 0
        || pin.issuer_config_generation.0 == 0
        || pin.not_after.0 <= pin.not_before.0
        || pin.public_key.len() != 64
        || !pin
            .public_key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("World resource configured issuer pin".into());
    }
    Ok(())
}
fn separate(primary: &Dir, guard: &Dir) -> Result<()> {
    use cap_std::fs::MetadataExt;
    let a = primary.dir_metadata().map_err(|e| e.to_string())?;
    let b = guard.dir_metadata().map_err(|e| e.to_string())?;
    if a.dev() == b.dev() && a.ino() == b.ino() {
        return Err("World resource guard must be outside primary restore unit".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "resource_world_paid_ledger_v1_tests.rs"]
mod tests;
