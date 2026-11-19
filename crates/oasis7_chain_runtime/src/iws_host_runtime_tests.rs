use super::*;
#[test]
fn empty_host_has_no_fence_source_or_worker_permission() {
    let host = IwsHostRuntimeV1::default();
    let memory = MemoryBudget::new(1 << 20, 0).unwrap();
    let work = FormalArchiveReadWorkV1::new(100, 1024).unwrap();
    assert!(!host.is_installed());
    assert_eq!(host.current_fence(), None);
    assert_eq!(host.current_record_id(), None);
    assert_eq!(host.current_namespace_identities(), None);
    assert_eq!(host.generation(), 0);
    assert!(
        matches!(host.verify_current_worker_bounded(&memory,&work,&AtomicBool::new(false)),
        Err(IwsHostRuntimeErrorV1::Formal(FormalError::Invalid(reason))) if reason=="Host current owner missing")
    );
    assert_eq!(memory.used(), 0);
    assert_eq!(work.remaining_steps(), 100);
}
