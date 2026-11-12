//! Original first-index ProductValidation matching over actual selected rows.
//! The borrowed result is data, not a validated module call or receipt permit.
use crate::{MaterialStack, ProductValidationAttemptV1, ProductValidationReceiptV1, WorldError};
pub(in crate::world) fn receipt_for_output_from_rows_v1<'a>(
    rows: Option<&'a [ProductValidationReceiptV1]>,
    job_id: u64,
    index: Option<u32>,
    requester: &str,
    stack: &MaterialStack,
) -> Result<Option<&'a ProductValidationReceiptV1>, WorldError> {
    let Some(receipt) = rows.and_then(|rows| rows.iter().find(|row| row.validation_index == index))
    else {
        return Ok(None);
    };
    if receipt.requester_agent_id != requester || receipt.stack != *stack {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "product validation receipt conflicts with job: job_id={job_id} index={index:?}"
            ),
        });
    }
    Ok(Some(receipt))
}
pub(in crate::world) fn cached_decision_receipt_from_rows_v1<'a>(
    rows: Option<&'a [ProductValidationReceiptV1]>,
    job_id: u64,
    index: Option<u32>,
    requester: &str,
    module: &str,
    stack: &MaterialStack,
) -> Result<Option<&'a ProductValidationReceiptV1>, WorldError> {
    let Some(receipt) = rows.and_then(|rows| rows.iter().find(|row| row.validation_index == index))
    else {
        return Ok(None);
    };
    if receipt.requester_agent_id != requester
        || receipt.module_id != module
        || receipt.stack != *stack
    {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "product validation retry conflicts with persisted receipt: job_id={job_id} index={index:?}"
            ),
        });
    }
    Ok(Some(receipt))
}
pub(in crate::world) fn attempt_for_output_from_rows_v1<'a>(
    rows: Option<&'a [ProductValidationAttemptV1]>,
    job_id: u64,
    index: Option<u32>,
    requester: &str,
    stack: &MaterialStack,
    module: Option<&str>,
) -> Result<Option<&'a ProductValidationAttemptV1>, WorldError> {
    let attempt = rows.and_then(|rows| rows.iter().find(|row| row.validation_index == index));
    if let Some(attempt) = attempt
        && (attempt.requester_agent_id != requester
            || module.is_some_and(|module| attempt.module_id != module)
            || attempt.stack != *stack)
    {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "product validation attempt conflicts with job or receipt: job_id={job_id} index={index:?}"
            ),
        });
    }
    Ok(attempt)
}
