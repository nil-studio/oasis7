//! Offline certificate-bound checkpoint verification; dispatches before runtime startup.
#[cfg(not(test))]
use super::execution_bridge::controlled_bootstrap_anchor::{
    VerifiedBootstrapAnchor, verify_bootstrap_anchor,
};
#[cfg(test)]
use super::execution_bridge_real_tests::real_execution_bridge::controlled_bootstrap_anchor::{
    VerifiedBootstrapAnchor, verify_bootstrap_anchor,
};
use oasis7_distfs::controlled_authority::{
    activation::{InitialActivationPolicy, verify_initial_activation},
    replicated_protocol::{ArtifactRole, DurabilityEvidence, FixedTrust, HeadAnchor},
};
use oasis7_runtime::ReleaseSecurityPolicy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedBootstrapConfiguration {
    schema_version: u32,
    issuer_policy: IssuerPolicyConfiguration,
    minimum_head: HeadAnchor,
    release_security_policy: SecurityPolicyConfiguration,
}
/// Wire declaration only: the operator must authenticate this policy independently.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IssuerPolicyConfiguration {
    trust: FixedTrust,
    issuer_public_key: String,
    initial_state_root: String,
    execution_manifest_root: String,
    activation_height: u64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SecurityPolicyConfiguration {
    allow_builtin_manifest_fallback: bool,
    allow_identity_hash_signature: bool,
    allow_local_finality_signing: bool,
    allow_runtime_source_compile: bool,
}
impl From<SecurityPolicyConfiguration> for ReleaseSecurityPolicy {
    fn from(p: SecurityPolicyConfiguration) -> Self {
        Self {
            allow_builtin_manifest_fallback: p.allow_builtin_manifest_fallback,
            allow_identity_hash_signature: p.allow_identity_hash_signature,
            allow_local_finality_signing: p.allow_local_finality_signing,
            allow_runtime_source_compile: p.allow_runtime_source_compile,
        }
    }
}
#[derive(Debug, Serialize)]
struct BootstrapVerification {
    schema_version: u32,
    scope: &'static str,
    claim: &'static str,
    trust_basis: &'static str,
    world_id: String,
    chain_id: String,
    genesis_digest: String,
    authority_epoch: u64,
    qualified_head: HeadAnchor,
    envelope_digest: String,
    execution_height: u64,
    simulation_tick: u64,
    execution_state_root: String,
    journal_len: usize,
    module_count: usize,
}
pub(crate) fn load_verified_bootstrap(
    config_path: &std::path::Path,
    evidence_path: &std::path::Path,
) -> Result<
    (
        VerifiedBootstrapAnchor,
        InitialActivationPolicy,
        ReleaseSecurityPolicy,
    ),
    String,
> {
    let config: TrustedBootstrapConfiguration = serde_json::from_slice(
        &super::controlled_history_cli::read_bounded(config_path, 64 * 1024)?,
    )
    .map_err(|e| format!("trusted bootstrap configuration: {e}"))?;
    if config.schema_version != 1 {
        return Err("unsupported trusted bootstrap configuration version".into());
    }
    let evidence: DurabilityEvidence = serde_json::from_slice(
        &super::controlled_history_cli::read_bounded(evidence_path, 64 * 1024 * 1024)?,
    )
    .map_err(|e| format!("original bootstrap evidence: {e}"))?;
    let record = &evidence.proposal.body.record;
    let input_root = record
        .roots
        .get(&ArtifactRole::Input)
        .ok_or("missing original activation Input")?;
    let input = record
        .objects
        .iter()
        .find(|o| &o.content_hash == input_root)
        .ok_or("missing original activation envelope bytes")?;
    let p = config.issuer_policy;
    let policy = InitialActivationPolicy {
        trust: p.trust,
        issuer_public_key: p.issuer_public_key,
        initial_state_root: p.initial_state_root,
        execution_manifest_root: p.execution_manifest_root,
        activation_height: p.activation_height,
    };
    let activation =
        verify_initial_activation(&input.bytes, &evidence, &policy, &config.minimum_head)
            .map_err(|e| format!("initial activation evidence: {e}"))?;
    let security: ReleaseSecurityPolicy = config.release_security_policy.into();
    let verified = verify_bootstrap_anchor(&activation, record, &security)?;
    Ok((verified, policy, security))
}

pub(super) fn run<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut config_path = None;
    let mut evidence_path = None;
    while let Some(arg) = args.next() {
        let destination = match arg {
            "--trusted-config" => &mut config_path,
            "--evidence" => &mut evidence_path,
            _ => return Err(format!("unknown bootstrap verifier argument: {arg}")),
        };
        if destination.is_some() {
            return Err(format!("duplicate bootstrap verifier argument: {arg}"));
        }
        *destination = Some(PathBuf::from(
            args.next()
                .ok_or("missing bootstrap verifier argument value")?,
        ));
    }
    let (verified, policy, _) = load_verified_bootstrap(
        &config_path.ok_or("required --trusted-config")?,
        &evidence_path.ok_or("required --evidence")?,
    )?;
    let activation = verified.activation();
    let result = BootstrapVerification {
        schema_version: 1,
        scope: "offline_certificate_bound_no_state_change_bootstrap_prerequisite",
        claim: "verified_checkpoint_prerequisite_not_runtime_activation_readiness_or_formal_commit",
        trust_basis: "caller_independently_authenticated_issuer_policy_not_config_self_authentication_or_independent_lineage_fault_domains",
        world_id: policy.trust.world_id,
        chain_id: policy.trust.chain_id,
        genesis_digest: policy.trust.genesis_digest,
        authority_epoch: policy.trust.authority_epoch,
        qualified_head: activation.qualified_head().clone(),
        envelope_digest: activation.envelope_digest().to_string(),
        execution_height: verified.execution_height(),
        simulation_tick: verified.simulation_tick(),
        execution_state_root: activation.body().after_state_root.clone(),
        journal_len: verified.journal().len(),
        module_count: verified.snapshot().module_registry.records.len(),
    };
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
#[cfg(all(test, unix))]
#[path = "controlled_bootstrap_cli_tests.rs"]
mod tests;
