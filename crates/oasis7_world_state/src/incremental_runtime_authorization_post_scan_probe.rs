//! Measure retained JSON output credit using valid materializations, not caps.
use super::*;
pub(super) fn retained_json_bytes(
    root: &StateRootV1,
    pages: &Pages,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    max_heap: usize,
) -> usize {
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(pages, roots, memory, &mut value_io, &mut keys);
    let mut columns = read_columns_v1(
        profile(AuthorizationBodyKindV1::Audit),
        root,
        &mut ctx,
        &mut scan(),
        max_heap,
    )
    .unwrap();
    columns
        .spend_output(3 * std::mem::size_of::<MemoryReservation>() + 128)
        .unwrap();
    let mut charges = Vec::with_capacity(3);
    let mut work = json_work();
    let mut map = columns.cells_mut();
    let subject = json(
        AuthorizationBodyKindV1::Audit,
        &mut map,
        "subject",
        &mut ctx,
        &mut work,
        max_heap,
        &mut charges,
    )
    .unwrap();
    let audience = json(
        AuthorizationBodyKindV1::Audit,
        &mut map,
        "audience",
        &mut ctx,
        &mut work,
        max_heap,
        &mut charges,
    )
    .unwrap();
    let presenter = if present(&mut map, "presenter").unwrap() {
        Some(
            json(
                AuthorizationBodyKindV1::Audit,
                &mut map,
                "presenter",
                &mut ctx,
                &mut work,
                max_heap,
                &mut charges,
            )
            .unwrap(),
        )
    } else {
        None
    };
    let bytes = charges.iter().map(|charge| charge.bytes()).sum();
    drop(subject);
    drop(audience);
    drop(presenter);
    drop(charges);
    drop(columns);
    bytes
}
