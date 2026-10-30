use oasis7_agent_api::world_action::MaterialLedgerId;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    io::{self, Write},
};

// This integration-test executable owns its allocator. Only allocations on the
// measured test thread count; libtest and other tests cannot add noise.
struct MeasuredAllocator;
thread_local! {
    static MEASURING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATED: Cell<usize> = const { Cell::new(0) };
}
fn record_allocation(bytes: usize) {
    if MEASURING.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATED.try_with(|total| total.set(total.get().saturating_add(bytes)));
    }
}
// SAFETY: Every allocation and release is forwarded unchanged to System. The
// bookkeeping uses const-initialized thread-local Cells and never allocates.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        // SAFETY: The caller supplies the allocation layout required by System.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        // SAFETY: The caller supplies the allocation layout required by System.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The pointer and original layout are forwarded from the caller.
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, bytes: usize) -> *mut u8 {
        record_allocation(bytes);
        // SAFETY: The caller's original allocation and requested size are valid.
        unsafe { System.realloc(pointer, layout, bytes) }
    }
}
#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

fn measured<T>(action: impl FnOnce() -> T) -> (T, usize) {
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            MEASURING.with(|active| active.set(false));
        }
    }
    ALLOCATED.with(|total| total.set(0));
    MEASURING.with(|active| assert!(!active.replace(true)));
    let stop = Stop;
    let result = action();
    drop(stop);
    (result, ALLOCATED.with(Cell::get))
}

struct HashOutput {
    hash: blake3::Hasher,
    bytes: usize,
}
impl Write for HashOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.hash.update(bytes);
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn large_ledger_serialization_has_fixed_additional_allocation() {
    let id = "x".repeat(8 * 1024 * 1024);
    let mut expected = blake3::Hasher::new();
    expected.update(b"\"agent:");
    expected.update(id.as_bytes());
    expected.update(b"\"");
    let expected_hash = expected.finalize();
    let expected_bytes = id.len() + 8;
    let ledger = MaterialLedgerId::Agent(id);
    let mut output = HashOutput {
        hash: blake3::Hasher::new(),
        bytes: 0,
    };
    let (result, allocated) = measured(|| serde_json::to_writer(&mut output, &ledger));
    result.unwrap();
    assert_eq!(output.bytes, expected_bytes);
    assert_eq!(output.hash.finalize(), expected_hash);
    assert!(
        allocated <= 4096,
        "serialization requested {allocated} bytes"
    );
}

#[test]
fn refused_output_does_not_clone_a_large_ledger_before_admission() {
    struct Refused;
    impl Write for Refused {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::WriteZero.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let ledger = MaterialLedgerId::Agent("x".repeat(8 * 1024 * 1024));
    let (result, allocated) = measured(|| serde_json::to_writer(Refused, &ledger));
    assert!(result.unwrap_err().is_io());
    assert!(
        allocated <= 4096,
        "refused output requested {allocated} bytes"
    );
}

#[test]
fn ledger_json_and_cbor_keep_the_original_string_wire() {
    #[cfg(feature = "schema")]
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(MaterialLedgerId)).unwrap()["type"],
        "string"
    );
    for (ledger, text, json, cbor) in [
        (
            MaterialLedgerId::World,
            "world",
            "\"world\"",
            b"\x65world".as_slice(),
        ),
        (
            MaterialLedgerId::Agent("a".into()),
            "agent:a",
            "\"agent:a\"",
            b"\x67agent:a".as_slice(),
        ),
        (
            MaterialLedgerId::Site(String::new()),
            "site:",
            "\"site:\"",
            b"\x65site:".as_slice(),
        ),
        (
            MaterialLedgerId::Factory("中".into()),
            "factory:中",
            "\"factory:中\"",
            b"\x6bfactory:\xe4\xb8\xad".as_slice(),
        ),
    ] {
        assert_eq!(serde_json::to_string(&ledger).unwrap(), json);
        assert_eq!(serde_cbor::to_vec(&ledger).unwrap(), cbor);
        assert_eq!(ledger.to_string(), text);
    }
    let ledger = MaterialLedgerId::Agent("a\"\\\n中".into());
    let text = "agent:a\"\\\n中";
    assert_eq!(
        serde_json::to_vec(&ledger).unwrap(),
        serde_json::to_vec(text).unwrap()
    );
    assert_eq!(
        serde_cbor::to_vec(&ledger).unwrap(),
        serde_cbor::to_vec(&text).unwrap()
    );
    assert_eq!(
        serde_json::from_slice::<MaterialLedgerId>(&serde_json::to_vec(&ledger).unwrap()).unwrap(),
        ledger
    );
    assert_eq!(
        serde_cbor::from_slice::<MaterialLedgerId>(&serde_cbor::to_vec(&ledger).unwrap()).unwrap(),
        ledger
    );
}
