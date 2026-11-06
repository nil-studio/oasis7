//! Execute the immutable worker itself before admitting its typed protocol.
use super::*;
const HANDSHAKE: &[u8] = b"oasis7-worker-v1;typed-v1;wasmtime;regex-v1";
impl<A: CanonicalCborAllocator> BoundedWorkerSandboxV1<'_, A> {
    /// Checks the actual child protocol and compiled features under the same
    /// kernel/process limits as an invocation. This grants no module authority.
    pub fn verify_native_protocol_v1(
        &self,
        request: &ModuleCallRequest,
    ) -> Result<(), WorkerExecutionErrorV1> {
        let error = |message| {
            host_failure(failure(
                request,
                ModuleCallErrorCode::SandboxUnavailable,
                message,
            ))
        };
        if !matches!(self.executable, WorkerExecutableV1::SealedFd(_))
            || self.limits.address_space_bytes == 0
            || self.limits.wall_time_ms == 0
            || self.limits.max_request_bytes == 0
            || self.limits.max_output_bytes < HANDSHAKE.len() as u64
        {
            return Err(error("native handshake explicit limits/sealed executable"));
        }
        let bytes = self
            .limits
            .address_space_bytes
            .checked_add(128 << 10)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| error("native handshake admission overflow"))?;
        let _fee = self
            .allocator
            .reserve(bytes)
            .map_err(|_| error("native handshake admission"))?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(host_failure(failure(
                request,
                ModuleCallErrorCode::Interrupted,
                "native handshake cancelled",
            )));
        }
        #[cfg(all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ))]
        {
            struct Check<'a> {
                request: &'a ModuleCallRequest,
                seen: bool,
            }
            impl ModuleRawOutputSinkV1 for Check<'_> {
                fn consume_output(&mut self, bytes: &[u8]) -> Result<(), ModuleCallFailure> {
                    if self.seen || bytes != HANDSHAKE {
                        return Err(failure(
                            self.request,
                            ModuleCallErrorCode::SandboxUnavailable,
                            "native worker protocol/features mismatch",
                        ));
                    }
                    self.seen = true;
                    Ok(())
                }
            }
            let mut sink = Check {
                request,
                seen: false,
            };
            let result = run_shared(
                self,
                request,
                &(),
                &mut sink,
                WorkerProtocolV1 {
                    mode: Some("--oasis7-wasm-worker-handshake-v1"),
                    request_max: 1,
                    output_max: HANDSHAKE.len() as u64,
                },
                &mut || false,
            )
            .map_err(host_failure)?;
            if result.is_some() || !sink.seen {
                return Err(error("native worker handshake missing"));
            }
            Ok(())
        }
        #[cfg(not(all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )))]
        {
            Err(error("native worker handshake unsupported platform"))
        }
    }
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub(super) fn child_handshake(output_max: u64) -> Result<(), &'static str> {
    // Generated from the actual executable's compile-time feature selection,
    // never from request fields or metadata supplied by the parent.
    #[cfg(feature = "wasmtime")]
    let bytes = HANDSHAKE;
    #[cfg(not(feature = "wasmtime"))]
    let bytes = b"oasis7-worker-v1;typed-v1;no-wasmtime".as_slice();
    if bytes.len() as u64 > output_max {
        return Err("handshake output bound");
    }
    let mut output = std::io::stdout();
    output.write_all(&[0]).map_err(|_| "handshake output")?;
    output
        .write_all(&(bytes.len() as u64).to_be_bytes())
        .map_err(|_| "handshake output")?;
    output.write_all(bytes).map_err(|_| "handshake output")?;
    output.flush().map_err(|_| "handshake flush")
}
