//! Work-only correlation graph/copies and original typed validation calls.
use super::*;
use crate::world_service::{
    PaidServiceProtocolRefusalV1, ServiceAuthorityWorkErrorV1 as Error, request_digest_with_work_v1,
};
use oasis7_wasm_abi::{CanonicalCborStreamError as WorkError, CapabilityCanonicalWorkV1};
use std::mem::size_of;
pub struct PaidRequestCorrelationDataV1<C> {
    value: RequestCorrelation,
    _fees: Vec<C>,
    _slots: Option<C>,
    _outer: Option<C>,
}
impl<C> PaidRequestCorrelationDataV1<C> {
    pub fn correlation(&self) -> &RequestCorrelation {
        &self.value
    }
}
impl PaidRequestCorrelationDataV1<()> {
    pub(crate) fn into_legacy(self) -> RequestCorrelation {
        debug_assert!(self._fees.is_empty() && self._slots.is_none() && self._outer.is_none());
        self.value
    }
}
pub(crate) struct PaidLegacyIdentityDataV1<C> {
    value: LegacyIdentity,
    _fees: Vec<C>,
    _slots: Option<C>,
    _outer: Option<C>,
}
impl<C> PaidLegacyIdentityDataV1<C> {
    pub(crate) fn identity(&self) -> &LegacyIdentity {
        &self.value
    }
}
impl PaidLegacyIdentityDataV1<()> {
    pub(super) fn legacy(value: LegacyIdentity) -> Self {
        Self {
            value,
            _fees: Vec::new(),
            _slots: None,
            _outer: None,
        }
    }
}
pub(crate) struct Phase<'a, A: CapabilityCanonicalWorkV1> {
    pub(crate) source: &'a A,
    fees: Vec<A::Charge>,
    slots: Option<A::Charge>,
    outer: Option<A::Charge>,
}
impl<'a, A: CapabilityCanonicalWorkV1> Phase<'a, A> {
    pub(crate) fn new(source: &'a A) -> Result<Self, Error<A::Charge>> {
        source.charge_work(1, 0).map_err(Error::Work)?;
        let outer = if source.is_metered() {
            Some(
                source
                    .reserve(
                        size_of::<Self>() + size_of::<PaidRequestCorrelationDataV1<A::Charge>>(),
                    )
                    .map_err(Error::Work)?,
            )
        } else {
            None
        };
        Ok(Self {
            source,
            fees: Vec::new(),
            slots: None,
            outer,
        })
    }
    pub(crate) fn trace<T: Serialize + ?Sized>(&self, value: &T) -> Result<(), Error<A::Charge>> {
        super::trace_work::trace(value, self.source).map_err(Error::Work)
    }
    pub(crate) fn hold(&mut self, n: usize) -> Result<(), Error<A::Charge>> {
        self.source.charge_work(1, n as u64).map_err(Error::Work)?;
        if !self.source.is_metered() {
            return Ok(());
        }
        if self.fees.len() == self.fees.capacity() {
            let capacity = self
                .fees
                .capacity()
                .checked_mul(2)
                .unwrap_or(usize::MAX)
                .max(4);
            let n = capacity
                .checked_mul(size_of::<A::Charge>())
                .ok_or(Error::Work(WorkError::ResourceLimited(
                    "correlation fee slots",
                )))?;
            self.source.charge_work(1, n as u64).map_err(Error::Work)?;
            let fee = self.source.reserve(n).map_err(Error::Work)?;
            self.fees
                .try_reserve_exact(capacity - self.fees.len())
                .map_err(|_| Error::Work(WorkError::ResourceLimited("correlation fee slots")))?;
            if self.fees.capacity() > capacity {
                return Err(Error::Work(WorkError::ResourceLimited(
                    "correlation fee capacity",
                )));
            }
            self.slots = Some(fee);
        }
        self.fees.push(self.source.reserve(n).map_err(Error::Work)?);
        Ok(())
    }
    pub(crate) fn fail<T>(&self, message: &str) -> Result<T, Error<A::Charge>> {
        Err(Error::Protocol(
            crate::world_service::service_protocol_refusal_with_work_v1(self.source, message)?,
        ))
    }
    pub(crate) fn owned_error(
        &self,
        message: String,
    ) -> Result<Error<A::Charge>, Error<A::Charge>> {
        self.source
            .charge_work(1, message.len() as u64)
            .map_err(Error::Work)?;
        let fee = if self.source.is_metered() {
            Some(
                self.source
                    .reserve(
                        size_of::<PaidServiceProtocolRefusalV1<A::Charge>>()
                            .max(size_of::<Error<A::Charge>>())
                            + message.len(),
                    )
                    .map_err(Error::Work)?,
            )
        } else {
            None
        };
        Ok(Error::Protocol(PaidServiceProtocolRefusalV1 {
            message,
            fee,
        }))
    }
    pub(crate) fn copy(&mut self, text: &str) -> Result<String, Error<A::Charge>> {
        self.hold(size_of::<String>() + text.len())?;
        let mut bytes = Vec::with_capacity(text.len());
        for part in text.as_bytes().chunks(8192) {
            self.source
                .charge_work(1, part.len() as u64)
                .map_err(Error::Work)?;
            bytes.extend_from_slice(part)
        }
        Ok(String::from_utf8(bytes).expect("original UTF-8"))
    }
    pub(crate) fn prefixed(
        &mut self,
        prefix: &str,
        text: &str,
    ) -> Result<String, Error<A::Charge>> {
        let n = prefix
            .len()
            .checked_add(text.len())
            .ok_or(Error::Work(WorkError::ResourceLimited("correlation scope")))?;
        self.hold(size_of::<String>() + n)?;
        let mut output = Vec::with_capacity(n);
        for part in [prefix, text] {
            for part in part.as_bytes().chunks(8192) {
                self.source
                    .charge_work(1, part.len() as u64)
                    .map_err(Error::Work)?;
                output.extend_from_slice(part);
            }
        }
        Ok(String::from_utf8(output).expect("original UTF-8"))
    }
    pub(crate) fn decimal(&mut self, n: u64) -> Result<String, Error<A::Charge>> {
        self.hold(size_of::<String>() + 20)?;
        self.source.charge_work(20, 20).map_err(Error::Work)?;
        Ok(n.to_string())
    }
    pub(crate) fn digest<T: Serialize>(
        &mut self,
        domain: &str,
        value: &T,
    ) -> Result<String, Error<A::Charge>> {
        let digest = request_digest_with_work_v1(domain, value, self.source)?;
        self.copy(digest.as_str())
    }
    pub(crate) fn validation<T: Serialize + ?Sized, E: std::fmt::Display>(
        &mut self,
        value: &T,
        validate: impl FnOnce() -> Result<(), E>,
    ) -> Result<(), Error<A::Charge>> {
        self.trace(value)?;
        self.hold(4096)?;
        match validate() {
            Ok(()) => Ok(()),
            Err(error) => {
                let message = error.to_string();
                Err(self.owned_error(message)?)
            }
        }
    }
    pub(crate) fn json_value<T: Serialize>(
        &mut self,
        value: &T,
    ) -> Result<serde_json::Value, Error<A::Charge>> {
        let (nodes, bytes) =
            super::trace_work::trace_size(value, self.source).map_err(Error::Work)?;
        let n = nodes
            .checked_mul(1024)
            .and_then(|n| bytes.checked_mul(4).and_then(|b| n.checked_add(b)))
            .and_then(|n| n.checked_add(4096))
            .ok_or(Error::Work(WorkError::ResourceLimited(
                "receipt JSON graph",
            )))?;
        self.hold(n)?;
        self.source
            .charge_work(1 + n as u64, bytes as u64)
            .map_err(Error::Work)?;
        match serde_json::to_value(value) {
            Ok(value) => Ok(value),
            Err(error) => Err(self.owned_error(error.to_string())?),
        }
    }
    pub(crate) fn clone_json(
        &mut self,
        value: &serde_json::Value,
    ) -> Result<serde_json::Value, Error<A::Charge>> {
        let (nodes, bytes) =
            super::trace_work::trace_size(value, self.source).map_err(Error::Work)?;
        let n = nodes
            .checked_mul(2048)
            .and_then(|n| bytes.checked_mul(8).and_then(|b| n.checked_add(b)))
            .and_then(|n| n.checked_add(16384))
            .ok_or(Error::Work(WorkError::ResourceLimited(
                "receipt decode graph",
            )))?;
        self.hold(n)?;
        self.source
            .charge_work(1 + n as u64, bytes as u64)
            .map_err(Error::Work)?;
        Ok(value.clone())
    }
    pub(crate) fn get<'v>(
        &self,
        value: &'v serde_json::Value,
        key: &str,
    ) -> Result<Option<&'v serde_json::Value>, Error<A::Charge>> {
        let steps = value
            .as_object()
            .map_or(1, |v| v.len().checked_ilog2().unwrap_or(0) as u64 + 2);
        self.source
            .charge_work(
                steps.checked_mul(key.len() as u64 + 1).ok_or(Error::Work(
                    WorkError::ResourceLimited("receipt key lookup"),
                ))?,
                key.len() as u64,
            )
            .map_err(Error::Work)?;
        Ok(value.get(key))
    }
    pub(crate) fn finish_legacy(
        self,
        value: LegacyIdentity,
    ) -> PaidLegacyIdentityDataV1<A::Charge> {
        PaidLegacyIdentityDataV1 {
            value,
            _fees: self.fees,
            _slots: self.slots,
            _outer: self.outer,
        }
    }
    pub(crate) fn finish(
        self,
        value: RequestCorrelation,
    ) -> PaidRequestCorrelationDataV1<A::Charge> {
        PaidRequestCorrelationDataV1 {
            value,
            _fees: self.fees,
            _slots: self.slots,
            _outer: self.outer,
        }
    }
    pub(crate) fn decode_json<T: serde::de::DeserializeOwned>(
        &mut self,
        bytes: &[u8],
    ) -> Result<Result<T, serde_json::Error>, Error<A::Charge>> {
        let n = bytes
            .len()
            .checked_mul(32)
            .and_then(|n| n.checked_add(size_of::<T>() + 128 * 1024))
            .ok_or(Error::Work(WorkError::ResourceLimited(
                "browser closed JSON graph",
            )))?;
        self.hold(n)?;
        for part in bytes.chunks(8192) {
            self.source
                .charge_work(1 + 16 * part.len() as u64, part.len() as u64)
                .map_err(Error::Work)?;
        }
        Ok(serde_json::from_slice(bytes))
    }
}
