//! A separately committed physical claim precedes every fixed World send.
use super::resource_final_config::ResourceFinalConfigV2;
use super::*;
use oasis7_custody_core::native_binding::ManagedNativeSessionLifetimeV1;
use oasis7_custody_core::provisioning::{
    OwnerSetupResourceFinalHttpV2, PaidResourceReadIssuerFloorV1,
};
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ResourceFinalSubstepV2 {
    NativeOwnerControl,
    WorldOwnerPreflight,
    WorldResourceFinal,
}
impl ResourceFinalSubstepV2 {
    fn label(self) -> &'static str {
        match self {
            Self::NativeOwnerControl => "native_owner_control",
            Self::WorldOwnerPreflight => "world_owner_preflight",
            Self::WorldResourceFinal => "world_resource_final",
        }
    }
}
pub(super) struct ResourceFinalScopeV2<'a> {
    pub(super) custody: &'a CustodyConfig,
    pub(super) identity: &'a super::super::config::IdentityConfig,
    pub(super) resource: &'a ResourceFinalConfigV2,
    pub(super) configuration_digest: &'a str,
}
pub(super) struct ResourceNativeInvocationV2 {
    frame: OwnerSetupResourceFinalHttpV2,
    storage_key: String,
    configuration_digest: String,
}
pub(super) struct ObservedResourceInvocationV2 {
    invocation: ResourceNativeInvocationV2,
    baseline: ManagedNativeSessionLifetimeV1,
}
impl ResourceNativeInvocationV2 {
    pub(super) fn frame(&self) -> &OwnerSetupResourceFinalHttpV2 {
        &self.frame
    }
}
impl ObservedResourceInvocationV2 {
    pub(super) fn frame(&self) -> &OwnerSetupResourceFinalHttpV2 {
        &self.invocation.frame
    }
    pub(super) fn valid_until(&self) -> u64 {
        self.invocation
            .frame
            .paid()
            .payload
            .expires_at
            .0
            .min(self.baseline.session_expires_at.0)
            .min(self.baseline.session_idle_expires_at.0)
    }
}
fn unavailable() -> StoreError {
    reject("resource_final_unavailable")
}
fn verify_floor(
    tx: &mut IdentityTransaction<'_>,
    s: &ResourceFinalScopeV2<'_>,
    f: &OwnerSetupResourceFinalHttpV2,
    now: u64,
) -> Result<(), StoreError> {
    let CustodyIntentV1::OwnerSetupV2 { plan } = &f.input().request.intent else {
        return Err(unavailable());
    };
    let floor = super::cleanup_registry::admit_issuer_floor(
        tx,
        &s.resource.issuer_pin,
        &s.resource.issuer_floor,
    )?;
    f.paid()
        .verify_issuer(
            &s.resource.paid_pin(&plan.world_identity),
            &PaidResourceReadIssuerFloorV1 {
                issuer_epoch: floor.issuer_epoch,
                config_generation: floor.config_generation,
            },
            now,
        )
        .map_err(|_| unavailable())
}
fn physical(
    tx: &mut IdentityTransaction<'_>,
    s: &ResourceFinalScopeV2<'_>,
    f: &OwnerSetupResourceFinalHttpV2,
    step: ResourceFinalSubstepV2,
) -> Result<(), StoreError> {
    let p = &f.paid().payload;
    let bytes = serde_json::to_vec(&(
        "oasis7.custody.owner-resource-physical.v2",
        &p.custody_account_id,
        &p.paid_attempt_id,
        step,
    ))
    .map_err(|_| unavailable())?;
    let attempt = format!("owner-resource-v2:{}", blake3::hash(&bytes).to_hex());
    let proof = serde_json::to_vec(f.paid()).map_err(|_| unavailable())?;
    super::cleanup_registry::claim_physical_attempt(
        tx,
        &s.resource.issuer(),
        &attempt,
        &blake3::hash(&proof).to_hex(),
        step.label(),
    )
}
pub(super) fn claim_native(
    tx: &mut IdentityTransaction<'_>,
    s: &ResourceFinalScopeV2<'_>,
    frame: OwnerSetupResourceFinalHttpV2,
    storage_key: String,
    now: u64,
) -> Result<ResourceNativeInvocationV2, StoreError> {
    frame.validate_bindings().map_err(|_| unavailable())?;
    verify_floor(tx, s, &frame, now)?;
    super::resource_final_consent::historical(tx, s.custody, s.identity, frame.input())?;
    // Read the original Cookie's identity without treating it as Current yet.
    let session = tx
        .get::<AccountSession>(&storage_key)?
        .ok_or_else(unavailable)?
        .value;
    if session.account_id != frame.paid().payload.participant_native_account_id
        || session.session_id != frame.participant().session.session_ref
    {
        return Err(unavailable());
    }
    physical(tx, s, &frame, ResourceFinalSubstepV2::NativeOwnerControl)?;
    Ok(ResourceNativeInvocationV2 {
        frame,
        storage_key,
        configuration_digest: s.configuration_digest.into(),
    })
}
fn current(
    tx: &mut IdentityTransaction<'_>,
    s: &ResourceFinalScopeV2<'_>,
    i: &ResourceNativeInvocationV2,
    now: u64,
) -> Result<ManagedNativeSessionLifetimeV1, StoreError> {
    if i.configuration_digest != s.configuration_digest
        || i.frame.paid().payload.resource_flag_generation != s.resource.resource_feature_generation
    {
        return Err(unavailable());
    }
    verify_floor(tx, s, &i.frame, now)?;
    super::resource_final_consent::current(
        tx,
        s.custody,
        s.identity,
        i.frame.input(),
        s.configuration_digest,
        now,
    )?;
    let observed = managed_binding::observe_managed_lifetime(
        tx,
        &i.storage_key,
        s.custody,
        s.identity,
        &i.frame.paid().payload.native_request_digest,
        now,
    )?;
    observed.validate().map_err(|_| unavailable())?;
    let participant = i.frame.participant();
    if !observed.status.active
        || observed.status.frozen
        || observed.status.pending
        || observed.status.receipt != participant.native_binding
        || observed.status.session_ref != participant.session.session_ref
        || observed.status.generation != participant.session.native_binding_generation
    {
        return Err(unavailable());
    }
    Ok(observed)
}
pub(super) fn claim_preflight(
    tx: &mut IdentityTransaction<'_>,
    s: &ResourceFinalScopeV2<'_>,
    invocation: ResourceNativeInvocationV2,
    now: u64,
) -> Result<ObservedResourceInvocationV2, StoreError> {
    let baseline = current(tx, s, &invocation, now)?;
    physical(
        tx,
        s,
        &invocation.frame,
        ResourceFinalSubstepV2::WorldOwnerPreflight,
    )?;
    Ok(ObservedResourceInvocationV2 {
        invocation,
        baseline,
    })
}
pub(super) fn claim_resource(
    tx: &mut IdentityTransaction<'_>,
    s: &ResourceFinalScopeV2<'_>,
    observed: ObservedResourceInvocationV2,
    now: u64,
) -> Result<ObservedResourceInvocationV2, StoreError> {
    let actual = current(tx, s, &observed.invocation, now)?;
    if !super::cleanup_registry::same_security(&observed.baseline, &actual) {
        return Err(unavailable());
    }
    physical(
        tx,
        s,
        &observed.invocation.frame,
        ResourceFinalSubstepV2::WorldResourceFinal,
    )?;
    Ok(observed)
}
pub(super) fn finish(
    tx: &mut IdentityTransaction<'_>,
    s: &ResourceFinalScopeV2<'_>,
    observed: ObservedResourceInvocationV2,
    now: u64,
) -> Result<(), StoreError> {
    let actual = current(tx, s, &observed.invocation, now)?;
    if !super::cleanup_registry::same_security(&observed.baseline, &actual) {
        return Err(unavailable());
    }
    Ok(())
}
