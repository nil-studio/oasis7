//! Narrow byte-sequence accounting inside the real ModuleOutput struct.
//! Shared CBOR preflight and the closed 77-source inventory remain mandatory.
//! Only the exact root `new_state` Option<Vec<u8>> path is discounted; every
//! other value retains its original conservative shape/depth charge.
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::cbor_decode_preflight::{CborPreflightWorkV1, CborShapeV1};
use oasis7_wasm_abi::{ModuleEffectIntent, ModuleEmit, ModuleOutput, ModuleTickLifecycleDirective};
#[derive(Debug)]
enum Stop {
    Shape,
    Work,
    Cancelled,
}
struct Probe<'a, 'w, C> {
    raw: &'a [u8],
    at: usize,
    work: &'w mut CborPreflightWorkV1,
    cancelled: &'w mut C,
}
impl<'a, C: FnMut() -> bool> Probe<'a, '_, C> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Stop> {
        if (self.cancelled)() {
            return Err(Stop::Cancelled);
        }
        self.work.bytes_remaining = self
            .work
            .bytes_remaining
            .checked_sub(n as u64)
            .ok_or(Stop::Work)?;
        let end = self
            .at
            .checked_add(n)
            .filter(|n| *n <= self.raw.len())
            .ok_or(Stop::Shape)?;
        let b = &self.raw[self.at..end];
        self.at = end;
        Ok(b)
    }
    fn head(&mut self) -> Result<(u8, u64), Stop> {
        self.work.values_remaining = self
            .work
            .values_remaining
            .checked_sub(1)
            .ok_or(Stop::Work)?;
        let b = self.take(1)?[0];
        let n = match b & 31 {
            0..=23 => (b & 31) as u64,
            24 => self.take(1)?[0] as u64,
            25 => u16::from_be_bytes(self.take(2)?.try_into().map_err(|_| Stop::Shape)?) as u64,
            26 => u32::from_be_bytes(self.take(4)?.try_into().map_err(|_| Stop::Shape)?) as u64,
            27 => u64::from_be_bytes(self.take(8)?.try_into().map_err(|_| Stop::Shape)?),
            _ => return Err(Stop::Shape),
        };
        Ok((b >> 5, n))
    }
    fn text(&mut self) -> Result<&'a str, Stop> {
        let (m, n) = self.head()?;
        if m != 3 {
            return Err(Stop::Shape);
        }
        let n = usize::try_from(n).map_err(|_| Stop::Shape)?;
        std::str::from_utf8(self.take(n)?).map_err(|_| Stop::Shape)
    }
    fn skip(&mut self, depth: usize) -> Result<(), Stop> {
        if depth > self.work.max_depth.min(64) {
            return Err(Stop::Work);
        }
        let (m, n) = self.head()?;
        match m {
            0 | 1 | 7 => Ok(()),
            2 | 3 => {
                let n = usize::try_from(n).map_err(|_| Stop::Shape)?;
                self.take(n)?;
                Ok(())
            }
            4 | 5 => {
                let count = if m == 5 {
                    n.checked_mul(2).ok_or(Stop::Shape)?
                } else {
                    n
                };
                if count > self.raw.len().saturating_sub(self.at) as u64 {
                    return Err(Stop::Shape);
                }
                for _ in 0..count {
                    self.skip(depth + 1)?;
                }
                Ok(())
            }
            _ => Err(Stop::Shape),
        }
    }
    fn state(&mut self) -> Result<u64, Stop> {
        let (m, n) = self.head()?;
        if (m, n) == (7, 22) {
            return Ok(0);
        }
        if m != 4 || n > self.raw.len().saturating_sub(self.at) as u64 {
            return Err(Stop::Shape);
        }
        for _ in 0..n {
            let (m, b) = self.head()?;
            if m != 0 || b > 255 {
                return Err(Stop::Shape);
            }
        }
        Ok(n)
    }
    fn root(&mut self) -> Result<u64, Stop> {
        let (m, n) = self.head()?;
        if m != 5 || n > self.raw.len().saturating_sub(self.at) as u64 {
            return Err(Stop::Shape);
        }
        let mut state = None;
        for _ in 0..n {
            let key = self.text()?;
            if key == "new_state" {
                if state.is_some() {
                    return Err(Stop::Shape);
                }
                state = Some(self.state()?);
            } else {
                self.skip(1)?;
            }
        }
        if self.at != self.raw.len() {
            return Err(Stop::Shape);
        }
        state.ok_or(Stop::Shape)
    }
}
pub(super) struct ModuleOutputStateBudgetV1 {
    pub(super) remainder: CborShapeV1,
    pub(super) owned_bytes: usize,
}
/// The serde 1.0.229 regular-struct visitor calls next_value<Option<Vec<u8>>>
/// directly. There is no internally/adjacently tagged parent or Content replay
/// around this root field. Its byte elements therefore need Vec<u8> capacity,
/// not EffectIntent/Content/JSON-node width. Pinning the actual ABI source and
/// Serde lock before typed decode prevents an attribute/layout drift bypass.
pub(super) fn module_output_state_budget_v1<C: FnMut() -> bool>(
    raw: &[u8],
    shape: CborShapeV1,
    work: &mut CborPreflightWorkV1,
    cancelled: &mut C,
) -> Result<Option<ModuleOutputStateBudgetV1>, ValueTreeError> {
    let mut p = Probe {
        raw,
        at: 0,
        work,
        cancelled,
    };
    let count = match p.root() {
        Ok(n) => n,
        Err(Stop::Shape) => return Ok(None),
        Err(Stop::Work) => {
            return Err(ValueTreeError::ResourceLimited("module output schema work"));
        }
        Err(Stop::Cancelled) => return Err(ValueTreeError::Cancelled),
    };
    let mut remainder = shape;
    remainder.sequence_entries =
        remainder
            .sequence_entries
            .checked_sub(count)
            .ok_or(ValueTreeError::Invalid(
                "module output byte sequence accounting",
            ))?;
    remainder.values = remainder
        .values
        .checked_sub(count)
        .ok_or(ValueTreeError::Invalid(
            "module output byte token accounting",
        ))?;
    let owned_bytes = usize::try_from(count)
        .ok()
        .and_then(|n| n.checked_mul(4))
        .ok_or(ValueTreeError::ResourceLimited("module output byte heap"))?;
    Ok(Some(ModuleOutputStateBudgetV1 {
        remainder,
        owned_bytes,
    }))
}
#[allow(dead_code)]
fn layout_guard(output: &ModuleOutput) {
    let ModuleOutput {
        new_state,
        effects,
        emits,
        tick_lifecycle,
        output_bytes,
    } = output;
    let _: &Option<Vec<u8>> = new_state;
    let _: &Vec<ModuleEffectIntent> = effects;
    let _: &Vec<ModuleEmit> = emits;
    let _: &Option<ModuleTickLifecycleDirective> = tick_lifecycle;
    let _: &u64 = output_bytes;
}
#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_proto::cbor_decode_preflight::preflight_definite_cbor_v1;
    fn work() -> CborPreflightWorkV1 {
        CborPreflightWorkV1 {
            bytes_remaining: 8 << 20,
            values_remaining: 8 << 20,
            max_depth: 64,
        }
    }
    fn probe(raw: &[u8]) -> Option<ModuleOutputStateBudgetV1> {
        let shape = preflight_definite_cbor_v1(raw, &mut work(), &mut || false).unwrap();
        module_output_state_budget_v1(raw, shape, &mut work(), &mut || false).unwrap()
    }
    #[test]
    fn output_byte_discount_is_exact_root_path_and_keeps_unknown_arrays_generic() {
        let output = ModuleOutput {
            new_state: Some(vec![0, 1, 255]),
            effects: vec![],
            emits: vec![],
            tick_lifecycle: None,
            output_bytes: 0,
        };
        let mut value = serde_json::to_value(&output).unwrap();
        value["unknown"] = serde_json::json!([0, 1, 2, 3]);
        let raw = serde_cbor::to_vec(&value).unwrap();
        let result = probe(&raw).unwrap();
        assert_eq!(result.owned_bytes, 12);
        assert!(result.remainder.sequence_entries >= 4);
        value["new_state"] = serde_json::json!([0, 256]);
        let raw = serde_cbor::to_vec(&value).unwrap();
        assert!(probe(&raw).is_none());
        let raw = serde_cbor::to_vec(
            &serde_json::json!({"effects":[],"emits":[],"output_bytes":0,"unknown":[0,1,2]}),
        )
        .unwrap();
        assert!(probe(&raw).is_none());
    }
    #[test]
    fn output_byte_discount_never_refills_failed_work_or_cancellation() {
        let raw = serde_cbor::to_vec(&serde_json::json!({"new_state":[0,1,2]})).unwrap();
        let shape = preflight_definite_cbor_v1(&raw, &mut work(), &mut || false).unwrap();
        let mut empty = work();
        empty.values_remaining = 0;
        assert!(matches!(
            module_output_state_budget_v1(&raw, shape, &mut empty, &mut || false),
            Err(ValueTreeError::ResourceLimited(_))
        ));
        assert!(matches!(
            module_output_state_budget_v1(&raw, shape, &mut work(), &mut || true),
            Err(ValueTreeError::Cancelled)
        ));
    }
}
