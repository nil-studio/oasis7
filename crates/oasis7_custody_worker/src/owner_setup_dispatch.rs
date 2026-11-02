//! Setup source inspection and fixed-stage Account dispatch.
use super::*;
#[cfg(feature = "approval-harness")]
#[path = "setup_diagnostic.rs"]
mod diagnostic;
use oasis7_custody_core::provisioning::*;

fn source(
    saved: &ProvisionApprovalSnapshot,
    id: &str,
    stage: Option<OwnerSetupStageV1>,
) -> Result<(
    oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
    oasis7_custody_core::approval::SignedApprovalVerificationV1,
)> {
    let approved = saved
        .state
        .owner_setup_phase_original(id)
        .map_err(|_| denied())?;
    let inspection = if let Some(stage) = stage {
        &saved
            .state
            .owner_setup_outbox(id, stage)
            .ok_or_else(denied)?
            .source
    } else {
        saved
            .state
            .owner_setup_phase_source(id)
            .map_err(|_| denied())?
    };
    let signed = inspection
        .verification
        .clone()
        .or_else(|| {
            saved
                .state
                .owner_setup_phase_source(id)
                .ok()
                .and_then(|v| v.verification.clone())
        })
        .or_else(|| approved.verification().cloned())
        .ok_or_else(denied)?;
    Ok((approved.input().clone(), signed))
}
pub(super) async fn observe(
    state: &JsValue,
    env: &Env,
    id: &str,
    stage: Option<OwnerSetupStageV1>,
    inspection_only: bool,
) -> Result<authority::Observations> {
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let (input, signed) = source(&saved, id, stage)?;
    setup_authority::observe(env, &saved, input, &signed, inspection_only).await
}
pub(super) async fn prepare_source(
    state: &JsValue,
    env: &Env,
    id: &str,
    stage: Option<OwnerSetupStageV1>,
) -> Result<authority::Observations> {
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let inspection = if let Some(stage) = stage {
        &saved
            .state
            .owner_setup_outbox(id, stage)
            .ok_or_else(denied)?
            .source
    } else {
        saved
            .state
            .owner_setup_phase_source(id)
            .map_err(|_| denied())?
    };
    if inspection.verification.is_some() {
        return observe(state, env, id, stage, false).await;
    }
    if inspection.claimed_at.is_some() {
        return Err(denied());
    }
    let before = observe(state, env, id, stage, true).await?;
    let verify = match stage {
        Some(stage) => store.claim_owner_setup_stage_source(id, stage, || {
            setup_authority::current(state, env, &before, false)
        }),
        None => store
            .claim_owner_setup_source(id, || setup_authority::current(state, env, &before, false)),
    }
    .map_err(|_| denied())?;
    let raw = serde_json::to_string(&verify)?;
    let signed = serde_json::from_str(&super::super::super::text(
        super::super::super::verify_native(env, &raw)
            .await
            .map_err(|_| denied())?,
    )?)
    .map_err(|_| denied())?;
    let (input, _) = source(&saved, id, stage)?;
    let after = setup_authority::observe(env, &saved, input, &signed, false).await?;
    match stage {
        Some(stage) => {
            store
                .complete_owner_setup_stage_source(id, stage, signed, || {
                    setup_authority::current(state, env, &after, true)
                })
                .await
        }
        None => {
            store
                .complete_owner_setup_source(id, signed, || {
                    setup_authority::current(state, env, &after, true)
                })
                .await
        }
    }
    .map_err(|_| denied())?;
    Ok(after)
}

use crate::owner_setup_rpc::{
    OwnerSetupPublished, OwnerSetupRuntimeAction, OwnerSetupRuntimeRpc, OwnerSetupSignerPermit,
};
fn role_allowed(action: &OwnerSetupRuntimeAction, role: Role) -> bool {
    match action {
        OwnerSetupRuntimeAction::GetPermit {
            purpose: OwnerSetupReadPurposeV1::Sign,
            ..
        }
        | OwnerSetupRuntimeAction::PublishCandidate { .. }
        | OwnerSetupRuntimeAction::PublishResourceCandidate { .. } => role == Role::Owner,
        _ => role == Role::Identity,
    }
}
pub(super) async fn dispatch(
    state: &JsValue,
    env: &Env,
    rpc: OwnerSetupRuntimeRpc,
    static_role: Role,
) -> Result<String> {
    rpc.request.validate_sdk_selector().map_err(|_| denied())?;
    if !role_allowed(&rpc.action, static_role) {
        return Err(denied());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    if saved
        .state
        .owner_setup_engine_version(&rpc.request.setup_id)
        .map_err(|_| denied())?
        != rpc.request.api_version
        || rpc.request.api_version == 2
            && env
                .var("CUSTODY_OWNER_SETUP_V2_ENABLED")
                .ok()
                .map(|v| v.to_string())
                .as_deref()
                != Some("true")
    {
        return Err(denied());
    }

    let draft = saved.state.draft();
    if rpc.request.custody_account_id != draft.entry.custody_account_id
        || rpc.request.provision_id != draft.entry.provision_id
        || store
            .owner_setup_plan(&rpc.request.setup_id)
            .map_err(|_| denied())?
            .provision_id
            != draft.entry.provision_id
    {
        return Err(denied());
    }
    let id = &rpc.request.setup_id;
    let stage = rpc.stage;
    #[cfg(feature = "approval-harness")]
    let diagnostic = matches!(
        &rpc.action,
        OwnerSetupRuntimeAction::ClaimLookup | OwnerSetupRuntimeAction::RecordResult { .. }
    )
    .then(|| {
        let source_observed = saved
            .state
            .owner_setup_outbox(id, stage)
            .and_then(|o| o.source.verification.as_ref())
            .map(|s| s.verification.observed_at.0);
        diagnostic::Trace::new("owner", stage, source_observed)
    });
    macro_rules! stage_result {
        ($code:literal,$value:expr) => {{
            let result = $value;
            #[cfg(feature = "approval-harness")]
            if result.is_err() {
                if let Some(trace) = &diagnostic {
                    trace.fail($code);
                }
            }
            result
        }};
    }
    store.owner_setup_outbox(id, stage).map_err(|_| denied())?;
    if let OwnerSetupRuntimeAction::ArchiveCanonical { fact } = rpc.action {
        let world = setup_authority::public_world(env)?;
        let fact = store
            .archive_owner_setup_canonical(id, stage, *fact, &world.service_public_key)
            .await
            .map_err(|_| denied())?;
        return Ok(serde_json::to_string(&fact)?);
    }
    setup_authority::enabled(env)?;
    match rpc.action {
        OwnerSetupRuntimeAction::ArchiveCanonical { .. } => Err(denied()),
        OwnerSetupRuntimeAction::GetPermit { purpose, access } => {
            if purpose == OwnerSetupReadPurposeV1::Session && access.is_some() {
                return Err(denied());
            }
            let read = store
                .reserve_owner_setup_read(id, stage, purpose, static_role)
                .map_err(|_| denied())?;
            let observed = observe(state, env, id, Some(stage), false).await?;
            if purpose == OwnerSetupReadPurposeV1::Sign
                && stage == OwnerSetupStageV1::FinalObservation
                && saved
                    .state
                    .owner_setup_v2_plan()
                    .is_some_and(|p| p.setup_id == *id && p.selection.resource().is_some())
            {
                let permit = store
                    .owner_resource_signing_permit_v2(id, read, access.map(|v| *v), || {
                        setup_authority::current(state, env, &observed, true)
                    })
                    .await
                    .map_err(|_| denied())?;
                return Ok(serde_json::to_string(&permit)?);
            }
            if purpose == OwnerSetupReadPurposeV1::Sign {
                let (permit, signing) = store
                    .owner_setup_signing_permit(id, read, access.map(|v| *v), || {
                        setup_authority::current(state, env, &observed, true)
                    })
                    .await
                    .map_err(|_| denied())?;
                Ok(serde_json::to_string(&OwnerSetupSignerPermit {
                    permit,
                    signing,
                })?)
            } else {
                let permit = store
                    .authorize_owner_setup_read(id, read, static_role, || {
                        setup_authority::current(state, env, &observed, true)
                    })
                    .await
                    .map_err(|_| denied())?;
                Ok(serde_json::to_string(&permit)?)
            }
        }
        OwnerSetupRuntimeAction::PublishCandidate { candidate } => {
            let read = store
                .reserve_owner_setup_read(id, stage, OwnerSetupReadPurposeV1::Sign, static_role)
                .map_err(|_| denied())?;
            let observed = observe(state, env, id, Some(stage), false).await?;
            let permit = store
                .publish_owner_setup_original(id, read, *candidate.clone(), || {
                    setup_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&OwnerSetupPublished {
                permit,
                candidate: *candidate,
            })?)
        }
        OwnerSetupRuntimeAction::PublishResourceCandidate { candidate } => {
            if stage != OwnerSetupStageV1::FinalObservation
                || saved
                    .state
                    .owner_setup_v2_plan()
                    .is_none_or(|p| p.setup_id != *id || p.selection.resource().is_none())
            {
                return Err(denied());
            }
            let read = store
                .reserve_owner_setup_read(id, stage, OwnerSetupReadPurposeV1::Sign, static_role)
                .map_err(|_| denied())?;
            let observed = observe(state, env, id, Some(stage), false).await?;
            let published = store
                .publish_owner_resource_original_v2(id, read, *candidate, || {
                    setup_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&published)?)
        }
        OwnerSetupRuntimeAction::Admit { evidence } => {
            let resource_final = stage == OwnerSetupStageV1::FinalObservation
                && saved
                    .state
                    .owner_setup_v2_plan()
                    .is_some_and(|p| p.setup_id == *id && p.selection.resource().is_some());
            if resource_final {
                store
                    .reserve_owner_resource_admission_callback_v2(id)
                    .map_err(|_| denied())?;
                return resource_final_v2::admit(state, env, &store, id, *evidence).await;
            }
            store
                .reserve_owner_setup_admission_callback(id, stage)
                .map_err(|_| denied())?;
            let observed = observe(state, env, id, Some(stage), false).await?;
            let permit = store
                .admit_and_claim_owner_setup_send(id, stage, *evidence, || {
                    setup_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&permit)?)
        }
        OwnerSetupRuntimeAction::ClaimLookup => {
            stage_result!(
                "lookup.local_reservation",
                store.reserve_owner_setup_lookup(id, stage)
            )
            .map_err(|_| denied())?;
            let observed = stage_result!(
                "lookup.observe",
                observe(state, env, id, Some(stage), false).await
            )?;
            let permit = stage_result!(
                "lookup.store",
                store
                    .claim_owner_setup_lookup(id, stage, || {
                        stage_result!(
                            "lookup.current",
                            setup_authority::current(state, env, &observed, true)
                        )
                    })
                    .await
            )
            .map_err(|_| denied())?;
            Ok(serde_json::to_string(&permit)?)
        }
        OwnerSetupRuntimeAction::RecordResult { result, session } => {
            stage_result!(
                "result.local_reservation",
                store.reserve_owner_setup_callback(id, stage)
            )
            .map_err(|_| denied())?;
            let observed = stage_result!(
                "result.observe",
                async {
                    if matches!(
                        stage,
                        OwnerSetupStageV1::Delegation
                            | OwnerSetupStageV1::Bind
                            | OwnerSetupStageV1::FinalObservation
                    ) {
                        let paid = store.snapshot().map_err(|_| denied())?;
                        setup_authority::observe_paid_result(env, &paid, id, stage).await
                    } else {
                        observe(state, env, id, Some(stage), false).await
                    }
                }
                .await
            )?;
            stage_result!(
                "result.store",
                store
                    .complete_owner_setup_stage(
                        id,
                        OwnerSetupCompletionV1 {
                            stage,
                            result: *result,
                            session: *session,
                        },
                        || stage_result!(
                            "result.current",
                            setup_authority::current(state, env, &observed, true)
                        ),
                    )
                    .await
            )
            .map_err(|_| denied())?;
            let progress = store
                .snapshot()
                .map_err(|_| denied())?
                .state
                .owner_setup_progress(id)
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&progress)?)
        }
        OwnerSetupRuntimeAction::RecordResourceResult { completed } => {
            if stage != OwnerSetupStageV1::FinalObservation
                || saved
                    .state
                    .owner_setup_v2_plan()
                    .is_none_or(|p| p.setup_id != *id || p.selection.resource().is_none())
            {
                return Err(denied());
            }
            store
                .reserve_owner_resource_result_callback_v2(id)
                .map_err(|_| denied())?;
            let paid = store.snapshot().map_err(|_| denied())?;
            let observed = setup_authority::observe_paid_resource_result_v2(env, &paid, id).await?;
            store
                .complete_owner_resource_result_v2(id, *completed, || {
                    setup_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(
                &store
                    .snapshot()
                    .map_err(|_| denied())?
                    .state
                    .owner_setup_progress(id)
                    .map_err(|_| denied())?,
            )?)
        }
        OwnerSetupRuntimeAction::MarkUnknown => {
            store
                .mark_owner_setup_unknown(id, stage)
                .map_err(|_| denied())?;
            let progress = store
                .snapshot()
                .map_err(|_| denied())?
                .state
                .owner_setup_progress(id)
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&progress)?)
        }
    }
}

#[path = "owner_setup_resource_dispatch_v2.rs"]
mod resource_final_v2;
