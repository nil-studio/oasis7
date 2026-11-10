//! Closed regex protocol; opaque regex allocations occur only under worker RLIMIT_AS.
use super::*;
use oasis7_wasm_abi::CanonicalCborStreamError;
use serde::Serialize;
pub const REGEX_MAX_PROBES_V1: usize = 4096;
pub const REGEX_MAX_OUTPUT_BYTES_V1: u64 = REGEX_MAX_PROBES_V1 as u64 + 1;
pub const REGEX_MAX_PATTERN_BYTES_V1: usize = 64 << 10;
pub const REGEX_MAX_TEXT_BYTES_V1: usize = 4 << 20;
pub const REGEX_MAX_REQUEST_BYTES_V1: u64 = 16 << 20;
pub const REGEX_MAX_WORK_V1: u64 = 32 << 20;
#[derive(Serialize, Clone, Copy)]
pub struct RegexProbeV1<'a> {
    pub pattern: &'a str,
    pub text: Option<&'a str>,
}
#[cfg(all(
    feature = "wasmtime",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
struct ProbeSequence<I>(I);
#[cfg(all(
    feature = "wasmtime",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
impl<'p, I> Serialize for ProbeSequence<I>
where
    I: Clone + ExactSizeIterator<Item = RegexProbeV1<'p>>,
{
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for probe in self.0.clone() {
            seq.serialize_element(&probe)?;
        }
        seq.end()
    }
}
#[cfg(all(
    feature = "wasmtime",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnedProbe {
    pattern: String,
    text: Option<String>,
}
pub struct RegexResultsV1<Q> {
    values: Vec<bool>,
    _charge: Q,
}
impl<Q> RegexResultsV1<Q> {
    pub fn values(&self) -> &[bool] {
        &self.values
    }
}
#[derive(Debug)]
pub enum RegexWorkerErrorV1 {
    Admission(CanonicalCborStreamError),
    Unsupported,
    Cancelled,
    Limit,
    InvalidPattern { probe: usize },
    Protocol,
    Host(ModuleCallFailure),
}
fn validate<'a>(
    probes: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    count: usize,
    max_request: u64,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(), RegexWorkerErrorV1> {
    if count > REGEX_MAX_PROBES_V1 || max_request == 0 {
        return Err(RegexWorkerErrorV1::Limit);
    }
    let max_request = max_request.min(REGEX_MAX_REQUEST_BYTES_V1);
    let mut request = 16u64;
    let mut work = 0u64;
    for (pattern, text) in probes {
        if cancelled() {
            return Err(RegexWorkerErrorV1::Cancelled);
        }
        if pattern.len() > REGEX_MAX_PATTERN_BYTES_V1
            || text.is_some_and(|s| s.len() > REGEX_MAX_TEXT_BYTES_V1)
        {
            return Err(RegexWorkerErrorV1::Limit);
        }
        // CBOR sequence/struct/map keys and length heads have a fixed per-probe upper bound.
        let size = (pattern.len() as u64)
            .checked_add(text.map_or(0, |s| s.len() as u64))
            .ok_or(RegexWorkerErrorV1::Limit)?;
        request = request
            .checked_add(size)
            .and_then(|n| n.checked_add(64))
            .ok_or(RegexWorkerErrorV1::Limit)?;
        let search = text
            .map_or(Some(pattern.len().max(1) as u64), |s| {
                (pattern.len().max(1) as u64).checked_mul(s.len().max(1) as u64)
            })
            .ok_or(RegexWorkerErrorV1::Limit)?;
        work = work
            .checked_add(size)
            .and_then(|n| n.checked_add(search))
            .ok_or(RegexWorkerErrorV1::Limit)?;
        if request > max_request || work > REGEX_MAX_WORK_V1 {
            return Err(RegexWorkerErrorV1::Limit);
        }
    }
    Ok(())
}
#[cfg(any(
    test,
    all(
        feature = "wasmtime",
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
))]
fn decode_results(
    bytes: &[u8],
    count: usize,
    values: &mut Vec<bool>,
) -> Result<Option<usize>, RegexWorkerErrorV1> {
    match bytes.first() {
        Some(0) => {
            if bytes.len() != count.checked_add(1).ok_or(RegexWorkerErrorV1::Protocol)?
                || bytes[1..].iter().any(|v| *v > 1)
                || values.capacity() < count
            {
                return Err(RegexWorkerErrorV1::Protocol);
            }
            values.extend(bytes[1..].iter().map(|v| *v == 1));
            Ok(None)
        }
        Some(1) => {
            if bytes.len() != 5 {
                return Err(RegexWorkerErrorV1::Protocol);
            }
            let index = u32::from_be_bytes(
                bytes[1..]
                    .try_into()
                    .map_err(|_| RegexWorkerErrorV1::Protocol)?,
            ) as usize;
            if index >= count {
                return Err(RegexWorkerErrorV1::Protocol);
            }
            Ok(Some(index))
        }
        _ => Err(RegexWorkerErrorV1::Protocol),
    }
}
impl<A: CanonicalCborAllocator> BoundedWorkerSandboxV1<'_, A> {
    pub fn match_regex_batch_v1<C: FnMut() -> bool>(
        &mut self,
        probes: &[RegexProbeV1<'_>],
        cancelled: &mut C,
    ) -> Result<RegexResultsV1<A::Charge>, RegexWorkerErrorV1> {
        self.match_regex_batch_iter_v1(probes.iter().copied(), cancelled)
    }
    pub fn match_regex_batch_iter_v1<'p, C, I>(
        &mut self,
        probes: I,
        cancelled: &mut C,
    ) -> Result<RegexResultsV1<A::Charge>, RegexWorkerErrorV1>
    where
        C: FnMut() -> bool,
        I: Clone + ExactSizeIterator<Item = RegexProbeV1<'p>>,
    {
        let count = probes.len();
        let mut observed = false;
        let mut cancelled = || {
            observed |= self.cancelled.load(Ordering::Acquire) || cancelled();
            observed
        };
        validate(
            probes.clone().map(|p| (p.pattern, p.text)),
            count,
            self.limits.max_request_bytes,
            &mut cancelled,
        )?;
        if cancelled() {
            return Err(RegexWorkerErrorV1::Cancelled);
        }
        if !matches!(
            self.executable,
            WorkerExecutableV1::SealedFd(_) | WorkerExecutableV1::NativeBoundSealedFd { .. }
        ) {
            return Err(RegexWorkerErrorV1::Unsupported);
        }
        if self.limits.address_space_bytes == 0
            || self.limits.wall_time_ms == 0
            || self.limits.max_output_bytes < (count as u64 + 1).max(5)
        {
            return Err(RegexWorkerErrorV1::Limit);
        }
        #[cfg(not(all(
            feature = "wasmtime",
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )))]
        {
            Err(RegexWorkerErrorV1::Unsupported)
        }
        #[cfg(all(
            feature = "wasmtime",
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ))]
        {
            let fee = self
                .limits
                .address_space_bytes
                .checked_add(self.limits.max_output_bytes)
                .and_then(|n| n.checked_add(128 << 10))
                .and_then(|n| usize::try_from(n).ok())
                .ok_or(RegexWorkerErrorV1::Limit)?;
            let charge = self
                .allocator
                .reserve(fee)
                .map_err(RegexWorkerErrorV1::Admission)?;
            let mut values = Vec::new();
            values
                .try_reserve_exact(count)
                .map_err(|_| RegexWorkerErrorV1::Limit)?;
            if values.capacity() > count {
                return Err(RegexWorkerErrorV1::Limit);
            }
            let diagnostic = ModuleCallRequest {
                module_id: String::new(),
                trace_id: String::new(),
                entrypoint: String::new(),
                wasm_hash: String::new(),
                input: Vec::new(),
                limits: Default::default(),
                wasm_bytes: Default::default(),
            };
            struct Sink<'a> {
                values: &'a mut Vec<bool>,
                count: usize,
                invalid: Option<usize>,
                seen: bool,
            }
            impl ModuleRawOutputSinkV1 for Sink<'_> {
                fn consume_output(&mut self, b: &[u8]) -> Result<(), ModuleCallFailure> {
                    let invalid = || {
                        failure(
                            &ModuleCallRequest {
                                module_id: String::new(),
                                trace_id: String::new(),
                                entrypoint: String::new(),
                                wasm_hash: String::new(),
                                input: Vec::new(),
                                limits: Default::default(),
                                wasm_bytes: Default::default(),
                            },
                            ModuleCallErrorCode::InvalidOutput,
                            "regex protocol",
                        )
                    };
                    if self.seen {
                        return Err(invalid());
                    }
                    self.seen = true;
                    self.invalid =
                        decode_results(b, self.count, self.values).map_err(|_| invalid())?;
                    Ok(())
                }
            }
            let mut sink = Sink {
                values: &mut values,
                count,
                invalid: None,
                seen: false,
            };
            let request_max = self
                .limits
                .max_request_bytes
                .min(REGEX_MAX_REQUEST_BYTES_V1);
            let output_max = self.limits.max_output_bytes.min(REGEX_MAX_OUTPUT_BYTES_V1);
            let result = if let WorkerExecutableV1::NativeBoundSealedFd { descriptor, .. } =
                &self.executable
            {
                if descriptor.is_empty() || descriptor.len() > 1 << 20 {
                    return Err(RegexWorkerErrorV1::Limit);
                }
                run_shared(
                    self,
                    &diagnostic,
                    &super::native_bound::NativeBoundRegexRequestV1 {
                        descriptor,
                        probes: ProbeSequence(probes),
                    },
                    &mut sink,
                    WorkerProtocolV1 {
                        mode: Some("--oasis7-native-bound-regex-worker-v1"),
                        request_max,
                        output_max,
                    },
                    &mut cancelled,
                )
            } else {
                run_shared(
                    self,
                    &diagnostic,
                    &ProbeSequence(probes),
                    &mut sink,
                    WorkerProtocolV1 {
                        mode: Some("regex-batch-v1"),
                        request_max,
                        output_max,
                    },
                    &mut cancelled,
                )
            };
            if cancelled() {
                return Err(RegexWorkerErrorV1::Cancelled);
            }
            match result {
                Ok(None) => {}
                Ok(Some(_)) => return Err(RegexWorkerErrorV1::Protocol),
                Err(e) => return Err(RegexWorkerErrorV1::Host(e)),
            }
            if let Some(probe) = sink.invalid {
                return Err(RegexWorkerErrorV1::InvalidPattern { probe });
            }
            if !sink.seen || values.len() != count {
                return Err(RegexWorkerErrorV1::Protocol);
            }
            Ok(RegexResultsV1 {
                values,
                _charge: charge,
            })
        }
    }
}
#[cfg(all(
    feature = "wasmtime",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub(super) fn child_regex(
    bytes: &[u8],
    request_max: u64,
    output_max: u64,
) -> Result<(), &'static str> {
    use std::io::Write;
    if request_max > REGEX_MAX_REQUEST_BYTES_V1 || output_max > REGEX_MAX_OUTPUT_BYTES_V1 {
        return Err("regex protocol caps");
    }
    let probes: Vec<OwnedProbe> = serde_cbor::from_slice(bytes).map_err(|_| "regex request")?;
    let values = evaluate_regex_probes(&probes, request_max, output_max)?;
    let mut out = std::io::stdout().lock();
    out.write_all(&[0]).map_err(|_| "regex output")?;
    out.write_all(&(values.len() as u64).to_be_bytes())
        .map_err(|_| "regex output")?;
    out.write_all(&values).map_err(|_| "regex output")?;
    out.flush().map_err(|_| "regex output")
}
#[cfg(all(
    feature = "wasmtime",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub(super) fn evaluate_regex_probes(
    probes: &[OwnedProbe],
    request_max: u64,
    output_max: u64,
) -> Result<Vec<u8>, &'static str> {
    if request_max > REGEX_MAX_REQUEST_BYTES_V1 || output_max > REGEX_MAX_OUTPUT_BYTES_V1 {
        return Err("regex protocol caps");
    }
    validate(
        probes
            .iter()
            .map(|p| (p.pattern.as_str(), p.text.as_deref())),
        probes.len(),
        request_max,
        || false,
    )
    .map_err(|_| "regex bounds")?;
    if output_max < (probes.len() as u64 + 1).max(5) {
        return Err("regex output bound");
    }
    let mut values = Vec::with_capacity(probes.len() + 1);
    values.push(0);
    for (index, probe) in probes.iter().enumerate() {
        let compiled = match regex::Regex::new(&probe.pattern) {
            Ok(r) => r,
            Err(_) => {
                values.clear();
                values.push(1);
                values.extend_from_slice(&(index as u32).to_be_bytes());
                break;
            }
        };
        values.push(u8::from(
            probe
                .text
                .as_ref()
                .is_none_or(|text| compiled.is_match(text)),
        ));
    }
    if values.len() as u64 > output_max {
        return Err("regex output bound");
    }
    Ok(values)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_response_count_and_tags() {
        for bytes in [
            &[][..],
            &[0][..],
            &[0, 2][..],
            &[0, 1, 0][..],
            &[1, 0, 0, 0, 1][..],
            &[2, 0][..],
        ] {
            let mut values = Vec::with_capacity(1);
            assert!(decode_results(bytes, 1, &mut values).is_err());
            assert!(values.is_empty());
        }
        let mut values = Vec::with_capacity(1);
        assert_eq!(decode_results(&[0, 1], 1, &mut values).unwrap(), None);
        assert_eq!(values, [true]);
        let mut values = Vec::with_capacity(1);
        assert_eq!(
            decode_results(&[1, 0, 0, 0, 0], 1, &mut values).unwrap(),
            Some(0)
        );
        assert!(values.is_empty());
    }
    #[test]
    fn closed_request_limits() {
        assert!(validate([("a", Some("b"))], 1, 1024, || false).is_ok());
        assert!(validate([("a", Some("b"))], 1, u64::MAX, || false).is_ok());
        let pattern = "a".repeat(8192);
        let text = "a".repeat(8192);
        assert!(matches!(
            validate(
                [(pattern.as_str(), Some(text.as_str()))],
                1,
                REGEX_MAX_REQUEST_BYTES_V1,
                || false
            ),
            Err(RegexWorkerErrorV1::Limit)
        ));
        assert!(matches!(
            validate([("a", None)], 1, 1024, || true),
            Err(RegexWorkerErrorV1::Cancelled)
        ));
        assert!(matches!(
            validate([], REGEX_MAX_PROBES_V1 + 1, 1024, || false),
            Err(RegexWorkerErrorV1::Limit)
        ));
        assert!(matches!(
            validate([("a", None)], 1, 1, || false),
            Err(RegexWorkerErrorV1::Limit)
        ));
    }
}
