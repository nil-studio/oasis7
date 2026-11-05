//! Allocation-accounted routing for the strict Runtime pre-action path.
//!
//! The action is serialized into a pre-admitted byte buffer and parsed into a
//! Vec-backed JSON arena. This avoids
//! serde_json::Value's default BTreeMap object storage, whose node allocations
//! cannot be admitted through a public capacity API. The default entry rejects
//! regex; the backend entry delegates one borrowed batch to a data provider.
//! Runtime owns selection and isolation of its production provider.

mod bounded_json;
mod bounded_match;
mod bounded_regex;
pub use bounded_regex::{RegexBatchBackendV1, RegexBatchOutputV1, RegexProbeV1};

use oasis7_wasm_abi::ModuleSubscription;
use serde::Serialize;
use std::fmt;

/// Shared limits consumed by serialization and routing. The Runtime owner must
/// keep one value across all selected actions in the execution.
#[derive(Debug, Clone)]
pub struct RouterWorkV1 {
    pub nodes_remaining: u64,
    pub edges_remaining: u64,
    pub string_bytes_remaining: u64,
    pub match_steps_remaining: u64,
    pub max_depth: usize,
}

/// Failure from strict pre-action JSON construction or matching.
#[derive(Debug)]
pub enum BoundedRouterErrorV1<E, RE = std::convert::Infallible> {
    Regex(RE),
    Cancelled,
    ResourceLimited(&'static str),
    Unsupported(&'static str),
    Invalid(&'static str),
    Serialize,
    /// The caller's Ordinary-memory admission result, kept typed for Runtime.
    Admission(E),
}

impl<E> BoundedRouterErrorV1<E> {
    fn from_internal(error: bounded_json::InternalError) -> Self {
        match error {
            bounded_json::InternalError::Cancelled => Self::Cancelled,
            bounded_json::InternalError::ResourceLimited(reason) => Self::ResourceLimited(reason),
            bounded_json::InternalError::Invalid(reason) => Self::Invalid(reason),
            bounded_json::InternalError::Serialize => Self::Serialize,
        }
    }
}

/// Serialize an action envelope into a bounded arena and route it against
/// PREACTION subscriptions.
///
/// `reserve_ordinary` must return an RAII reservation for exactly the requested
/// logical capacities from the Runtime's Ordinary pool. Its result must remain
/// alive for the matching call; the arena holds it until matching completes.
/// This function calls the reservation callback once after a no-allocation
/// sizing pass and before creating the arena buffers. A serialized JSON byte
/// bounds the count of nodes, edges and decoded string bytes individually; the
/// reservation covers those exact capacity bounds and the serialized buffer.
///
/// The serializer runs twice. Both passes consume the same work counters and
/// cancellation callback; the construction pass fails closed if it exceeds the
/// first pass's plan. The source type is expected to be the closed action
/// envelope and its JSON-compatible fields, with stable serialization and unique
/// object keys. Arbitrary serializers with side effects are outside this API
/// contract. The caller must supply a stack-safe maximum depth.
pub fn pre_action_matches_serialized_bounded_v1<T, C, R, E>(
    envelope: &T,
    subscriptions: &[ModuleSubscription],
    module_id: &str,
    action_kind: &str,
    work: &mut RouterWorkV1,
    cancelled: &mut C,
    mut reserve_ordinary: impl FnMut(usize) -> Result<R, E>,
) -> Result<bool, BoundedRouterErrorV1<E>>
where
    T: Serialize + ?Sized,
    C: FnMut() -> bool,
{
    let arena = bounded_json::build_action_json(envelope, work, cancelled, &mut reserve_ordinary)?;
    bounded_match::pre_action_matches(
        &arena,
        subscriptions,
        module_id,
        action_kind,
        work,
        cancelled,
    )
}

impl<E: fmt::Display, RE: fmt::Display> fmt::Display for BoundedRouterErrorV1<E, RE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Regex(error) => write!(f, "bounded router regex failed: {error}"),
            Self::Cancelled => f.write_str("bounded router cancelled"),
            Self::ResourceLimited(reason) => write!(f, "bounded router resource limited: {reason}"),
            Self::Unsupported(reason) => write!(f, "bounded router unsupported: {reason}"),
            Self::Invalid(reason) => write!(f, "bounded router invalid: {reason}"),
            Self::Serialize => f.write_str("bounded router serialization failed"),
            Self::Admission(error) => write!(f, "bounded router admission failed: {error}"),
        }
    }
}

#[cfg(test)]
#[path = "bounded_tests.rs"]
mod tests;

/// Routes with a data-only regex backend. The backend confers no execution authority.
#[expect(
    clippy::too_many_arguments,
    reason = "Envelope and routing context retain compatibility while shared budget, cancellation, admission and backend remain independent contracts"
)]
pub fn pre_action_matches_serialized_bounded_with_regex_v1<T, C, R, E, B>(
    envelope: &T,
    subscriptions: &[ModuleSubscription],
    module_id: &str,
    action_kind: &str,
    work: &mut RouterWorkV1,
    cancelled: &mut C,
    reserve_ordinary: impl FnMut(usize) -> Result<R, E>,
    backend: &mut B,
) -> Result<bool, BoundedRouterErrorV1<E, B::Error>>
where
    T: Serialize + ?Sized,
    C: FnMut() -> bool,
    B: RegexBatchBackendV1,
{
    subscription_matches_serialized_bounded_with_regex_v1(
        envelope,
        subscriptions,
        module_id,
        action_kind,
        oasis7_wasm_abi::ModuleSubscriptionStage::PreAction,
        work,
        cancelled,
        reserve_ordinary,
        backend,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Explicit routing stage and bounded shared resources"
)]
pub fn subscription_matches_serialized_bounded_with_regex_v1<T, C, R, E, B>(
    envelope: &T,
    subscriptions: &[ModuleSubscription],
    module_id: &str,
    action_kind: &str,
    stage: oasis7_wasm_abi::ModuleSubscriptionStage,
    work: &mut RouterWorkV1,
    cancelled: &mut C,
    mut reserve_ordinary: impl FnMut(usize) -> Result<R, E>,
    backend: &mut B,
) -> Result<bool, BoundedRouterErrorV1<E, B::Error>>
where
    T: Serialize + ?Sized,
    C: FnMut() -> bool,
    B: RegexBatchBackendV1,
{
    if !matches!(
        stage,
        oasis7_wasm_abi::ModuleSubscriptionStage::PreAction
            | oasis7_wasm_abi::ModuleSubscriptionStage::PostAction
            | oasis7_wasm_abi::ModuleSubscriptionStage::PostEvent
    ) {
        return Err(BoundedRouterErrorV1::Invalid("routing stage"));
    }
    let arena = bounded_json::build_action_json(envelope, work, cancelled, &mut reserve_ordinary)
        .map_err(BoundedRouterErrorV1::with_regex_error)?;
    bounded_match::with_regex(
        &arena,
        subscriptions,
        module_id,
        action_kind,
        stage,
        work,
        cancelled,
        &mut reserve_ordinary,
        backend,
    )
}
