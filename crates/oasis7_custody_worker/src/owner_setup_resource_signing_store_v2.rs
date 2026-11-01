//! Existing paid sign/publish RPCs, explicitly consuming actual Resource slots.
use super::*;
use crate::owner_setup_rpc::{
    OwnerSetupResourcePublishedV2, OwnerSetupResourceSignerPermitV2,
    OwnerSetupResourceSigningBundleV2, OwnerSetupResourceSigningReadV2,
};
use oasis7_custody_core::DecimalU64;
impl OwnerProvisionApprovalStore {
    pub(crate) async fn owner_resource_signing_permit_v2<F>(
        &self,
        id: &str,
        read: OwnerSetupReadTicket,
        access: Option<OwnerSetupAccessEvidenceV1>,
        current: F,
    ) -> Result<OwnerSetupResourceSignerPermitV2>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        if read.stage != OwnerSetupStageV1::FinalObservation
            || read.purpose != OwnerSetupReadPurposeV1::Sign
            || !matches!(read.ticket, 3 | 4)
            || (read.ticket == 3) != access.is_some()
        {
            return Err(rejected());
        }
        if self
            .snapshot()?
            .state
            .owner_setup_v2_plan()
            .is_none_or(|p| p.setup_id != id || p.selection.resource().is_none())
        {
            return Err(rejected());
        }
        self.native_verified(
            &current,
            "owner_resource_signing_read",
            id,
            |state, scope, context, time| {
                let trusted = OwnerSetupCurrent {
                    provision: context,
                    native_scope: scope,
                };
                let checks = if let Some(access) = &access {
                    state.owner_setup_access_checks(id, read.stage, access, &trusted, time)
                } else {
                    state.owner_setup_access_current_checks(id, read.stage, &trusted, time)
                }
                .map_err(|_| rejected())?;
                Ok(vec![checks])
            },
            |state, scope, context, time, cache| {
                let trusted = OwnerSetupCurrent {
                    provision: context,
                    native_scope: scope,
                };
                if let Some(access) = &access {
                    state
                        .record_owner_setup_access(
                            id,
                            read.stage,
                            access.clone(),
                            &trusted,
                            time,
                            cache,
                        )
                        .map_err(|_| rejected())?;
                }
                let permit = state
                    .authorize_owner_setup_read(
                        id,
                        read,
                        oasis7_custody_core::http::Role::Owner,
                        &trusted,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())?;
                let resource_signing = if read.ticket == 3 {
                    OwnerSetupResourceSigningReadV2::OuterAccess
                } else {
                    let prepared = state
                        .claim_owner_setup_resource_read_v2(id, &trusted, time, cache)
                        .map_err(|_| rejected())?;
                    OwnerSetupResourceSigningReadV2::VaultClaim {
                        bundle: Box::new(OwnerSetupResourceSigningBundleV2 {
                            request: prepared.request().clone(),
                            owner_public_key: prepared.owner_public_key().into(),
                            source_record_digest: prepared.source_record_digest().into(),
                            valid_until: DecimalU64(prepared.valid_until()),
                            observation: prepared.observation().clone(),
                            participant_access: prepared.participant_access().clone(),
                        }),
                    }
                };
                Ok(OwnerSetupResourceSignerPermitV2 {
                    permit,
                    resource_signing,
                    resource_source_record_digest: state
                        .owner_setup_resource_source_record_digest_v2(id)
                        .map_err(|_| rejected())?,
                })
            },
        )
        .await
    }
    pub(crate) async fn publish_owner_resource_original_v2<F>(
        &self,
        id: &str,
        read: OwnerSetupReadTicket,
        candidate: OwnerSetupResourceCandidateV2,
        current: F,
    ) -> Result<OwnerSetupResourcePublishedV2>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        if read.stage != OwnerSetupStageV1::FinalObservation
            || read.ticket != OWNER_SETUP_MAX_ORACLE_READS
            || read.purpose != OwnerSetupReadPurposeV1::Sign
        {
            return Err(rejected());
        }
        self.native_verified(
            &current,
            "owner_resource_original_published",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .owner_setup_access_current_checks(
                            id,
                            read.stage,
                            &OwnerSetupCurrent {
                                provision: context,
                                native_scope: scope,
                            },
                            time,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                let trusted = OwnerSetupCurrent {
                    provision: context,
                    native_scope: scope,
                };
                state
                    .authorize_owner_setup_read(
                        id,
                        read,
                        oasis7_custody_core::http::Role::Owner,
                        &trusted,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())?;
                state
                    .publish_owner_setup_resource_read_v2(
                        id,
                        candidate.clone(),
                        &trusted,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())?;
                let permit = state
                    .owner_setup_resource_published_permit_v2(id, &trusted, time, cache)
                    .map_err(|_| rejected())?;
                Ok(OwnerSetupResourcePublishedV2 {
                    permit,
                    candidate: candidate.clone(),
                    resource_source_record_digest: state
                        .owner_setup_resource_source_record_digest_v2(id)
                        .map_err(|_| rejected())?,
                })
            },
        )
        .await
    }
}
