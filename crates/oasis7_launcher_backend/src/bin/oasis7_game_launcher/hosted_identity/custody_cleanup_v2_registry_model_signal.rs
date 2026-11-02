//! Read-only signal extraction inside the successful activation transaction.
use super::super::model_proxy_basis::ActiveModelSignal;
use super::*;

pub(in crate::hosted_identity::custody) fn active_model_signal(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::super::config::IdentityConfig,
    ack: &SignedNativeActivationAckV2,
) -> Result<Option<ActiveModelSignal>, StoreError> {
    let index = tx
        .get::<ReferenceIndex>(&reference_key(&ack.ack.opaque_registration_ref))?
        .ok_or_else(unavailable)?
        .value;
    let row = tx
        .get::<Registration>(&index.registration_key)?
        .ok_or_else(unavailable)?
        .value;
    let Phase::Active {
        certificate,
        ack: saved_ack,
        ..
    } = &row.phase
    else {
        return Err(unavailable());
    };
    let input = &row.pending.certificate.input;
    if index.api_version != 2
        || row.api_version != 2
        || record_key(&row.pending)? != index.registration_key
        || saved_ack.as_ref() != ack
        || certificate.certificate.approved_input_digest
            != input
                .approved_test_session_v2_digest()
                .map_err(|_| unavailable())?
        || certificate.certificate.session_id != ack.ack.session_id
        || ack.ack.approved_input_digest != row.pending.certificate.approved_input_digest
    {
        return Err(unavailable());
    }
    let Some(NativeCustodySummaryContractV1::FullTestSessionProxyV2(_)) =
        &input.approval.proof.model_budget_summary_contract
    else {
        return Ok(None);
    };
    super::super::cleanup_consent::validate_saved_full_input(
        tx,
        config,
        identity,
        input,
        &row.pending.certificate.intent_digest,
    )?;
    Ok(Some(ActiveModelSignal::from_registered_input(
        input.as_ref().clone(),
    )))
}
