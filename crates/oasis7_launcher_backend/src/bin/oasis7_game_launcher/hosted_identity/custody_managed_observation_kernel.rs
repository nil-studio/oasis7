//! Shared actual Native managed-SID observation kernel. No Cloud private SID
//! generation, Account consume state or Owner signing authority is inferred.
use super::*;
pub(in super::super::super) fn observe_managed_lifetime(
    tx: &mut IdentityTransaction<'_>,
    storage_key: &str,
    config: &CustodyConfig,
    identity: &super::super::super::config::IdentityConfig,
    nonce: &str,
    now: u64,
) -> Result<ManagedNativeSessionLifetimeV1, StoreError> {
    let epoch = identity.rp.identity_instance_epoch;
    let namespace = identity.namespace.key();
    // A revoked original cookie may read only its own non-secret inactive fact.
    let session = tx
        .get::<AccountSession>(storage_key)?
        .ok_or_else(|| reject("session_expired"))?
        .value;
    let binding = tx
        .get::<ManagedBindingRecord>(&format!("custody_managed_binding:{}", session.account_id))?
        .ok_or_else(|| reject("authentication_failed"))?
        .value;
    let receipt = &binding.receipt.receipt;
    if session.origin_credential_id != receipt.native_credential_ref
        || session.auth_method != "native_key"
    {
        return Err(reject("authentication_failed"));
    }
    let account = tx
        .get::<AccountRecord>(&format!("account:{}", session.account_id))?
        .ok_or_else(|| reject("authentication_failed"))?
        .value;
    let credential = tx
        .get::<CredentialRecord>(&credential_key(&receipt.native_credential_ref))?
        .ok_or_else(|| reject("authentication_failed"))?
        .value;
    let pending_key = format!("custody_managed_invalidation:{}", session.account_id);
    let mut pending = tx.get::<ManagedInvalidationPending>(&pending_key)?;
    if (receipt.identity_instance_epoch.0 != epoch || receipt.identity_namespace != namespace)
        && pending.is_none()
    {
        let fact = ManagedInvalidationPending {
            binding_operation_id: receipt.binding_operation_id.clone(),
            native_account_id: session.account_id.clone(),
            generation: 2,
            frozen: true,
            pending: true,
            invalidation: ManagedNativeInvalidationV1 {
                api_version: 1,
                receipt: receipt.clone(),
                service_epoch: binding.statement.statement.service_epoch,
                native_account_security_epoch: DecimalU64(account.security_epoch),
                native_credential_version: DecimalU64(credential.version),
                session_ref: Some(session.session_id.clone()),
                generation: DecimalU64(2),
                reason: "namespace_changed".into(),
                sequence: DecimalU64(1),
                changed_at: DecimalU64(now),
            },
        };
        tx.create(
            &pending_key,
            "custody_managed_invalidation",
            Some(&session.account_id),
            None,
            &fact,
        )?;
        pending = tx.get::<ManagedInvalidationPending>(&pending_key)?;
    }
    let pin_current = owner_pin::binding_is_current(tx, config, identity, &binding);
    let active = pin_current
        && valid_session(tx, storage_key, epoch, now).is_ok()
        && credential.state == "active"
        && account.status == "active"
        && matches!(credential.public_material, CredentialMaterial::NativeKey { public_key } if hex::encode(public_key) == receipt.login_public_key)
        && pending.is_none();
    let status = ManagedNativeSessionStatusV1 {
        api_version: 1,
        receipt: receipt.clone(),
        service_epoch: binding.statement.statement.service_epoch,
        native_account_security_epoch: DecimalU64(account.security_epoch),
        native_credential_version: DecimalU64(credential.version),
        session_ref: session.session_id,
        nonce: nonce.into(),
        generation: DecimalU64(pending.as_ref().map_or(1, |p| p.value.generation)),
        active,
        frozen: !pin_current || pending.as_ref().is_some_and(|p| p.value.frozen),
        pending: !pin_current || pending.is_some(),
        observed_at: DecimalU64(now),
        expires_at: DecimalU64(now.checked_add(5_000).ok_or(StoreError::Corrupt)?),
    };
    Ok(ManagedNativeSessionLifetimeV1 {
        api_version: 1,
        status,
        session_expires_at: DecimalU64(session.expires_at),
        session_idle_expires_at: DecimalU64(session.idle_expires_at),
    })
}
