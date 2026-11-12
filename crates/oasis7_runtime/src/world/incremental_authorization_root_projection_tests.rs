use super::*;
use oasis7_world_state::capability_authorization::*;

/// Frozen legacy data shape copied from the original implementation, rather
/// than comparing two callers of the new projection.
#[derive(Serialize)]
struct LegacyRootBody<'a> {
    grants: &'a std::collections::BTreeMap<String, serde_json::Value>,
    revocation: &'a oasis7_world_state::CapabilityRevocationState,
    invocation_contexts: &'a std::collections::BTreeMap<
        String,
        oasis7_world_state::capability_authorization::CapabilityInvocationContext,
    >,
    budget_accounts: &'a std::collections::BTreeMap<
        String,
        oasis7_world_state::capability_authorization::CapabilityBudgetAccount,
    >,
    nonce_records: &'a std::collections::BTreeMap<
        String,
        oasis7_world_state::capability_authorization::CapabilityAuthorizationNonceRecord,
    >,
    receipts: &'a std::collections::BTreeMap<
        String,
        oasis7_world_state::capability_authorization::CapabilityAuthorizationAuditReceipt,
    >,
    effect_receipt_links: &'a std::collections::BTreeMap<
        String,
        oasis7_world_state::capability_authorization::CapabilityEffectReceiptLink,
    >,
}
#[test]
fn borrowed_authority_projection_matches_original_clear_and_preserves_other_fields() {
    let mut revocation = oasis7_world_state::CapabilityRevocationState::default();
    let grants = std::collections::BTreeMap::<String, serde_json::Value>::new();
    let invocation_contexts =
        std::collections::BTreeMap::<String, CapabilityInvocationContext>::new();
    let budget_accounts = std::collections::BTreeMap::<String, CapabilityBudgetAccount>::new();
    let nonce_records =
        std::collections::BTreeMap::<String, CapabilityAuthorizationNonceRecord>::new();
    let receipts = std::collections::BTreeMap::<String, CapabilityAuthorizationAuditReceipt>::new();
    let effect_receipt_links =
        std::collections::BTreeMap::<String, CapabilityEffectReceiptLink>::new();
    revocation.epoch = 7;
    revocation.safe_alternatives.insert(
        "outer".into(),
        serde_json::json!({"state":"executed","nested":[1,true,"payload"]}),
    );
    revocation.system_identities.insert("native".into(), 11);
    for nonempty in [false, true] {
        if nonempty {
            revocation.world_service_results.insert(
                "result".into(),
                serde_json::json!({"huge":["correlation","only"]}),
            );
        }
        let mut old = revocation.clone();
        old.world_service_results.clear();
        let legacy = LegacyRootBody {
            grants: &grants,
            revocation: &old,
            invocation_contexts: &invocation_contexts,
            budget_accounts: &budget_accounts,
            nonce_records: &nonce_records,
            receipts: &receipts,
            effect_receipt_links: &effect_receipt_links,
        };
        let authority = AuthorityRevocationViewV1(&revocation);
        let actual = AuthorizationRootProjectionV1 {
            grants: &grants,
            revocation: &authority,
            invocation_contexts: &invocation_contexts,
            budget_accounts: &budget_accounts,
            nonce_records: &nonce_records,
            receipts: &receipts,
            effect_receipt_links: &effect_receipt_links,
        };
        assert_eq!(
            oasis7_wasm_abi::encode_canonical_cbor(&actual).unwrap(),
            oasis7_wasm_abi::encode_canonical_cbor(&legacy).unwrap()
        );
        let json = serde_json::to_value(&authority).unwrap();
        assert!(json.get("world_service_results").is_none());
        assert_eq!(json["epoch"], 7);
        assert_eq!(json["safe_alternatives"]["outer"]["state"], "executed");
        assert_eq!(revocation.world_service_results.is_empty(), !nonempty);
    }
}
