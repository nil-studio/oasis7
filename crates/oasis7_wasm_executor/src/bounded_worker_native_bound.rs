//! Mandatory native environment checks inside the actual invocation process.
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use super::*;
/// The binary owner supplies the original native capture implementation. An
/// absent verifier never substitutes a successful empty environment check.
pub trait NativeEnvironmentVerifierV1 {
    type Guard;
    fn before(&mut self, descriptor: &[u8]) -> Result<Self::Guard, &'static str>;
    fn after(&mut self, descriptor: &[u8], guard: &Self::Guard) -> Result<(), &'static str>;
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[derive(serde::Serialize)]
pub(super) struct NativeBoundRequestV1<'a> {
    pub request: &'a ModuleCallRequest,
    pub descriptor: &'a [u8],
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DecodedNativeBoundRequestV1 {
    request: ModuleCallRequest,
    descriptor: Vec<u8>,
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub(super) fn child_call<V: NativeEnvironmentVerifierV1>(
    bytes: &[u8],
    output_max: u64,
    verifier: &mut V,
) -> Result<(), &'static str> {
    let DecodedNativeBoundRequestV1 {
        request,
        descriptor,
    } = serde_cbor::from_slice(bytes).map_err(|_| "native-bound worker request")?;
    // The existing native descriptor's original bounded format is at most 1MiB.
    if descriptor.is_empty()
        || descriptor.len() > 1 << 20
        || request.module_id.len() > 256
        || request.trace_id.len() > 256
        || request.entrypoint.len() > 256
        || request.wasm_hash.len() != 64
        || request.limits.max_output_bytes > output_max
    {
        return Err("native-bound worker request limits");
    }
    let guard = verifier.before(&descriptor)?;
    let config = crate::WasmExecutorConfig {
        max_fuel: request.limits.max_gas,
        max_mem_bytes: request.limits.max_mem_bytes,
        max_output_bytes: output_max,
        max_call_ms: 60_000,
        max_cache_entries: 1,
        compiled_cache_dir: None,
        ..crate::WasmExecutorConfig::default()
    };
    let executor = crate::WasmExecutor::new_for_bounded_worker_v1(config)
        .map_err(|_| "native-bound worker engine init")?;
    let mut output = None;
    let result = executor.call_for_bounded_worker_v1(&request, |bytes| {
        if output.is_some() || bytes.len() as u64 > output_max {
            return Err(failure(
                &request,
                ModuleCallErrorCode::InvalidOutput,
                "native-bound worker output bound",
            ));
        }
        output = Some(bytes.to_vec());
        Ok(())
    });
    // No output frame escapes until this *same* child checks its post-call
    // actual mapped files, pinned vnode metadata and exact signed descriptor.
    verifier.after(&descriptor, &guard)?;
    let (tag, bytes) = match result {
        Ok(()) => (0, output.ok_or("native-bound worker missing output")?),
        Err(WorkerExecutionErrorV1::Host(_)) => return Err("native-bound worker host failure"),
        Err(WorkerExecutionErrorV1::Vm(error)) => {
            if output.is_some() {
                return Err("native-bound worker partial output failure");
            }
            let mut length = error.detail.len().min(4096);
            while !error.detail.is_char_boundary(length) {
                length -= 1;
            }
            (
                encode_failure(&error.code),
                error.detail.as_bytes()[..length].to_vec(),
            )
        }
    };
    write_native_response(tag, &bytes)
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn write_native_response(tag: u8, bytes: &[u8]) -> Result<(), &'static str> {
    let mut stdout = std::io::stdout();
    stdout
        .write_all(&[tag])
        .map_err(|_| "native-bound output")?;
    stdout
        .write_all(&(bytes.len() as u64).to_be_bytes())
        .map_err(|_| "native-bound output")?;
    for part in bytes.chunks(64 << 10) {
        stdout.write_all(part).map_err(|_| "native-bound output")?;
    }
    stdout.flush().map_err(|_| "native-bound output flush")
}

#[cfg(all(
    feature = "wasmtime",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[derive(serde::Serialize)]
pub(super) struct NativeBoundRegexRequestV1<'a, P> {
    pub descriptor: &'a [u8],
    pub probes: P,
}
#[cfg(all(
    feature = "wasmtime",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DecodedNativeBoundRegexRequestV1 {
    descriptor: Vec<u8>,
    probes: Vec<super::regex_batch::OwnedProbe>,
}
#[cfg(all(
    feature = "wasmtime",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub(super) fn child_regex_call<V: NativeEnvironmentVerifierV1>(
    bytes: &[u8],
    request_max: u64,
    output_max: u64,
    verifier: &mut V,
) -> Result<(), &'static str> {
    let DecodedNativeBoundRegexRequestV1 { descriptor, probes } =
        serde_cbor::from_slice(bytes).map_err(|_| "native-bound regex request")?;
    if descriptor.is_empty() || descriptor.len() > 1 << 20 {
        return Err("native-bound regex descriptor limit");
    }
    let guard = verifier.before(&descriptor)?;
    let values = super::regex_batch::evaluate_regex_probes(&probes, request_max, output_max)?;
    // Including invalid-pattern results: neither success bits nor a pattern
    // error may escape an unverified post-call native environment.
    verifier.after(&descriptor, &guard)?;
    write_native_response(0, &values)
}
