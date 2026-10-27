//! Separate, prepaid own-session status capability. Never constructs a login permit.
use super::*;
use crate::native_runtime_rpc::{NativeRecoveryAction, NativeRecoveryRpc};
use crate::provision_approval_store::native::NativeStageScopeCurrent;
use oasis7_custody_core::provisioning::{
    PersistedNativeSessionAttestation, SessionStatusRecoveryFundingV1,
    SessionStatusRecoveryOriginalV1,
};

fn source(
    saved: &ProvisionApprovalSnapshot,
    original: &SessionStatusRecoveryOriginalV1,
) -> Result<(
    oasis7_custody_core::approval_admission::ApprovalAdmissionInput,
    oasis7_custody_core::approval::SignedApprovalVerificationV1,
)> {
    let approved = match &original.funding {
        SessionStatusRecoveryFundingV1::Provision => {
            saved.state.accepted_approval().map_err(|_| denied())
        }
        SessionStatusRecoveryFundingV1::ManagedLogin { operation_id } => saved
            .state
            .managed_login_original(operation_id)
            .ok_or_else(denied),
    }
    .map_err(|_| denied())?;
    let signed = original
        .source_inspection
        .verification
        .clone()
        .or_else(|| approved.verification().cloned())
        .ok_or_else(denied)?;
    Ok((approved.input().clone(), signed))
}

pub(super) async fn recover(state: &JsValue, env: &Env, rpc: NativeRecoveryRpc) -> Result<String> {
    rpc.request.validate().map_err(|_| denied())?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let draft = saved.state.draft();
    if rpc.request.custody_account_id != draft.entry.custody_account_id
        || rpc.request.provision_id != draft.entry.provision_id
    {
        return Err(denied());
    }
    let id = &rpc.request.operation_id;
    // RecordResult never creates a reservation. Its exact historical retry is
    // handled by Store before any fresh authority observation.
    if let NativeRecoveryAction::RecordResult { status, session } = rpc.action {
        let original = store
            .session_status_recovery_original(&rpc.request)
            .map_err(|_| denied())?;
        let attestation = PersistedNativeSessionAttestation {
            status_digest: &session.status_digest,
            session_ref: &session.session_ref,
            private_sid_generation: session.private_sid_generation.0,
        };
        if original.result.is_some() {
            let result = store
                .complete_session_status_recovery(id, *status, &attestation, || {
                    Err(JsValue::from_str("historical read cannot authorize"))
                })
                .await
                .map_err(|_| denied())?;
            return Ok(serde_json::to_string(&result)?);
        }
        let (input, signed) = source(&saved, &original)?;
        let observed = authority::observe(env, draft, input, &signed, false).await?;
        let result = store
            .complete_session_status_recovery(id, *status, &attestation, || {
                authority::current(state, env, &observed, true)
            })
            .await
            .map_err(|_| denied())?;
        return Ok(serde_json::to_string(&result)?);
    }
    let config = NativeConfig::load(env)?;
    let scope = NativeStageScopeCurrent {
        http_origin: config.http_origin,
        native_root_key_id: config.native_root_key_id,
        native_root_public_key: config.native_root_public_key,
        owner_binding_pin: config.owner_pin,
    };
    let original = if saved.state.session_status_recoveries().contains_key(id) {
        store
            .session_status_recovery_original(&rpc.request)
            .map_err(|_| denied())?
    } else {
        store
            .reserve_session_status_recovery(
                rpc.request.clone(),
                &crate::approval_crypto::verification_nonce().map_err(|_| denied())?,
                &crate::approval_crypto::verification_nonce().map_err(|_| denied())?,
                &scope,
            )
            .map_err(|_| denied())?
    };
    // Reserve the bounded local read before Directory, Fence, or native I/O.
    let ticket = store
        .reserve_session_status_recovery_read(id)
        .map_err(|_| denied())?;
    let (input, signed) = source(&saved, &original)?;
    if original.source_inspection.verification.is_none() {
        if original.source_inspection.claimed_at.is_some() {
            return Err(denied());
        }
        let before = authority::observe(env, draft, input.clone(), &signed, true).await?;
        let request = store
            .claim_session_recovery_origin(id, || authority::current(state, env, &before, false))
            .map_err(|_| denied())?;
        let raw = serde_json::to_string(&request)?;
        let fresh = serde_json::from_str(&super::super::text(
            super::super::verify_native(env, &raw)
                .await
                .map_err(|_| denied())?,
        )?)
        .map_err(|_| denied())?;
        let after = authority::observe(env, draft, input, &fresh, false).await?;
        store
            .complete_session_recovery_origin(id, fresh, || {
                authority::current(state, env, &after, true)
            })
            .await
            .map_err(|_| denied())?;
        store
            .claim_session_status_recovery(id, || authority::current(state, env, &after, true))
            .await
            .map_err(|_| denied())?;
    }
    let latest = store
        .session_status_recovery_original(&rpc.request)
        .map_err(|_| denied())?;
    let (input, signed) = source(&saved, &latest)?;
    let observed = authority::observe(env, draft, input, &signed, false).await?;
    if latest.claimed_at.is_none() {
        store
            .claim_session_status_recovery(id, || authority::current(state, env, &observed, true))
            .await
            .map_err(|_| denied())?;
    }
    let view = store
        .authorize_session_status_recovery_read(id, ticket, || {
            authority::current(state, env, &observed, true)
        })
        .await
        .map_err(|_| denied())?;
    Ok(serde_json::to_string(&view)?)
}
