use crate::management_metadata::*;
use serde_json::{Value, json};
pub(super) fn schemas() -> Value {
    json!({
        "ManagementParentsQueryV1":schemars::schema_for!(ManagementParentsQueryV1),
        "ManagementParentsPageV1":schemars::schema_for!(ManagementParentsPageV1),
        "CustodyManagementCapabilityV1":schemars::schema_for!(CustodyManagementCapabilityV1),
    })
}
pub(super) fn fixtures() -> Value {
    let q = ManagementParentsQueryV1 {
        api_version: 1,
        limit: 16,
        after_custody_account_id: None,
        directory_revision: None,
    };
    let page = ManagementParentsPageV1 {
        api_version: 1,
        directory_revision: "ab".repeat(32),
        frozen: false,
        inventory_complete: true,
        items: vec![ManagementParentV1 {
            custody_account_id: "custody-one".into(),
            provision_id: "provision-one".into(),
            environment: "test".into(),
            world_id: "world-one".into(),
        }],
        next_after_custody_account_id: None,
    };
    page.validate(&q).unwrap();
    json!({"ManagementParentsQueryV1":[q],"ManagementParentsPageV1":[page],
        "CustodyManagementCapabilityV1":[CustodyManagementCapabilityV1::configured(None,"test"),CustodyManagementCapabilityV1::configured(Some("account-authority:test"),"test")]})
}
