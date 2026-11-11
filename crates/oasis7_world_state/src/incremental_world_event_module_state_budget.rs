//! Allocation-free admission for one EXACT ModuleStateUpdated envelope.
//! Not a decoder/proof: the existing typed decoder and canonical byte compare
//! remain mandatory. Call only after the shared 77-source inventory check and
//! structural preflight. Every unrecognized shape uses the old safe bound.
use crate::{CausedBy, WorldEvent, WorldEventBody};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
use oasis7_wasm_abi::ModuleStateUpdate;
struct Probe<'a, 'w, C> {
    raw: &'a [u8],
    at: usize,
    text: usize,
    state: usize,
    work: &'w mut CborPreflightWorkV1,
    cancelled: &'w mut C,
}
#[derive(Debug)]
enum Stop {
    Shape,
    Work,
    Cancelled,
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
        let out = &self.raw[self.at..end];
        self.at = end;
        Ok(out)
    }
    fn head(&mut self) -> Result<(u8, u64), Stop> {
        self.work.values_remaining = self
            .work
            .values_remaining
            .checked_sub(1)
            .ok_or(Stop::Work)?;
        let b = self.take(1)?[0];
        let ai = b & 31;
        let n = match ai {
            0..=23 => ai as u64,
            24 => self.take(1)?[0] as u64,
            25 => u16::from_be_bytes(self.take(2)?.try_into().map_err(|_| Stop::Shape)?) as u64,
            26 => u32::from_be_bytes(self.take(4)?.try_into().map_err(|_| Stop::Shape)?) as u64,
            27 => u64::from_be_bytes(self.take(8)?.try_into().map_err(|_| Stop::Shape)?),
            _ => return Err(Stop::Shape),
        };
        Ok((b >> 5, n))
    }
    fn unsigned(&mut self) -> Result<u64, Stop> {
        let (m, n) = self.head()?;
        if m != 0 {
            return Err(Stop::Shape);
        }
        Ok(n)
    }
    fn text(&mut self) -> Result<&'a str, Stop> {
        let (m, n) = self.head()?;
        if m != 3 {
            return Err(Stop::Shape);
        }
        let n = usize::try_from(n).map_err(|_| Stop::Shape)?;
        self.text = self.text.checked_add(n).ok_or(Stop::Work)?;
        std::str::from_utf8(self.take(n)?).map_err(|_| Stop::Shape)
    }
    fn map(&mut self, n: u64) -> Result<(), Stop> {
        if self.head()? != (5, n) {
            return Err(Stop::Shape);
        }
        Ok(())
    }
    fn caused(&mut self) -> Result<(), Stop> {
        if self.raw.get(self.at) == Some(&0xf6) {
            self.head()?;
            return Ok(());
        }
        self.map(2)?;
        let mut kind = None;
        let mut data = None;
        for _ in 0..2 {
            match self.text()? {
                "type" if kind.is_none() => kind = Some(self.text()?),
                "data" if data.is_none() => {
                    data = Some(match self.raw.get(self.at).map(|b| b >> 5) {
                        Some(0) => {
                            self.unsigned()?;
                            0
                        }
                        Some(5) => {
                            self.map(1)?;
                            if self.text()? != "intent_id" {
                                return Err(Stop::Shape);
                            }
                            self.text()?;
                            1
                        }
                        _ => return Err(Stop::Shape),
                    });
                }
                _ => return Err(Stop::Shape),
            }
        }
        match (kind, data) {
            (Some("Action"), Some(0)) | (Some("Effect"), Some(1)) => Ok(()),
            _ => Err(Stop::Shape),
        }
    }
    fn payload(&mut self) -> Result<(), Stop> {
        self.map(3)?;
        let mut mask = 0;
        for _ in 0..3 {
            match self.text()? {
                "module_id" if mask & 1 == 0 => {
                    mask |= 1;
                    self.text()?;
                }
                "trace_id" if mask & 2 == 0 => {
                    mask |= 2;
                    self.text()?;
                }
                "state" if mask & 4 == 0 => {
                    mask |= 4;
                    let (m, n) = self.head()?;
                    if m != 4 {
                        return Err(Stop::Shape);
                    }
                    let n = usize::try_from(n).map_err(|_| Stop::Shape)?;
                    // At least one encoded byte/value must actually remain. No
                    // declared array hint reaches serde before this bounded loop.
                    if n > self.raw.len().saturating_sub(self.at) {
                        return Err(Stop::Shape);
                    }
                    self.state = n;
                    for _ in 0..n {
                        if self.unsigned()? > 255 {
                            return Err(Stop::Shape);
                        }
                    }
                }
                _ => return Err(Stop::Shape),
            }
        }
        if mask != 7 {
            return Err(Stop::Shape);
        }
        Ok(())
    }
    fn body(&mut self) -> Result<(), Stop> {
        self.map(2)?;
        // Canonical CBOR orders the short kind key before payload. Serde
        // 1.0.229 de/enum_adjacently.rs tag-first path next_value_seed decodes
        // payload directly. Content-before-tag uses the conservative fallback:
        // never subtract byte tokens from its actual buffered Content budget.
        if self.text()? != "kind"
            || self.text()? != "ModuleStateUpdated"
            || self.text()? != "payload"
        {
            return Err(Stop::Shape);
        }
        self.payload()
    }
    fn world(&mut self) -> Result<(), Stop> {
        self.map(4)?;
        let mut mask = 0;
        for _ in 0..4 {
            match self.text()? {
                "id" if mask & 1 == 0 => {
                    mask |= 1;
                    self.unsigned()?;
                }
                "time" if mask & 2 == 0 => {
                    mask |= 2;
                    self.unsigned()?;
                }
                "caused_by" if mask & 4 == 0 => {
                    mask |= 4;
                    self.caused()?;
                }
                "body" if mask & 8 == 0 => {
                    mask |= 8;
                    self.body()?;
                }
                _ => return Err(Stop::Shape),
            }
        }
        if mask != 15 || self.at != self.raw.len() {
            return Err(Stop::Shape);
        }
        Ok(())
    }
}
fn bound_error() -> ValueTreeError {
    ValueTreeError::ResourceLimited("ModuleStateUpdated schema heap bound")
}
/// None is ALWAYS the original safe conservative fallback, not permission to
/// decode an arbitrary sequence as byte storage. Work spent probing stays spent.
pub(super) fn module_state_world_event_heap_bound_v1<C: FnMut() -> bool>(
    raw: &[u8],
    work: &mut CborPreflightWorkV1,
    cancelled: &mut C,
) -> Result<Option<usize>, ValueTreeError> {
    let mut p = Probe {
        raw,
        at: 0,
        text: 0,
        state: 0,
        work,
        cancelled,
    };
    match p.world() {
        Err(Stop::Shape) => return Ok(None),
        Err(Stop::Work) => {
            return Err(ValueTreeError::ResourceLimited(
                "ModuleStateUpdated schema work",
            ));
        }
        Err(Stop::Cancelled) => return Err(ValueTreeError::Cancelled),
        Ok(()) => {}
    }
    // Four-times byte storage covers Vec<u8> growth even if serde's cautious
    // preallocation changes. The only adjacent payload containing state is
    // proven tag-first; there is NO Content/Box<Action> per state byte.
    // All other nodes are fixed in count, with <=16x observed metadata text
    // covering temporary Content copies, diagnostics and String capacities.
    // The common canonical comparator independently admits its own buffers.
    let bytes = p
        .state
        .checked_mul(4)
        .and_then(|n| p.text.checked_mul(16).and_then(|t| n.checked_add(t)))
        .and_then(|n| n.checked_add(std::mem::size_of::<WorldEvent>()))
        .and_then(|n| n.checked_add(65536))
        .ok_or_else(bound_error)?;
    Ok(Some(bytes))
}
// Explicit type guards supplement the shared source/default/serde inventory.
#[allow(dead_code)]
fn layout_guard(event: &WorldEvent, value: &ModuleStateUpdate) {
    let WorldEvent {
        id,
        time,
        caused_by,
        body,
    } = event;
    let _: &u64 = id;
    let _: &u64 = time;
    let _: &Option<CausedBy> = caused_by;
    let _: &WorldEventBody = body;
    let ModuleStateUpdate {
        module_id,
        trace_id,
        state,
    } = value;
    let _: &String = module_id;
    let _: &String = trace_id;
    let _: &Vec<u8> = state;
    match caused_by {
        None => {}
        Some(CausedBy::Action(id)) => {
            let _: &u64 = id;
        }
        Some(CausedBy::Effect { intent_id }) => {
            let _: &String = intent_id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn work() -> CborPreflightWorkV1 {
        CborPreflightWorkV1 {
            bytes_remaining: 8 << 20,
            values_remaining: 8 << 20,
            max_depth: 32,
        }
    }
    fn event() -> WorldEvent {
        WorldEvent {
            id: 7,
            time: 11,
            caused_by: Some(CausedBy::Effect {
                intent_id: "actual-effect".into(),
            }),
            body: WorldEventBody::ModuleStateUpdated(ModuleStateUpdate {
                module_id: "module".into(),
                trace_id: "trace".into(),
                state: vec![0, 1, 255],
            }),
        }
    }
    #[test]
    fn module_state_budget_requires_entire_real_closed_envelope_and_actual_u8s() {
        let value = event();
        let raw = oasis7_wasm_abi::encode_canonical_cbor(&value).unwrap();
        assert!(
            module_state_world_event_heap_bound_v1(&raw, &mut work(), &mut || false)
                .unwrap()
                .unwrap()
                < 1 << 20
        );
        let mut modified = serde_json::to_value(&value).unwrap();
        modified["body"]["payload"]["state"] = serde_json::json!([0, 256]);
        let raw = oasis7_wasm_abi::encode_canonical_cbor(&modified).unwrap();
        assert!(
            module_state_world_event_heap_bound_v1(&raw, &mut work(), &mut || false)
                .unwrap()
                .is_none()
        );
        let mut modified = serde_json::to_value(&value).unwrap();
        modified["body"]["payload"]["unknown"] = serde_json::json!([0, 1, 2]);
        let raw = oasis7_wasm_abi::encode_canonical_cbor(&modified).unwrap();
        assert!(
            module_state_world_event_heap_bound_v1(&raw, &mut work(), &mut || false)
                .unwrap()
                .is_none()
        );
        let raw = oasis7_wasm_abi::encode_canonical_cbor(&value).unwrap();
        let mut exhausted = work();
        exhausted.values_remaining = 0;
        assert!(matches!(
            module_state_world_event_heap_bound_v1(&raw, &mut exhausted, &mut || false),
            Err(ValueTreeError::ResourceLimited(_))
        ));
        assert!(matches!(
            module_state_world_event_heap_bound_v1(&raw, &mut work(), &mut || true),
            Err(ValueTreeError::Cancelled)
        ));
    }
    #[test]
    fn module_state_budget_never_discounts_content_before_tag_buffering() {
        struct Body<'a>(&'a ModuleStateUpdate);
        impl serde::Serialize for Body<'_> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeMap;
                let mut map = s.serialize_map(Some(2))?;
                map.serialize_entry("payload", self.0)?;
                map.serialize_entry("kind", "ModuleStateUpdated")?;
                map.end()
            }
        }
        #[derive(serde::Serialize)]
        struct Event<'a> {
            id: u64,
            time: u64,
            caused_by: Option<CausedBy>,
            body: Body<'a>,
        }
        let state = ModuleStateUpdate {
            module_id: "module".into(),
            trace_id: "trace".into(),
            state: vec![0, 1, 255],
        };
        let raw = serde_cbor::to_vec(&Event {
            id: 1,
            time: 2,
            caused_by: None,
            body: Body(&state),
        })
        .unwrap();
        assert!(
            module_state_world_event_heap_bound_v1(&raw, &mut work(), &mut || false)
                .unwrap()
                .is_none()
        );
    }
}
