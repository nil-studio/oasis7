use super::*;
use oasis7_wasm_abi::{CanonicalCborWorkV1, LegacyCapabilityCanonicalWorkV1};
use std::{
    cell::{Cell, RefCell, RefMut},
    rc::Rc,
};

struct Charge(Rc<Cell<usize>>, usize);
impl Drop for Charge {
    fn drop(&mut self) {
        self.0.set(self.0.get() - self.1);
    }
}
struct Meter {
    used: Rc<Cell<usize>>,
    calls: Cell<usize>,
    fail_at: Cell<usize>,
    work: RefCell<CanonicalCborWorkV1>,
}
impl Meter {
    fn new() -> Self {
        Self {
            used: Rc::new(Cell::new(0)),
            calls: Cell::new(0),
            fail_at: Cell::new(usize::MAX),
            work: RefCell::new(CanonicalCborWorkV1 {
                visits_remaining: 1_000_000,
                bytes_remaining: 1 << 24,
                max_depth: 64,
                max_map_keys: 1024,
                max_key_bytes: 1 << 16,
            }),
        }
    }
}
impl CanonicalCborAllocator for Meter {
    type Charge = Charge;
    fn reserve(&self, n: usize) -> Result<Charge, CanonicalCborStreamError> {
        let used = self
            .used
            .get()
            .checked_add(n)
            .filter(|n| *n <= 16 << 20)
            .ok_or(CanonicalCborStreamError::ResourceLimited(
                "projection test memory",
            ))?;
        self.used.set(used);
        Ok(Charge(Rc::clone(&self.used), n))
    }
    fn charge_work(&self, _: u64, _: u64) -> Result<(), CanonicalCborStreamError> {
        let call = self.calls.get() + 1;
        self.calls.set(call);
        if call >= self.fail_at.get() {
            return Err(CanonicalCborStreamError::Cancelled);
        }
        Ok(())
    }
}
impl CapabilityCanonicalWorkV1 for Meter {
    type Loan<'a> = RefMut<'a, CanonicalCborWorkV1>;
    fn is_metered(&self) -> bool {
        true
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        self.work.borrow_mut()
    }
}
fn original_hash(domain: &str, value: &impl Serialize) -> String {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.world-service.v1", domain, value))
        .unwrap();
    blake3::hash(&bytes).to_hex().to_string()
}
fn leaves() -> Vec<(ProjectionAudienceV1, String)> {
    vec![
        (ProjectionAudienceV1::public(), "03".repeat(32)),
        (
            ProjectionAudienceV1::agent("中文", "02".repeat(32)).unwrap(),
            "02".repeat(32),
        ),
        (
            ProjectionAudienceV1::agent("a", "01".repeat(32)).unwrap(),
            "01".repeat(32),
        ),
    ]
}

#[test]
fn projection_paid_and_compat_paths_match_original_json_and_odd_tree_hashes() {
    let audience = ProjectionAudienceV1::public();
    let value = serde_json::json!({"中文": [null, true, -2], "z": 1.5, "a": "x\u{0}\n\""});
    let original_json = "{\"a\":\"x\\u0000\\n\\\"\",\"z\":1.5,\"中文\":[null,true,-2]}"
        .as_bytes()
        .to_vec();
    let expected = original_hash("formal-projection-leaf-v1", &(&audience, original_json));
    let meter = Meter::new();
    let paid = projection_leaf_with_work_v1(&audience, &value, &meter).unwrap();
    assert_eq!(paid.as_str(), expected);
    assert_eq!(projection_leaf(&audience, &value).unwrap(), expected);
    assert!(meter.used.get() > 0);
    drop(paid);
    assert_eq!(meter.used.get(), 0);
    let before = meter.work.borrow().visits_remaining;
    let tree = projection_tree_with_work_v1(leaves(), &meter).unwrap();
    let mut ordered = leaves();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    let a = original_hash("formal-projection-node-v1", &(&ordered[0].1, &ordered[1].1));
    let b = original_hash("formal-projection-node-v1", &(&ordered[2].1, &ordered[2].1));
    let top = original_hash("formal-projection-node-v1", &(&a, &b));
    let expected = original_hash("formal-projection-root-v1", &(3u64, &top));
    let root = projection_root_with_work_v1(tree.value(), &meter).unwrap();
    assert_eq!(root.as_str(), expected);
    assert_eq!(
        ProjectionCommitmentsV1::from_leaves(leaves())
            .unwrap()
            .root()
            .unwrap(),
        expected
    );
    assert!(meter.work.borrow().visits_remaining < before);
    assert!(meter.used.get() > 0);
    drop(root);
    drop(tree);
    assert_eq!(meter.used.get(), 0);
}

#[test]
fn projection_cancellation_at_each_actual_debit_stops_and_releases_fees() {
    let audience = ProjectionAudienceV1::public();
    let payload = serde_json::json!({"z": ["中", 3], "a": true});
    let complete = Meter::new();
    drop(projection_leaf_with_work_v1(&audience, &payload, &complete).unwrap());
    for fail_at in 1..=complete.calls.get() {
        let meter = Meter::new();
        meter.fail_at.set(fail_at);
        assert!(
            matches!(
                projection_leaf_with_work_v1(&audience, &payload, &meter),
                Err(ProjectionCommitmentWorkErrorV1::Work(
                    CanonicalCborStreamError::Cancelled
                ))
            ),
            "leaf debit {fail_at}"
        );
        assert_eq!(meter.calls.get(), fail_at);
        assert_eq!(meter.used.get(), 0);
    }
    let complete = Meter::new();
    drop(projection_tree_with_work_v1(leaves(), &complete).unwrap());
    for fail_at in 1..=complete.calls.get() {
        let meter = Meter::new();
        meter.fail_at.set(fail_at);
        assert!(
            matches!(
                projection_tree_with_work_v1(leaves(), &meter),
                Err(ProjectionCommitmentWorkErrorV1::Work(
                    CanonicalCborStreamError::Cancelled
                ))
            ),
            "tree debit {fail_at}"
        );
        assert_eq!(meter.calls.get(), fail_at);
        assert_eq!(meter.used.get(), 0);
    }
    let meter = Meter::new();
    drop(projection_leaf_with_work_v1(&audience, &payload, &meter).unwrap());
    let before = meter.work.borrow().visits_remaining;
    meter.fail_at.set(meter.calls.get() + 1);
    assert!(projection_leaf_with_work_v1(&audience, &payload, &meter).is_err());
    assert_eq!(meter.work.borrow().visits_remaining, before);
    assert_eq!(meter.used.get(), 0);
}

#[test]
fn projection_protocol_refusal_owns_fee_and_legacy_keeps_bounds() {
    let meter = Meter::new();
    let mut duplicate = leaves();
    duplicate.push(duplicate[0].clone());
    let error = projection_tree_with_work_v1(duplicate, &meter)
        .err()
        .unwrap();
    assert!(
        matches!(&error, ProjectionCommitmentWorkErrorV1::Protocol(p)
        if p.message() == "formal projection duplicate/invalid scope")
    );
    assert!(meter.used.get() > 0);
    drop(error);
    assert_eq!(meter.used.get(), 0);
    let bad = ProjectionAudienceV1 {
        scope_id: "public".into(),
        reader_public_key: Some("01".repeat(32)),
    };
    let _: Result<(), String> = bad.validate();
    let error = projection_leaf_with_work_v1(&bad, &(), &meter)
        .err()
        .unwrap();
    assert!(
        matches!(&error, ProjectionCommitmentWorkErrorV1::Protocol(p) if p.message() == "formal projection audience")
    );
    drop(error);
    assert_eq!(meter.used.get(), 0);
    let boundary = "x".repeat(MAX_PROJECTION_BYTES - 2);
    assert!(
        projection_leaf_with_work_v1(
            &ProjectionAudienceV1::public(),
            &boundary,
            &LegacyCapabilityCanonicalWorkV1
        )
        .is_ok()
    );
    let too_large = boundary + "x";
    assert_eq!(
        projection_leaf(&ProjectionAudienceV1::public(), &too_large).unwrap_err(),
        "formal projection byte bound"
    );
}
