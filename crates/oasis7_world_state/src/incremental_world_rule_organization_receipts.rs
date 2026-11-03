use super::*;
use crate::incremental_utf8_output::read_text_value;
use std::io::Write;
pub(super) type ReceiptKeys = [(WorldFieldKeyV1, u64); 4];
pub(super) fn paths(ordinal: u64) -> [RecordPathV1<'static>; 4] {
    [
        RecordPathV1::Record,
        RecordPathV1::Collection("stage_receipts"),
        RecordPathV1::Nested("stage_receipts", "length"),
        RecordPathV1::Ordinal("stage_receipts", ordinal),
    ]
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldRuleOrganizationReceiptValueV1 {
    RulesAbsent,
    CaseAbsent,
    OutOfBounds { length: u64 },
    Value { length: u64, utf8_bytes: u64 },
}
pub(super) fn value<R: ValueNodeReader>(
    present: bool,
    query: Query,
    data: (&ReceiptKeys, &[Option<[u8; 32]>; 4]),
    values: &mut ValueReadContext<'_, R>,
    record_bytes: u64,
    mut output: &mut dyn Write,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<WorldRuleOrganizationReceiptValueV1, ValueTreeError> {
    use WorldRuleOrganizationReceiptValueV1 as V;
    let (keys, roots) = data;
    let Query {
        ordinal,
        max_payload,
        expected_len,
        ..
    } = query;
    let result = if !present {
        if roots.iter().any(Option::is_some) {
            return Err(ValueTreeError::Invalid("orphan organization receipt"));
        }
        V::RulesAbsent
    } else if roots[0].is_none() {
        if roots[1..].iter().any(Option::is_some) {
            return Err(ValueTreeError::Invalid("orphan organization receipt owner"));
        }
        V::CaseAbsent
    } else {
        for i in 0..2 {
            let root = roots[i].ok_or(ValueTreeError::Invalid(
                "organization receipt missing metadata",
            ))?;
            marker(
                &root,
                &keys[i].0,
                keys[i].1,
                values,
                record_bytes,
                cancelled,
            )?;
        }
        let root = roots[2].ok_or(ValueTreeError::Invalid(
            "organization receipt missing length",
        ))?;
        let length = u64::from_be_bytes(fixed(
            &root,
            &keys[2],
            MemberPayloadCodecV1::U64,
            values,
            record_bytes,
            cancelled,
        )?);
        if expected_len.is_some_and(|v| v != length) {
            return Err(ValueTreeError::Invalid(
                "organization receipt expected length",
            ));
        }
        if ordinal >= length {
            if roots[3].is_some() {
                return Err(ValueTreeError::Invalid(
                    "organization receipt outside declared length",
                ));
            }
            V::OutOfBounds { length }
        } else {
            let root = roots[3].ok_or(ValueTreeError::Invalid(
                "organization receipt missing ordinal",
            ))?;
            let utf8_bytes = read_text_value(
                &root,
                &keys[3].0,
                values,
                profile(
                    MemberPayloadCodecV1::Text,
                    keys[3].1,
                    max_payload,
                    record_bytes,
                ),
                &mut output,
                "organization receipt UTF-8",
                cancelled,
            )?;
            V::Value { length, utf8_bytes }
        }
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(result)
}
pub(super) struct Query {
    pub(super) ordinal: u64,
    pub(super) max_payload: u64,
    pub(super) expected_len: Option<u64>,
}
#[derive(Clone, Copy)]
pub enum WorldRuleOrganizationReceiptChangeV1<'a> {
    Replace {
        case: &'a str,
        ordinal: u64,
        expected_len: u64,
        expected: &'a str,
        after: &'a str,
        max_current_payload_bytes: u64,
    },
    Append {
        case: &'a str,
        expected_len: u64,
        after: &'a str,
    },
    Pop {
        case: &'a str,
        expected_len: u64,
        expected: &'a str,
        max_current_payload_bytes: u64,
    },
}
