use super::*;

#[test]
fn recovery_is_reserved_within_shared_total_and_overflow_is_rejected() {
    assert!(MemoryBudget::new(10, 11).is_err());
    let budget = MemoryBudget::new(100, 20).unwrap();
    let ordinary = budget.reserve(80, MemoryClass::Ordinary).unwrap();
    assert!(budget.reserve(1, MemoryClass::Ordinary).is_err());
    let recovery = budget.reserve(20, MemoryClass::Recovery).unwrap();
    assert!(budget.reserve(1, MemoryClass::Recovery).is_err());
    assert!(budget.reserve(usize::MAX, MemoryClass::Recovery).is_err());
    assert_eq!(budget.used(), 100);
    drop(ordinary);
    assert_eq!(budget.used(), 20);
    drop(recovery);
    assert_eq!(budget.used(), 0);
}

#[test]
fn eviction_does_not_release_reader_owned_charge_and_split_transfers_accounting() {
    let budget = MemoryBudget::new(100, 0).unwrap();
    let mut charge = budget.reserve(100, MemoryClass::Ordinary).unwrap();
    let output_charge = charge.split(60).unwrap();
    assert!(charge.split(41).is_err());
    let cached = Arc::new(output_charge.retain(vec![0u8; 60]));
    let reader = cached.clone();
    drop(cached);
    drop(charge);
    assert_eq!(budget.used(), 60);
    assert_eq!(reader.len(), 60);
    assert_eq!(reader.reserved_bytes(), 60);
    assert!(budget.reserve(41, MemoryClass::Ordinary).is_err());
    drop(reader);
    assert_eq!(budget.used(), 0);
}

#[test]
fn concurrent_consumers_cannot_over_admit_and_cancel_releases_capacity() {
    let budget = MemoryBudget::new(64, 0).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(9));
    let mut workers = Vec::new();
    for _ in 0..8 {
        let budget = budget.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            let reservation = budget.reserve(16, MemoryClass::Ordinary);
            barrier.wait();
            barrier.wait();
            reservation.is_ok()
        }));
    }
    barrier.wait();
    assert_eq!(budget.used(), 64);
    barrier.wait();
    let accepted = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .filter(|accepted| *accepted)
        .count();
    assert_eq!(accepted, 4);
    assert_eq!(budget.used(), 0);
}
