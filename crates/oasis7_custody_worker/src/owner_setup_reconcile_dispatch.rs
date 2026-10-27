use super::*;
fn role_allowed(action: &ReconcileAction, role: Role) -> bool {
    match action {
        ReconcileAction::GetPermit {
            purpose: OwnerSetupReadPurposeV1::Sign,
            ..
        }
        | ReconcileAction::PublishReads { .. } => role == Role::Owner,
        _ => role == Role::Identity,
    }
}
pub(super) async fn dispatch(
    state: &JsValue,
    env: &Env,
    rpc: ReconcileRuntimeRpc,
    role: Role,
) -> Result<String> {
    rpc.selector.request.validate().map_err(|_| denied())?;
    if !role_allowed(&rpc.action, role) {
        return Err(denied());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let id = &rpc.selector.request.reconcile_id;
    let attempt = &rpc.selector.request.attempt_id;
    let approval = store.reconcile_approval(id).map_err(|_| denied())?;
    if rpc.selector.custody_account_id != saved.state.draft().entry.custody_account_id
        || rpc.selector.provision_id != saved.state.draft().entry.provision_id
        || approval.plan.provision_id != rpc.selector.provision_id
    {
        return Err(denied());
    }
    store.reconcile_attempt(id, attempt).map_err(|_| denied())?;
    if let ReconcileAction::ArchiveFact { fact } = rpc.action {
        let world = setup_authority::public_world(env)?;
        let result = store
            .archive_reconcile_fact(id, attempt, *fact, &world.service_public_key)
            .await
            .map_err(|_| denied())?;
        return Ok(serde_json::to_string(&result)?);
    }
    setup_authority::enabled(env)?;
    match rpc.action {
        ReconcileAction::ArchiveFact { .. } => Err(denied()),
        ReconcileAction::GetPermit { purpose, access } => {
            if purpose == OwnerSetupReadPurposeV1::Session && access.is_some() {
                return Err(denied());
            }
            let read = store
                .reserve_reconcile_read(id, attempt, purpose, role)
                .map_err(|_| denied())?;
            let observed =
                reconcile_authority::observe(state, env, id, Some(attempt), false).await?;
            let permit = store
                .reconcile_permit(id, attempt, read, role, access.map(|v| *v), || {
                    setup_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&permit)?)
        }
        ReconcileAction::PublishReads { candidate } => {
            let read = store
                .reserve_reconcile_read(id, attempt, OwnerSetupReadPurposeV1::Sign, role)
                .map_err(|_| denied())?;
            if read.ticket != 5 {
                return Err(denied());
            }
            let observed =
                reconcile_authority::observe(state, env, id, Some(attempt), false).await?;
            let permit = store
                .publish_owner_setup_reconcile_reads(id, attempt, *candidate.clone(), || {
                    setup_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&ReconcilePublished {
                permit,
                candidate: *candidate,
            })?)
        }
        ReconcileAction::Admit { evidence } => {
            store
                .reserve_reconcile_callback(id, attempt, OwnerSetupReconcileCallbackV1::Admission)
                .map_err(|_| denied())?;
            let observed =
                reconcile_authority::observe(state, env, id, Some(attempt), false).await?;
            let permit = store
                .record_owner_setup_reconcile_admission(id, attempt, *evidence, || {
                    setup_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&permit)?)
        }
        ReconcileAction::ClaimLookup => {
            store
                .reserve_reconcile_callback(id, attempt, OwnerSetupReconcileCallbackV1::Lookup)
                .map_err(|_| denied())?;
            let observed =
                reconcile_authority::observe(state, env, id, Some(attempt), false).await?;
            let permit = store
                .claim_owner_setup_reconcile_lookup(id, attempt, || {
                    setup_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&permit)?)
        }
        ReconcileAction::ClaimResult => {
            store
                .reserve_reconcile_callback(id, attempt, OwnerSetupReconcileCallbackV1::Result)
                .map_err(|_| denied())?;
            let observed =
                reconcile_authority::observe(state, env, id, Some(attempt), false).await?;
            let permit = store
                .owner_setup_reconcile_poststate_permit(id, attempt, || {
                    setup_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&permit)?)
        }
        ReconcileAction::RecordResult { completion } => {
            let observed =
                reconcile_authority::observe(state, env, id, Some(attempt), false).await?;
            store
                .complete_owner_setup_reconcile(id, attempt, (*completion).into(), || {
                    setup_authority::current(state, env, &observed, true)
                })
                .await
                .map_err(|_| denied())?;
            let result = store
                .snapshot()
                .map_err(|_| denied())?
                .state
                .owner_setup_reconcile_progress(id)
                .map_err(|_| denied())?;
            Ok(serde_json::to_string(&result)?)
        }
    }
}
