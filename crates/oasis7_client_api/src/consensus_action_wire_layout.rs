//! Pure owned-layout inventory for shared payload headers and legacy services.
//! No decoder, allocator, executor or source qualification is created here.
use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
#[derive(Clone, Copy, Debug)]
pub struct ConsensusPayloadOwnedLayoutV1 {
    pub root: usize,
    pub sequence_element: usize,
    pub map_pair: usize,
    pub set_element: usize,
    pub box_pointee: usize,
    pub default_heap_per_value: usize,
    pub symbols_bytes: usize,
    pub visits: usize,
    pub missing: Option<&'static str>,
}
impl ConsensusPayloadOwnedLayoutV1 {
    pub fn scalar<T>() -> Self {
        Self {
            root: std::mem::size_of::<T>(),
            sequence_element: 0,
            map_pair: 0,
            set_element: 0,
            box_pointee: 0,
            default_heap_per_value: 0,
            symbols_bytes: 0,
            visits: 1,
            missing: None,
        }
    }
    pub fn merge(&mut self, other: Self) {
        self.sequence_element = self.sequence_element.max(other.sequence_element);
        self.map_pair = self.map_pair.max(other.map_pair);
        self.set_element = self.set_element.max(other.set_element);
        self.box_pointee = self.box_pointee.max(other.box_pointee);
        self.default_heap_per_value = self
            .default_heap_per_value
            .max(other.default_heap_per_value);
        self.symbols_bytes = self.symbols_bytes.saturating_add(other.symbols_bytes);
        self.visits = self.visits.saturating_add(other.visits);
        self.missing = self.missing.or(other.missing)
    }
    pub fn symbol(&mut self, value: &str) {
        self.symbols_bytes = self.symbols_bytes.saturating_add(value.len() + 8)
    }
}
type Layout = ConsensusPayloadOwnedLayoutV1;
trait OwnedLayout {
    fn layout<LE>(runtime: Layout, visit: &mut dyn FnMut() -> Result<(), LE>)
    -> Result<Layout, LE>;
}
fn projected<O, T: OwnedLayout, LE>(
    _: for<'a> fn(&'a O) -> &'a T,
    runtime: Layout,
    visit: &mut dyn FnMut() -> Result<(), LE>,
) -> Result<Layout, LE> {
    T::layout(runtime, visit)
}
fn variant_projected<O, T: OwnedLayout, LE>(
    _: for<'a> fn(&'a O) -> Option<&'a T>,
    runtime: Layout,
    visit: &mut dyn FnMut() -> Result<(), LE>,
) -> Result<Layout, LE> {
    T::layout(runtime, visit)
}
macro_rules! primitive_layout {
    ($($ty:ty),+ $(,)?) => {$(impl OwnedLayout for $ty {
        fn layout<LE>(_:Layout, visit:&mut dyn FnMut()->Result<(),LE>)->Result<Layout,LE>{visit()?;Ok(Layout::scalar::<Self>())}
    })+};
}
macro_rules! record_layout {
    ($ty:ty,{$($field:tt),+$(,)?})=>{impl OwnedLayout for $ty {
        fn layout<LE>(runtime:Layout,visit:&mut dyn FnMut()->Result<(),LE>)->Result<Layout,LE>{
            // Compiler-enforced exhaustive fields: a new owned wire field must
            // add its real typed child rather than silently inheriting a bound.
            let _:fn(&Self)=|owner|{let Self{$($field: _),+}=owner;};
            visit()?;let mut out=Layout::scalar::<Self>();
            $(out.symbol(stringify!($field));out.symbol(stringify!($field));out.merge(projected::<Self,_,LE>(|owner|&owner.$field,runtime,visit)?);)+
            Ok(out)
        }
    }};
}
macro_rules! enum_layout {
    ($ty:ty,units{$($unit:ident),*$(,)?},tuples{$($tuple:ident($value:ident)),*$(,)?},records{$($record:ident{$($field:ident),+}),*$(,)?})=>{impl OwnedLayout for $ty {
        fn layout<LE>(runtime:Layout,visit:&mut dyn FnMut()->Result<(),LE>)->Result<Layout,LE>{
            // Unit-only enums have no child that consumes the runtime layout.
            let _ = runtime;
            let _:fn(&Self)=|owner|match owner{$(Self::$unit=>{},)*$(Self::$tuple(_)=>{},)*$(Self::$record{$($field:_),+}=>{},)*};
            visit()?;let mut out=Layout::scalar::<Self>();
            $(out.symbol(stringify!($unit));out.symbol(stringify!($unit));)*
            $(out.symbol(stringify!($tuple));out.symbol(stringify!($tuple));out.merge(variant_projected::<Self,_,LE>(|owner|match owner{Self::$tuple($value)=>Some($value),_=>None},runtime,visit)?);)*
            $(out.symbol(stringify!($record));out.symbol(stringify!($record));$(out.symbol(stringify!($field));out.symbol(stringify!($field));out.merge(variant_projected::<Self,_,LE>(|owner|match owner{Self::$record{$field,..}=>Some($field),_=>None},runtime,visit)?);)+)*
            Ok(out)
        }
    }};
}
primitive_layout!(
    bool,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
    char,
    ()
);
impl OwnedLayout for String {
    fn layout<LE>(_: Layout, visit: &mut dyn FnMut() -> Result<(), LE>) -> Result<Layout, LE> {
        visit()?;
        Ok(Layout::scalar::<Self>())
    }
}
impl<T: OwnedLayout> OwnedLayout for Option<T> {
    fn layout<LE>(
        runtime: Layout,
        visit: &mut dyn FnMut() -> Result<(), LE>,
    ) -> Result<Layout, LE> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.merge(T::layout(runtime, visit)?);
        Ok(out)
    }
}
impl<T: OwnedLayout> OwnedLayout for Box<T> {
    fn layout<LE>(
        runtime: Layout,
        visit: &mut dyn FnMut() -> Result<(), LE>,
    ) -> Result<Layout, LE> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.box_pointee = std::mem::size_of::<T>();
        out.merge(T::layout(runtime, visit)?);
        Ok(out)
    }
}
macro_rules! sequence_layout {($($seq:ident),+) => {$(impl<T:OwnedLayout> OwnedLayout for $seq<T>{
    fn layout<LE>(runtime:Layout,visit:&mut dyn FnMut()->Result<(),LE>)->Result<Layout,LE>{visit()?;let mut out=Layout::scalar::<Self>();out.sequence_element=std::mem::size_of::<T>();out.merge(T::layout(runtime,visit)?);Ok(out)}
})+};}
sequence_layout!(Vec, VecDeque);
impl<T: OwnedLayout, const N: usize> OwnedLayout for [T; N] {
    fn layout<LE>(
        runtime: Layout,
        visit: &mut dyn FnMut() -> Result<(), LE>,
    ) -> Result<Layout, LE> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.merge(T::layout(runtime, visit)?);
        Ok(out)
    }
}
macro_rules! map_layout {($($map:ident),+) => {$(impl<K:OwnedLayout,V:OwnedLayout> OwnedLayout for $map<K,V>{
    fn layout<LE>(runtime:Layout,visit:&mut dyn FnMut()->Result<(),LE>)->Result<Layout,LE>{visit()?;let mut out=Layout::scalar::<Self>();out.map_pair=std::mem::size_of::<(K,V)>();out.merge(K::layout(runtime,visit)?);out.merge(V::layout(runtime,visit)?);Ok(out)}
})+};}
map_layout!(BTreeMap, HashMap);
macro_rules! set_layout {($($set:ident),+) => {$(impl<K:OwnedLayout> OwnedLayout for $set<K>{
    fn layout<LE>(runtime:Layout,visit:&mut dyn FnMut()->Result<(),LE>)->Result<Layout,LE>{visit()?;let mut out=Layout::scalar::<Self>();out.set_element=std::mem::size_of::<K>();out.merge(K::layout(runtime,visit)?);Ok(out)}
})+};}
set_layout!(BTreeSet, HashSet);
impl<A: OwnedLayout, B: OwnedLayout> OwnedLayout for (A, B) {
    fn layout<LE>(
        runtime: Layout,
        visit: &mut dyn FnMut() -> Result<(), LE>,
    ) -> Result<Layout, LE> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.merge(A::layout(runtime, visit)?);
        out.merge(B::layout(runtime, visit)?);
        Ok(out)
    }
}
impl<A: OwnedLayout, B: OwnedLayout, C: OwnedLayout> OwnedLayout for (A, B, C) {
    fn layout<LE>(
        runtime: Layout,
        visit: &mut dyn FnMut() -> Result<(), LE>,
    ) -> Result<Layout, LE> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.merge(A::layout(runtime, visit)?);
        out.merge(B::layout(runtime, visit)?);
        out.merge(C::layout(runtime, visit)?);
        Ok(out)
    }
}
impl OwnedLayout for serde_json::Value {
    fn layout<LE>(_: Layout, visit: &mut dyn FnMut() -> Result<(), LE>) -> Result<Layout, LE> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.sequence_element = std::mem::size_of::<Self>();
        out.map_pair = std::mem::size_of::<(String, Self)>();
        Ok(out)
    }
}
impl OwnedLayout for serde_cbor::Value {
    fn layout<LE>(_: Layout, visit: &mut dyn FnMut() -> Result<(), LE>) -> Result<Layout, LE> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.sequence_element = std::mem::size_of::<Self>();
        out.map_pair = std::mem::size_of::<(Self, Self)>();
        out.box_pointee = std::mem::size_of::<Self>();
        Ok(out)
    }
}
macro_rules! runtime_layout {($($ty:ty),+) => {$(impl OwnedLayout for $ty{
    fn layout<LE>(runtime:Layout,visit:&mut dyn FnMut()->Result<(),LE>)->Result<Layout,LE>{visit()?;let mut out=Layout::scalar::<Self>();out.merge(runtime);Ok(out)}
})+};}

runtime_layout!(Action, GovernanceAuthorizationV1, GameplaySubmissionOrigin);
primitive_layout!(IgnoredSidePayloadV1);
impl<B: OwnedLayout> OwnedLayout for ConsensusActionPayloadEnvelope<B> {
    fn layout<LE>(
        runtime: Layout,
        visit: &mut dyn FnMut() -> Result<(), LE>,
    ) -> Result<Layout, LE> {
        let _: fn(&Self) = |owner| {
            let Self {
                version: _,
                gameplay_submission_origin: _,
                auth: _,
                body: _,
            } = owner;
        };
        visit()?;
        let mut out = Layout::scalar::<Self>();
        for symbol in ["version", "gameplay_submission_origin", "auth", "body"] {
            out.symbol(symbol);
        }
        out.merge(Option::<GameplaySubmissionOrigin>::layout(runtime, visit)?);
        out.merge(Option::<ConsensusActionAuthEnvelope>::layout(
            runtime, visit,
        )?);
        out.merge(B::layout(runtime, visit)?);
        Ok(out)
    }
}
enum_layout!(ConsensusActionAuthEnvelope,units{},tuples{MainTokenAction(proof),GovernanceAction(proof)},records{});
impl<S: OwnedLayout, U: OwnedLayout + Default, C: OwnedLayout, W: OwnedLayout> OwnedLayout
    for ConsensusActionPayloadBody<S, U, C, W>
{
    fn layout<LE>(
        runtime: Layout,
        visit: &mut dyn FnMut() -> Result<(), LE>,
    ) -> Result<Layout, LE> {
        let _: fn(&Self) = |owner| match owner {
            Self::WorldCleanupV2 { request: _ } | Self::WorldServiceIntent { request: _ } => {}
            Self::RuntimeAction { action: _ } => {}
            Self::SimulatorAction {
                action: _,
                submitter: _,
            } => {}
        };
        visit()?;
        let mut out = Layout::scalar::<Self>();
        for symbol in [
            "world_cleanup_v2",
            "world_service_intent",
            "runtime_action",
            "simulator_action",
            "request",
            "action",
            "submitter",
        ] {
            out.symbol(symbol);
        }
        out.merge(Action::layout(runtime, visit)?);
        out.merge(S::layout(runtime, visit)?);
        out.merge(U::layout(runtime, visit)?);
        out.merge(C::layout(runtime, visit)?);
        out.merge(W::layout(runtime, visit)?);
        Ok(out)
    }
}
impl<T: OwnedLayout> OwnedLayout for crate::world_service::SignedReadRequest<T> {
    fn layout<LE>(
        runtime: Layout,
        visit: &mut dyn FnMut() -> Result<(), LE>,
    ) -> Result<Layout, LE> {
        let _: fn(&Self) = |owner| {
            let Self {
                request: _,
                subject_public_key: _,
                signature_hex: _,
            } = owner;
        };
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.symbol("request");
        out.symbol("subject_public_key");
        out.symbol("signature_hex");
        out.merge(T::layout(runtime, visit)?);
        out.merge(String::layout(runtime, visit)?);
        Ok(out)
    }
}
impl<T: OwnedLayout> OwnedLayout for crate::world_service::SubmitIntentRequest<T> {
    fn layout<LE>(
        runtime: Layout,
        visit: &mut dyn FnMut() -> Result<(), LE>,
    ) -> Result<Layout, LE> {
        let _: fn(&Self) = |owner| {
            let Self {
                contract_version: _,
                correlation: _,
                deadline_unix_ms: _,
                signed_payload: _,
            } = owner;
        };
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.symbol("contract_version");
        out.symbol("correlation");
        out.symbol("deadline_unix_ms");
        out.symbol("signed_payload");
        out.merge(T::layout(runtime, visit)?);
        out.merge(projected::<Self, _, LE>(
            |owner| &owner.correlation,
            runtime,
            visit,
        )?);
        out.merge(Option::<u64>::layout(runtime, visit)?);
        Ok(out)
    }
}

#[path = "consensus_action_wire_layout_service.rs"]
mod service;
/// Layout of the Runtime projection: ignored side payloads allocate no DTO.
/// Every actual Runtime child comes from the original WS all-action inventory.
pub fn runtime_payload_owned_layout_v1<LE>(
    runtime: Layout,
    visit: &mut dyn FnMut() -> Result<(), LE>,
) -> Result<Layout, LE> {
    let mut out = RuntimePayloadProjectionV1::layout(runtime, visit)?;
    out.merge(serde_cbor::Value::layout(runtime, visit)?);
    Ok(out)
}
/// Complete actual legacy service graph, including the original cleanup and
/// main-token auth DTOs. This remains a DATA inventory, never a permission.
pub fn legacy_payload_owned_layout_v1<LE>(
    runtime: Layout,
    visit: &mut dyn FnMut() -> Result<(), LE>,
) -> Result<Layout, LE> {
    let mut out = crate::world_service::SubmitIntentRequest::<
        crate::legacy_world_service::WorldServicePayloadV1,
    >::layout(runtime, visit)?;
    out.merge(crate::world_service::task_cleanup_v2::TaskCleanupSubmitV2::layout(runtime, visit)?);
    out.merge(MainTokenActionAuthProof::layout(runtime, visit)?);
    out.default_heap_per_value = out.default_heap_per_value.max(96 + 28);
    Ok(out)
}

// The retained canonical result uses this exact original concrete Service graph.
// Exhaustive field destructuring makes additions participate in admission.
record_layout!(crate::legacy_world_service::CanonicalIntentResultV1, {
    request, action_id, committed_height, receipt, rejected
});
/// Retained Service result layout, using the caller's actual closed Action inventory.
/// The caller must preserve its runtime inventory's missing marker and visits;
/// this layer cannot replace Runtime-owned children with a scalar placeholder.
/// This only describes allocation; the consumer validates canonical bytes and binding.
pub fn canonical_intent_result_owned_layout_v1<LE>(
    runtime: Layout,
    visit: &mut dyn FnMut() -> Result<(), LE>,
) -> Result<Layout, LE> {
    let mut out = crate::legacy_world_service::CanonicalIntentResultV1::layout(runtime, visit)?;
    out.default_heap_per_value = out.default_heap_per_value.max(96 + 28);
    Ok(out)
}

/// The same concrete capability-context inventory used by legacy Service DTOs.
pub fn capability_invocation_context_owned_layout_v1<LE>(
    visit: &mut dyn FnMut() -> Result<(), LE>,
) -> Result<Layout, LE> {
    oasis7_agent_api::CapabilityInvocationContext::layout(Layout::scalar::<()>(), visit)
}

/// Original signed close-task request graph, including the full retained
/// fence and permitted invocation context. This describes DATA allocation only.
pub fn task_cleanup_request_owned_layout_v1<LE>(
    visit: &mut dyn FnMut() -> Result<(), LE>,
) -> Result<ConsensusPayloadOwnedLayoutV1, LE> {
    let mut out = crate::world_service::SignedReadRequest::<
        oasis7_agent_api::game::GameTaskRevokeAndCloseV2,
    >::layout(Layout::scalar::<()>(), visit)?;
    out.default_heap_per_value = out.default_heap_per_value.max(96 + 28);
    Ok(out)
}

/// Complete original Closed task graph, including both fence variants.
pub fn task_closed_owned_layout_v1<LE>(
    visit: &mut dyn FnMut() -> Result<(), LE>,
) -> Result<ConsensusPayloadOwnedLayoutV1, LE> {
    let mut out = oasis7_agent_api::game::GameTaskClosedV2::layout(Layout::scalar::<()>(), visit)?;
    out.default_heap_per_value = out.default_heap_per_value.max(96 + 28);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_service_result_propagates_actual_action_inventory_and_missing_marker() {
        let mut runtime = Layout::scalar::<Action>();
        runtime.sequence_element = 4096;
        runtime.missing = Some("unclosed runtime action child");
        let result =
            canonical_intent_result_owned_layout_v1(runtime, &mut || Ok::<_, ()>(())).unwrap();
        assert_eq!(result.missing, runtime.missing);
        assert!(result.sequence_element >= runtime.sequence_element);
        assert!(result.visits > runtime.visits);
    }
}
