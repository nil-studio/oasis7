//! Allocation-free structural preflight before typed serde decoding.
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;

use super::{ObserverLoadError, ObserverReadLimits, WorldEvent};

struct Budget {
    remaining: usize,
    container_limit: usize,
    string_limit: usize,
    depth: usize,
}
impl Budget {
    fn new(limits: ObserverReadLimits) -> Self {
        // A journal event is one of the largest elements stored in observer
        // containers. Use its in-memory width before any Vec capacity hint is
        // honored; byte/string payloads have their separate length bound.
        let width = std::mem::size_of::<WorldEvent>().max(256);
        Self {
            remaining: limits.max_elements,
            container_limit: limits
                .max_elements
                .min(limits.max_single_allocation_bytes / width),
            string_limit: limits.max_single_allocation_bytes,
            depth: 0,
        }
    }
    fn spend<E: serde::de::Error>(&mut self) -> Result<(), E> {
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or_else(|| E::custom("ResourceLimited: serde element budget"))?;
        Ok(())
    }
    fn enter<E: serde::de::Error>(&mut self, hint: Option<usize>) -> Result<(), E> {
        if self.depth >= 64 || hint.is_some_and(|n| n > self.container_limit) {
            return Err(E::custom("ResourceLimited: serde container/depth budget"));
        }
        self.depth += 1;
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Schema {
    Generic,
    SnapshotRoot,
    ModuleArtifacts,
    ClosedRecordRoot,
    ArtifactObjects,
    ArtifactObject,
    FormalWriterEnvelope,
}
struct Check<'a>(&'a mut Budget, Schema);
impl<'de> DeserializeSeed<'de> for Check<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        self.0.spend()?;
        match self.1 {
            Schema::Generic | Schema::ClosedRecordRoot | Schema::FormalWriterEnvelope => {
                decoder.deserialize_any(self)
            }
            Schema::SnapshotRoot | Schema::ModuleArtifacts | Schema::ArtifactObject => {
                decoder.deserialize_map(self)
            }
            Schema::ArtifactObjects => decoder.deserialize_seq(self),
        }
    }
}
impl<'de> Visitor<'de> for Check<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a budgeted persisted value")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_none<E: serde::de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_some<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        self.deserialize(decoder)
    }
    fn visit_newtype_struct<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        self.deserialize(decoder)
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<(), E> {
        if value.len() > self.0.string_limit {
            Err(E::custom("ResourceLimited: serde string budget"))
        } else {
            Ok(())
        }
    }
    fn visit_borrowed_str<E: serde::de::Error>(self, value: &'de str) -> Result<(), E> {
        self.visit_str(value)
    }
    fn visit_string<E: serde::de::Error>(self, value: String) -> Result<(), E> {
        self.visit_str(&value)
    }
    fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<(), E> {
        if value.len() > self.0.string_limit {
            Err(E::custom("ResourceLimited: serde byte budget"))
        } else {
            Ok(())
        }
    }
    fn visit_borrowed_bytes<E: serde::de::Error>(self, value: &'de [u8]) -> Result<(), E> {
        self.visit_bytes(value)
    }
    fn visit_byte_buf<E: serde::de::Error>(self, value: Vec<u8>) -> Result<(), E> {
        self.visit_bytes(&value)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.0.enter(seq.size_hint())?;
        let mut length = 0usize;
        while seq
            .next_element_seed(Check(
                self.0,
                if matches!(self.1, Schema::ArtifactObjects) {
                    Schema::ArtifactObject
                } else {
                    Schema::Generic
                },
            ))?
            .is_some()
        {
            length += 1;
            if length > self.0.container_limit {
                return Err(serde::de::Error::custom(
                    "ResourceLimited: serde sequence budget",
                ));
            }
        }
        self.0.depth -= 1;
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.0.enter(map.size_hint())?;
        let mut length = 0usize;
        if matches!(self.1, Schema::FormalWriterEnvelope) {
            while let Some(schema) = map.next_key_seed(WriterEnvelopeKey(self.0))? {
                length += 1;
                if length > self.0.container_limit {
                    return Err(serde::de::Error::custom(
                        "ResourceLimited: writer envelope map",
                    ));
                }
                map.next_value_seed(Check(self.0, schema))?;
            }
            self.0.depth -= 1;
            return Ok(());
        }
        loop {
            let artifact_field = if matches!(
                self.1,
                Schema::SnapshotRoot | Schema::ClosedRecordRoot | Schema::ArtifactObject
            ) {
                let selected = match self.1 {
                    Schema::SnapshotRoot => "module_artifact_bytes",
                    Schema::ClosedRecordRoot => "objects",
                    Schema::ArtifactObject => "bytes",
                    _ => unreachable!(),
                };
                match map.next_key_seed(SnapshotKey(self.0, selected))? {
                    Some(value) => value,
                    None => break,
                }
            } else {
                if map.next_key_seed(Check(self.0, Schema::Generic))?.is_none() {
                    break;
                }
                false
            };
            length += 1;
            if length > self.0.container_limit {
                return Err(serde::de::Error::custom(
                    "ResourceLimited: serde map budget",
                ));
            }
            if matches!(self.1, Schema::ModuleArtifacts)
                || (matches!(self.1, Schema::ArtifactObject) && artifact_field)
            {
                map.next_value_seed(BytePayload(self.0))?;
            } else {
                let schema = if artifact_field {
                    if matches!(self.1, Schema::ClosedRecordRoot) {
                        Schema::ArtifactObjects
                    } else {
                        Schema::ModuleArtifacts
                    }
                } else {
                    Schema::Generic
                };
                map.next_value_seed(Check(self.0, schema))?;
            }
        }
        self.0.depth -= 1;
        Ok(())
    }
}
// Only exact typed schema paths select byte payloads: Snapshot root
// module_artifact_bytes values or ClosedRecord root objects[].bytes. Nested
// maps and journal events never inherit those states.
struct SnapshotKey<'a>(&'a mut Budget, &'static str);
impl<'de> DeserializeSeed<'de> for SnapshotKey<'_> {
    type Value = bool;
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<bool, D::Error> {
        self.0.spend()?;
        decoder.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for SnapshotKey<'_> {
    type Value = bool;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Snapshot field")
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<bool, E> {
        if value.len() > self.0.string_limit {
            return Err(E::custom("ResourceLimited: serde string budget"));
        }
        Ok(value == self.1)
    }
}
struct BytePayload<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for BytePayload<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        self.0.spend()?;
        decoder.deserialize_any(self)
    }
}
struct ByteElement<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for ByteElement<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        self.0.spend()?;
        <u8 as serde::Deserialize>::deserialize(decoder).map(|_| ())
    }
}
impl<'de> Visitor<'de> for BytePayload<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an artifact u8 sequence")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let limit = self.0.string_limit.min(self.0.remaining);
        // Byte arrays allocate one byte per entry; preserve pre-allocation
        // hint rejection without applying WorldEvent's unrelated width.
        if seq.size_hint().is_some_and(|n| n > limit) {
            return Err(serde::de::Error::custom(
                "ResourceLimited: artifact byte capacity",
            ));
        }
        self.0.enter(None)?;
        let mut length = 0usize;
        while seq.next_element_seed(ByteElement(self.0))?.is_some() {
            length += 1;
            if length > limit {
                return Err(serde::de::Error::custom(
                    "ResourceLimited: artifact byte capacity",
                ));
            }
        }
        self.0.depth -= 1;
        Ok(())
    }
    fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<(), E> {
        if bytes.len() > self.0.string_limit {
            return Err(E::custom("ResourceLimited: artifact byte capacity"));
        }
        self.0.remaining = self
            .0
            .remaining
            .checked_sub(bytes.len())
            .ok_or_else(|| E::custom("ResourceLimited: serde element budget"))?;
        Ok(())
    }
    fn visit_borrowed_bytes<E: serde::de::Error>(self, bytes: &'de [u8]) -> Result<(), E> {
        self.visit_bytes(bytes)
    }
    fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<(), E> {
        self.visit_bytes(&bytes)
    }
}
// Only the persisted writer DTO's declared material fields receive byte-aware
// ClosedRecord traversal. Arbitrary nested arrays retain generic typed limits.
struct WriterEnvelopeKey<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for WriterEnvelopeKey<'_> {
    type Value = Schema;
    fn deserialize<D: serde::Deserializer<'de>>(self, decoder: D) -> Result<Schema, D::Error> {
        self.0.spend()?;
        decoder.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for WriterEnvelopeKey<'_> {
    type Value = Schema;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("writer DTO field")
    }
    fn visit_str<E: serde::de::Error>(self, key: &str) -> Result<Schema, E> {
        if key.len() > self.0.string_limit {
            return Err(E::custom("ResourceLimited: writer key"));
        }
        Ok(match key {
            "material" | "frozen_material" => Schema::ClosedRecordRoot,
            "candidate" => Schema::FormalWriterEnvelope,
            _ => Schema::Generic,
        })
    }
}

pub(super) fn formal_writer_envelope_cbor(
    bytes: &[u8],
    limits: ObserverReadLimits,
) -> Result<(), ObserverLoadError> {
    let mut budget = Budget::new(limits);
    let mut decoder = serde_cbor::Deserializer::from_slice(bytes);
    Check(&mut budget, Schema::FormalWriterEnvelope)
        .deserialize(&mut decoder)
        .map_err(failure)?;
    decoder.end().map_err(failure)
}

fn failure(error: impl fmt::Display) -> ObserverLoadError {
    let message = error.to_string();
    if message.contains("ResourceLimited") {
        ObserverLoadError::ResourceLimited
    } else {
        ObserverLoadError::IntegrityFailure(message)
    }
}
pub(super) fn json(bytes: &[u8], limits: ObserverReadLimits) -> Result<(), ObserverLoadError> {
    let mut budget = Budget::new(limits);
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    Check(&mut budget, Schema::Generic)
        .deserialize(&mut decoder)
        .map_err(failure)?;
    decoder.end().map_err(failure)
}
pub(super) fn cbor(bytes: &[u8], limits: ObserverReadLimits) -> Result<(), ObserverLoadError> {
    let mut budget = Budget::new(limits);
    let mut decoder = serde_cbor::Deserializer::from_slice(bytes);
    Check(&mut budget, Schema::Generic)
        .deserialize(&mut decoder)
        .map_err(failure)?;
    decoder.end().map_err(failure)
}

pub(super) fn snapshot_cbor(
    bytes: &[u8],
    limits: ObserverReadLimits,
) -> Result<(), ObserverLoadError> {
    let mut budget = Budget::new(limits);
    let mut decoder = serde_cbor::Deserializer::from_slice(bytes);
    Check(&mut budget, Schema::SnapshotRoot)
        .deserialize(&mut decoder)
        .map_err(failure)?;
    decoder.end().map_err(failure)
}

pub(super) fn closed_record_cbor(
    bytes: &[u8],
    limits: ObserverReadLimits,
) -> Result<(), ObserverLoadError> {
    let mut budget = Budget::new(limits);
    let mut decoder = serde_cbor::Deserializer::from_slice(bytes);
    Check(&mut budget, Schema::ClosedRecordRoot)
        .deserialize(&mut decoder)
        .map_err(failure)?;
    decoder.end().map_err(failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formal_writer_envelope_budget_counts_only_declared_artifact_bytes_as_bytes() {
        let limits = ObserverReadLimits {
            max_file_bytes: 128 * 1024,
            max_single_allocation_bytes: 32 * 1024,
            max_elements: 64 * 1024,
            ..ObserverReadLimits::default()
        };
        let value = serde_json::json!({"schema_version": 1, "candidate": {
            "material": {"objects": [{"bytes": vec![7u8; 20_000], "content_hash": "abc", "references": []}]},
            "frozen_material": null,
        }});
        let bytes = serde_cbor::to_vec(&value).unwrap();
        assert!(formal_writer_envelope_cbor(&bytes, limits).is_ok());
        assert!(cbor(&bytes, limits).is_err());
        let ordinary = serde_cbor::to_vec(&serde_json::json!({"candidate": {
            "opaque": vec![7u8; 20_000],
        }}))
        .unwrap();
        assert!(formal_writer_envelope_cbor(&ordinary, limits).is_err());
        let oversized = serde_cbor::to_vec(&serde_json::json!({"material": {
            "objects": [{"bytes": vec![7u8; 40_000]}],
        }}))
        .unwrap();
        assert!(formal_writer_envelope_cbor(&oversized, limits).is_err());
    }

    #[test]
    fn refuses_small_inputs_with_excessive_typed_element_capacity() {
        let limits = ObserverReadLimits {
            max_elements: 3,
            ..ObserverReadLimits::default()
        };
        assert!(matches!(
            json(b"[0,0,0,0]", limits),
            Err(ObserverLoadError::ResourceLimited)
        ));
        // Definite CBOR array declaring an enormous number of elements. The
        // preflight rejects its size hint before typed Vec can reserve it.
        assert!(matches!(
            cbor(
                &[0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
                limits
            ),
            Err(ObserverLoadError::ResourceLimited)
        ));
        assert!(json(b"[0,1]", limits).is_ok());
    }
}

#[cfg(test)]
#[path = "observer_decode_budget_tests.rs"]
mod snapshot_tests;

#[cfg(test)]
mod closed_record_tests {
    use super::*;
    use serde_cbor::Value;
    fn record(payload: Value) -> Vec<u8> {
        serde_cbor::to_vec(&Value::Map(std::collections::BTreeMap::from([(
            Value::Text("objects".into()),
            Value::Array(vec![Value::Map(std::collections::BTreeMap::from([(
                Value::Text("bytes".into()),
                payload,
            )]))]),
        )])))
        .unwrap()
    }
    #[test]
    fn closed_record_payload_path_has_byte_width_and_strict_bounds() {
        let width = std::mem::size_of::<WorldEvent>().max(256);
        let limits = ObserverReadLimits {
            max_elements: 300_000,
            max_single_allocation_bytes: width * 2,
            ..ObserverReadLimits::default()
        };
        assert!(
            closed_record_cbor(
                &record(Value::Array(vec![Value::Integer(0); width])),
                limits
            )
            .is_ok()
        );
        for payload in [
            Value::Array(vec![Value::Integer(0); width * 2 + 1]),
            Value::Bytes(vec![0; width * 2 + 1]),
            Value::Array(vec![Value::Integer(256)]),
            Value::Array(vec![Value::Array(vec![])]),
        ] {
            assert!(closed_record_cbor(&record(payload), limits).is_err());
        }
        let huge_hint = b"\xa1\x67objects\x81\xa1\x65bytes\x9a\x00\x10\x00\x00";
        assert!(matches!(
            closed_record_cbor(huge_hint, limits),
            Err(ObserverLoadError::ResourceLimited)
        ));
        let global = ObserverReadLimits {
            max_elements: 3,
            ..limits
        };
        assert!(closed_record_cbor(&record(Value::Bytes(vec![0; 4])), global).is_err());
        // A nested same-name field remains an ordinary WorldEvent-width sequence.
        let fake = serde_cbor::to_vec(
            &serde_json::json!({"nested":{"objects":[{"bytes":vec![0u8;width*2+1]}]}}),
        )
        .unwrap();
        assert!(closed_record_cbor(&fake, limits).is_err());
    }
    #[test]
    fn closed_record_large_artifact_preflight_without_default_changes() {
        let bytes = if let Ok(path) = std::env::var("OASIS7_CONTROLLED_HISTORY_WASM_FIXTURE") {
            let bytes = std::fs::read(path).unwrap();
            assert_eq!(bytes.len(), 203153);
            assert!(bytes.starts_with(b"\0asm"));
            bytes
        } else {
            vec![0; 203153]
        };
        let encoded = record(Value::Array(
            bytes
                .into_iter()
                .map(|b| Value::Integer(b.into()))
                .collect(),
        ));
        assert!(cbor(&encoded, ObserverReadLimits::default()).is_err());
        assert!(closed_record_cbor(&encoded, ObserverReadLimits::default()).is_ok());
    }
}
