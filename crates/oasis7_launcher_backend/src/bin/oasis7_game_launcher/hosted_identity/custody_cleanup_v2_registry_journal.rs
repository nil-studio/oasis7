//! Durable physical attempts and monotonic configured issuer observations.
use super::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhysicalAttemptClaim {
    api_version: u8,
    certificate_digest: String,
    stage: String,
}
pub(in super::super) fn claim_physical_attempt(
    tx: &mut IdentityTransaction<'_>,
    issuer: &AccountMachineIssuerV2,
    attempt: &str,
    digest: &str,
    stage: &str,
) -> Result<(), StoreError> {
    // Key rotation cannot make the same physical prepayment spendable twice.
    let material = serde_json::to_vec(&(
        issuer.namespace.as_str(),
        issuer.service_id.as_str(),
        issuer.environment.as_str(),
        attempt,
    ))
    .map_err(|_| unavailable())?;
    let key = format!(
        "custody_cleanup_physical_attempt_v2:{}",
        blake3::hash(&material).to_hex()
    );
    if tx.get::<PhysicalAttemptClaim>(&key)?.is_some() {
        return Err(unavailable());
    }
    tx.create(
        &key,
        "custody_cleanup_physical_attempt_v2",
        None,
        None,
        &PhysicalAttemptClaim {
            api_version: 2,
            certificate_digest: digest.into(),
            stage: stage.into(),
        },
    )
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedIssuerFloor {
    api_version: u8,
    floor: AccountMachineIssuerFloorV2,
}
pub(in super::super) fn admit_issuer_floor(
    tx: &mut IdentityTransaction<'_>,
    pin: &AccountMachineIssuerPinV2,
    configured: &AccountMachineIssuerFloorV2,
) -> Result<AccountMachineIssuerFloorV2, StoreError> {
    tx.machine_storage_head().ok_or_else(unavailable)?;
    let material = serde_json::to_vec(&(&pin.namespace, &pin.service_id, &pin.environment))
        .map_err(|_| unavailable())?;
    let key = format!(
        "custody_cleanup_issuer_floor_v2:{}",
        blake3::hash(&material).to_hex()
    );
    let row = tx.get::<PersistedIssuerFloor>(&key)?;
    let prior = row.as_ref().map(|r| &r.value.floor);
    if row.as_ref().is_some_and(|r| r.value.api_version != 2)
        || prior.is_some_and(|p| {
            pin.issuer_epoch.0 < p.issuer_epoch.0 || pin.config_generation.0 < p.config_generation.0
        })
        || pin.issuer_epoch.0 < configured.issuer_epoch.0
        || pin.config_generation.0 < configured.config_generation.0
    {
        return Err(unavailable());
    }
    let floor = AccountMachineIssuerFloorV2 {
        issuer_epoch: pin.issuer_epoch,
        config_generation: pin.config_generation,
    };
    let next = PersistedIssuerFloor {
        api_version: 2,
        floor: floor.clone(),
    };
    match row {
        Some(row) if row.value.floor != floor => tx.replace(
            &key,
            row.row_version,
            "custody_cleanup_issuer_floor_v2",
            None,
            None,
            &next,
        )?,
        None => tx.create(&key, "custody_cleanup_issuer_floor_v2", None, None, &next)?,
        Some(_) => {}
    }
    Ok(floor)
}
