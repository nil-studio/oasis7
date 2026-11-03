//! Shared exact payload equality for qualified point updates.
use super::*;
pub(super) fn equal(a: Option<MemberPayloadRefV1<'_>>, b: Option<MemberPayloadRefV1<'_>>) -> bool {
    use MemberPayloadRefV1 as P;
    match (a, b) {
        (None, None) => true,
        (Some(P::Unit), Some(P::Unit)) => true,
        (Some(P::Text(a)), Some(P::Text(b))) => a == b,
        (Some(P::U64(a)), Some(P::U64(b))) => a == b,
        (Some(P::I64(a)), Some(P::I64(b))) => a == b,
        (Some(P::Bool(a)), Some(P::Bool(b))) => a == b,
        _ => false,
    }
}
