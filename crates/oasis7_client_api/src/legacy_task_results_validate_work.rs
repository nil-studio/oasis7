//! Original receipt association over typed signed-result DATA. This performs
//! no admission and gives rejected operations no fabricated success receipt.
use super::*;
use crate::world_service::ServiceAuthorityWorkErrorV1 as Error;
use crate::world_service_correlation::work::Phase;
use oasis7_wasm_abi::CapabilityCanonicalWorkV1;
fn same_text<A: CapabilityCanonicalWorkV1>(
    phase: &Phase<'_, A>,
    value: &serde_json::Value,
    key: &str,
    expected: &str,
) -> Result<bool, Error<A::Charge>> {
    let actual = phase.get(value, key)?.and_then(serde_json::Value::as_str);
    phase.trace(&actual)?;
    phase.trace(expected)?;
    Ok(actual == Some(expected))
}
pub fn validate_result_with_work_v1<A: CapabilityCanonicalWorkV1>(
    result: &CanonicalIntentResultV1,
    source: &A,
) -> Result<(), Error<A::Charge>> {
    let mut phase = Phase::new(source)?;
    if let WorldServicePayloadV1::GameStarterClaim(signed) = &result.request.signed_payload {
        if result.rejected.is_some() {
            if !result.receipt.is_null() {
                return phase.fail("rejected starter action has success receipt");
            }
        } else {
            let Some(claim) = phase.get(&result.receipt, "claim")? else {
                return phase.fail("starter result missing actual claim");
            };
            if !same_text(&phase, claim, "agent_id", &signed.request.agent_id)?
                || !same_text(&phase, claim, "player_id", &signed.request.owner_binding)?
                || !same_text(&phase, claim, "public_key", &signed.subject_public_key)?
                || phase
                    .get(claim, "amount")?
                    .and_then(serde_json::Value::as_u64)
                    .is_none_or(|v| v == 0)
            {
                return phase.fail("starter result scope invalid");
            }
        }
    }
    if let WorldServicePayloadV1::GameParticipantRegister { owner, .. } =
        &result.request.signed_payload
    {
        if result.rejected.is_some() {
            if !result.receipt.is_null() {
                return phase.fail("rejected participant operation has success receipt");
            }
        } else {
            let original = phase.clone_json(&result.receipt)?;
            let receipt: oasis7_agent_api::game::GameParticipantReceiptV1 =
                match serde_json::from_value(original) {
                    Ok(v) => v,
                    Err(error) => return Err(phase.owned_error(error.to_string())?),
                };
            let r = &owner.request;
            let account = phase.digest(
                "participant-account-world",
                &(
                    &r.identity_namespace.service_id,
                    &r.identity_namespace.environment_id,
                    &r.account_id,
                    &r.world_id,
                    &r.genesis_digest,
                ),
            )?;
            let agent = phase.prefixed(
                "participant:",
                account.strip_prefix("blake3:").unwrap_or(&account),
            )?;
            phase.trace(&receipt)?;
            phase.trace(r)?;
            if receipt.api_rev != 1
                || !receipt
                    .identity_namespace
                    .same_authority(&r.identity_namespace)
                || receipt.account_id != r.account_id
                || receipt.world_id != r.world_id
                || receipt.genesis_digest != r.genesis_digest
                || receipt.owner_public_key != r.owner_public_key
                || receipt.owner_generation.0 == 0
                || receipt.owner_binding.is_empty()
                || receipt.agent_id.is_empty()
                || (!receipt.imported
                    && (receipt.owner_generation.0 != 1
                        || receipt.owner_binding != account
                        || receipt.agent_id != agent))
                || receipt.client_operation_id.is_empty()
                || receipt.business_digest.is_empty()
            {
                return phase.fail("participant receipt scope invalid");
            }
        }
    }
    // Deliberately compute the original expected receipt before inspecting the
    // rejection flag. Verification/hash errors retain the original first cause.
    let expected = match &result.request.signed_payload {
        WorldServicePayloadV1::GameTaskCreate(_) => {
            let value = create_control_with_work_v1(&result.request, source)?;
            Some(phase.json_value(value.control())?)
        }
        WorldServicePayloadV1::GameExecutorControl(_) => {
            let value = executor_receipt_with_work_v1(&result.request, source)?;
            Some(phase.json_value(value.receipt())?)
        }
        WorldServicePayloadV1::GameGoalPublish(_) => {
            let value =
                publish_receipt_with_work_v1(&result.request, result.committed_height, source)?;
            Some(phase.json_value(value.receipt())?)
        }
        _ => None,
    };
    if let Some(expected) = expected {
        if result.rejected.is_some() {
            if !result.receipt.is_null() {
                return phase.fail("rejected task operation contains success receipt");
            }
        } else {
            phase.trace(&result.receipt)?;
            phase.trace(&expected)?;
            if result.receipt != expected {
                return phase.fail("invalid canonical task operation receipt");
            }
        }
    }
    source.charge_work(0, 0).map_err(Error::Work)?;
    Ok(())
}
