//! New consent starts inside the independent restore lineage, before fresh UV.
//! Legacy metadata records never acquire this provenance by being read later.
use super::*;
use oasis7_custody_core::native_cleanup_v2::{
    NativeStorageHeadV2, native_guarded_consent_request_digest_v2,
};
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GuardedConsentUseV2 {
    TermWideCleanup,
    NativeModelProxy,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GuardedConsentProducerV2 {
    GuardedOptionsBeforeUvV2,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GuardedConsentProvenanceV2 {
    producer: GuardedConsentProducerV2,
    uses: Vec<GuardedConsentUseV2>,
    options_head: NativeStorageHeadV2,
    request_digest: String,
    native_configuration_digest: String,
    #[serde(deserialize_with = "required_nullable")]
    pub(super) cleanup_configuration_digest: Option<String>,
}
fn required_nullable<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
fn unavailable() -> StoreError {
    reject("guarded_consent_unavailable")
}
pub(super) struct GuardedConsentOptionsV2 {
    uses: Vec<GuardedConsentUseV2>,
    native_configuration_digest: String,
    cleanup_configuration_digest: Option<String>,
}
fn native_configuration_digest(
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
) -> Result<String, StoreError> {
    let sid_config = super::super::digest(
        &identity.session_key,
        "guarded-consent-session-configuration-v2",
        "fixed",
    );
    let bytes = serde_json::to_vec(&(
        "oasis7.custody.guarded-consent-config.v2",
        &config.key_id,
        hex::encode(config.key.verifying_key().to_bytes()),
        &config.audience,
        config.full_session_v2_summary_contract_enabled,
        config.model_budget_summary_contract_enabled,
        identity.namespace.key(),
        &identity.rp.rp_id,
        &identity.rp.allowed_origins,
        identity.rp.version,
        identity.rp.identity_instance_epoch,
        sid_config,
    ))
    .map_err(|_| unavailable())?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}
// Explicit World3 cannot fall through ordinary consent without fixed consumer configuration.
pub(super) fn require_supported_profile(
    config: &CustodyConfig,
    request: &ApprovalOptionsRequestV1,
) -> Result<(), StoreError> {
    if matches!(&request.intent, CustodyIntentV1::TestSessionV2 { plan, .. }
        if plan.termination.cleanup_transport_policy == oasis7_custody_core::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2)
    {
        if !config.full_session_v2_summary_contract_enabled {
            return Err(unavailable());
        }
        // This explicit new producer requires its own fixed-purpose pin and
        // the same protected options-before-MFA lineage as the old owned mode.
        super::cleanup_config::MachineConfig::from_env()?.preflight_access_pin()?;
    }
    Ok(())
}
pub(super) fn options_binding(
    service: &IdentityService,
    request: &ApprovalOptionsRequestV1,
    summary: Option<&NativeCustodySummaryContractV1>,
) -> Result<Option<GuardedConsentOptionsV2>, StoreError> {
    let mut uses = Vec::new();
    let cleanup = matches!(&request.intent,
        CustodyIntentV1::TestSessionV2 { plan, .. }
        if matches!(plan.termination.cleanup_transport_policy, oasis7_custody_core::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedV2 | oasis7_custody_core::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2))
        && std::env::var("OASIS7_CUSTODY_CLEANUP_MACHINE_V2_ENABLED")
            .ok()
            .as_deref()
            == Some("true");
    let proxy = matches!(
        summary,
        Some(NativeCustodySummaryContractV1::FullTestSessionProxyV2(_))
    );
    if !cleanup && !proxy {
        return Ok(None);
    }
    let config = service.custody.as_ref().ok_or_else(unavailable)?;
    let cleanup_configuration_digest = if cleanup {
        uses.push(GuardedConsentUseV2::TermWideCleanup);
        let machine = super::cleanup_config::MachineConfig::from_env()?;
        let gateway = service
            .participant_entry
            .as_ref()
            .ok_or_else(unavailable)?
            .owned_owner_read_gateway();
        Some(machine.fingerprint(config, &service.config, &gateway.fingerprint()?)?)
    } else {
        None
    };
    if proxy {
        uses.push(GuardedConsentUseV2::NativeModelProxy);
    }
    Ok(Some(GuardedConsentOptionsV2 {
        uses,
        cleanup_configuration_digest,
        native_configuration_digest: native_configuration_digest(config, &service.config)?,
    }))
}
impl GuardedConsentOptionsV2 {
    pub(super) fn provenance(
        &self,
        tx: &IdentityTransaction<'_>,
        record: &ApprovalRecord,
    ) -> Result<GuardedConsentProvenanceV2, StoreError> {
        let head = tx.machine_storage_head().ok_or_else(unavailable)?.clone();
        head.validate().map_err(|_| unavailable())?;
        Ok(GuardedConsentProvenanceV2 {
            producer: GuardedConsentProducerV2::GuardedOptionsBeforeUvV2,
            uses: self.uses.clone(),
            options_head: head,
            request_digest: native_guarded_consent_request_digest_v2(
                &record.request,
                record.model_budget_summary_contract.as_ref(),
            )
            .map_err(|_| unavailable())?,
            native_configuration_digest: self.native_configuration_digest.clone(),
            cleanup_configuration_digest: self.cleanup_configuration_digest.clone(),
        })
    }
}
/// Validates the original producer/lineage, never current user authority. The
/// caller separately checks full proof signatures, current sessions and scope.
pub(super) fn require_guarded_consent_v2(
    tx: &IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    record: &ApprovalRecord,
    required: GuardedConsentUseV2,
    cleanup_configuration: Option<&str>,
) -> Result<(), StoreError> {
    let tag = record
        .guarded_consent_provenance
        .as_ref()
        .ok_or_else(unavailable)?;
    let head = tx.machine_storage_head().ok_or_else(unavailable)?;
    tag.options_head.validate().map_err(|_| unavailable())?;
    if tag.producer != GuardedConsentProducerV2::GuardedOptionsBeforeUvV2
        || !(1..=2).contains(&tag.uses.len())
        || !tag.uses.contains(&required)
        || tag.uses.len() == 2 && tag.uses[0] == tag.uses[1]
        || tag.options_head.instance_id != head.instance_id
        || tag.options_head.revision.0 > head.revision.0
        || tag.options_head.revision == head.revision
            && tag.options_head.commitment != head.commitment
        || tag.request_digest
            != native_guarded_consent_request_digest_v2(
                &record.request,
                record.model_budget_summary_contract.as_ref(),
            )
            .map_err(|_| unavailable())?
        || tag.native_configuration_digest != native_configuration_digest(config, identity)?
        || matches!(required, GuardedConsentUseV2::TermWideCleanup)
            && (cleanup_configuration.is_none()
                || tag.cleanup_configuration_digest.as_deref() != cleanup_configuration)
        || matches!(required, GuardedConsentUseV2::NativeModelProxy)
            && !matches!(
                record.model_budget_summary_contract,
                Some(NativeCustodySummaryContractV1::FullTestSessionProxyV2(_))
            )
    {
        return Err(unavailable());
    }
    Ok(())
}
/// Complete and its exact historical retry cannot escape a lineage that was
/// already required at options. Missing legacy provenance remains legacy data.
pub(super) fn verify_existing_guarded_consent_v2(
    tx: &IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    record: &ApprovalRecord,
) -> Result<(), StoreError> {
    require_supported_profile(config, &record.request)?;
    let Some(tag) = &record.guarded_consent_provenance else {
        if matches!(&record.request.intent, CustodyIntentV1::TestSessionV2 { plan, .. }
            if plan.termination.cleanup_transport_policy == oasis7_custody_core::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2)
        {
            return Err(unavailable());
        }
        return Ok(());
    };
    if !(1..=2).contains(&tag.uses.len()) || tag.uses.len() == 2 && tag.uses[0] == tag.uses[1] {
        return Err(unavailable());
    }
    for use_case in &tag.uses {
        require_guarded_consent_v2(
            tx,
            config,
            identity,
            record,
            *use_case,
            tag.cleanup_configuration_digest.as_deref(),
        )?;
    }
    Ok(())
}
