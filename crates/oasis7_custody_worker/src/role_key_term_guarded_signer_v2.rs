//! One already-paid Owner Actor edge; no ordinary authority callbacks.
use super::*;
use ed25519_dalek::Signer;
use oasis7_agent_api::game::{GAME_TASK_STOP_DOMAIN_V2, SignedGameTaskStopV2};
use oasis7_custody_core::{
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
        AccountMachinePurposeV2,
    },
    provisioning::MAX_TEST_SESSION_AUTHORITIES_V2,
    term_guarded_signer_v2::SignedTermGuardedStopSignerPermitV2,
};
use worker::Env;
const MAX_ROWS: usize = MAX_TEST_SESSION_AUTHORITIES_V2;
const MAX_DUAL_ROWS: usize = MAX_TEST_SESSION_AUTHORITIES_V2 * 2;
#[path = "role_key_term_guarded_close_v2.rs"]
mod close;
#[path = "role_key_term_guarded_qualification_v2.rs"]
mod qualification;
pub(crate) use qualification::TermOwnerSlotQualificationRequestV2;
const ROW_BYTES: usize = 32768;
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct TermGuardedSignerConfigurationV2 {
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    close: Option<(AccountMachineIssuerPinV2, AccountMachineIssuerFloorV2)>,
}
impl TermGuardedSignerConfigurationV2 {
    pub(crate) fn new(
        pin: AccountMachineIssuerPinV2,
        floor: AccountMachineIssuerFloorV2,
    ) -> Result<Self> {
        let issuer = AccountMachineIssuerV2 {
            namespace: pin.namespace.clone(),
            service_id: pin.service_id.clone(),
            environment: pin.environment.clone(),
            key_id: pin.key_id.clone(),
            issuer_epoch: pin.issuer_epoch,
            config_generation: pin.config_generation,
            native_audience_origin: pin.native_audience_origin.clone(),
        };
        issuer
            .verify_pin(
                &pin,
                &floor,
                AccountMachinePurposeV2::TermGuardedStopSignerV2,
                pin.not_before.0,
                pin.not_after.0,
                pin.not_before.0,
            )
            .map_err(|_| rejected())?;
        Ok(Self {
            pin,
            floor,
            close: None,
        })
    }
    pub(crate) fn with_close(
        mut self,
        pin: AccountMachineIssuerPinV2,
        floor: AccountMachineIssuerFloorV2,
    ) -> Result<Self> {
        let issuer = AccountMachineIssuerV2 {
            namespace: pin.namespace.clone(),
            service_id: pin.service_id.clone(),
            environment: pin.environment.clone(),
            key_id: pin.key_id.clone(),
            issuer_epoch: pin.issuer_epoch,
            config_generation: pin.config_generation,
            native_audience_origin: pin.native_audience_origin.clone(),
        };
        issuer
            .verify_pin(
                &pin,
                &floor,
                AccountMachinePurposeV2::TermGuardedCloseSignerV2,
                pin.not_before.0,
                pin.not_after.0,
                pin.not_before.0,
            )
            .map_err(|_| rejected())?;
        if pin.namespace != self.pin.namespace
            || pin.service_id != self.pin.service_id
            || pin.environment != self.pin.environment
            || pin.key_id != self.pin.key_id
            || pin.public_key != self.pin.public_key
            || pin.issuer_epoch != self.pin.issuer_epoch
            || pin.config_generation != self.pin.config_generation
            || pin.native_audience_origin != self.pin.native_audience_origin
        {
            return Err(rejected());
        }
        self.close = Some((pin, floor));
        Ok(self)
    }
    fn maximum(&self) -> usize {
        if self.close.is_some() {
            MAX_DUAL_ROWS
        } else {
            MAX_ROWS
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    schema_version: u8,
    scope: RoleKeySlotScopeV1,
    public_key: PublicRoleKey,
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    created_at: DecimalU64,
    maximum_claims: DecimalU64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    close_pin: Option<AccountMachineIssuerPinV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    close_floor: Option<AccountMachineIssuerFloorV2>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    id: String,
    permit: SignedTermGuardedStopSignerPermitV2,
    claimed_at: DecimalU64,
    #[serde(deserialize_with = "required")]
    signed_at: Option<DecimalU64>,
    #[serde(deserialize_with = "required")]
    original: Option<SignedGameTaskStopV2>,
}
fn required<'de, D, T>(d: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(d)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRow {
    id: String,
    payload: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TermInventory {
    role: Vec<Row>,
    tables: Vec<String>,
    metadata: Option<Vec<StoredRow>>,
    claims: Option<Vec<StoredRow>>,
    qualifications: Option<Vec<StoredRow>>,
}
struct Checked {
    slot: RoleKeySlotV1,
    claims: Vec<Claim>,
    qualifications: Vec<qualification::Claim>,
    clock_floor: u64,
    close_claims: Vec<close::Claim>,
    lineage: Option<oasis7_custody_core::term_owner_slot_lineage_v2::TermOwnerSlotLineageV2>,
}
fn clock() -> Result<u64> {
    let n = now()?;
    if n == 0 { Err(rejected()) } else { Ok(n) }
}
fn json<T: Serialize>(value: &T) -> Result<String> {
    let raw = serde_json::to_string(value).map_err(|_| rejected())?;
    if raw.len() > ROW_BYTES {
        return Err(rejected());
    }
    Ok(raw)
}
fn strict<T: for<'de> Deserialize<'de> + Serialize>(raw: &str) -> Result<T> {
    if raw.is_empty() || raw.len() > ROW_BYTES {
        return Err(rejected());
    }
    let v: T = serde_json::from_str(raw).map_err(|_| rejected())?;
    let original: serde_json::Value = serde_json::from_str(raw).map_err(|_| rejected())?;
    if serde_json::to_value(&v).map_err(|_| rejected())? != original {
        return Err(rejected());
    }
    Ok(v)
}
fn id(permit: &SignedTermGuardedStopSignerPermitV2) -> Result<String> {
    let p = &permit.payload;
    oasis7_wasm_abi::canonical_hash(&(
        "oasis7.custody.term-owner-signer-once.v2",
        &p.issuer.namespace,
        &p.issuer.service_id,
        &p.issuer.environment,
        &p.paid_attempt_id,
        &p.recipient_path,
        &p.owner_scope,
    ))
    .map_err(|_| rejected())
}
fn check_original(c: &Claim) -> Result<()> {
    match (&c.original, c.signed_at) {
        (None, None) => Ok(()),
        (Some(original), Some(at)) => {
            original.validate_shape().map_err(|_| rejected())?;
            if original.request != c.permit.payload.original
                || at.0 < c.claimed_at.0
                || at.0 >= c.permit.payload.expires_at.0
            {
                return Err(rejected());
            }
            oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
                GAME_TASK_STOP_DOMAIN_V2,
                &original.request,
                &original.subject_public_key,
                &original.signature_hex,
            )
            .map_err(|_| rejected())
        }
        _ => Err(rejected()),
    }
}
fn check_inventory(raw: &str, config: &TermGuardedSignerConfigurationV2) -> Result<Checked> {
    // SQL bounds each row and caps scan at existing Account max+1. Reject any
    // missing/partial state; never CREATE or infer an empty history here.
    if raw.len() > (MAX_DUAL_ROWS + MAX_ROWS + 1) * (ROW_BYTES * 2 + 1024) {
        return Err(rejected());
    }
    let v: TermInventory = serde_json::from_str(raw).map_err(|_| rejected())?;
    let role = Inventory {
        records: v.role,
        table_names: v.tables,
    };
    inventory_value(&role)?;
    let snapshot = decode(&role.records[0].payload)?;
    let slot = snapshot.slot.ok_or_else(rejected)?;
    if slot.scope.context.identity.role != Role::Owner {
        return Err(rejected());
    }
    let metadata = v.metadata.ok_or_else(rejected)?;
    if metadata.len() != 1 || metadata[0].id != "singleton" {
        return Err(rejected());
    }
    let meta: Metadata = strict(&metadata[0].payload)?;
    if meta.schema_version != if config.close.is_some() { 2 } else { 1 }
        || meta.scope != slot.scope
        || meta.public_key != slot.public_ref
        || meta.pin != config.pin
        || meta.floor != config.floor
        || meta.created_at.0 == 0
        || meta.maximum_claims.0 != u64::try_from(config.maximum()).map_err(|_| rejected())?
        || meta.close_pin.as_ref() != config.close.as_ref().map(|v| &v.0)
        || meta.close_floor.as_ref() != config.close.as_ref().map(|v| &v.1)
        || meta.pin.public_key == slot.public_ref.public_key
    {
        return Err(rejected());
    }
    let rows = v.claims.ok_or_else(rejected)?;
    if rows.len() > config.maximum() {
        return Err(rejected());
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut claims = Vec::new();
    let mut close_claims = Vec::new();
    let mut qualifications = Vec::new();
    let mut attempts = std::collections::BTreeSet::new();
    let mut lineage = None;
    let mut floor = meta.created_at.0;
    for row in rows {
        if let Ok(claim) = strict::<close::Claim>(&row.payload) {
            let (pin, pinned_floor) = config.close.as_ref().ok_or_else(rejected)?;
            close::check_claim(&claim, &slot, pin, pinned_floor)?;
            if row.id != claim.id
                || !ids.insert(claim.id.clone())
                || !attempts.insert(claim.permit.payload.paid_attempt_id.clone())
                || claim.claimed_at.0 < meta.created_at.0
            {
                return Err(rejected());
            }
            let observed = claim
                .permit
                .payload
                .account_lineage
                .as_ref()
                .ok_or_else(rejected)?;
            if lineage.as_ref().is_some_and(|v| v != observed) {
                return Err(rejected());
            }
            lineage = Some(observed.clone());
            floor = floor
                .max(claim.claimed_at.0)
                .max(claim.signed_at.map_or(0, |v| v.0));
            close_claims.push(claim);
            continue;
        }
        let claim: Claim = strict(&row.payload)?;
        if !attempts.insert(claim.permit.payload.paid_attempt_id.clone()) {
            return Err(rejected());
        }
        if config.close.is_some() {
            let observed = claim
                .permit
                .payload
                .account_lineage
                .as_ref()
                .ok_or_else(rejected)?;
            if lineage.as_ref().is_some_and(|v| v != observed) {
                return Err(rejected());
            }
            lineage = Some(observed.clone());
        }
        if row.id != claim.id
            || claim.id != id(&claim.permit)?
            || !ids.insert(claim.id.clone())
            || claim.claimed_at.0 < meta.created_at.0
        {
            return Err(rejected());
        }
        claim
            .permit
            .verify(
                &config.pin,
                &config.floor,
                &slot.scope,
                &slot.public_ref,
                claim.claimed_at.0,
            )
            .map_err(|_| rejected())?;
        check_original(&claim)?;
        floor = floor
            .max(claim.claimed_at.0)
            .max(claim.signed_at.map_or(0, |v| v.0));
        claims.push(claim);
    }
    match (config.close.as_ref(), v.qualifications) {
        (None, None) => (),
        (Some(_), Some(rows)) if rows.len() <= MAX_ROWS => {
            for row in rows {
                let c: qualification::Claim = strict(&row.payload)?;
                qualification::check_claim(&c, &slot, config)?;
                if row.id != c.id
                    || !ids.insert(c.id.clone())
                    || !attempts.insert(c.request.paid_attempt_id.clone())
                    || c.claimed_at.0 < meta.created_at.0
                {
                    return Err(rejected());
                }
                floor = floor.max(c.claimed_at.0);
                qualifications.push(c);
            }
        }
        _ => return Err(rejected()),
    }
    Ok(Checked {
        slot,
        claims,
        clock_floor: floor,
        close_claims,
        qualifications,
        lineage,
    })
}
/// Only the real first slot creation transaction may install this metadata.
/// This is storage initialization, not a Term permit or a signing handle.
pub(super) fn new_metadata(
    slot: &RoleKeySlotV1,
    config: &TermGuardedSignerConfigurationV2,
) -> Result<String> {
    slot.validate().map_err(|_| rejected())?;
    if slot.scope.context.identity.role != Role::Owner
        || slot.scope.context.identity.environment != config.pin.environment
        || slot.public_ref.public_key == config.pin.public_key
    {
        return Err(rejected());
    }
    json(&Metadata {
        schema_version: if config.close.is_some() { 2 } else { 1 },
        scope: slot.scope.clone(),
        public_key: slot.public_ref.clone(),
        pin: config.pin.clone(),
        floor: config.floor.clone(),
        created_at: DecimalU64(clock()?),
        maximum_claims: DecimalU64(u64::try_from(config.maximum()).map_err(|_| rejected())?),
        close_pin: config.close.as_ref().map(|v| v.0.clone()),
        close_floor: config.close.as_ref().map(|v| v.1.clone()),
    })
}
pub(super) fn check_existing(
    state: &JsValue,
    slot: &RoleKeySlotV1,
    config: &TermGuardedSignerConfigurationV2,
) -> Result<()> {
    let actual = check_inventory(&read_state(state)?, config)?;
    if actual.slot != *slot || clock()? < actual.clock_floor {
        return Err(rejected());
    }
    Ok(())
}
#[wasm_bindgen(module = "/platform/term_guarded_signer_sql_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readTermGuardedSignerState)]
    fn read_state(state: &JsValue) -> Result<String>;
    #[wasm_bindgen(catch,js_name=claimTermGuardedSigner)]
    fn claim_once(state: &JsValue, transition: &js_sys::Function) -> Result<String>;
    #[wasm_bindgen(catch,js_name=recordTermGuardedSignerOriginal)]
    fn record_original(state: &JsValue, id: &str, transition: &js_sys::Function) -> Result<()>;
}
pub(crate) struct TermSignerKeyMaterialV2 {
    wrapping_version: DecimalU64,
    kek: Zeroizing<[u8; 32]>,
}
fn load_material(env: &Env) -> Result<TermSignerKeyMaterialV2> {
    let raw = Zeroizing::new(
        env.secret("CUSTODY_ROLE_KEY_KEK")
            .map_err(|_| rejected())?
            .to_string(),
    );
    if raw.len() != 64 {
        return Err(rejected());
    }
    let mut kek = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(raw.as_bytes(), kek.as_mut_slice()).map_err(|_| rejected())?;
    if hex::encode(kek.as_slice()) != raw.as_str() {
        return Err(rejected());
    }
    let wrapping_version: DecimalU64 = serde_json::from_value(
        env.var("CUSTODY_ROLE_WRAPPING_KEY_VERSION")
            .map_err(|_| rejected())?
            .to_string()
            .into(),
    )
    .map_err(|_| rejected())?;
    if wrapping_version.0 == 0 {
        return Err(rejected());
    }
    Ok(TermSignerKeyMaterialV2 {
        wrapping_version,
        kek,
    })
}
impl RoleKeyStore {
    pub(crate) async fn sign_term_guarded_stop_v2(
        &self,
        env: &Env,
        permit: SignedTermGuardedStopSignerPermitV2,
        config: &TermGuardedSignerConfigurationV2,
    ) -> Result<SignedGameTaskStopV2> {
        self.sign_term_guarded_stop_with_loan_v2(permit, config, || load_material(env))
            .await
    }
    #[cfg(feature = "approval-harness")]
    pub(crate) async fn test_sign_term_guarded_with_loan_v2<K>(
        &self,
        permit: SignedTermGuardedStopSignerPermitV2,
        config: &TermGuardedSignerConfigurationV2,
        loan: K,
    ) -> Result<SignedGameTaskStopV2>
    where
        K: FnOnce() -> Result<(DecimalU64, [u8; 32])>,
    {
        self.sign_term_guarded_stop_with_loan_v2(permit, config, || {
            let (wrapping_version, kek) = loan()?;
            Ok(TermSignerKeyMaterialV2 {
                wrapping_version,
                kek: Zeroizing::new(kek),
            })
        })
        .await
    }
    async fn sign_term_guarded_stop_with_loan_v2<K>(
        &self,
        permit: SignedTermGuardedStopSignerPermitV2,
        config: &TermGuardedSignerConfigurationV2,
        loan: K,
    ) -> Result<SignedGameTaskStopV2>
    where
        K: FnOnce() -> Result<TermSignerKeyMaterialV2>,
    {
        let claim_id = id(&permit)?;
        let mut callback = |raw: String| -> Result<String> {
            let checked = check_inventory(&raw, config)?;
            let at = clock()?;
            if at < checked.clock_floor
                || checked.claims.len() + checked.close_claims.len() >= config.maximum()
                || checked
                    .close_claims
                    .iter()
                    .any(|c| c.permit.payload.paid_attempt_id == permit.payload.paid_attempt_id)
                || (config.close.is_some() && permit.payload.account_lineage.is_none())
                || checked
                    .lineage
                    .as_ref()
                    .is_some_and(|v| Some(v) != permit.payload.account_lineage.as_ref())
                || checked.claims.iter().any(|c| c.id == claim_id)
                || checked
                    .qualifications
                    .iter()
                    .any(|c| c.request.paid_attempt_id == permit.payload.paid_attempt_id)
            {
                return Err(rejected());
            }
            permit
                .verify(
                    &config.pin,
                    &config.floor,
                    &checked.slot.scope,
                    &checked.slot.public_ref,
                    at,
                )
                .map_err(|_| rejected())?;
            json(&Claim {
                id: claim_id.clone(),
                permit: permit.clone(),
                claimed_at: DecimalU64(at),
                signed_at: None,
                original: None,
            })
        };
        let function = Closure::borrow_mut(&mut callback);
        // The FFI returns only after SQL has durably committed the unique claim.
        let raw = claim_once(&self.state, function.as_ref().unchecked_ref())?;
        drop(function);
        let slot = decode(&raw)?.slot.ok_or_else(rejected)?;
        permit
            .verify(
                &config.pin,
                &config.floor,
                &slot.scope,
                &slot.public_ref,
                clock()?,
            )
            .map_err(|_| rejected())?;
        // Secret access is deliberately after the durable claim above.
        let material = loan()?;
        if slot.envelope.wrapping_key_version != material.wrapping_version {
            return Err(rejected());
        }
        let seed = envelope_crypto::open(
            &slot.envelope,
            &slot.scope.context,
            material.kek.as_slice().try_into().map_err(|_| rejected())?,
        )
        .await
        .map_err(|_| rejected())?;
        if public(seed.as_bytes())? != slot.public_ref.public_key {
            return Err(rejected());
        }
        let before_sign = clock()?;
        let current = check_inventory(&read_state(&self.state)?, config)?;
        if current.slot != slot
            || before_sign < current.clock_floor
            || !current
                .claims
                .iter()
                .any(|c| c.id == claim_id && c.permit == permit && c.original.is_none())
        {
            return Err(rejected());
        }
        permit
            .verify(
                &config.pin,
                &config.floor,
                &slot.scope,
                &slot.public_ref,
                before_sign,
            )
            .map_err(|_| rejected())?;
        let signing = SigningKey::from_bytes(seed.as_bytes().try_into().map_err(|_| rejected())?);
        let bytes = Zeroizing::new(
            oasis7_client_api::world_service::signing_bytes(
                GAME_TASK_STOP_DOMAIN_V2,
                &permit.payload.original,
            )
            .map_err(|_| rejected())?,
        );
        let original = SignedGameTaskStopV2 {
            request: permit.payload.original.clone(),
            subject_public_key: slot.public_ref.public_key.clone(),
            signature_hex: hex::encode(signing.sign(&bytes).to_bytes()),
        };
        let saved_original = original.clone();
        let mut finish = |raw: String| -> Result<String> {
            let checked = check_inventory(&raw, config)?;
            if checked.slot != slot || clock()? < checked.clock_floor {
                return Err(rejected());
            }
            let mut claim = checked
                .claims
                .into_iter()
                .find(|c| c.id == claim_id)
                .ok_or_else(rejected)?;
            if claim.permit != permit || claim.original.is_some() {
                return Err(rejected());
            }
            claim.signed_at = Some(DecimalU64(before_sign));
            claim.original = Some(saved_original.clone());
            check_original(&claim)?;
            json(&claim)
        };
        let function = Closure::borrow_mut(&mut finish);
        record_original(&self.state, &claim_id, function.as_ref().unchecked_ref())?;
        drop(function);
        Ok(original)
    }
}
