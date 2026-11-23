//! Only signed, checked permission chains can retain an earlier origin across
//! an active epoch that has not qualified its first completion.
use super as formal;
use super::fencing::{EpochSealReceiptV1, TrustedFencingPolicyV1, verify_epoch_seal};
use super::issuer::{IssuerAnchorV1, SignedIssuanceWitnessV1};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPermissionBootstrapV1 {
    pub predecessor_checkpoint: formal::FormalReadCheckpointV1,
    pub predecessor_checkpoint_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_context_version: Option<u32>,
    pub certificate: formal::SignedRecoveryCertificateV1,
    pub fencing: FencingBootstrapV1,
    pub tail: formal::RecoveryTailWitnessV1,
    pub issuer_witness: SignedIssuanceWitnessV1,
    pub minimum_issuer_anchor: IssuerAnchorV1,
    #[serde(default)]
    pub prior_permission: Option<Box<RecoveryPermissionBootstrapV1>>,
}
pub struct RecoveryPermissionViewV1<'a> {
    pub checkpoint: &'a formal::FormalReadCheckpointV1,
    pub checkpoint_digest: &'a str,
    pub predecessor_context_version: Option<u32>,
    pub certificate: &'a formal::SignedRecoveryCertificateV1,
    pub fencing: &'a FencingBootstrapV1,
    pub tail: &'a formal::RecoveryTailWitnessV1,
    pub witness: &'a SignedIssuanceWitnessV1,
    pub minimum: &'a IssuerAnchorV1,
    pub prior: Option<&'a RecoveryPermissionBootstrapV1>,
}
#[derive(Clone)]
pub struct VerifiedRecoveryBootstrapV1 {
    active_predecessor: formal::VerifiedAuthorityV1,
    cut: formal::VerifiedFormalCommitV1,
    recovery: formal::VerifiedRecovery,
    external_anchor: formal::VerifiedFormalReadAnchorV1,
    retained_context: Option<formal::VerifiedActiveRecoveryContextV2>,
    evidence: RecoveryPermissionBootstrapV1,
    external_issuer_minimum: Option<IssuerAnchorV1>,
}
impl RecoveryPermissionBootstrapV1 {
    fn view(&self) -> RecoveryPermissionViewV1<'_> {
        RecoveryPermissionViewV1 {
            checkpoint: &self.predecessor_checkpoint,
            checkpoint_digest: &self.predecessor_checkpoint_digest,
            predecessor_context_version: self.predecessor_context_version,
            certificate: &self.certificate,
            fencing: &self.fencing,
            tail: &self.tail,
            witness: &self.issuer_witness,
            minimum: &self.minimum_issuer_anchor,
            prior: self.prior_permission.as_deref(),
        }
    }
}
pub fn verify_recovery_permission(
    view: RecoveryPermissionViewV1<'_>,
    trust: &formal::InitialActivationTrustV1,
    initial: &formal::SignedInitialActivationV1,
) -> Result<VerifiedRecoveryBootstrapV1, String> {
    verify_permission_depth(view, trust, initial, 0)
}
const MAX_RECOVERY_PERMISSION_DEPTH: usize = 128;
fn verify_permission_depth(
    view: RecoveryPermissionViewV1<'_>,
    trust: &formal::InitialActivationTrustV1,
    initial: &formal::SignedInitialActivationV1,
    depth: usize,
) -> Result<VerifiedRecoveryBootstrapV1, String> {
    if depth >= MAX_RECOVERY_PERMISSION_DEPTH {
        return Err("recovery permission chain depth bound".into());
    }
    if &view.checkpoint.initial_certificate != initial {
        return Err("recovery bootstrap initial certificate mismatch".into());
    }
    match (view.certificate.body.schema_version, view.prior.is_some()) {
        (1 | 3, false) | (2, true) => {}
        _ => return Err("recovery schema/prior permission mismatch".into()),
    }
    let previous = view
        .prior
        .map(|prior| verify_permission_depth(prior.view(), trust, initial, depth + 1))
        .transpose()?;
    let anchor = match view.predecessor_context_version {
        None => formal::VerifiedFormalReadAnchorV1::from_fixed_pin(
            view.checkpoint,
            view.checkpoint_digest,
            trust,
        )
        .map_err(|e| e.to_string())?,
        Some(2) if view.certificate.body.schema_version == 2 => {
            let previous = previous
                .as_ref()
                .ok_or("retained predecessor context requires prior permission")?;
            let derived = previous
                .external_anchor
                .with_pending_recovery(&previous.recovery)
                .map_err(|e| e.to_string())?;
            if derived.checkpoint() != *view.checkpoint
                || view.checkpoint.digest().map_err(|e| e.to_string())? != view.checkpoint_digest
            {
                return Err(
                    "retained predecessor context differs from checked prior successor/cut".into(),
                );
            }
            derived
        }
        _ => return Err("unsupported recovery predecessor context version".into()),
    };
    let active = anchor.authority();
    let context = match view.certificate.body.schema_version {
        1 | 3 if view.prior.is_none() => None,
        2 => {
            let previous = previous
                .as_ref()
                .ok_or("schema2 recovery requires prior verified permission")?;
            let retained_floor = &previous.evidence.issuer_witness.body.current_anchor;
            require_forward_floor(view.minimum, retained_floor)?;
            require_forward_floor(&view.witness.body.request.minimum_anchor, retained_floor)?;
            if previous
                .recovery
                .successor()
                .authority_id()
                .map_err(|error| error.to_string())?
                != active.authority_id().map_err(|error| error.to_string())?
                || previous.cut.record() != anchor.head().record()
                || previous.cut.finalizing_authority_id() != anchor.head().finalizing_authority_id()
            {
                return Err(
                    "schema2 prior permission differs from pinned active authority/cut".into(),
                );
            }
            let original = if previous.recovery.body().tail == formal::RecoveryTailV1::Empty
                && !matches!(view.tail, formal::RecoveryTailWitnessV1::Empty)
            {
                previous.recovery.successor()
            } else {
                previous.recovery.original_authority()
            };
            Some(
                formal::VerifiedActiveRecoveryContextV2::from_previous(
                    &previous.recovery,
                    original,
                    anchor.head(),
                    view.tail,
                )
                .map_err(|error| error.to_string())?,
            )
        }
        _ => return Err("recovery schema/prior permission mismatch".into()),
    };
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: active.policy().fencing_provider_public_key.clone(),
        authorized_resource_bindings: active.policy().authorized_fencing_resource_bindings.clone(),
    };
    let intent = match view.fencing {
        FencingBootstrapV1::Dual { primary, replica } => {
            let primary = verify_epoch_seal(primary, &policy).map_err(|error| error.to_string())?;
            let replica = verify_epoch_seal(replica, &policy).map_err(|error| error.to_string())?;
            if let Some(context) = &context {
                formal::VerifiedRecoveryIntent::verify_retained(
                    view.certificate.body.clone(),
                    context,
                    [&primary, &replica],
                )
            } else {
                formal::VerifiedRecoveryIntent::verify(
                    view.certificate.body.clone(),
                    active,
                    anchor.head(),
                    [&primary, &replica],
                    view.tail,
                )
            }
        }
        FencingBootstrapV1::NecessaryEndpoint { seal } => {
            let seal = verify_epoch_seal(seal, &policy).map_err(|error| error.to_string())?;
            if let Some(context) = &context {
                let coverage =
                    formal::VerifiedAppendFencingCoverageV1::derive_from_active_recovery_context(
                        context, &seal,
                    )
                    .map_err(|error| error.to_string())?;
                formal::VerifiedRecoveryIntent::verify_retained_single_survivor(
                    view.certificate.body.clone(),
                    context,
                    &coverage,
                )
            } else {
                let coverage =
                    formal::VerifiedAppendFencingCoverageV1::derive_from_necessary_endpoint_seal(
                        active,
                        anchor.head(),
                        &seal,
                    )
                    .map_err(|error| error.to_string())?;
                formal::VerifiedRecoveryIntent::verify_single_survivor(
                    view.certificate.body.clone(),
                    active,
                    anchor.head(),
                    &coverage,
                    view.tail,
                )
            }
        }
    }
    .map_err(|error| error.to_string())?;
    if view.minimum.digest.len() != 64
        || !view
            .minimum
            .digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("ISSUER_UNPROVEN: malformed independently retained floor".into());
    }
    let observed = &view.witness.body.current_anchor;
    if observed.revision < view.minimum.revision
        || (observed.revision == view.minimum.revision && observed.digest != view.minimum.digest)
        || (view.minimum.revision == 0
            && *view.minimum
                != IssuerAnchorV1::initial(active.policy()).map_err(|error| error.to_string())?)
    {
        return Err("ISSUER_UNPROVEN: recovery witness below independently retained floor".into());
    }
    let issuance =
        formal::issuer::verify_issuance_witness(view.witness, view.certificate, active.policy())
            .map_err(|error| error.to_string())?;
    let recovery = formal::VerifiedRecovery::verify(view.certificate, &intent, active, &issuance)
        .map_err(|error| error.to_string())?;
    Ok(VerifiedRecoveryBootstrapV1 {
        active_predecessor: active.clone(),
        cut: anchor.head().clone(),
        recovery,
        external_anchor: anchor,
        retained_context: context,
        external_issuer_minimum: None,
        evidence: RecoveryPermissionBootstrapV1 {
            predecessor_checkpoint: view.checkpoint.clone(),
            predecessor_checkpoint_digest: view.checkpoint_digest.into(),
            predecessor_context_version: view.predecessor_context_version,
            certificate: view.certificate.clone(),
            fencing: view.fencing.clone(),
            tail: view.tail.clone(),
            issuer_witness: view.witness.clone(),
            minimum_issuer_anchor: view.minimum.clone(),
            prior_permission: view.prior.cloned().map(Box::new),
        },
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FencingBootstrapV1 {
    Dual {
        primary: Box<EpochSealReceiptV1>,
        replica: Box<EpochSealReceiptV1>,
    },
    NecessaryEndpoint {
        seal: Box<EpochSealReceiptV1>,
    },
}
impl RecoveryPermissionBootstrapV1 {
    /// The minimum is independently provisioned; it is never taken from this
    /// permission envelope. A later live floor may exceed the request's floor:
    /// the authenticated current witness, rather than that old request, must
    /// meet it. Recursive prior/request floors are still checked by `verify`.
    pub fn verify_at_minimum(
        &self,
        trust: &formal::InitialActivationTrustV1,
        initial: &formal::SignedInitialActivationV1,
        external_minimum: &IssuerAnchorV1,
    ) -> Result<VerifiedRecoveryBootstrapV1, String> {
        let mut verified = self.verify(trust, initial)?;
        if external_minimum.digest.len() != 64
            || !external_minimum
                .digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || (external_minimum.revision == 0
                && *external_minimum
                    != IssuerAnchorV1::initial(verified.active_predecessor.policy())
                        .map_err(|e| e.to_string())?)
        {
            return Err("ISSUER_UNPROVEN: invalid independently provisioned issuer minimum".into());
        }
        require_forward_floor(
            &verified.evidence.issuer_witness.body.current_anchor,
            external_minimum,
        )?;
        verified.external_issuer_minimum = Some(external_minimum.clone());
        Ok(verified)
    }
    pub fn verify(
        &self,
        trust: &formal::InitialActivationTrustV1,
        initial: &formal::SignedInitialActivationV1,
    ) -> Result<VerifiedRecoveryBootstrapV1, String> {
        verify_recovery_permission(self.view(), trust, initial)
    }
}
impl VerifiedRecoveryBootstrapV1 {
    pub fn external_issuer_minimum(&self) -> Option<&IssuerAnchorV1> {
        self.external_issuer_minimum.as_ref()
    }
    pub fn retained_context(&self) -> Option<&formal::VerifiedActiveRecoveryContextV2> {
        self.retained_context.as_ref()
    }
    pub fn permission_evidence(&self) -> &RecoveryPermissionBootstrapV1 {
        &self.evidence
    }
    pub fn active_predecessor(&self) -> &formal::VerifiedAuthorityV1 {
        &self.active_predecessor
    }
    pub fn cut(&self) -> &formal::VerifiedFormalCommitV1 {
        &self.cut
    }
    pub fn recovery(&self) -> &formal::VerifiedRecovery {
        &self.recovery
    }
    pub fn original_authority(&self) -> &formal::VerifiedAuthorityV1 {
        self.recovery.original_authority()
    }
    pub fn external_anchor(&self) -> &formal::VerifiedFormalReadAnchorV1 {
        &self.external_anchor
    }
}

fn require_forward_floor(current: &IssuerAnchorV1, minimum: &IssuerAnchorV1) -> Result<(), String> {
    if current.revision < minimum.revision
        || (current.revision == minimum.revision && current.digest != minimum.digest)
    {
        return Err("ISSUER_UNPROVEN: recovery permission issuer floor rollback".into());
    }
    Ok(())
}
#[cfg(test)]
#[path = "recovery_bootstrap_tests.rs"]
mod tests;
