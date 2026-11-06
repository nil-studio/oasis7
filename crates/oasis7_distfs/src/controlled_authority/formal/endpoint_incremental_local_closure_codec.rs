//! Frozen local record v1. CBOR contains one definite bytes value; no map,
//! sequence hint, String or recursively owned DTO is decoded here.
use super::*;
use serde::{
    Deserializer, Serializer,
    de::{self, Visitor},
};
const DOMAIN: &[u8] = b"oasis7.iws.local-data-closure.v1\0";
const PAYLOAD_BYTES: usize = DOMAIN.len() + 4 + 3 * 32 + 8 + 4 * 32 + 4 * 8 + 33 + 2 * (8 + 8 + 32);
// 256..65535 definite CBOR bytes uses exactly a three-byte header.
const _: () = assert!(PAYLOAD_BYTES >= 256 && PAYLOAD_BYTES <= 65535);
pub(crate) const WIRE_BYTES: usize = 3 + PAYLOAD_BYTES;
struct Output {
    bytes: [u8; PAYLOAD_BYTES],
    at: usize,
}
impl Output {
    fn put(&mut self, value: &[u8]) {
        self.bytes[self.at..self.at + value.len()].copy_from_slice(value);
        self.at += value.len();
    }
    fn location(&mut self, value: &LocalFactLocationV1) {
        self.put(&value.height.to_be_bytes());
        self.put(&value.ordinal.to_be_bytes());
        self.put(&value.original);
    }
}
struct Input<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Input<'_> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0; N];
        out.copy_from_slice(&self.bytes[self.at..self.at + N]);
        self.at += N;
        out
    }
    fn number(&mut self) -> u64 {
        u64::from_be_bytes(self.take())
    }
    fn location(&mut self) -> LocalFactLocationV1 {
        LocalFactLocationV1 {
            height: self.number(),
            ordinal: self.number(),
            original: self.take(),
        }
    }
}
impl Serialize for LocalDataClosureRecordV1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Exhaustive fields intentionally make every schema addition a build
        // break rather than silently omitting a new authority/namespace column.
        let Self {
            version,
            instance,
            authority,
            activation,
            epoch,
            record,
            body,
            roots,
            manifest,
            page_lease,
            native_lease,
            native_generation,
            cognition,
            runtime,
        } = self;
        let mut out = Output {
            bytes: [0; PAYLOAD_BYTES],
            at: 0,
        };
        out.put(DOMAIN);
        out.put(&version.to_be_bytes());
        for hash in [instance, authority, activation] {
            out.put(hash);
        }
        out.put(&epoch.to_be_bytes());
        for hash in [record, body, roots, manifest] {
            out.put(hash);
        }
        for number in [page_lease.0, page_lease.1, native_lease.0, native_lease.1] {
            out.put(&number.to_be_bytes());
        }
        match native_generation {
            None => {
                out.put(&[0]);
                out.put(&[0; 32]);
            }
            Some(digest) => {
                out.put(&[1]);
                out.put(digest);
            }
        }
        out.location(cognition);
        out.location(runtime);
        if out.at != PAYLOAD_BYTES {
            return Err(serde::ser::Error::custom(
                "local closure fixed encode length",
            ));
        }
        serializer.serialize_bytes(&out.bytes)
    }
}
impl<'de> Deserialize<'de> for LocalDataClosureRecordV1 {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct Fixed;
        impl<'de> Visitor<'de> for Fixed {
            type Value = LocalDataClosureRecordV1;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("fixed local data closure v1 bytes")
            }
            fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                if bytes.len() != PAYLOAD_BYTES || !bytes.starts_with(DOMAIN) {
                    return Err(E::custom("local closure fixed domain/length"));
                }
                let mut input = Input {
                    bytes,
                    at: DOMAIN.len(),
                };
                let version = u32::from_be_bytes(input.take());
                if version != 1 {
                    return Err(E::custom("local closure unknown version"));
                }
                let value = LocalDataClosureRecordV1 {
                    version,
                    instance: input.take(),
                    authority: input.take(),
                    activation: input.take(),
                    epoch: input.number(),
                    record: input.take(),
                    body: input.take(),
                    roots: input.take(),
                    manifest: input.take(),
                    page_lease: (input.number(), input.number()),
                    native_lease: (input.number(), input.number()),
                    native_generation: {
                        let tag = input.take::<1>()[0];
                        let digest = input.take::<32>();
                        match tag {
                            0 if digest == [0; 32] => None,
                            1 => Some(digest),
                            _ => return Err(E::custom("local closure generation option")),
                        }
                    },
                    cognition: input.location(),
                    runtime: input.location(),
                };
                if input.at != PAYLOAD_BYTES {
                    return Err(E::custom("local closure fixed decode length"));
                }
                Ok(value)
            }
            fn visit_borrowed_bytes<E: de::Error>(
                self,
                bytes: &'de [u8],
            ) -> Result<Self::Value, E> {
                self.visit_bytes(bytes)
            }
        }
        decoder.deserialize_bytes(Fixed)
    }
}

#[cfg(test)]
#[path = "endpoint_incremental_local_closure_codec_tests.rs"]
mod tests;
