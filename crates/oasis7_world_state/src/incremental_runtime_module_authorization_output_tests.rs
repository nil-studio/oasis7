use super::*;
use std::{
    cell::Cell,
    io::{self, Write},
};
struct Output {
    mode: u8,
    calls: usize,
    bytes: Vec<u8>,
    stop: Rc<Cell<bool>>,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        match self.mode {
            1 => Err(io::ErrorKind::Interrupted.into()),
            2 => Ok(bytes.len() + 1),
            3 => Ok(0),
            mode => {
                let n = if mode == 4 {
                    bytes.len()
                } else {
                    1.min(bytes.len())
                };
                self.bytes.extend_from_slice(&bytes[..n]);
                if mode == 4 {
                    self.stop.set(true);
                }
                Ok(n)
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn qualified_snapshot_text_output_has_bounded_retries_and_exact_failure_boundaries() {
    let backing = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = backing.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages(Rc::new(RefCell::new(BTreeMap::new())));
    let name = "字🙂\0original";
    let grant = CapabilityGrant {
        name: name.into(),
        effect_kinds: vec![],
        expiry: None,
    };
    let root = genesis::body(
        genesis::Body::Grant(&grant),
        &mut writer(&mut pages, &backing, &mut io(), &mut keys()),
        &mut work(),
    )
    .unwrap();
    drop(grant);
    for mode in 0..5 {
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let mut state_io = io();
        let state_io = RefCell::new(&mut state_io);
        let mut never = || false;
        let cancel = RefCell::new(&mut never);
        let error = RefCell::new(None);
        let state = tree::Source {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &state_io,
            cancelled: &cancel,
            error: &error,
            labels: genesis::LABELS,
        };
        let mut q = io();
        let mut k = keys();
        let stop = Rc::new(Cell::new(false));
        let mut ctx = WorldFieldReadContextV1 {
            root: &root,
            state: &state,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut q,
            },
            key_work: &mut k,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || stop.get(),
        };
        let proof = qualify_module_authorization_body_v1(
            ModuleAuthorizationFieldV1::Capabilities,
            &mut ctx,
            &mut StateScanBudget {
                nodes_remaining: 100_000,
                read_bytes_remaining: 64 << 20,
                leaves_remaining: 10_000,
            },
            &mut ModuleAuthorizationReadWorkV1 {
                cells_remaining: 10_000,
                max_payload_bytes: 2 << 20,
            },
        )
        .unwrap();
        let _output = memory.reserve(128, MemoryClass::Ordinary).unwrap();
        let mut output = Output {
            mode,
            calls: 0,
            bytes: Vec::with_capacity(128),
            stop: stop.clone(),
        };
        let result = read_qualified_module_authorization_cell_v1(
            &proof,
            ModuleAuthorizationAddressV1::fixed("name"),
            &mut ctx,
            &mut ModuleAuthorizationReadWorkV1 {
                cells_remaining: if mode == 1 { 2 } else { 100 },
                max_payload_bytes: 128,
            },
            &mut output,
        );
        assert!(output.calls > 0);
        match (mode, result) {
            (0, Ok(ModuleAuthorizationValueV1::Text { utf8_bytes })) => {
                assert_eq!(utf8_bytes, name.len() as u64);
                assert_eq!(output.bytes, name.as_bytes());
                assert!(output.calls > 1);
            }
            (1, Err(ValueTreeError::ResourceLimited("module authorization output work"))) => {
                assert_eq!(output.calls, 1)
            }
            (2, Err(ValueTreeError::Invalid(_))) => {}
            (3, Err(ValueTreeError::Io(e))) => assert_eq!(e.kind(), io::ErrorKind::WriteZero),
            (4, Err(ValueTreeError::Cancelled)) => assert_eq!(output.bytes, name.as_bytes()),
            (mode, other) => panic!("unexpected output mode {mode}: {other:?}"),
        }
    }
}
