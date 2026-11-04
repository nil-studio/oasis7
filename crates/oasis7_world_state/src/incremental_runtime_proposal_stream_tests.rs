//! Full family qualification is independent of selected cold DTO output quota.
use super::*;
#[test]
fn proposal_full_family_streams_large_text_json_and_patch_without_selected_heap() {
    let memory = MemoryBudget::new(192 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let mut source = proposal(ProposalStatus::Proposed, true);
    source.author = "author".repeat(32768);
    source.manifest.content = serde_json::Value::String("json".repeat(65536));
    source
        .patch
        .as_mut()
        .unwrap()
        .ops
        .push(ManifestPatchOp::Remove {
            path: vec!["segment".repeat(16384)],
        });
    let mut entries = BTreeMap::new();
    entries.insert(13, source);
    let (pages, roots) = seed(&memory, &entries);
    drop(entries);
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
    ctx.semantic_bytes = 4096;
    ctx.record_bytes = 3 << 20;
    let mut work = rw();
    work.max_selected_heap = 1;
    let family = qualify_runtime_proposals_v1(&roots, &mut ctx, &mut scan(), &mut work).unwrap();
    assert_eq!(family.proposals(), 1);
    let scope = read_runtime_proposal_body_qualification_v1(&family, &roots, 13, &mut ctx)
        .unwrap()
        .unwrap();
    assert!(matches!(
        recover_runtime_proposal_body_v1(scope.root(), &mut ctx, &mut scan(), &mut rw().json, 1),
        Err(ValueTreeError::ResourceLimited(_))
    ));
}
