//! Trusted single-use worker. No input is decoded and no Engine is constructed
//! until Linux confirms the inherited hard address-space/core limits.
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use super::*;
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use std::io::{self, Read, Write};
pub fn worker_main_v1() -> Result<(), &'static str> {
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    {
        linux_main()
    }
    #[cfg(not(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    {
        Err("unsupported kernel worker platform")
    }
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn linux_main() -> Result<(), &'static str> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--oasis7-wasm-worker-v1") {
        return Err("worker mode flag required");
    }
    let mut number = || {
        args.next()
            .ok_or("missing worker limit")?
            .parse::<u64>()
            .map_err(|_| "invalid worker limit")
    };
    let request_max = number()?;
    let output_max = number()?;
    let cap = number()?;
    let mode = args.next();
    if args.next().is_some()
        || mode.as_deref().is_some_and(|mode| mode != "regex-batch-v1")
        || request_max == 0
        || output_max == 0
        || cap == 0
    {
        return Err("invalid worker limits");
    }
    #[repr(C)]
    struct Limit {
        soft: u64,
        hard: u64,
    }
    unsafe extern "C" {
        fn getrlimit(resource: i32, limit: *mut Limit) -> i32;
    }
    let mut address = Limit { soft: 0, hard: 0 };
    let mut core = Limit { soft: 1, hard: 1 };
    // SAFETY: both pointers refer to initialized ABI-compatible Linux64 structs.
    if unsafe { getrlimit(9, &mut address) } != 0
        || unsafe { getrlimit(4, &mut core) } != 0
        || address.soft != cap
        || address.hard != cap
        || core.soft != 0
        || core.hard != 0
    {
        return Err("worker hard limit absent");
    }
    if mode.as_deref() == Some("regex-batch-v1")
        && (request_max > super::regex_batch::REGEX_MAX_REQUEST_BYTES_V1
            || output_max > super::regex_batch::REGEX_MAX_OUTPUT_BYTES_V1)
    {
        return Err("regex protocol caps");
    }
    let mut bytes = Vec::new();
    io::stdin()
        .take(request_max.checked_add(1).ok_or("request limit overflow")?)
        .read_to_end(&mut bytes)
        .map_err(|_| "worker input")?;
    if bytes.len() as u64 > request_max {
        return Err("worker input bound");
    }
    if mode.as_deref() == Some("regex-batch-v1") {
        #[cfg(feature = "wasmtime")]
        {
            return super::regex_batch::child_regex(&bytes, request_max, output_max);
        }
        #[cfg(not(feature = "wasmtime"))]
        {
            return Err("regex worker feature unavailable");
        }
    }
    let request: ModuleCallRequest =
        serde_cbor::from_slice(&bytes).map_err(|_| "worker request decode")?;
    drop(bytes);
    if request.module_id.len() > 256
        || request.trace_id.len() > 256
        || request.entrypoint.len() > 256
        || request.wasm_hash.len() != 64
        || request.limits.max_output_bytes > output_max
    {
        return Err("worker request limits");
    }
    let config = crate::WasmExecutorConfig {
        max_fuel: request.limits.max_gas,
        max_mem_bytes: request.limits.max_mem_bytes,
        max_output_bytes: output_max,
        max_call_ms: 60_000,
        max_cache_entries: 1,
        compiled_cache_dir: None,
        ..crate::WasmExecutorConfig::default()
    };
    let executor =
        crate::WasmExecutor::new_for_bounded_worker_v1(config).map_err(|_| "worker engine init")?;
    struct Sink {
        seen: bool,
        max: u64,
        output: io::Stdout,
    }
    impl ModuleRawOutputSinkV1 for Sink {
        fn consume_output(&mut self, b: &[u8]) -> Result<(), ModuleCallFailure> {
            if self.seen || b.len() as u64 > self.max {
                return Err(ModuleCallFailure {
                    module_id: String::new(),
                    trace_id: String::new(),
                    code: ModuleCallErrorCode::OutputTooLarge,
                    detail: String::new(),
                });
            }
            self.seen = true;
            let result = (|| {
                self.output.write_all(&[0])?;
                self.output.write_all(&(b.len() as u64).to_be_bytes())?;
                for part in b.chunks(64 << 10) {
                    self.output.write_all(part)?;
                }
                self.output.flush()
            })();
            result.map_err(|_: io::Error| ModuleCallFailure {
                module_id: String::new(),
                trace_id: String::new(),
                code: ModuleCallErrorCode::InvalidOutput,
                detail: String::new(),
            })
        }
    }
    let mut sink = Sink {
        seen: false,
        max: output_max,
        output: io::stdout(),
    };
    let result = executor.call_for_bounded_worker_v1(&request, |bytes| sink.consume_output(bytes));
    if let Err(error) = result {
        let failure = match error {
            super::WorkerExecutionErrorV1::Host(_) => return Err("worker host execution failure"),
            super::WorkerExecutionErrorV1::Vm(failure) => failure,
        };
        if sink.seen {
            return Err("worker partial output failure");
        }
        sink.output
            .write_all(&[super::encode_failure(&failure.code)])
            .map_err(|_| "worker failure output")?;
        let mut end = failure.detail.len().min(4096);
        while !failure.detail.is_char_boundary(end) {
            end -= 1;
        }
        sink.output
            .write_all(&(end as u64).to_be_bytes())
            .map_err(|_| "worker failure output")?;
        sink.output
            .write_all(&failure.detail.as_bytes()[..end])
            .map_err(|_| "worker failure output")?;
        sink.output.flush().map_err(|_| "worker failure flush")?;
    } else if !sink.seen {
        return Err("worker missing output");
    }
    Ok(())
}
