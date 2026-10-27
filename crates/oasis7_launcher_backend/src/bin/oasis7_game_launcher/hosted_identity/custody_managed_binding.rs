//! Native-account binding authority, separate from manager approval and participants.
use super::*;
use ed25519_dalek::VerifyingKey;
use oasis7_custody_core::native_binding::*;

#[path = "custody_owner_pin.rs"]
pub(in super::super) mod owner_pin;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ManagedBindingConfigState {
    Disabled,
    Ready,
    InvalidPending(&'static str),
}
impl ManagedBindingConfigState {
    pub(super) fn invalid(reason: &str) -> Self {
        // Parser messages are classified into fixed codes, never copied from input values.
        Self::InvalidPending(if reason.contains("rotation") {
            "owner_pin_rotation_configuration_invalid_pending"
        } else if reason.contains("true/false") {
            "owner_pin_enable_configuration_invalid_pending"
        } else {
            "owner_pin_configuration_invalid_pending"
        })
    }
    pub(super) fn pending_code(self) -> Option<&'static str> {
        match self {
            Self::InvalidPending(code) => Some(code),
            _ => None,
        }
    }
}

pub(super) struct ManagedBindingConfig {
    owner: NativeBindingPinV1,
    rotation: Option<SignedOwnerPinRotationManifestV1>,
}
impl ManagedBindingConfig {
    #[cfg(test)]
    pub(super) fn native_http_fixture() -> Self {
        Self {
            rotation: None,
            owner: NativeBindingPinV1 {
                key_id: "owner-native-test".into(),
                public_key: hex::encode(
                    SigningKey::from_bytes(&[93; 32]).verifying_key().to_bytes(),
                ),
                environment: "test".into(),
                service_identity: "owner-native-test".into(),
                service_epoch: DecimalU64(1),
            },
        }
    }
    #[cfg(test)]
    pub(super) fn fixture_pin(&self) -> &NativeBindingPinV1 {
        &self.owner
    }
    pub(super) fn owner_public_key(&self) -> &str {
        &self.owner.public_key
    }
    pub(super) fn from_env() -> Result<Option<Self>, String> {
        match std::env::var("OASIS7_CUSTODY_MANAGED_BINDINGS_ENABLED")
            .ok()
            .as_deref()
        {
            None | Some("false") => return Ok(None),
            Some("true") => {}
            _ => return Err("managed bindings require an explicit true/false flag".into()),
        }
        let pin = std::env::var("OASIS7_CUSTODY_MANAGED_BINDING_OWNER_PIN")
            .map_err(|_| "managed binding Owner pin required")?;
        let owner: NativeBindingPinV1 =
            serde_json::from_str(&pin).map_err(|_| "invalid managed binding Owner pin")?;
        owner
            .validate()
            .map_err(|_| "invalid managed binding Owner pin")?;
        let public: [u8; 32] = hex::decode(&owner.public_key)
            .map_err(|_| "invalid managed binding Owner key")?
            .try_into()
            .map_err(|_| "invalid managed binding Owner key")?;
        if VerifyingKey::from_bytes(&public)
            .map_err(|_| "invalid managed binding Owner key")?
            .is_weak()
        {
            return Err("weak managed binding Owner key".into());
        }
        let rotation = std::env::var("OASIS7_CUSTODY_MANAGED_BINDING_OWNER_ROTATION_MANIFEST")
            .ok()
            .map(|raw| {
                if raw.len() > 32768 {
                    return Err("managed Owner rotation manifest too large");
                }
                let signed: SignedOwnerPinRotationManifestV1 = serde_json::from_str(&raw)
                    .map_err(|_| "invalid managed Owner rotation manifest")?;
                signed
                    .validate()
                    .map_err(|_| "invalid managed Owner rotation manifest")?;
                Ok(signed)
            })
            .transpose()?;
        Ok(Some(Self { owner, rotation }))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChallengeRecord {
    signed: SignedNativeBindingChallengeV1,
    origin: String,
    consumed_statement: Option<String>,
    account_security_epoch: u64,
    credential_version: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManagedBindingRecord {
    pub(super) receipt: SignedNativeBindingReceiptV1,
    statement: SignedOwnerNativeBindingStatementV1,
    login_key_signature: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChallengeIndex {
    registration_operation_id: String,
    challenge_ids: Vec<String>,
}
fn registration_fact(
    tx: &IdentityTransaction<'_>,
    id: &str,
    owner: &str,
    origin: &str,
) -> Result<(OperationReceipt, CeremonyRecord), StoreError> {
    let receipt = tx
        .get::<OperationReceipt>(&format!("receipt:{id}"))?
        .ok_or_else(|| reject("authentication_failed"))?
        .value;
    let ceremony = tx
        .get::<CeremonyRecord>(&format!("ceremony:{id}"))?
        .ok_or_else(|| reject("authentication_failed"))?
        .value;
    if receipt.kind != "machine_register"
        || receipt.state != "committed"
        || receipt.owner_context_digest != owner
        || receipt.operation_id != id
        || ceremony.owner_context_digest != owner
        || ceremony.kind != "machine_register"
        || ceremony.status != "consumed"
        || ceremony.result_ref.as_ref() != receipt.result_ids.first()
        || ceremony
            .server_state
            .as_ref()
            .and_then(|s| s["options"]["audience"].as_str())
            != Some(origin)
    {
        return Err(reject("authentication_failed"));
    }
    Ok((receipt, ceremony))
}
fn original_registration(
    tx: &IdentityTransaction<'_>,
    id: &str,
    owner: &str,
    origin: &str,
) -> Result<(OperationReceipt, AccountRecord, CredentialRecord), StoreError> {
    let (receipt, ceremony) = registration_fact(tx, id, owner, origin)?;
    let account_id = receipt.result_ids.first().ok_or(StoreError::Corrupt)?;
    let account = tx
        .get::<AccountRecord>(&format!("account:{account_id}"))?
        .ok_or(StoreError::Corrupt)?
        .value;
    let options = &ceremony.server_state.as_ref().ok_or(StoreError::Corrupt)?["options"];
    let key = super::super::native_key::public_key(options)?;
    let lookup = tx
        .get::<CredentialLookup>(&super::super::native_key::lookup_key(&key))?
        .ok_or(StoreError::Corrupt)?
        .value;
    let credential = tx
        .get::<CredentialRecord>(&credential_key(&lookup.credential_record_id))?
        .ok_or(StoreError::Corrupt)?
        .value;
    if account.status != "active"
        || account.account_id != *account_id
        || account.security_epoch == 0
        || lookup.account_id != *account_id
        || lookup.state != "active"
        || lookup.canonical_key != key
        || credential.account_id != *account_id
        || credential.state != "active"
        || credential.version == 0
        || credential.registered_via != "machine_registration"
        || !matches!(credential.public_material, CredentialMaterial::NativeKey { public_key } if public_key == key)
    {
        return Err(reject("authentication_failed"));
    }
    Ok((receipt, account, credential))
}
fn public_registration_digest(
    tx: &IdentityTransaction<'_>,
    id: &str,
    namespace: &str,
    epoch: u64,
) -> Result<String, StoreError> {
    let ceremony = tx
        .get::<CeremonyRecord>(&format!("ceremony:{id}"))?
        .ok_or(StoreError::Corrupt)?
        .value;
    let state = ceremony.server_state.as_ref().ok_or(StoreError::Corrupt)?;
    if ceremony.namespace.key() != namespace {
        return Err(reject("authentication_failed"));
    }
    let request = NativeRegistrationRequestV1 {
        identity_namespace: namespace.into(),
        identity_instance_epoch: DecimalU64(epoch),
        registration_operation_id: id.into(),
        login_public_key: state["options"]["public_key"]
            .as_str()
            .ok_or(StoreError::Corrupt)?
            .into(),
        display_name: state["display_name"]
            .as_str()
            .ok_or(StoreError::Corrupt)?
            .into(),
        audience: state["options"]["audience"]
            .as_str()
            .ok_or(StoreError::Corrupt)?
            .into(),
    };
    request.digest().map_err(|_| StoreError::Corrupt)
}
fn registration_digest(receipt: &OperationReceipt) -> Result<String, StoreError> {
    Ok(oasis7_custody_core::approval::bytes_digest(
        &serde_json::to_vec(receipt).map_err(|_| StoreError::Corrupt)?,
    ))
}
fn verify_statement(
    config: &CustodyConfig,
    signed: &SignedOwnerNativeBindingStatementV1,
    now: u64,
) -> Result<(), StoreError> {
    let binding = config
        .managed_binding
        .as_ref()
        .ok_or_else(|| reject("custody_disabled"))?;
    signed.validate().map_err(|_| reject("invalid_request"))?;
    binding
        .owner
        .validate_statement(&signed.statement, now)
        .map_err(|_| reject("authentication_failed"))?;
    let bytes: [u8; 32] = hex::decode(&binding.owner.public_key)
        .map_err(|_| StoreError::Corrupt)?
        .try_into()
        .map_err(|_| StoreError::Corrupt)?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|_| StoreError::Corrupt)?;
    let sig = URL_SAFE_NO_PAD
        .decode(&signed.signature)
        .ok()
        .and_then(|b| Signature::from_slice(&b).ok())
        .ok_or_else(|| reject("authentication_failed"))?;
    if key.is_weak() {
        return Err(reject("authentication_failed"));
    }
    key.verify_strict(
        &signed
            .statement
            .signing_bytes()
            .map_err(|_| reject("invalid_request"))?,
        &sig,
    )
    .map_err(|_| reject("authentication_failed"))
}
impl IdentityService {
    pub(in super::super) fn managed_adapter_configuration_error(&self) -> Option<&'static str> {
        self.custody
            .as_ref()
            .and_then(|c| c.managed_binding_state.pending_code())
    }
    pub(in super::super) fn managed_login_adapter_enabled(&self) -> bool {
        self.custody
            .as_ref()
            .is_some_and(|c| c.managed_binding.is_some())
    }
    #[cfg(test)]
    pub(in super::super) fn enable_managed_login_test_adapter(&mut self) {
        self.custody = Some(CustodyConfig {
            key: SigningKey::from_bytes(&[95; 32]),
            key_id: "native-managed-test-root".into(),
            audience: "native-managed-test".into(),
            adapter_secret: [94; 32],
            revocation: None,
            managed_binding: Some(ManagedBindingConfig::native_http_fixture()),
            managed_binding_state: ManagedBindingConfigState::Ready,
        });
    }
    pub(in super::super) fn managed_binding_route(
        &mut self,
        method: &str,
        path: &str,
        owner: &str,
        payload: &Value,
        origin: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        if method == "POST" && path == "/custody/managed-bindings/lookup" {
            return self.managed_binding_lookup(owner, payload, origin);
        }
        if method == "POST" && path == "/custody/managed-bindings/complete" {
            return self.managed_binding_complete(owner, payload, origin, now);
        }
        if let Some(code) = self.managed_adapter_configuration_error() {
            return Err(reject(code));
        }
        let config = self
            .custody
            .as_ref()
            .filter(|c| c.managed_binding.is_some())
            .ok_or_else(|| reject("custody_disabled"))?;
        if config
            .managed_binding
            .as_ref()
            .ok_or(StoreError::Corrupt)?
            .owner
            .environment
            != self.config.namespace.environment_id
        {
            return Err(reject("authentication_failed"));
        }
        match (method, path) {
            ("POST", "/custody/managed-bindings/options") => {
                self.managed_binding_options(owner, payload, origin, now)
            }
            ("POST", "/custody/managed-bindings/lookup") => {
                self.managed_binding_lookup(owner, payload, origin)
            }
            ("POST", "/custody/managed-bindings/complete") => {
                self.managed_binding_complete(owner, payload, origin, now)
            }
            _ => Err(reject("not_found")),
        }
    }
    fn managed_binding_options(
        &mut self,
        owner: &str,
        payload: &Value,
        origin: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            registration_operation_id: String,
            binding_operation_id: String,
        }
        let request: Request =
            serde_json::from_value(payload.clone()).map_err(|_| reject("invalid_request"))?;
        if request.registration_operation_id.is_empty()
            || request.registration_operation_id.len() > 256
            || request.binding_operation_id.is_empty()
            || request.binding_operation_id.len() > 256
            || request.binding_operation_id.trim() != request.binding_operation_id
            || request.binding_operation_id.chars().any(char::is_control)
        {
            return Err(reject("invalid_request"));
        }
        let config = self
            .custody
            .as_ref()
            .ok_or_else(|| reject("custody_disabled"))?;
        let namespace = self.config.namespace.clone();
        let epoch = self.config.rp.identity_instance_epoch;
        let native_config = &self.config;
        self.store.transaction(|tx| {
            owner_pin::guard(tx, config, native_config, now)?;
            let (receipt, account, credential) =
                original_registration(tx, &request.registration_operation_id, owner, origin)?;
            let key = format!("custody_managed_challenge:{}", request.binding_operation_id);
            if let Some(old) = tx.get::<ChallengeRecord>(&key)? {
                let c = &old.value.signed.challenge;
                if c.owner_context_ref != owner
                    || old.value.origin != origin
                    || c.native_account_id != account.account_id
                    || c.registration_operation_id != request.registration_operation_id
                    || c.challenge_id != request.binding_operation_id
                {
                    return Err(reject("idempotency_conflict"));
                }
                return serde_json::to_value(old.value.signed).map_err(|_| StoreError::Corrupt);
            }
            if tx
                .get::<ManagedBindingRecord>(&format!(
                    "custody_managed_binding:{}",
                    account.account_id
                ))?
                .is_some()
            {
                return Err(reject("idempotency_conflict"));
            }
            let index_key = format!(
                "custody_managed_challenge_index:{}",
                request.registration_operation_id
            );
            let mut index = tx.get::<ChallengeIndex>(&index_key)?;
            // Unpublished legacy single-challenge rows are never silently overwritten.
            if index.is_none()
                && tx
                    .get::<Value>(&format!(
                        "custody_managed_challenge:{}",
                        request.registration_operation_id
                    ))?
                    .is_some()
            {
                return Err(reject("managed_binding_legacy_unknown"));
            }
            if index.as_ref().is_some_and(|i| {
                i.value.registration_operation_id != request.registration_operation_id
                    || i.value.challenge_ids.len() >= 16
            }) {
                return Err(reject("managed_binding_challenge_capacity"));
            }
            let CredentialMaterial::NativeKey { public_key } = credential.public_material else {
                return Err(StoreError::Corrupt);
            };
            let mut nonce = [0u8; 32];
            getrandom::fill(&mut nonce)
                .map_err(|_| StoreError::Unavailable("entropy unavailable".into()))?;
            let challenge = NativeBindingChallengeV1 {
                api_version: 1,
                challenge_id: request.binding_operation_id.clone(),
                nonce: hex::encode(nonce),
                identity_namespace: namespace.key(),
                identity_instance_epoch: DecimalU64(epoch),
                registration_operation_id: receipt.operation_id.clone(),
                registration_request_digest: public_registration_digest(
                    tx,
                    &receipt.operation_id,
                    &namespace.key(),
                    epoch,
                )?,
                registration_receipt_digest: registration_digest(&receipt)?,
                native_account_id: account.account_id.clone(),
                native_credential_ref: credential.credential_record_id,
                login_public_key: hex::encode(public_key),
                owner_context_ref: owner.into(),
                issued_at: DecimalU64(now),
                expires_at: DecimalU64(now.checked_add(30_000).ok_or(StoreError::Corrupt)?),
            };
            let bytes = challenge.signing_bytes().map_err(|_| StoreError::Corrupt)?;
            let signed = SignedNativeBindingChallengeV1 {
                challenge,
                key_id: config.key_id.clone(),
                signature: URL_SAFE_NO_PAD.encode(config.key.sign(&bytes).to_bytes()),
            };
            tx.create(
                &key,
                "custody_managed_challenge",
                Some(&account.account_id),
                None,
                &ChallengeRecord {
                    signed: signed.clone(),
                    origin: origin.into(),
                    consumed_statement: None,
                    account_security_epoch: account.security_epoch,
                    credential_version: credential.version,
                },
            )?;
            if let Some(index) = &mut index {
                index
                    .value
                    .challenge_ids
                    .push(request.binding_operation_id.clone());
                tx.replace(
                    &index_key,
                    index.row_version,
                    "custody_managed_challenge_index",
                    Some(&account.account_id),
                    None,
                    &index.value,
                )?;
            } else {
                tx.create(
                    &index_key,
                    "custody_managed_challenge_index",
                    Some(&account.account_id),
                    None,
                    &ChallengeIndex {
                        registration_operation_id: request.registration_operation_id.clone(),
                        challenge_ids: vec![request.binding_operation_id.clone()],
                    },
                )?;
            }
            serde_json::to_value(signed).map_err(|_| StoreError::Corrupt)
        })
    }
    fn managed_binding_lookup(
        &mut self,
        owner: &str,
        payload: &Value,
        origin: &str,
    ) -> Result<Value, StoreError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            registration_operation_id: String,
        }
        let request: Request =
            serde_json::from_value(payload.clone()).map_err(|_| reject("invalid_request"))?;
        self.store.transaction(|tx| {
            let (registration, _) =
                registration_fact(tx, &request.registration_operation_id, owner, origin)?;
            let account_id = registration.result_ids.first().ok_or(StoreError::Corrupt)?;
            let record = tx
                .get::<ManagedBindingRecord>(&format!("custody_managed_binding:{account_id}"))?
                .ok_or_else(|| reject("not_found"))?
                .value;
            if record.receipt.receipt.registration_operation_id != request.registration_operation_id
            {
                return Err(StoreError::Corrupt);
            }
            serde_json::to_value(record.receipt).map_err(|_| StoreError::Corrupt)
        })
    }
    fn managed_binding_complete(
        &mut self,
        owner: &str,
        payload: &Value,
        origin: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        let request: NativeBindingCompleteV1 =
            serde_json::from_value(payload.clone()).map_err(|_| reject("invalid_request"))?;
        request.validate().map_err(|_| reject("invalid_request"))?;
        let signed = request.statement;
        let statement = &signed.statement;
        let c = &statement.challenge;
        let config = self.custody.as_ref();
        let native_config = &self.config;
        let namespace = self.config.namespace.clone();
        let epoch = self.config.rp.identity_instance_epoch;
        let statement_digest = statement.digest().map_err(|_| reject("invalid_request"))?;
        self.store.transaction(|tx| {
            let (registration_fact, _) = registration_fact(tx, &c.registration_operation_id, owner, origin)?;
            let challenge_key = format!("custody_managed_challenge:{}", statement.binding_operation_id);
            if statement.binding_operation_id != c.challenge_id { return Err(reject("authentication_failed")); }
            let historical_key = format!("custody_managed_binding:{}", registration_fact.result_ids.first().ok_or(StoreError::Corrupt)?);
            if let Some(record) = tx.get::<ManagedBindingRecord>(&historical_key)? {
                let original = tx.get::<ChallengeRecord>(&challenge_key)?.ok_or(StoreError::Corrupt)?.value;
                if record.value.statement != signed || record.value.login_key_signature != request.login_key_signature || original.consumed_statement.as_ref() != Some(&statement_digest) || original.origin != origin || original.signed.challenge != *c { return Err(reject("idempotency_conflict")); }
                return serde_json::to_value(record.value.receipt).map_err(|_| StoreError::Corrupt);
            }
            let config = config.ok_or_else(|| reject("custody_disabled"))?;
            if let Some(code) = config.managed_binding_state.pending_code() { return Err(reject(code)); }
            let config = Some(config).filter(|c| c.managed_binding.is_some()).ok_or_else(|| reject("custody_disabled"))?;
            owner_pin::guard(tx, config, native_config, now)?;
            let (registration, account, credential) = original_registration(tx, &c.registration_operation_id, owner, origin)?;
            let mut original = tx.get::<ChallengeRecord>(&challenge_key)?.ok_or_else(|| reject("authentication_failed"))?;
            if original.value.signed.challenge != *c || original.value.origin != origin || c.owner_context_ref != owner
                || c.identity_namespace != namespace.key() || c.identity_instance_epoch.0 != epoch
                || original.value.account_security_epoch != account.security_epoch || original.value.credential_version != credential.version
                || c.native_account_id != account.account_id || c.native_credential_ref != credential.credential_record_id
                || c.registration_receipt_digest != registration_digest(&registration)? || c.registration_request_digest != public_registration_digest(tx, &registration.operation_id, &namespace.key(), epoch)?
                || !matches!(credential.public_material, CredentialMaterial::NativeKey { public_key } if hex::encode(public_key) == c.login_public_key)
            { return Err(reject("authentication_failed")); }
            let binding_key = format!("custody_managed_binding:{}", account.account_id);
            if let Some(record) = tx.get::<ManagedBindingRecord>(&binding_key)? {
                if record.value.statement != signed || record.value.login_key_signature != request.login_key_signature || original.value.consumed_statement.as_ref() != Some(&statement_digest) {
                    return Err(reject("idempotency_conflict"));
                }
                // Return only the original non-secret fact; no login cookie or authority renewal.
                return serde_json::to_value(record.value.receipt).map_err(|_| StoreError::Corrupt);
            }
            if original.value.consumed_statement.is_some() { return Err(reject("idempotency_conflict")); }
            verify_statement(config, &signed, now)?;
            let login_key: [u8; 32] = hex::decode(&c.login_public_key).map_err(|_| StoreError::Corrupt)?.try_into().map_err(|_| StoreError::Corrupt)?;
            let login_key = VerifyingKey::from_bytes(&login_key).map_err(|_| reject("authentication_failed"))?;
            let possession = URL_SAFE_NO_PAD.decode(&request.login_key_signature).ok().and_then(|v| Signature::from_slice(&v).ok()).ok_or_else(|| reject("authentication_failed"))?;
            if login_key.is_weak() { return Err(reject("authentication_failed")); }
            login_key.verify_strict(&statement.possession_signing_bytes().map_err(|_| reject("invalid_request"))?, &possession).map_err(|_| reject("authentication_failed"))?;
            let receipt = NativeBindingReceiptV1 {
                api_version: 1, binding_operation_id: statement.binding_operation_id.clone(), statement_digest: statement_digest.clone(),
                environment: statement.environment.clone(), custody_account_id: statement.custody_account_id.clone(), provision_id: statement.provision_id.clone(),
                identity_namespace: namespace.key(), identity_instance_epoch: DecimalU64(epoch), native_account_id: account.account_id.clone(),
                native_credential_ref: credential.credential_record_id, login_public_key: c.login_public_key.clone(), login_key_ref: statement.login_key_ref.clone(),
                registration_operation_id: c.registration_operation_id.clone(), registration_receipt_digest: c.registration_receipt_digest.clone(),
                native_account_security_epoch: DecimalU64(account.security_epoch), native_credential_version: DecimalU64(credential.version), committed_at: DecimalU64(now),
            };
            receipt.validate_for(statement).map_err(|_| reject("authentication_failed"))?;
            let result = SignedNativeBindingReceiptV1 { signature: URL_SAFE_NO_PAD.encode(config.key.sign(&receipt.signing_bytes().map_err(|_| StoreError::Corrupt)?).to_bytes()), key_id: config.key_id.clone(), receipt };
            tx.create(&binding_key, "custody_managed_binding", Some(&account.account_id), None, &ManagedBindingRecord { receipt: result.clone(), statement: signed.clone(), login_key_signature: request.login_key_signature.clone() })?;
            original.value.consumed_statement = Some(statement_digest.clone());
            tx.replace(&challenge_key, original.row_version, "custody_managed_challenge", Some(&account.account_id), None, &original.value)?;
            audit(tx, &account, "custody_native_bound", now)?;
            serde_json::to_value(result).map_err(|_| StoreError::Corrupt)
        })
    }
}

#[cfg(test)]
#[path = "custody_managed_binding_tests.rs"]
mod tests;

/// Local durable pending work, never a manager directory event or a closure ACK.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManagedInvalidationPending {
    pub(super) binding_operation_id: String,
    pub(super) native_account_id: String,
    pub(super) generation: u64,
    pub(super) frozen: bool,
    pub(super) pending: bool,
    invalidation: ManagedNativeInvalidationV1,
}
pub(in super::super) fn record_change(
    tx: &mut IdentityTransaction<'_>,
    key: &str,
    kind: &str,
    previous: &Value,
    next: Option<&Value>,
) -> Result<(), StoreError> {
    let Some(account_id) = previous["account_id"].as_str() else {
        return Ok(());
    };
    let Some(binding) =
        tx.get::<ManagedBindingRecord>(&format!("custody_managed_binding:{account_id}"))?
    else {
        return Ok(());
    };
    let receipt = &binding.value.receipt.receipt;
    let invalidated = match kind {
        "account" => next.is_none_or(|n| {
            n["status"] != previous["status"]
                || n["security_epoch"] != previous["security_epoch"]
                || n["policy_version"] != previous["policy_version"]
        }),
        "credential"
            if previous["credential_record_id"].as_str()
                == Some(receipt.native_credential_ref.as_str()) =>
        {
            next.is_none_or(|n| {
                n["state"] != previous["state"]
                    || n["version"] != previous["version"]
                    || n["public_material"] != previous["public_material"]
            })
        }
        "session"
            if previous["origin_credential_id"].as_str()
                == Some(receipt.native_credential_ref.as_str()) =>
        {
            next.is_none_or(|n| {
                n["revoked_at"] != previous["revoked_at"]
                    || n["issued_security_epoch"] != previous["issued_security_epoch"]
                    || n["origin_credential_version"] != previous["origin_credential_version"]
                    || n["expires_at"]
                        .as_u64()
                        .zip(previous["expires_at"].as_u64())
                        .is_some_and(|(next, before)| next < before)
                    || n["idle_expires_at"]
                        .as_u64()
                        .zip(previous["idle_expires_at"].as_u64())
                        .is_some_and(|(next, before)| next < before)
            })
        }
        _ => false,
    };
    if !invalidated {
        return Ok(());
    }
    let pending_key = format!("custody_managed_invalidation:{account_id}");
    let old = tx.get::<ManagedInvalidationPending>(&pending_key)?;
    let pending = ManagedInvalidationPending {
        binding_operation_id: receipt.binding_operation_id.clone(),
        native_account_id: account_id.into(),
        generation: old
            .as_ref()
            .map_or(2, |v| v.value.generation.saturating_add(1)),
        frozen: true,
        pending: true,
        invalidation: ManagedNativeInvalidationV1 {
            api_version: 1,
            receipt: receipt.clone(),
            service_epoch: binding.value.statement.statement.service_epoch,
            native_account_security_epoch: DecimalU64(if kind == "account" {
                next.and_then(|n| n["security_epoch"].as_u64())
                    .unwrap_or(receipt.native_account_security_epoch.0)
                    .max(receipt.native_account_security_epoch.0)
            } else {
                tx.get::<AccountRecord>(&format!("account:{account_id}"))?
                    .map_or(receipt.native_account_security_epoch.0, |v| {
                        v.value
                            .security_epoch
                            .max(receipt.native_account_security_epoch.0)
                    })
            }),
            native_credential_version: DecimalU64(if kind == "credential" {
                next.and_then(|n| n["version"].as_u64())
                    .unwrap_or(receipt.native_credential_version.0)
                    .max(receipt.native_credential_version.0)
            } else {
                tx.get::<CredentialRecord>(&credential_key(&receipt.native_credential_ref))?
                    .map_or(receipt.native_credential_version.0, |v| {
                        v.value.version.max(receipt.native_credential_version.0)
                    })
            }),
            session_ref: (kind == "session")
                .then(|| previous["session_id"].as_str().unwrap_or(key).to_owned()),
            generation: DecimalU64(
                old.as_ref()
                    .map_or(2, |v| v.value.generation.saturating_add(1)),
            ),
            sequence: DecimalU64(
                old.as_ref()
                    .map_or(1, |v| v.value.invalidation.sequence.0.saturating_add(1)),
            ),
            reason: format!("{kind}_invalidated"),
            changed_at: DecimalU64(super::super::now_ms().max(receipt.committed_at.0)),
        },
    };
    if let Some(old) = old {
        tx.replace(
            &pending_key,
            old.row_version,
            "custody_managed_invalidation",
            Some(account_id),
            None,
            &pending,
        )?;
    } else {
        tx.create(
            &pending_key,
            "custody_managed_invalidation",
            Some(account_id),
            None,
            &pending,
        )?;
    }
    Ok(())
}

impl IdentityService {
    pub(in super::super) fn managed_session_status(
        &mut self,
        sid: Option<&str>,
        payload: &Value,
        now: u64,
    ) -> Result<Value, StoreError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            nonce: String,
        }
        let request: Request =
            serde_json::from_value(payload.clone()).map_err(|_| reject("invalid_request"))?;
        if request.nonce.len() != 64
            || !request
                .nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(reject("invalid_request"));
        }
        let sid = sid.ok_or_else(|| reject("session_expired"))?;
        let storage_key = session_key(&self.config.session_key, sid);
        if let Some(code) = self.managed_adapter_configuration_error() {
            return Err(reject(code));
        }
        let config = self
            .custody
            .as_ref()
            .filter(|c| c.managed_binding.is_some())
            .ok_or_else(|| reject("custody_disabled"))?;
        let epoch = self.config.rp.identity_instance_epoch;
        let namespace = self.config.namespace.key();
        self.store.transaction(|tx| {
            // A revoked original cookie may read only its own non-secret inactive fact.
            let session = tx.get::<AccountSession>(&storage_key)?.ok_or_else(|| reject("session_expired"))?.value;
            let binding = tx.get::<ManagedBindingRecord>(&format!("custody_managed_binding:{}", session.account_id))?.ok_or_else(|| reject("authentication_failed"))?.value;
            let receipt = &binding.receipt.receipt;
            if session.origin_credential_id != receipt.native_credential_ref || session.auth_method != "native_key" { return Err(reject("authentication_failed")); }
            let account = tx.get::<AccountRecord>(&format!("account:{}", session.account_id))?.ok_or_else(|| reject("authentication_failed"))?.value;
            let credential = tx.get::<CredentialRecord>(&credential_key(&receipt.native_credential_ref))?.ok_or_else(|| reject("authentication_failed"))?.value;
            let pending_key = format!("custody_managed_invalidation:{}", session.account_id);
            let mut pending = tx.get::<ManagedInvalidationPending>(&pending_key)?;
            if (receipt.identity_instance_epoch.0 != epoch || receipt.identity_namespace != namespace) && pending.is_none() {
                let fact = ManagedInvalidationPending {
                    binding_operation_id: receipt.binding_operation_id.clone(), native_account_id: session.account_id.clone(), generation: 2, frozen: true, pending: true,
                    invalidation: ManagedNativeInvalidationV1 { api_version: 1, receipt: receipt.clone(), service_epoch: binding.statement.statement.service_epoch, native_account_security_epoch: DecimalU64(account.security_epoch), native_credential_version: DecimalU64(credential.version), session_ref: Some(session.session_id.clone()), generation: DecimalU64(2), reason: "namespace_changed".into(), sequence: DecimalU64(1), changed_at: DecimalU64(now) },
                };
                tx.create(&pending_key, "custody_managed_invalidation", Some(&session.account_id), None, &fact)?;
                pending = tx.get::<ManagedInvalidationPending>(&pending_key)?;
            }
            let pin_current = owner_pin::binding_is_current(tx, config, &self.config, &binding);
            let active = pin_current && valid_session(tx, &storage_key, epoch, now).is_ok()
                && credential.state == "active" && account.status == "active"
                && matches!(credential.public_material, CredentialMaterial::NativeKey { public_key } if hex::encode(public_key) == receipt.login_public_key)
                && pending.is_none();
            let status = ManagedNativeSessionStatusV1 {
                api_version: 1, receipt: receipt.clone(), service_epoch: binding.statement.statement.service_epoch,
                native_account_security_epoch: DecimalU64(account.security_epoch), native_credential_version: DecimalU64(credential.version),
                session_ref: session.session_id, nonce: request.nonce.clone(), generation: DecimalU64(pending.as_ref().map_or(1, |p| p.value.generation)),
                active, frozen: !pin_current || pending.as_ref().is_some_and(|p| p.value.frozen), pending: !pin_current || pending.is_some(), observed_at: DecimalU64(now), expires_at: DecimalU64(now.checked_add(5_000).ok_or(StoreError::Corrupt)?),
            };
            let signed = SignedManagedNativeSessionStatusV1 { signature: URL_SAFE_NO_PAD.encode(config.key.sign(&status.signing_bytes().map_err(|_| reject("authentication_failed"))?).to_bytes()), key_id: config.key_id.clone(), status };
            serde_json::to_value(signed).map_err(|_| StoreError::Corrupt)
        })
    }
    pub(in super::super) fn managed_invalidation(
        &mut self,
        owner: &str,
        payload: &Value,
        origin: &str,
    ) -> Result<Value, StoreError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            registration_operation_id: String,
        }
        let request: Request =
            serde_json::from_value(payload.clone()).map_err(|_| reject("invalid_request"))?;
        if let Some(code) = self.managed_adapter_configuration_error() {
            return Err(reject(code));
        }
        let config = self
            .custody
            .as_ref()
            .filter(|c| c.managed_binding.is_some())
            .ok_or_else(|| reject("custody_disabled"))?;
        self.store.transaction(|tx| {
            // Unlike fresh admission, an invalidation remains observable after account/key revocation.
            let (registration, _) =
                registration_fact(tx, &request.registration_operation_id, owner, origin)?;
            let account_id = registration.result_ids.first().ok_or(StoreError::Corrupt)?;
            let pending = tx
                .get::<ManagedInvalidationPending>(&format!(
                    "custody_managed_invalidation:{}",
                    account_id
                ))?
                .ok_or_else(|| reject("not_found"))?
                .value;
            let invalidation = pending.invalidation;
            let signed = SignedManagedNativeInvalidationV1 {
                signature: URL_SAFE_NO_PAD.encode(
                    config
                        .key
                        .sign(
                            &invalidation
                                .signing_bytes()
                                .map_err(|_| StoreError::Corrupt)?,
                        )
                        .to_bytes(),
                ),
                key_id: config.key_id.clone(),
                invalidation,
            };
            serde_json::to_value(signed).map_err(|_| StoreError::Corrupt)
        })
    }
}
