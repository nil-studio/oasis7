//! Explicit schema-1/2 storage. Original signed byte vectors remain opaque.
use super::super::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::BTreeMap, io::Write, sync::Arc};

pub(super) const MAX_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Stored {
    schema_version: u32,
    pub namespace: String,
    pub binding_digest: String,
    body: Body,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Body {
    State {
        #[serde(serialize_with = "Signer::serialize", deserialize_with = "read_signer")]
        value: Box<BftSignerBackendSnapshotV3>,
    },
    Retention {
        #[serde(
            serialize_with = "Envelope::serialize",
            deserialize_with = "read_envelope"
        )]
        value: Box<BftOriginalRetentionTxnSnapshotV1>,
    },
}
fn read_signer<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Box<BftSignerBackendSnapshotV3>, D::Error> {
    Signer::deserialize(d).map(Box::new)
}
fn read_envelope<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Box<BftOriginalRetentionTxnSnapshotV1>, D::Error> {
    Envelope::deserialize(d).map(Box::new)
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BftSignerBackendSnapshotV3", deny_unknown_fields)]
struct Signer {
    state: BftSignerStateV3,
    lease_generation: u64,
    lease_token_digest: String,
    protected_time_floor_ms: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BftFreshCoverageSnapshotV1", deny_unknown_fields)]
struct Fresh {
    #[serde(with = "Signer")]
    signer: BftSignerBackendSnapshotV3,
    marker: BftPrivateRetentionCreationMarkerV1,
    original: Option<BftPrivateRetentionActivationV1>,
    reserved_response_bytes: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BftFirstReservedOriginalV1", deny_unknown_fields)]
struct Reservation {
    ledger: BftPrivateRetentionLedgerV1,
    #[serde(serialize_with = "write_input", deserialize_with = "read_input")]
    input: Arc<BftPrivateRetentionAdmissionInputV1>,
}
fn write_input<S: Serializer>(
    input: &Arc<BftPrivateRetentionAdmissionInputV1>,
    s: S,
) -> Result<S::Ok, S::Error> {
    input.as_ref().serialize(s)
}
fn read_input<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Arc<BftPrivateRetentionAdmissionInputV1>, D::Error> {
    BftPrivateRetentionAdmissionInputV1::deserialize(d).map(Arc::new)
}
fn write_reservation<S: Serializer>(
    value: &Option<BftFirstReservedOriginalV1>,
    s: S,
) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Ref<'a>(#[serde(with = "Reservation")] &'a BftFirstReservedOriginalV1);
    value.as_ref().map(Ref).serialize(s)
}
fn read_reservation<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<BftFirstReservedOriginalV1>, D::Error> {
    #[derive(Deserialize)]
    struct Owned(#[serde(with = "Reservation")] BftFirstReservedOriginalV1);
    Option::<Owned>::deserialize(d).map(|value| value.map(|v| v.0))
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BftFirstReserveSnapshotV1", deny_unknown_fields)]
struct First {
    #[serde(with = "Fresh")]
    current: BftFreshCoverageSnapshotV1,
    #[serde(
        serialize_with = "write_reservation",
        deserialize_with = "read_reservation"
    )]
    reservation: Option<BftFirstReservedOriginalV1>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BftOriginalRetentionTxnSnapshotV1", deny_unknown_fields)]
struct Envelope {
    #[serde(with = "First")]
    current: BftFirstReserveSnapshotV1,
    original_operations: BTreeMap<String, BftRetainedOriginalOperationV1>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "read_current"
    )]
    current_retention: Option<BftCurrentRetentionV1>,
}
fn read_current<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<BftCurrentRetentionV1>, D::Error> {
    BftCurrentRetentionV1::deserialize(d).map(Some)
}

impl Stored {
    pub fn new(
        namespace: &str,
        signer: BftSignerBackendSnapshotV3,
        retained: Option<BftOriginalRetentionTxnSnapshotV1>,
    ) -> Result<Self, BftSignerErrorV3> {
        let binding_digest = signer.state.binding_digest.clone();
        let schema_version = if retained
            .as_ref()
            .is_some_and(|value| value.current_retention.is_some())
        {
            2
        } else {
            1
        };
        let body = match retained {
            Some(value) if value.current.current.signer == signer => Body::Retention {
                value: Box::new(value),
            },
            Some(_) => {
                return Err(super::invalid(
                    "PG retained signer differs from original snapshot",
                ));
            }
            None => Body::State {
                value: Box::new(signer),
            },
        };
        let stored = Self {
            schema_version,
            namespace: namespace.into(),
            binding_digest,
            body,
        };
        stored.validate()?;
        Ok(stored)
    }
    pub fn signer(&self) -> &BftSignerBackendSnapshotV3 {
        match &self.body {
            Body::State { value } => value,
            Body::Retention { value } => &value.current.current.signer,
        }
    }
    pub fn signer_mut(&mut self) -> &mut BftSignerBackendSnapshotV3 {
        match &mut self.body {
            Body::State { value } => value,
            Body::Retention { value } => &mut value.current.current.signer,
        }
    }
    pub fn has_retention(&self) -> bool {
        matches!(self.body, Body::Retention { .. })
    }
    fn validate(&self) -> Result<(), BftSignerErrorV3> {
        let signer = self.signer();
        super::validate_identity(&self.namespace, &self.binding_digest)?;
        let supported = match (&self.body, self.schema_version) {
            (Body::State { .. }, 1) => true,
            (Body::Retention { value }, 1) => value.current_retention.is_none(),
            (Body::Retention { value }, 2) => value.current_retention.is_some(),
            _ => false,
        };
        if !supported
            || signer.state.schema_version != 3
            || signer.state.binding_digest != self.binding_digest
            || signer.lease_generation == 0
            || signer.protected_time_floor_ms < 0
        {
            return Err(super::invalid("PG namespace version/binding/floor invalid"));
        }
        state::validate_digest(&signer.state.state_digest)?;
        if self.schema_version == 1 {
            legacy_head(
                signer.state.private_retention.as_ref(),
                &self.binding_digest,
            )?;
        } else if let Some(head) = &signer.state.private_retention {
            head.validate_supported_head(&self.binding_digest)?;
        }
        if !signer.lease_token_digest.is_empty() {
            state::validate_digest(&signer.lease_token_digest)?;
        }
        if let Body::Retention { value } = &self.body {
            value.validate()?;
            value.current.current.marker.digest()?;
            if value.current.current.marker.source.binding_digest != self.binding_digest {
                return Err(super::invalid("PG retention namespace binding differs"));
            }
            if let Some(reservation) = &value.current.reservation {
                reservation.input.validate()?;
                reservation.ledger.map.require_reserved()?;
            }
            for head in [
                value
                    .current
                    .current
                    .original
                    .as_ref()
                    .map(|v| &v.receipt.body.head),
                value
                    .current
                    .reservation
                    .as_ref()
                    .map(|v| &v.ledger.receipt.body.head),
            ] {
                legacy_head(head, &self.binding_digest)?;
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, BftSignerErrorV3> {
        self.validate()?;
        let mut out = Encoding {
            bytes: Vec::new(),
            original: None,
            offset: 0,
        };
        serde_cbor::to_writer(&mut out, self)
            .map_err(|_| super::invalid("PG namespace encoding exceeds bound"))?;
        Ok(out.bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, BftSignerErrorV3> {
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return Err(super::invalid("PG namespace byte bound"));
        }
        crypto::scan_cbor_bounds(bytes)?;
        let value: Self =
            serde_cbor::from_slice(bytes).map_err(|_| super::invalid("PG namespace decoding"))?;
        value.validate()?;
        let mut compare = Encoding {
            bytes: Vec::new(),
            original: Some(bytes),
            offset: 0,
        };
        serde_cbor::to_writer(&mut compare, &value)
            .map_err(|_| super::invalid("PG namespace noncanonical bytes"))?;
        if compare.offset != bytes.len() {
            return Err(super::invalid("PG namespace trailing bytes"));
        }
        Ok(value)
    }
}
fn legacy_head(
    head: Option<&BftPrivateRetentionHeadV1>,
    binding: &str,
) -> Result<(), BftSignerErrorV3> {
    if let Some(head) = head {
        head.validate_supported_head(binding)?;
        if head.schema_version != 2 {
            return Err(super::invalid(
                "PG original/legacy checkpoint requires Head2",
            ));
        }
    }
    Ok(())
}
// Streaming encode/compare keeps Arc input borrowed; no second full encoding
// or serde Value tree is allocated when validating a decoded namespace.
struct Encoding<'a> {
    bytes: Vec<u8>,
    original: Option<&'a [u8]>,
    offset: usize,
}
impl Write for Encoding<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .offset
            .checked_add(bytes.len())
            .filter(|n| *n <= MAX_BYTES)
            .ok_or_else(|| std::io::Error::other("PG namespace capacity"))?;
        if let Some(original) = self.original {
            if original.get(self.offset..end) != Some(bytes) {
                return Err(std::io::Error::other("PG namespace canonical mismatch"));
            }
        } else {
            self.bytes.extend_from_slice(bytes);
        }
        self.offset = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
