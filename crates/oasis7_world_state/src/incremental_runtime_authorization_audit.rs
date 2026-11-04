//! Borrowed exhaustive audit columns for Runtime authorization storage.
//! This traversal does not grant installation or authorization authority.
use crate::CapabilityAuthorizationAuditReceipt;

pub enum AuthorizationAuditValueV1<'a> {
    Text(&'a str),
    OptionalText(Option<&'a str>),
    Unsigned(u64),
    OptionalUnsigned(Option<u64>),
    Signed(i64),
    OptionalSigned(Option<i64>),
    Json(&'a serde_json::Value),
    OptionalJson(Option<&'a serde_json::Value>),
    TextSet(&'a std::collections::BTreeSet<String>),
}
/// The sink must consume each borrowed value before returning. A complete
/// owner uses closed column names and the existing typed record/JSON engines.
pub fn visit_authorization_audit_columns_v1<E>(
    receipt: &CapabilityAuthorizationAuditReceipt,
    mut sink: impl FnMut(&'static str, AuthorizationAuditValueV1<'_>) -> Result<(), E>,
) -> Result<(), E> {
    let CapabilityAuthorizationAuditReceipt {
        receipt_id,
        root_receipt_id,
        grant_id,
        subject,
        presenter,
        audience,
        scope_hash,
        module_id,
        module_version,
        manifest_hash,
        catalog_snapshot_id,
        response_nonce,
        authorization_nonce_key_hash,
        decision,
        denial_code,
        budget_before,
        budget_after,
        world_head_before,
        world_head_after,
        branch_id,
        finality_epoch,
        finality_block_hash,
        finality_status,
        state_hash_before,
        state_hash_after,
        committed_effect_receipt_id,
        committed_effect_receipt_ids,
        canonical_request_hash,
        canonical_result_hash,
    } = receipt;
    sink("receipt_id", AuthorizationAuditValueV1::Text(receipt_id))?;
    sink(
        "root_receipt_id",
        AuthorizationAuditValueV1::OptionalText(root_receipt_id.as_deref()),
    )?;
    sink(
        "grant_id",
        AuthorizationAuditValueV1::OptionalText(grant_id.as_deref()),
    )?;
    sink("subject", AuthorizationAuditValueV1::Json(subject))?;
    sink(
        "presenter",
        AuthorizationAuditValueV1::OptionalJson(presenter.as_ref()),
    )?;
    sink("audience", AuthorizationAuditValueV1::Json(audience))?;
    sink("scope_hash", AuthorizationAuditValueV1::Text(scope_hash))?;
    sink(
        "module_id",
        AuthorizationAuditValueV1::OptionalText(module_id.as_deref()),
    )?;
    sink(
        "module_version",
        AuthorizationAuditValueV1::OptionalText(module_version.as_deref()),
    )?;
    sink(
        "manifest_hash",
        AuthorizationAuditValueV1::OptionalText(manifest_hash.as_deref()),
    )?;
    sink(
        "catalog_snapshot_id",
        AuthorizationAuditValueV1::OptionalText(catalog_snapshot_id.as_deref()),
    )?;
    sink(
        "response_nonce",
        AuthorizationAuditValueV1::OptionalText(response_nonce.as_deref()),
    )?;
    sink(
        "authorization_nonce_key_hash",
        AuthorizationAuditValueV1::OptionalText(authorization_nonce_key_hash.as_deref()),
    )?;
    sink("decision", AuthorizationAuditValueV1::Text(decision))?;
    sink(
        "denial_code",
        AuthorizationAuditValueV1::OptionalText(denial_code.as_deref()),
    )?;
    sink(
        "budget_before",
        AuthorizationAuditValueV1::Signed(*budget_before),
    )?;
    sink(
        "budget_after",
        AuthorizationAuditValueV1::OptionalSigned(*budget_after),
    )?;
    sink(
        "world_head_before",
        AuthorizationAuditValueV1::Unsigned(*world_head_before),
    )?;
    sink(
        "world_head_after",
        AuthorizationAuditValueV1::OptionalUnsigned(*world_head_after),
    )?;
    sink("branch_id", AuthorizationAuditValueV1::Text(branch_id))?;
    sink(
        "finality_epoch",
        AuthorizationAuditValueV1::Unsigned(*finality_epoch),
    )?;
    sink(
        "finality_block_hash",
        AuthorizationAuditValueV1::OptionalText(finality_block_hash.as_deref()),
    )?;
    sink(
        "finality_status",
        AuthorizationAuditValueV1::Text(finality_status),
    )?;
    sink(
        "state_hash_before",
        AuthorizationAuditValueV1::Text(state_hash_before),
    )?;
    sink(
        "state_hash_after",
        AuthorizationAuditValueV1::OptionalText(state_hash_after.as_deref()),
    )?;
    sink(
        "committed_effect_receipt_id",
        AuthorizationAuditValueV1::OptionalText(committed_effect_receipt_id.as_deref()),
    )?;
    sink(
        "committed_effect_receipt_ids",
        AuthorizationAuditValueV1::TextSet(committed_effect_receipt_ids),
    )?;
    sink(
        "canonical_request_hash",
        AuthorizationAuditValueV1::Text(canonical_request_hash),
    )?;
    sink(
        "canonical_result_hash",
        AuthorizationAuditValueV1::Text(canonical_result_hash),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_preserves_optional_empty_and_all_effect_receipts() {
        let receipt: CapabilityAuthorizationAuditReceipt =
            serde_json::from_value(serde_json::json!({
                "receipt_id":"inner", "root_receipt_id":"", "subject":{"kind":"future","raw":[0,1]},
                "audience":null, "scope_hash":"scope", "decision":"accepted", "budget_before":-1,
                "budget_after":0, "world_head_before":0, "world_head_after":0,
                "branch_id":"", "finality_epoch":0, "finality_status":"", "state_hash_before":"",
                "canonical_request_hash":"req", "canonical_result_hash":"result",
                "committed_effect_receipt_ids":["second","first"]
            }))
            .unwrap();
        let mut columns = std::collections::BTreeSet::new();
        visit_authorization_audit_columns_v1(&receipt, |name, value| {
            assert!(columns.insert(name));
            match (name, value) {
                ("root_receipt_id", AuthorizationAuditValueV1::OptionalText(v)) => {
                    assert_eq!(v, Some(""))
                }
                ("grant_id", AuthorizationAuditValueV1::OptionalText(v)) => assert_eq!(v, None),
                ("budget_before", AuthorizationAuditValueV1::Signed(v)) => assert_eq!(v, -1),
                ("budget_after", AuthorizationAuditValueV1::OptionalSigned(v)) => {
                    assert_eq!(v, Some(0))
                }
                ("world_head_after", AuthorizationAuditValueV1::OptionalUnsigned(v)) => {
                    assert_eq!(v, Some(0))
                }
                ("subject", AuthorizationAuditValueV1::Json(v)) => {
                    assert!(std::ptr::eq(v, &receipt.subject))
                }
                ("committed_effect_receipt_ids", AuthorizationAuditValueV1::TextSet(v)) => {
                    assert_eq!(v.len(), 2);
                    assert!(std::ptr::eq(v, &receipt.committed_effect_receipt_ids));
                }
                _ => (),
            }
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(columns.len(), 29);
        let mut calls = 0;
        let result = visit_authorization_audit_columns_v1(&receipt, |_, _| {
            calls += 1;
            Err("stop")
        });
        assert_eq!(result, Err("stop"));
        assert_eq!(calls, 1);
    }
}
