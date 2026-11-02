use super::*;
#[test]
fn authenticated_control_pages_transfer_hashes_and_refuse_missing_state_or_zero_allowance() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (pages, roots) = seed(&memory, "source 界", Some(0));
    let mut reads = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: &roots.state[0],
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut reads,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut scan = StateScanBudget {
        nodes_remaining: 100_000,
        leaves_remaining: 1000,
        read_bytes_remaining: 64 << 20,
    };
    assert!(matches!(
        mark_runtime_controls_pages_v1(
            &roots,
            &mut ctx,
            &mut RuntimeControlsPagesWorkV1 {
                outer_scan: &mut scan,
                max_text_bytes: 1024,
                max_pages: 0,
            }
        ),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    let mut sink = Pages::new(&memory);
    let pageset = transfer_runtime_controls_pages_v1(
        &roots,
        &mut ctx,
        &mut RuntimeControlsPagesWorkV1 {
            outer_scan: &mut scan,
            max_text_bytes: 1024,
            max_pages: 8192,
        },
        &mut sink,
    )
    .unwrap();
    pageset.qualification().validate_binding(&roots).unwrap();
    assert!(pageset.page_count() > 0);
    pageset
        .visit_pages::<ValueTreeError>(|id| {
            let source =
                ValueNodeReader::read_node(&pages, id, 1 << 20, &memory, MemoryClass::Ordinary)?;
            let target =
                ValueNodeReader::read_node(&sink, id, 1 << 20, &memory, MemoryClass::Ordinary)?;
            assert_eq!(source.as_slice(), target.as_slice());
            assert_eq!(blake3::hash(&target).as_bytes(), id);
            Ok(())
        })
        .unwrap();
    // Persisted test pages are independent of the source collection. Buffered
    // durable-store finish and fresh Recovery pools are a separate acceptance.
    let root_id =
        *blake3::Hash::from_hex(&roots.state_root(StateNamespaceV1::Authorization).node_root)
            .unwrap()
            .as_bytes();
    assert!(pages.nodes.borrow_mut().remove(&root_id).is_some());
    assert!(matches!(
        mark_runtime_controls_pages_v1(
            &roots,
            &mut ctx,
            &mut RuntimeControlsPagesWorkV1 {
                outer_scan: &mut scan,
                max_text_bytes: 1024,
                max_pages: 8192,
            }
        ),
        Err(ValueTreeError::Invalid(_))
    ));
}
