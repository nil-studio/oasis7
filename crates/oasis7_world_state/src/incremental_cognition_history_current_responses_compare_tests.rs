use super::*;
fn work() -> BusinessDiffProcessingWorkV1 {
    BusinessDiffProcessingWorkV1 {
        steps_remaining: 1_000_000,
        bytes_remaining: 1_000_000,
    }
}
#[test]
fn response_replace_preserves_unknown_and_nested_fields() {
    let memory = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    let mut work = work();
    let before = serde_json::json!({"envelope":{"unknown":[1,{"journal_head":"nested unchanged"}]},"response_digest":"old","unknown":{"a":[true,null]},"journal_head":null});
    let mut after = before.clone();
    after["journal_head"] = serde_json::json!("new");
    compare_response_change(&before, &after, 256, &memory, &mut work, &mut || false).unwrap();
    after["unknown"]["a"][0] = serde_json::json!(false);
    assert!(
        compare_response_change(&before, &after, 256, &memory, &mut work, &mut || false).is_err()
    );
}
#[test]
fn response_replace_does_not_drop_artifact_or_envelope() {
    let memory = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    let mut work = work();
    let before = serde_json::json!({"response_artifact":"id","envelope":{"a":1}});
    let mut after = before.clone();
    after["journal_head"] = serde_json::Value::Null;
    compare_response_change(&before, &after, 128, &memory, &mut work, &mut || false).unwrap();
    after.as_object_mut().unwrap().remove("response_artifact");
    assert!(
        compare_response_change(&before, &after, 128, &memory, &mut work, &mut || false).is_err()
    );
}
#[test]
fn response_replace_preserves_typed_resource_first_error() {
    let memory = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    let before = serde_json::json!({"x":1,"journal_head":null});
    let mut limited = BusinessDiffProcessingWorkV1 {
        steps_remaining: 0,
        bytes_remaining: 100,
    };
    assert!(matches!(
        compare_response_change(&before, &before, 64, &memory, &mut limited, &mut || false),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(matches!(
        compare_response_change(&before, &before, 64, &memory, &mut work(), &mut || true),
        Err(ValueTreeError::Cancelled)
    ));
}
