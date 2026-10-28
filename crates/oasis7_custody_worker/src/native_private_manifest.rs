//! Monotone Identity-private manifest. Contains opaque references and digests only;
//! each referenced secret is encrypted in a separate immutable record DO.
use super::{NativePrivatePurpose, NativePrivateScope};
use oasis7_custody_core::provisioning::NativeStageResultV1;
use oasis7_custody_core::{DecimalU64, approval::bytes_digest};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, prelude::*};
type Result<T> = std::result::Result<T, JsValue>;
const KEY: &str = "custody-native-manifest-v1";
const MAX_BYTES: usize = 32 * 1024;
fn rejected() -> JsValue {
    JsValue::from_str("native private manifest rejected")
}
fn id(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
fn digest(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) use oasis7_custody_core::native_history::{
    NativeHistoryPhaseV1 as NativePhase, NativeHistoryScopeV1 as ManifestScope,
};
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PhaseState {
    Prepared,
    Dispatched,
    Unknown,
    Complete,
    ExplicitLoginRequired,
}
/// Exact fresh phase authority, constructed by the fixed Account adapter only.
/// Deliberately lacks Deserialize and cannot be supplied as a browser grant.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ManifestCurrent {
    pub scope: ManifestScope,
    pub phase: NativePhase,
    pub operation_id: String,
    pub permit_id: String,
    pub request_digest: String,
    pub authority_revision: String,
    pub expires_at: DecimalU64,
}
impl ManifestCurrent {
    fn check(&self, scope: &ManifestScope) -> Result<()> {
        scope.validate().map_err(|_| rejected())?;
        let now = js_sys::Date::now();
        if !now.is_finite()
            || now < 0.0
            || now.fract() != 0.0
            || now > 9_007_199_254_740_991.0
            || &self.scope != scope
            || !id(&self.operation_id)
            || !id(&self.permit_id)
            || !digest(&self.request_digest)
            || !digest(&self.authority_revision)
            || self.expires_at.0 <= now as u64
        {
            return Err(rejected());
        }
        if (self.phase == NativePhase::Register
            && self.operation_id != scope.registration_operation_id)
            || (self.phase == NativePhase::InitialLogin
                && self.operation_id != scope.initial_login_operation_id)
            || (self.phase == NativePhase::ManagedLogin
                && self.operation_id == scope.initial_login_operation_id)
        {
            return Err(rejected());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhaseRecord {
    phase: NativePhase,
    operation_id: String,
    permit_id: String,
    request_digest: String,
    state: PhaseState,
    recovery_attempts: Vec<String>,
    result: Option<NativeStageResultV1>,
    #[serde(default)]
    result_permit_id: Option<String>,
    result_private_sid_generation: Option<DecimalU64>,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PrivateRecordKind {
    Preauth,
    OptionsRequest,
    Challenge,
    CompleteRequest,
    BindingStatement,
    BindingReceipt,
    LoginDispatch,
    SessionCookie,
    SessionCsrf,
}
impl PrivateRecordKind {
    fn purpose(self) -> NativePrivatePurpose {
        match self {
            Self::Preauth => NativePrivatePurpose::Preauth,
            Self::SessionCookie | Self::SessionCsrf => NativePrivatePurpose::NativeSession,
            _ => NativePrivatePurpose::OriginalRequest,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SecretRef {
    operation_id: String,
    purpose: NativePrivatePurpose,
    kind: PrivateRecordKind,
    record_id: String,
    version: DecimalU64,
    plaintext_digest: String,
    stored: bool,
    /// Identity SID slot only; never compared to native status.generation.
    private_session_generation: DecimalU64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u8,
    scope: ManifestScope,
    revision: DecimalU64,
    private_session_generation: DecimalU64,
    phases: Vec<PhaseRecord>,
    records: Vec<SecretRef>,
}
impl Manifest {
    fn validate(&self, expected: &ManifestScope) -> Result<()> {
        expected.validate().map_err(|_| rejected())?;
        if self.schema_version != 1
            || self.scope != *expected
            || self.phases.len() > 16
            || self.records.len() > 64
        {
            return Err(rejected());
        }
        for (index, phase) in self.phases.iter().enumerate() {
            if phase.recovery_attempts.len() > 16
                || phase.recovery_attempts.iter().enumerate().any(|(i, id)| {
                    id == &phase.permit_id
                        || !self::id(id)
                        || phase.recovery_attempts[..i].contains(id)
                })
            {
                return Err(rejected());
            }
            if phase.request_digest
                != oasis7_custody_core::native_history::stable_native_intent_digest(
                    expected,
                    phase.phase,
                    &phase.operation_id,
                )
                .map_err(|_| rejected())?
            {
                return Err(rejected());
            }
            if !id(&phase.operation_id)
                || !id(&phase.permit_id)
                || !digest(&phase.request_digest)
                || self.phases[..index]
                    .iter()
                    .any(|old| old.operation_id == phase.operation_id)
                || (index == 0 && phase.phase != NativePhase::Register)
                || (phase.phase == NativePhase::Register
                    && (index != 0 || phase.operation_id != expected.registration_operation_id))
                || (phase.phase == NativePhase::Binding
                    && (index == 0
                        || self.phases[..index].iter().any(|p| {
                            matches!(
                                p.phase,
                                NativePhase::InitialLogin | NativePhase::ManagedLogin
                            )
                        })
                        || !matches!(
                            self.phases[index - 1].state,
                            PhaseState::Complete | PhaseState::Unknown
                        )))
                || (phase.phase == NativePhase::InitialLogin
                    && (index < 2
                        || phase.operation_id != expected.initial_login_operation_id
                        || self.phases[index - 1].phase != NativePhase::Binding
                        || self.phases[index - 1].state != PhaseState::Complete))
                || (phase.phase == NativePhase::ManagedLogin
                    && (index < 3 || phase.operation_id == expected.initial_login_operation_id))
            {
                return Err(rejected());
            }
        }
        let mut session_generation = 0_u64;
        for (index, record) in self.records.iter().enumerate() {
            let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.native-private-ref.v1",
                expected,
                &record.operation_id,
                record.kind,
                record.version,
            ))
            .map_err(|_| rejected())?;
            if record.record_id != bytes_digest(&bytes)
                || self.records[..index]
                    .iter()
                    .any(|old| old.operation_id == record.operation_id && old.kind == record.kind)
            {
                return Err(rejected());
            }
            if record.kind == PrivateRecordKind::SessionCookie {
                session_generation += 1;
                if record.private_session_generation.0 != session_generation {
                    return Err(rejected());
                }
            }
            if record.kind == PrivateRecordKind::SessionCsrf
                && !self.records[..index].iter().any(|old| {
                    old.operation_id == record.operation_id
                        && old.kind == PrivateRecordKind::SessionCookie
                        && old.stored
                        && old.private_session_generation == record.private_session_generation
                })
            {
                return Err(rejected());
            }
            if record.purpose != record.kind.purpose()
                || record.version.0 != index as u64 + 1
                || !digest(&record.record_id)
                || !digest(&record.plaintext_digest)
                || !self
                    .phases
                    .iter()
                    .any(|p| p.operation_id == record.operation_id)
                || (record.purpose == NativePrivatePurpose::NativeSession
                    && (record.private_session_generation.0 == 0
                        || record.private_session_generation.0 > self.private_session_generation.0))
                || (record.purpose != NativePrivatePurpose::NativeSession
                    && record.private_session_generation.0 != 0)
            {
                return Err(rejected());
            }
        }
        if self.private_session_generation.0 != session_generation {
            return Err(rejected());
        }
        for phase in &self.phases {
            if phase.state == PhaseState::Complete && phase.result.is_none() {
                return Err(rejected());
            }
            if let Some(result_permit_id) = &phase.result_permit_id {
                if !id(result_permit_id)
                    || phase.result.is_none()
                    || (result_permit_id != &phase.permit_id
                        && !phase.recovery_attempts.contains(result_permit_id))
                {
                    return Err(rejected());
                }
            }
            if let Some(result) = &phase.result {
                let expected_state = if matches!(result, NativeStageResultV1::ExplicitLoginRequired)
                {
                    PhaseState::ExplicitLoginRequired
                } else {
                    PhaseState::Complete
                };
                if phase.state != expected_state {
                    return Err(rejected());
                }
                match (phase.phase, result) {
                    (NativePhase::Register, NativeStageResultV1::RegistrationChallenge(fact)) => {
                        fact.validate().map_err(|_| rejected())?
                    }
                    (NativePhase::Binding, NativeStageResultV1::BindingReceipt(fact)) => {
                        fact.validate().map_err(|_| rejected())?
                    }
                    (
                        NativePhase::InitialLogin | NativePhase::ManagedLogin,
                        NativeStageResultV1::SessionStatus(fact),
                    ) => {
                        fact.validate().map_err(|_| rejected())?;
                        let sid = self
                            .records
                            .iter()
                            .find(|r| {
                                r.operation_id == phase.operation_id
                                    && r.kind == PrivateRecordKind::SessionCookie
                                    && r.stored
                            })
                            .ok_or_else(rejected)?;
                        if phase.result_private_sid_generation
                            != Some(sid.private_session_generation)
                            || !self.records.iter().any(|r| {
                                r.operation_id == phase.operation_id
                                    && r.kind == PrivateRecordKind::SessionCsrf
                                    && r.stored
                                    && r.private_session_generation
                                        == sid.private_session_generation
                            })
                        {
                            return Err(rejected());
                        }
                    }
                    (
                        NativePhase::InitialLogin | NativePhase::ManagedLogin,
                        NativeStageResultV1::ExplicitLoginRequired,
                    ) => {}
                    _ => return Err(rejected()),
                }
                if !matches!(result, NativeStageResultV1::SessionStatus(_))
                    && phase.result_private_sid_generation.is_some()
                {
                    return Err(rejected());
                }
            } else if phase.result_private_sid_generation.is_some() {
                return Err(rejected());
            }
        }
        Ok(())
    }
    fn secret_scope(&self, record: &SecretRef) -> NativePrivateScope {
        NativePrivateScope {
            role_scope: self.scope.role_scope.clone(),
            identity_namespace: self.scope.identity_namespace.clone(),
            identity_instance_epoch: self.scope.identity_instance_epoch,
            record_id: record.record_id.clone(),
            record_version: record.version,
            purpose: record.purpose,
        }
    }
}
fn decode(raw: &str, scope: &ManifestScope) -> Result<Manifest> {
    if raw.len() > MAX_BYTES {
        return Err(rejected());
    }
    let manifest: Manifest = serde_json::from_str(raw).map_err(|_| rejected())?;
    manifest.validate(scope)?;
    Ok(manifest)
}
fn encode(manifest: &Manifest) -> Result<String> {
    manifest.validate(&manifest.scope)?;
    let raw = serde_json::to_string(manifest).map_err(|_| rejected())?;
    if raw.len() > MAX_BYTES {
        return Err(rejected());
    }
    Ok(raw)
}
#[wasm_bindgen(module = "/platform/identity_native_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readNativeManifest)]
    fn read_manifest(state: &JsValue) -> Result<String>;
}
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = accountTransition)]
    fn update(
        state: &JsValue,
        key: &str,
        initial: &str,
        transition: &js_sys::Function,
    ) -> Result<String>;
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    records: Vec<Row>,
    table_names: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: String,
    payload: String,
}
pub(crate) struct NativeHistoryLookup<'a> {
    pub environment: &'a str,
    pub account_id: &'a str,
    pub provision_id: &'a str,
    pub result_permit_id: &'a str,
    pub phase: NativePhase,
    pub native_operation_id: Option<&'a str>,
}
#[derive(Clone)]
pub(crate) struct NativeSessionReadRefs {
    pub cookie: NativePrivateScope,
    pub cookie_digest: String,
    pub csrf: NativePrivateScope,
    pub csrf_digest: String,
    pub private_sid_generation: DecimalU64,
}
#[derive(Clone)]
pub(crate) struct NativeHistorySnapshot {
    pub scope: ManifestScope,
    pub phase: NativePhase,
    pub native_operation_id: String,
    pub original_permit_id: String,
    pub result_permit_id: String,
    pub stable_intent_digest: String,
    pub result: NativeStageResultV1,
    pub private_sid_generation: Option<DecimalU64>,
    pub session_refs: Option<NativeSessionReadRefs>,
}
pub(crate) struct NativeManifestStore {
    state: JsValue,
}
impl NativeManifestStore {
    /// Private metadata only. Never decrypts, initializes, renews or grants.
    pub(crate) fn current_private_session_generation(&self) -> Result<DecimalU64> {
        let inventory: Inventory =
            serde_json::from_str(&read_manifest(&self.state)?).map_err(|_| rejected())?;
        if inventory.records.len() != 1
            || inventory.records[0].id != KEY
            || inventory.records[0].payload.len() > MAX_BYTES
            || inventory.table_names.iter().any(|n| {
                !matches!(
                    n.as_str(),
                    "table:custody_records"
                        | "table:custody_audit"
                        | "table:sqlite_sequence"
                        | "table:_cf_METADATA"
                )
            })
        {
            return Err(rejected());
        }
        let manifest: Manifest =
            serde_json::from_str(&inventory.records[0].payload).map_err(|_| rejected())?;
        manifest.validate(&manifest.scope)?;
        if manifest.private_session_generation.0 == 0 {
            return Err(rejected());
        }
        Ok(manifest.private_session_generation)
    }
    pub(crate) fn new(state: JsValue) -> Self {
        Self { state }
    }
    fn transition<F, T>(
        &self,
        current: F,
        mut action: impl FnMut(&mut Manifest, &ManifestCurrent) -> Result<T>,
    ) -> Result<T>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        let before = current()?;
        before.check(&before.scope)?;
        let initial = encode(&Manifest {
            schema_version: 1,
            scope: before.scope.clone(),
            revision: DecimalU64(0),
            private_session_generation: DecimalU64(0),
            phases: vec![],
            records: vec![],
        })?;
        let mut returned = None;
        let mut callback = |raw: String, inventory: String| -> Result<js_sys::Array> {
            let inventory: Inventory = serde_json::from_str(&inventory).map_err(|_| rejected())?;
            if inventory.records.len() != 1
                || inventory.records[0].id != KEY
                || inventory.records[0].payload != raw
                || inventory.table_names.iter().any(|name| {
                    !matches!(
                        name.as_str(),
                        "table:custody_records"
                            | "table:custody_audit"
                            | "table:sqlite_sequence"
                            | "table:_cf_METADATA"
                    )
                })
            {
                return Err(rejected());
            }
            let after = current()?;
            after.check(&before.scope)?;
            if after != before {
                return Err(rejected());
            }
            let mut manifest = decode(&raw, &before.scope)?;
            returned = Some(action(&mut manifest, &after)?);
            let unchanged = encode(&manifest)? == raw;
            let result = js_sys::Array::new();
            if unchanged {
                for value in [raw.as_str(), "", ""] {
                    result.push(&value.into());
                }
            } else {
                manifest.revision.0 = manifest.revision.0.checked_add(1).ok_or_else(rejected)?;
                let next = encode(&manifest)?;
                let audit = serde_json::to_string(&(
                    manifest.revision,
                    "native_private_manifest_advanced",
                    &after.operation_id,
                ))
                .map_err(|_| rejected())?;
                result.push(&next.into());
                result.push(&manifest.revision.0.to_string().into());
                result.push(&audit.into());
            }
            Ok(result)
        };
        let closure = Closure::borrow_mut(&mut callback);
        update(&self.state, KEY, &initial, closure.as_ref().unchecked_ref())?;
        drop(closure);
        returned.ok_or_else(rejected)
    }
    pub(crate) fn claim_phase<F>(&self, current: F) -> Result<PhaseState>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        self.transition(current, |manifest, authority| {
            if let Some(old) = manifest
                .phases
                .iter()
                .find(|p| p.operation_id == authority.operation_id)
            {
                if old.phase != authority.phase || old.request_digest != authority.request_digest {
                    return Err(rejected());
                }
                return Ok(old.state);
            }
            if let Some(previous) = manifest.phases.last_mut() {
                if authority.phase == NativePhase::Binding
                    && previous.phase == NativePhase::Binding
                    && previous.state == PhaseState::Dispatched
                {
                    previous.state = PhaseState::Unknown;
                }
            }
            if manifest.phases.last().is_some_and(|p| {
                !(matches!(
                    p.state,
                    PhaseState::Complete | PhaseState::ExplicitLoginRequired
                ) || (p.phase == NativePhase::Binding
                    && authority.phase == NativePhase::Binding
                    && p.state == PhaseState::Unknown))
            }) {
                return Err(rejected());
            }
            manifest.phases.push(PhaseRecord {
                phase: authority.phase,
                operation_id: authority.operation_id.clone(),
                permit_id: authority.permit_id.clone(),
                request_digest: authority.request_digest.clone(),
                state: PhaseState::Prepared,
                recovery_attempts: vec![],
                result: None,
                result_permit_id: None,
                result_private_sid_generation: None,
            });
            manifest.validate(&authority.scope)?;
            Ok(PhaseState::Prepared)
        })
    }
    pub(crate) fn set_phase_state<F>(&self, current: F, next: PhaseState) -> Result<()>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        self.transition(current, |manifest, authority| {
            let phase = manifest
                .phases
                .iter_mut()
                .find(|p| p.operation_id == authority.operation_id)
                .ok_or_else(rejected)?;
            if phase.phase != authority.phase || phase.request_digest != authority.request_digest {
                return Err(rejected());
            }
            let allowed = match (phase.state, next) {
                (PhaseState::Prepared, PhaseState::Dispatched)
                | (PhaseState::Dispatched, PhaseState::Complete) => true,
                (PhaseState::Prepared, PhaseState::Complete) => phase.phase == NativePhase::Binding,
                (PhaseState::Dispatched, PhaseState::ExplicitLoginRequired) => matches!(
                    phase.phase,
                    NativePhase::InitialLogin | NativePhase::ManagedLogin
                ),
                (PhaseState::Unknown, PhaseState::Complete) => !matches!(
                    phase.phase,
                    NativePhase::InitialLogin | NativePhase::ManagedLogin
                ),
                (PhaseState::Dispatched, PhaseState::Unknown) => !matches!(
                    phase.phase,
                    NativePhase::InitialLogin | NativePhase::ManagedLogin
                ),
                _ => phase.state == next && next != PhaseState::Dispatched,
            };
            if !allowed {
                return Err(rejected());
            }
            if next == PhaseState::Complete
                && matches!(
                    phase.phase,
                    NativePhase::InitialLogin | NativePhase::ManagedLogin
                )
                && ![
                    PrivateRecordKind::SessionCookie,
                    PrivateRecordKind::SessionCsrf,
                ]
                .iter()
                .all(|kind| {
                    manifest.records.iter().any(|r| {
                        r.operation_id == authority.operation_id && r.kind == *kind && r.stored
                    })
                })
            {
                return Err(rejected());
            }
            phase.state = next;
            Ok(())
        })
    }
    /// A record ID/version is derived inside the manifest transaction. Callers
    /// supply only the closed record kind and digest of exact bytes determined by the phase.
    pub(crate) fn reserve_record<F>(
        &self,
        current: F,
        kind: PrivateRecordKind,
        plaintext_digest: &str,
    ) -> Result<NativePrivateScope>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        let purpose = kind.purpose();
        if !digest(plaintext_digest) {
            return Err(rejected());
        }
        self.transition(current, |manifest, authority| {
            let phase = manifest
                .phases
                .iter()
                .find(|p| p.operation_id == authority.operation_id)
                .ok_or_else(rejected)?;
            if phase.phase != authority.phase
                || phase.request_digest != authority.request_digest
                || (matches!(
                    phase.state,
                    PhaseState::Unknown | PhaseState::ExplicitLoginRequired | PhaseState::Complete
                ) && !(phase.state == PhaseState::Unknown
                    && phase.phase == NativePhase::Register
                    && phase.recovery_attempts.last() == Some(&authority.permit_id)))
            {
                return Err(rejected());
            }
            if let Some(original) = manifest
                .records
                .iter()
                .find(|r| r.operation_id == authority.operation_id && r.kind == kind)
            {
                if original.plaintext_digest != plaintext_digest {
                    return Err(rejected());
                }
                return Ok(manifest.secret_scope(original));
            }
            if purpose == NativePrivatePurpose::NativeSession
                && (!matches!(
                    phase.phase,
                    NativePhase::InitialLogin | NativePhase::ManagedLogin
                ) || phase.state != PhaseState::Dispatched)
            {
                return Err(rejected());
            }
            let version = DecimalU64(manifest.records.len() as u64 + 1);
            let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7.custody.native-private-ref.v1",
                &authority.scope,
                &authority.operation_id,
                kind,
                version,
            ))
            .map_err(|_| rejected())?;
            let generation = if kind == PrivateRecordKind::SessionCookie {
                manifest.private_session_generation.0 = manifest
                    .private_session_generation
                    .0
                    .checked_add(1)
                    .ok_or_else(rejected)?;
                manifest.private_session_generation
            } else if kind == PrivateRecordKind::SessionCsrf {
                let cookie = manifest
                    .records
                    .iter()
                    .find(|r| {
                        r.operation_id == authority.operation_id
                            && r.kind == PrivateRecordKind::SessionCookie
                            && r.stored
                    })
                    .ok_or_else(rejected)?;
                cookie.private_session_generation
            } else {
                DecimalU64(0)
            };
            let record = SecretRef {
                operation_id: authority.operation_id.clone(),
                purpose,
                kind,
                record_id: bytes_digest(&bytes),
                version,
                plaintext_digest: plaintext_digest.to_owned(),
                stored: false,
                private_session_generation: generation,
            };
            let scope = manifest.secret_scope(&record);
            manifest.records.push(record);
            Ok(scope)
        })
    }
    pub(crate) fn record_stored<F>(&self, current: F, expected: &NativePrivateScope) -> Result<()>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        self.transition(current, |manifest, authority| {
            let index = manifest
                .records
                .iter()
                .position(|r| {
                    r.operation_id == authority.operation_id && r.record_id == expected.record_id
                })
                .ok_or_else(rejected)?;
            if manifest.secret_scope(&manifest.records[index]) != *expected {
                return Err(rejected());
            }
            manifest.records[index].stored = true;
            Ok(())
        })
    }
    /// Call on restart before any native send. An absent original SID is terminal
    /// for this login operation; a stored SID can only proceed to status verification.
    pub(crate) fn recover_phase<F>(&self, current: F) -> Result<PhaseState>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        self.transition(current, |manifest, authority| {
            let index = manifest
                .phases
                .iter()
                .position(|p| p.operation_id == authority.operation_id)
                .ok_or_else(rejected)?;
            if manifest.phases[index].phase != authority.phase
                || manifest.phases[index].request_digest != authority.request_digest
            {
                return Err(rejected());
            }
            if manifest.phases[index].state == PhaseState::Dispatched {
                let login = matches!(
                    authority.phase,
                    NativePhase::InitialLogin | NativePhase::ManagedLogin
                );
                let stored = manifest.records.iter().any(|r| {
                    r.operation_id == authority.operation_id
                        && r.kind == PrivateRecordKind::SessionCookie
                        && r.stored
                });
                if login && !stored {
                    manifest.phases[index].state = PhaseState::ExplicitLoginRequired;
                } else if !login {
                    manifest.phases[index].state = PhaseState::Unknown;
                }
            }
            Ok(manifest.phases[index].state)
        })
    }
    pub(crate) fn record_reference<F>(
        &self,
        current: F,
        kind: PrivateRecordKind,
    ) -> Result<Option<(NativePrivateScope, String, bool)>>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        self.transition(current, |manifest, authority| {
            let phase = manifest
                .phases
                .iter()
                .find(|p| p.operation_id == authority.operation_id)
                .ok_or_else(rejected)?;
            if phase.phase != authority.phase || phase.request_digest != authority.request_digest {
                return Err(rejected());
            }
            Ok(manifest
                .records
                .iter()
                .find(|r| r.operation_id == authority.operation_id && r.kind == kind)
                .map(|r| {
                    (
                        manifest.secret_scope(r),
                        r.plaintext_digest.clone(),
                        r.stored,
                    )
                }))
        })
    }
    pub(crate) fn registration_record_reference<F>(
        &self,
        current: F,
        kind: PrivateRecordKind,
    ) -> Result<Option<(NativePrivateScope, String, bool)>>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        self.transition(current, |manifest, _| {
            Ok(manifest
                .records
                .iter()
                .find(|r| {
                    r.operation_id == manifest.scope.registration_operation_id && r.kind == kind
                })
                .map(|r| {
                    (
                        manifest.secret_scope(r),
                        r.plaintext_digest.clone(),
                        r.stored,
                    )
                }))
        })
    }
    pub(crate) fn sid_generation<F>(&self, current: F) -> Result<DecimalU64>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        self.transition(current, |manifest, authority| {
            manifest
                .records
                .iter()
                .find(|r| {
                    r.operation_id == authority.operation_id
                        && r.kind == PrivateRecordKind::SessionCookie
                        && r.stored
                })
                .map(|r| r.private_session_generation)
                .ok_or_else(rejected)
        })
    }
    /// Fresh Account reservation only. Unknown originals never restore the old
    /// attempt's send right; a new exact attempt is consumed once and retained.
    pub(crate) fn claim_registration_recovery<F>(&self, current: F) -> Result<()>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        self.transition(current, |manifest, authority| {
            let phase = manifest
                .phases
                .iter_mut()
                .find(|p| p.operation_id == authority.operation_id)
                .ok_or_else(rejected)?;
            if authority.phase != NativePhase::Register
                || phase.phase != NativePhase::Register
                || phase.state != PhaseState::Unknown
                || phase.request_digest != authority.request_digest
                || phase.permit_id == authority.permit_id
                || phase.recovery_attempts.contains(&authority.permit_id)
            {
                return Err(rejected());
            }
            phase.recovery_attempts.push(authority.permit_id.clone());
            Ok(())
        })
    }
    /// Persist the exact public root fact before attempting Account submission.
    /// A completed local phase only re-submits this immutable fact after restart.
    pub(crate) fn persist_result<F>(
        &self,
        current: F,
        result: &NativeStageResultV1,
        private_sid_generation: Option<DecimalU64>,
    ) -> Result<()>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        self.transition(current, |manifest, authority| {
            let index = manifest
                .phases
                .iter()
                .position(|p| p.operation_id == authority.operation_id)
                .ok_or_else(rejected)?;
            let phase = &manifest.phases[index];
            if phase.phase != authority.phase || phase.request_digest != authority.request_digest {
                return Err(rejected());
            }
            if let Some(original) = &phase.result {
                return if original == result
                    && phase.result_private_sid_generation == private_sid_generation
                {
                    Ok(())
                } else {
                    Err(rejected())
                };
            }
            match result {
                NativeStageResultV1::SessionStatus(_) => {
                    let sid = manifest
                        .records
                        .iter()
                        .find(|r| {
                            r.operation_id == authority.operation_id
                                && r.kind == PrivateRecordKind::SessionCookie
                                && r.stored
                        })
                        .ok_or_else(rejected)?;
                    if private_sid_generation != Some(sid.private_session_generation) {
                        return Err(rejected());
                    }
                }
                _ if private_sid_generation.is_some() => return Err(rejected()),
                _ => {}
            }
            let next = if matches!(result, NativeStageResultV1::ExplicitLoginRequired) {
                PhaseState::ExplicitLoginRequired
            } else {
                PhaseState::Complete
            };
            let phase = &mut manifest.phases[index];
            phase.state = next;
            phase.result = Some(result.clone());
            phase.result_permit_id = Some(authority.permit_id.clone());
            phase.result_private_sid_generation = private_sid_generation;
            Ok(())
        })
    }
    pub(crate) fn completed_result<F>(
        &self,
        current: F,
    ) -> Result<Option<(NativeStageResultV1, Option<DecimalU64>)>>
    where
        F: Fn() -> Result<ManifestCurrent>,
    {
        self.transition(current, |manifest, authority| {
            let phase = manifest
                .phases
                .iter()
                .find(|p| p.operation_id == authority.operation_id)
                .ok_or_else(rejected)?;
            if phase.phase != authority.phase || phase.request_digest != authority.request_digest {
                return Err(rejected());
            }
            Ok(phase
                .result
                .clone()
                .map(|result| (result, phase.result_private_sid_generation)))
        })
    }
    /// Pure historical read: no initialization, authority issuance, crypto or writes.
    /// Stored scope validates local metadata; Account must independently match and
    /// verify this public fact against its own original plan and Root pins.
    pub(crate) fn read_completed_history(
        &self,
        lookup: &NativeHistoryLookup<'_>,
    ) -> Result<Option<NativeHistorySnapshot>> {
        let raw = read_manifest(&self.state)?;
        let inventory: Inventory = serde_json::from_str(&raw).map_err(|_| rejected())?;
        if inventory.table_names.iter().any(|n| {
            !matches!(
                n.as_str(),
                "table:custody_records"
                    | "table:custody_audit"
                    | "table:sqlite_sequence"
                    | "table:_cf_METADATA"
            )
        }) {
            return Err(rejected());
        }
        if inventory.records.is_empty() {
            return Ok(None);
        }
        if inventory.records.len() != 1
            || inventory.records[0].id != KEY
            || inventory.records[0].payload.len() > MAX_BYTES
        {
            return Err(rejected());
        }
        let manifest: Manifest =
            serde_json::from_str(&inventory.records[0].payload).map_err(|_| rejected())?;
        manifest.validate(&manifest.scope)?;
        let role = &manifest.scope.role_scope;
        if role.context.identity.environment != lookup.environment
            || role.context.identity.account_id != lookup.account_id
            || role.provision_id != lookup.provision_id
        {
            return Err(rejected());
        }
        let matching: Vec<_> = manifest
            .phases
            .iter()
            .filter(|phase| {
                let permit_id = phase.result_permit_id.as_deref().or({
                    if phase.recovery_attempts.is_empty() {
                        Some(phase.permit_id.as_str())
                    } else {
                        None
                    }
                });
                phase.result.is_some() && permit_id == Some(lookup.result_permit_id)
            })
            .collect();
        if matching.len() > 1 {
            return Err(rejected());
        }
        let Some(phase) = matching.first() else {
            return Ok(None);
        };
        let expected_native_operation = match lookup.phase {
            NativePhase::Register => Some(manifest.scope.registration_operation_id.as_str()),
            NativePhase::InitialLogin => Some(manifest.scope.initial_login_operation_id.as_str()),
            _ => lookup.native_operation_id,
        };
        if phase.phase != lookup.phase
            || expected_native_operation.is_some_and(|id| id != phase.operation_id)
        {
            return Err(rejected());
        }
        let result = phase.result.clone().ok_or_else(rejected)?;
        let session_refs = if let Some(generation) = phase.result_private_sid_generation {
            let find = |kind| {
                manifest
                    .records
                    .iter()
                    .find(|r| {
                        r.operation_id == phase.operation_id
                            && r.kind == kind
                            && r.stored
                            && r.private_session_generation == generation
                    })
                    .ok_or_else(rejected)
            };
            let cookie = find(PrivateRecordKind::SessionCookie)?;
            let csrf = find(PrivateRecordKind::SessionCsrf)?;
            Some(NativeSessionReadRefs {
                cookie: manifest.secret_scope(cookie),
                cookie_digest: cookie.plaintext_digest.clone(),
                csrf: manifest.secret_scope(csrf),
                csrf_digest: csrf.plaintext_digest.clone(),
                private_sid_generation: generation,
            })
        } else {
            None
        };
        Ok(Some(NativeHistorySnapshot {
            scope: manifest.scope.clone(),
            phase: phase.phase,
            native_operation_id: phase.operation_id.clone(),
            original_permit_id: phase.permit_id.clone(),
            result_permit_id: lookup.result_permit_id.into(),
            stable_intent_digest: phase.request_digest.clone(),
            result,
            private_sid_generation: phase.result_private_sid_generation,
            session_refs,
        }))
    }
}
