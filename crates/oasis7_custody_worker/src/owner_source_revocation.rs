//! Independently authenticated source events never require a live manager session.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::source_revocation::*;

struct SourcePins {
    native_key_id: String,
    native_key: String,
    audience: String,
    owner_key_id: String,
    owner_key: SigningKey,
}
impl SourcePins {
    fn load(env: &Env) -> Result<Self> {
        let get = |name| env.var(name).map(|v| v.to_string()).map_err(|_| denied());
        let seed = env
            .secret("CUSTODY_REVOCATION_OWNER_SIGNING_KEY")?
            .to_string();
        let bytes: [u8; 32] = hex::decode(&seed)
            .map_err(|_| denied())?
            .try_into()
            .map_err(|_| denied())?;
        if hex::encode(bytes) != seed {
            return Err(denied());
        }
        let pins = Self {
            native_key_id: get("CUSTODY_NATIVE_APPROVAL_KEY_ID")?,
            native_key: get("CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY")?,
            audience: get("CUSTODY_NATIVE_AUDIENCE")?,
            owner_key_id: get("CUSTODY_REVOCATION_OWNER_KEY_ID")?,
            owner_key: SigningKey::from_bytes(&bytes),
        };
        if pins.owner_key_id.is_empty()
            || pins.owner_key_id.len() > 128
            || hex::encode(pins.owner_key.verifying_key().as_bytes()) == pins.native_key
        {
            return Err(denied());
        }
        Ok(pins)
    }
    async fn verify(
        &self,
        signed: &SignedSourceRevocationEventV1,
        config: &DirectoryConfig,
    ) -> Result<ManagerPrincipal> {
        let event = &signed.event;
        event.validate().map_err(|_| denied())?;
        if event.key_id != self.native_key_id
            || event.identity_namespace != config.namespace
            || event.identity_instance_epoch != config.epoch
            || event.environment != config.environment
            || event.owner_audience != self.audience
            || event.observed_at.0 > clock()?.saturating_add(5000)
        {
            return Err(denied());
        }
        crate::approval_crypto::verify_ed25519(
            &self.native_key,
            &signed.signature,
            &event.signing_bytes().map_err(|_| denied())?,
        )
        .await
        .map_err(|_| denied())?;
        Ok(ManagerPrincipal {
            identity_namespace: event.identity_namespace.clone(),
            native_account_id: event.manager_account_id.clone(),
        })
    }
}

impl ManagerAssociations {
    pub(super) async fn source_freeze(
        &self,
        raw: &str,
        config: &DirectoryConfig,
    ) -> Result<serde_json::Value> {
        let signed: SignedSourceRevocationEventV1 =
            serde_json::from_str(raw).map_err(|_| denied())?;
        let pins = SourcePins::load(&self.env)?;
        let principal = pins.verify(&signed, config).await?;
        let event_digest = signed.digest().map_err(|_| denied())?;
        let event = &signed.event;
        let existing = self.snapshot_for(config, &principal, true, false)?;
        if let Some(saved) = existing.source_closures.get(&event_digest) {
            let ack = &saved.ack;
            if !existing.inventory_complete
                || existing.index.closure_status() != ManagerClosureStatus::Closed
                || ack.key_id != pins.owner_key_id
                || ack.event_id != event.event_id
                || ack.event_digest != event_digest
                || ack.identity_namespace != event.identity_namespace
                || ack.environment != event.environment
                || ack.identity_instance_epoch != event.identity_instance_epoch
                || ack.manager_account_id != event.manager_account_id
                || ack.owner_audience != event.owner_audience
            {
                return Err(denied());
            }
            crate::approval_crypto::verify_ed25519(
                &hex::encode(pins.owner_key.verifying_key().as_bytes()),
                &saved.signature,
                &ack.signing_bytes().map_err(|_| denied())?,
            )
            .await
            .map_err(|_| denied())?;
            return Ok(serde_json::to_value(Some(saved))?);
        }
        // Every source event joins the first permanent closure, rather than replacing its notice.
        let state = self.transaction_for(
            config,
            &principal,
            "source_revocation_received",
            || Ok(()),
            |s| {
                if s.source_closures.len() >= 256 {
                    return Err(denied());
                }
                if s.index.revocation().is_none() {
                    s.index
                        .begin_revocation(ManagerRevocationNotice {
                            operation_id: event.event_id.clone(),
                            manager: principal.clone(),
                            environment: config.environment.clone(),
                            identity_instance_epoch: config.epoch,
                            source_security_version: event.manager_security_version,
                            source_credential_ref: None,
                            source_credential_version: None,
                        })
                        .map_err(|_| denied())?;
                }
                Ok(())
            },
        )?;
        for target in state
            .index
            .pending_freeze_targets()
            .take(FANOUT_BATCH)
            .cloned()
            .collect::<Vec<_>>()
        {
            if let Ok(value) = self
                .account("/internal/freeze", &target, &target.custody_account_id)
                .await
                && let Ok(receipt) = serde_json::from_value::<AccountFreezeReceipt>(value)
                && receipt.target == target
            {
                let _ = self.transaction_for(
                    config,
                    &principal,
                    "source_account_frozen",
                    || Ok(()),
                    |s| {
                        s.index
                            .record_freeze_receipt(receipt.clone())
                            .map_err(|_| denied())
                    },
                );
            }
        }
        let current = self.snapshot_for(config, &principal, false, false)?;
        if !current.inventory_complete
            || current.index.closure_status() != ManagerClosureStatus::Closed
        {
            return Ok(serde_json::Value::Null);
        }
        // Signing and storing the acknowledgement happens inside the same synchronous transition.
        let saved = self.transaction_for(
            config,
            &principal,
            "source_closure_acknowledged",
            || Ok(()),
            |s| {
                if s.source_closures.contains_key(&event_digest) {
                    return Ok(());
                }
                if !s.inventory_complete
                    || s.index.closure_status() != ManagerClosureStatus::Closed
                    || s.source_closures.len() >= 256
                {
                    return Err(denied());
                }
                let ack = SourceClosureAckV1 {
                    version: 1,
                    key_id: pins.owner_key_id.clone(),
                    event_id: event.event_id.clone(),
                    event_digest: event_digest.clone(),
                    identity_namespace: event.identity_namespace.clone(),
                    environment: event.environment.clone(),
                    identity_instance_epoch: event.identity_instance_epoch,
                    manager_account_id: event.manager_account_id.clone(),
                    owner_audience: event.owner_audience.clone(),
                    closure_digest: digest("oasis7.custody.source-closure.v1", &s.index)?,
                    inventory_digest: digest("oasis7.custody.source-inventory.v1", &s.drafts)?,
                    closed_at: DecimalU64(clock()?.max(event.observed_at.0)),
                    status: SourceClosureStateV1::Closed,
                };
                let signature = URL_SAFE_NO_PAD.encode(
                    pins.owner_key
                        .sign(&ack.signing_bytes().map_err(|_| denied())?)
                        .to_bytes(),
                );
                s.source_closures.insert(
                    event_digest.clone(),
                    SignedSourceClosureAckV1 { ack, signature },
                );
                Ok(())
            },
        )?;
        Ok(serde_json::to_value(
            saved.source_closures.get(&event_digest),
        )?)
    }
}

pub(crate) async fn call(env: &Env, raw: &str) -> Result<String> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let signed: SignedSourceRevocationEventV1 = serde_json::from_str(raw).map_err(|_| denied())?;
    let config = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
    let principal = SourcePins::load(env)?.verify(&signed, &config).await?;
    let name = digest(
        "oasis7.custody.manager-directory.v1",
        &(&config.environment, &principal),
    )?;
    let stub = env
        .durable_object("MANAGER_ASSOCIATIONS")?
        .id_from_name(&name)?
        .get_stub()?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let response = stub
        .fetch_with_request(Request::new_with_init(
            "https://directory.internal/internal/source-freeze",
            &init,
        )?)
        .await?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    read_response(response).await
}
