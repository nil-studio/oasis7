//! Test-only original Index producers and genuine signed native checkpoints.
//! No mutable Hot setter, caller root, IWS GEN or execution permission is exposed.
use super::super::store::Index;
use super::*;

pub(in super::super) enum InsertPlanFixtureV1<'a> {
    Canonical(&'a [(&'a str, &'a str)]),
    FullDepth256,
    RepeatedBit,
    OutOfRangeBit,
    LongUnchosenSibling,
    LongLeafEntry,
    NonHexExistingKey,
    ShortHexExistingKey,
    OverlongExistingSamePrefix,
    ChosenMissing,
    ChosenCorruptWire,
}
fn leaf(store: &Store, key: String, entry: String) -> String {
    store
        .put(&Index::Leaf {
            request: key,
            entry,
        })
        .unwrap()
}
fn fork(store: &Store, bit: u16, left: String, right: String) -> String {
    store.put(&Index::Fork { bit, left, right }).unwrap()
}
fn build(store: &Store, shape: InsertPlanFixtureV1<'_>) -> Option<String> {
    let zero = "00".repeat(32);
    let entry = "09".repeat(32);
    Some(match shape {
        InsertPlanFixtureV1::Canonical(rows) => {
            let mut root = None;
            for (request, entry) in rows {
                root = Some(store.insert(root.as_deref(), request, entry).unwrap());
            }
            return root;
        }
        InsertPlanFixtureV1::FullDepth256 => {
            let mut root = store.insert(None, &zero, &entry).unwrap();
            for bit in 0..256usize {
                let mut key = [0u8; 32];
                key[bit / 8] = 128 >> (bit % 8);
                root = store
                    .insert(Some(&root), &hex::encode(key), &entry)
                    .unwrap();
            }
            root
        }
        InsertPlanFixtureV1::RepeatedBit => {
            let leaf = leaf(store, zero.clone(), entry);
            let child = fork(store, 0, leaf.clone(), leaf.clone());
            fork(store, 0, child, leaf)
        }
        InsertPlanFixtureV1::OutOfRangeBit => {
            let leaf = leaf(store, zero, entry);
            fork(store, 256, leaf.clone(), leaf)
        }
        InsertPlanFixtureV1::LongUnchosenSibling => {
            let selected = leaf(store, zero, entry);
            fork(store, 0, selected, "unselected/".repeat(512))
        }
        InsertPlanFixtureV1::LongLeafEntry => leaf(store, zero, "long entry/".repeat(512)),
        InsertPlanFixtureV1::NonHexExistingKey => leaf(store, "not-hex".into(), entry),
        InsertPlanFixtureV1::ShortHexExistingKey => leaf(store, "00".into(), entry),
        InsertPlanFixtureV1::OverlongExistingSamePrefix => leaf(store, "00".repeat(33), entry),
        InsertPlanFixtureV1::ChosenMissing => {
            let other = leaf(store, "ff".repeat(32), entry);
            fork(store, 0, "ab".repeat(32), other)
        }
        InsertPlanFixtureV1::ChosenCorruptWire => {
            // Real original writer and hash: the selected object is valid CBOR
            // but not Index. Only the root is used by native ref validation.
            let bad = store.put(&0u8).unwrap();
            let other = leaf(store, "ff".repeat(32), entry);
            fork(store, 0, bad, other)
        }
    })
}
pub(in super::super) fn with_index_insert_plan_fixture<T>(
    shape: InsertPlanFixtureV1<'_>,
    run: impl FnOnce(&Store, &Path) -> T,
) -> T {
    let fixture = Fixture::new();
    let mut endpoint = fixture.endpoint(EndpointRole::Primary);
    let index = build(&endpoint.store, shape);
    let mut guard = endpoint.guard().unwrap();
    let mut next = endpoint.store.body.clone();
    next.index = index;
    endpoint.publish_native(next, guard.as_mut()).unwrap();
    // The reader sees the actual signed checkpoint via the original verifier.
    endpoint.store.refresh_public(&endpoint.public_key).unwrap();
    run(&endpoint.store, &fixture.pdir)
}
