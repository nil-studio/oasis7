//! Persistent namespace Owner service-pin floor. No HTTP route can rotate this authority.
use super::*;
const KEY: &str = "custody_owner_pin_ledger:v1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Retired {
    pin: NativeBindingPinV1,
    operation_id: String,
    manifest_digest: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    schema_version: u8,
    identity_namespace: String,
    identity_instance_epoch: u64,
    native_root_key_id: String,
    native_root_public_key: String,
    current: NativeBindingPinV1,
    retired: Vec<Retired>,
}
fn digest<T: Serialize>(domain: &[u8], value: &T) -> Result<String, StoreError> {
    let mut bytes = domain.to_vec();
    bytes.push(0);
    bytes.extend(serde_json::to_vec(value).map_err(|_| StoreError::Corrupt)?);
    Ok(oasis7_custody_core::approval::bytes_digest(&bytes))
}
impl Ledger {
    fn digest(&self) -> Result<String, StoreError> {
        digest(b"oasis7-custody-native-owner-pin-ledger-v1", self)
    }
    fn validate(&self) -> Result<(), StoreError> {
        if self.schema_version != 1
            || self.identity_namespace.is_empty()
            || self.identity_namespace.len() > 256
            || self.native_root_key_id.is_empty()
            || self.native_root_key_id.len() > 256
            || self.native_root_key_id.trim() != self.native_root_key_id
            || self.native_root_key_id.chars().any(char::is_control)
            || self.identity_instance_epoch == 0
            || self.retired.len() > 256
        {
            return Err(StoreError::Corrupt);
        }
        self.current.validate().map_err(|_| StoreError::Corrupt)?;
        if self
            .retired
            .windows(2)
            .any(|pair| pair[0].pin.service_epoch.0 >= pair[1].pin.service_epoch.0)
        {
            return Err(StoreError::Corrupt);
        }
        let mut keys = std::collections::BTreeSet::new();
        let mut operations = std::collections::BTreeSet::new();
        for old in &self.retired {
            old.pin.validate().map_err(|_| StoreError::Corrupt)?;
            if old.pin.environment != self.current.environment
                || old.pin.service_identity != self.current.service_identity
                || old.pin.service_epoch.0 >= self.current.service_epoch.0
                || !keys.insert(old.pin.public_key.clone())
                || old.pin.public_key == self.current.public_key
                || !operations.insert(old.operation_id.clone())
                || old.manifest_digest.len() != 64
                || !old
                    .manifest_digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(StoreError::Corrupt);
            }
        }
        Ok(())
    }
}
fn root_public(root: &CustodyConfig) -> String {
    hex::encode(root.key.verifying_key().to_bytes())
}
fn inventory_empty(tx: &IdentityTransaction<'_>) -> Result<bool, StoreError> {
    for kind in [
        "custody_managed_challenge",
        "custody_managed_challenge_index",
        "custody_managed_binding",
        "custody_managed_invalidation",
        "native_managed_login_outcome",
    ] {
        if tx.count_kind(kind)? != 0 {
            return Ok(false);
        }
    }
    Ok(!tx
        .all::<CeremonyRecord>("ceremony", None)?
        .iter()
        .any(|(_, c)| c.value.kind == "native_managed_login"))
}
fn strict(key: &str, signature: &str, bytes: &[u8]) -> Result<(), StoreError> {
    let public: [u8; 32] = hex::decode(key)
        .map_err(|_| reject("owner_pin_rotation_rejected"))?
        .try_into()
        .map_err(|_| reject("owner_pin_rotation_rejected"))?;
    if hex::encode(public) != key {
        return Err(reject("owner_pin_rotation_rejected"));
    }
    let key =
        VerifyingKey::from_bytes(&public).map_err(|_| reject("owner_pin_rotation_rejected"))?;
    let raw = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| reject("owner_pin_rotation_rejected"))?;
    if URL_SAFE_NO_PAD.encode(&raw) != signature || key.is_weak() {
        return Err(reject("owner_pin_rotation_rejected"));
    }
    let signature =
        Signature::from_slice(&raw).map_err(|_| reject("owner_pin_rotation_rejected"))?;
    key.verify_strict(bytes, &signature)
        .map_err(|_| reject("owner_pin_rotation_rejected"))
}
/// The HMAC binds pending login to this exact configured service/native pin set.
/// Admission additionally requires the durable floor to accept it in the same transaction.
pub(in super::super::super) fn configured_fingerprint(
    root: &CustodyConfig,
    config: &super::super::super::config::IdentityConfig,
) -> Result<String, StoreError> {
    if let Some(code) = root.managed_binding_state.pending_code() {
        return Err(reject(code));
    }
    let managed = root
        .managed_binding
        .as_ref()
        .ok_or_else(|| reject("custody_disabled"))?;
    digest(
        b"oasis7-custody-native-owner-pin-scope-v1",
        &(
            config.namespace.key(),
            config.rp.identity_instance_epoch,
            &root.key_id,
            root_public(root),
            &managed.owner,
        ),
    )
}
fn synchronize(
    tx: &mut IdentityTransaction<'_>,
    root: &CustodyConfig,
    config: &super::super::super::config::IdentityConfig,
    now: u64,
) -> Result<Ledger, StoreError> {
    if let Some(code) = root.managed_binding_state.pending_code() {
        return Err(reject(code));
    }
    let managed = root
        .managed_binding
        .as_ref()
        .ok_or_else(|| reject("custody_disabled"))?;
    managed
        .owner
        .validate()
        .map_err(|_| reject("owner_pin_rejected"))?;
    let public: [u8; 32] = hex::decode(&managed.owner.public_key)
        .map_err(|_| reject("owner_pin_rejected"))?
        .try_into()
        .map_err(|_| reject("owner_pin_rejected"))?;
    if VerifyingKey::from_bytes(&public)
        .map_err(|_| reject("owner_pin_rejected"))?
        .is_weak()
    {
        return Err(reject("owner_pin_rejected"));
    }

    if managed.owner.environment != config.namespace.environment_id
        || managed.owner.public_key == root_public(root)
        || managed.owner.key_id == root.key_id
    {
        return Err(reject("owner_pin_rejected"));
    }
    let old = tx.get::<Ledger>(KEY)?;
    let Some(old) = old else {
        if managed.rotation.is_some() || !inventory_empty(tx)? {
            return Err(reject("owner_pin_legacy_unknown_pending"));
        }
        let ledger = Ledger {
            schema_version: 1,
            identity_namespace: config.namespace.key(),
            identity_instance_epoch: config.rp.identity_instance_epoch,
            native_root_key_id: root.key_id.clone(),
            native_root_public_key: root_public(root),
            current: managed.owner.clone(),
            retired: vec![],
        };
        ledger.validate()?;
        tx.create(KEY, "custody_owner_pin_ledger", None, None, &ledger)?;
        return Ok(ledger);
    };
    let mut ledger = old.value;
    ledger.validate()?;
    if ledger.identity_namespace != config.namespace.key()
        || ledger.identity_instance_epoch != config.rp.identity_instance_epoch
        || ledger.native_root_key_id != root.key_id
        || ledger.native_root_public_key != root_public(root)
    {
        return Err(reject("owner_pin_native_authority_pending"));
    }
    if ledger.current == managed.owner {
        return Ok(ledger);
    }
    if managed.owner.service_epoch.0 <= ledger.current.service_epoch.0
        || ledger.retired.len() >= 256
        || managed.owner.environment != ledger.current.environment
        || managed.owner.service_identity != ledger.current.service_identity
        || managed.owner.public_key == ledger.current.public_key
        || ledger
            .retired
            .iter()
            .any(|v| v.pin.public_key == managed.owner.public_key)
    {
        return Err(reject("owner_pin_rollback_or_reuse"));
    }
    let signed = managed
        .rotation
        .as_ref()
        .ok_or_else(|| reject("owner_pin_rotation_required"))?;
    signed
        .validate()
        .map_err(|_| reject("owner_pin_rotation_rejected"))?;
    signed
        .validate_root_key_id(&ledger.native_root_key_id)
        .map_err(|_| reject("owner_pin_rotation_rejected"))?;
    signed
        .manifest
        .validate_for(
            &ledger.digest()?,
            &ledger.current,
            &ledger.identity_namespace,
            DecimalU64(ledger.identity_instance_epoch),
            now,
        )
        .map_err(|_| reject("owner_pin_rotation_rejected"))?;
    if signed.manifest.new_pin != managed.owner
        || ledger
            .retired
            .iter()
            .any(|v| v.operation_id == signed.manifest.operation_id)
    {
        return Err(reject("owner_pin_rotation_rejected"));
    }
    strict(
        &ledger.native_root_public_key,
        &signed.native_root_signature,
        &signed
            .manifest
            .signing_bytes()
            .map_err(|_| reject("owner_pin_rotation_rejected"))?,
    )?;
    strict(
        &managed.owner.public_key,
        &signed.new_owner_signature,
        &signed
            .manifest
            .possession_signing_bytes()
            .map_err(|_| reject("owner_pin_rotation_rejected"))?,
    )?;
    ledger.retired.push(Retired {
        pin: ledger.current.clone(),
        operation_id: signed.manifest.operation_id.clone(),
        manifest_digest: signed
            .manifest
            .digest()
            .map_err(|_| reject("owner_pin_rotation_rejected"))?,
    });
    ledger.current = managed.owner.clone();
    ledger.validate()?;
    tx.replace(
        KEY,
        old.row_version,
        "custody_owner_pin_ledger",
        None,
        None,
        &ledger,
    )?;
    Ok(ledger)
}
/// Every new business write calls this inside its own SQL transaction.
pub(in super::super::super) fn guard(
    tx: &mut IdentityTransaction<'_>,
    root: &CustodyConfig,
    config: &super::super::super::config::IdentityConfig,
    now: u64,
) -> Result<String, StoreError> {
    synchronize(tx, root, config, now)?;
    configured_fingerprint(root, config)
}
/// An already managed account cannot turn a retired binding into a new managed SID.
/// Ordinary imported native keys have no custody binding and gain no custody authority here.
pub(in super::super::super) fn guard_login(
    tx: &mut IdentityTransaction<'_>,
    root: &CustodyConfig,
    config: &super::super::super::config::IdentityConfig,
    key: &[u8; 32],
    now: u64,
) -> Result<String, StoreError> {
    let fingerprint = guard(tx, root, config, now)?;
    if let Some(lookup) =
        tx.get::<CredentialLookup>(&super::super::super::native_key::lookup_key(key))?
    {
        if let Some(binding) = tx.get::<ManagedBindingRecord>(&format!(
            "custody_managed_binding:{}",
            lookup.value.account_id
        ))? && !binding_is_current(tx, root, config, &binding.value)
        {
            return Err(reject("owner_pin_binding_retired_pending"));
        }
    }
    Ok(fingerprint)
}
/// Read-only activity decision: unknown/mismatched pins never become active.
pub(super) fn binding_is_current(
    tx: &IdentityTransaction<'_>,
    root: &CustodyConfig,
    config: &super::super::super::config::IdentityConfig,
    binding: &ManagedBindingRecord,
) -> bool {
    let Some(managed) = root.managed_binding.as_ref() else {
        return false;
    };
    let Ok(Some(ledger)) = tx.get::<Ledger>(KEY) else {
        return false;
    };
    let ledger = ledger.value;
    let statement = &binding.statement.statement;
    ledger.validate().is_ok()
        && ledger.current == managed.owner
        && ledger.identity_namespace == config.namespace.key()
        && ledger.identity_instance_epoch == config.rp.identity_instance_epoch
        && ledger.native_root_key_id == root.key_id
        && ledger.native_root_public_key == root_public(root)
        && statement.service_epoch == ledger.current.service_epoch
        && statement.signer_key_id == ledger.current.key_id
        && statement.service_identity == ledger.current.service_identity
        && statement.environment == ledger.current.environment
}
#[cfg(test)]
#[path = "custody_owner_pin_tests.rs"]
mod tests;
