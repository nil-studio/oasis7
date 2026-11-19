//! Owned allocation shapes of the actual closed consensus payload types.
//! The field projections are type-only and never create a DTO or authority.
//! The existing Runtime all-action graph remains the sole WS Codec inventory.
use super::*;
use oasis7_runtime::{PendingActionOwnedLayoutV1 as Layout, pending_action_owned_decode_layout_v1};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

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
runtime_layout!(
    runtime::Action,
    runtime::GovernanceAuthorizationV1,
    runtime::GameplaySubmissionOrigin
);
record_layout!(ConsensusActionPayloadEnvelope,{version,gameplay_submission_origin,auth,body});
enum_layout!(ConsensusActionAuthEnvelope,units{},tuples{MainTokenAction(proof),GovernanceAction(proof)},records{});
enum_layout!(ConsensusActionPayloadBody,units{},tuples{},records{
    WorldCleanupV2{request},WorldServiceIntent{request},RuntimeAction{action},SimulatorAction{action,submitter}
});
impl<T: OwnedLayout> OwnedLayout for oasis7_client_api::world_service::SignedReadRequest<T> {
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
impl<T: OwnedLayout> OwnedLayout for oasis7_client_api::world_service::SubmitIntentRequest<T> {
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
#[path = "consensus_action_payload_layout_service.rs"]
mod service;
#[path = "consensus_action_payload_layout_simulator.rs"]
mod simulator;

/// This is an owned-layout inventory, not a decoder, grammar or grant. Every
/// typed child must be explicitly covered or this module does not compile.
pub fn consensus_payload_owned_layout_v1<LE>(
    visit: &mut dyn FnMut() -> Result<(), LE>,
) -> Result<Layout, LE> {
    let runtime = pending_action_owned_decode_layout_v1(visit)?;
    let mut out = ConsensusActionPayloadEnvelope::layout(runtime, visit)?;
    out.merge(serde_cbor::Value::layout(runtime, visit)?);
    // Closed nonempty serde defaults in this extra input graph: LeaseQuote
    // (six literal Strings, 96B), ObservationEnvelope (two schema Strings,
    // 28B), and ModuleVisualEntity ("artifact", 8B). Reserve their sum per
    // structural value rather than multiplying the entire symbol inventory.
    out.default_heap_per_value = out.default_heap_per_value.max(96 + 28 + 8);
    for symbol in [
        "type",
        "data",
        "kind",
        "payload",
        "operation",
        "value",
        "tag",
        "artifact",
        "cognition_units.v1",
        "provider_cognition",
        "agent_turn",
        "fixed_unit_experimental",
        "legacy-unbound",
        "legacy-unbound",
        "oc_dual_obs_v1",
        "oc_dual_act_v1",
    ] {
        out.symbol(symbol);
    }
    Ok(out)
}
