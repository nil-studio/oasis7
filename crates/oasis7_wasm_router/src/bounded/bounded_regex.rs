//! Data-only batch computation interface. Implementing this interface grants no
//! Native execution authority; the Runtime selects its own trusted provider.
#[derive(Clone, Copy, Debug)]
pub struct RegexProbeV1<'a> {
    pub pattern: &'a str,
    pub text: Option<&'a str>,
}

/// Keeps the provider's result admission alive throughout matching.
pub trait RegexBatchOutputV1 {
    fn values(&self) -> &[bool];
}

pub trait RegexBatchBackendV1 {
    type Error;
    type Output: RegexBatchOutputV1;
    /// One ordered result per probe; `None` validates compilation only.
    fn match_batch<C: FnMut() -> bool>(
        &mut self,
        probes: &[RegexProbeV1<'_>],
        cancelled: &mut C,
    ) -> Result<Self::Output, Self::Error>;
}

impl<E> super::BoundedRouterErrorV1<E> {
    pub(super) fn with_regex_error<RE>(self) -> super::BoundedRouterErrorV1<E, RE> {
        use super::BoundedRouterErrorV1 as Error;
        match self {
            Error::Cancelled => Error::Cancelled,
            Error::ResourceLimited(v) => Error::ResourceLimited(v),
            Error::Unsupported(v) => Error::Unsupported(v),
            Error::Invalid(v) => Error::Invalid(v),
            Error::Serialize => Error::Serialize,
            Error::Admission(v) => Error::Admission(v),
            Error::Regex(v) => match v {},
        }
    }
}
