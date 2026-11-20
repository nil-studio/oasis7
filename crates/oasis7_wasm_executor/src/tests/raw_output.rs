//! Actual Wasmtime execution: typed and raw consumers share the same VM path.
use super::*;

#[cfg(feature = "wasmtime")]
#[test]
fn raw_sink_receives_real_vm_bytes_before_typed_decode() {
    use oasis7_wasm_abi::module_raw_output::{ModuleRawOutputSinkV1, ModuleSandboxRawOutputV1};

    #[derive(Default)]
    struct Sink {
        bytes: Vec<u8>,
        calls: usize,
        reject: bool,
    }
    impl ModuleRawOutputSinkV1 for Sink {
        fn consume_output(&mut self, bytes: &[u8]) -> Result<(), ModuleCallFailure> {
            self.calls += 1;
            self.bytes = bytes.to_vec();
            if self.reject {
                return Err(ModuleCallFailure {
                    module_id: "sink".into(),
                    trace_id: "sink-refusal".into(),
                    code: ModuleCallErrorCode::InvalidOutput,
                    detail: "sink denied".into(),
                });
            }
            Ok(())
        }
    }

    let mut executor = test_executor(WasmExecutorConfig::default());
    let request_for = |wasm: Vec<u8>| {
        let mut request = make_request(ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 100_000,
            max_call_rate: 0,
            max_output_bytes: 4096,
            max_effects: 4,
            max_emits: 4,
        });
        request.wasm_hash = sha256_hex_for_test(&wasm);
        request.wasm_bytes = Arc::<[u8]>::from(wasm);
        request
    };

    let request = request_for(trivial_success_wasm());
    let mut sink = Sink::default();
    executor.call_to(&request, &mut sink).unwrap();
    assert_eq!(sink.calls, 1);
    assert_eq!(sink.bytes, trivial_output_bytes());
    assert_eq!(
        executor.call(&request).unwrap().output_bytes,
        sink.bytes.len() as u64
    );

    let bad = request_for(invalid_cbor_output_wasm());
    assert_eq!(
        executor.call(&bad).unwrap_err().code,
        ModuleCallErrorCode::InvalidOutput
    );
    let mut sink = Sink::default();
    executor.call_to(&bad, &mut sink).unwrap();
    assert_eq!(sink.calls, 1);
    assert_eq!(sink.bytes, vec![255]);

    let mut sink = Sink {
        reject: true,
        ..Default::default()
    };
    let error = executor.call_to(&request, &mut sink).unwrap_err();
    assert_eq!(sink.calls, 1);
    assert_eq!(error.code, ModuleCallErrorCode::InvalidOutput);
    assert_eq!(error.detail, "sink denied");
    assert_eq!(error.module_id, "sink");
}
