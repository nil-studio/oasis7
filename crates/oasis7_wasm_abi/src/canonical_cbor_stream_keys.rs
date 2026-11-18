use super::*;
use serde::ser::{SerializeMap, SerializeStruct, SerializeStructVariant};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Map,
    Struct,
    Variant(&'static str),
}
pub(super) struct Encoded<Q> {
    pub bytes: Vec<u8>,
    _charge: Q,
}
struct Key<Q> {
    encoded: Encoded<Q>,
    _model: Q,
}
pub(super) struct Map<'a, 'e, 'v, T: ?Sized, A: CanonicalCborAllocator, C: FnMut() -> bool> {
    pub engine: &'e mut Engine<'a, A, C>,
    pub out: &'e mut dyn Write,
    pub root: &'v T,
    pub kind: Kind,
    keys: Vec<Key<A::Charge>>,
    rows: Vec<(ciborium::Value, ciborium::Value)>,
    pending: Option<Key<A::Charge>>,
    max_encoded_key_bytes: usize,
    _rows: A::Charge,
}

// The fixed row layout and comparator do not depend on the map's source or
// context types. Share this stable sort across their generic instantiations.
#[inline(never)]
fn sort_rows(rows: &mut [(ciborium::Value, ciborium::Value)]) -> u64 {
    let mut comparisons = 0u64;
    rows.sort_by(|(left, _), (right, _)| {
        comparisons = comparisons.saturating_add(1);
        ciborium::value::CanonicalValue::from(left.clone())
            .cmp(&ciborium::value::CanonicalValue::from(right.clone()))
    });
    comparisons
}

fn comparison_byte_bound<A: CanonicalCborAllocator, C: FnMut() -> bool>(
    engine: &mut Engine<'_, A, C>,
    rows: &[(ciborium::Value, ciborium::Value)],
    scratch: usize,
) -> Result<usize, Error> {
    let header = std::mem::size_of::<ciborium::Value>();
    let mut longest_text = 0;
    for (key, _) in rows {
        // Inspect only the already retained model, after paying for its header.
        engine.step()?;
        engine.allocator.charge_work(0, header as u64)?;
        let ciborium::Value::Text(text) = key else {
            // In particular, Tag(Text) keeps the original comparator fallback.
            return Ok(scratch);
        };
        longest_text = longest_text.max(text.len());
    }
    // ciborium 0.2.2's Text/Text comparator reads lengths, then UTF-8 bytes;
    // it does not re-encode either model. The unchanged comparator first
    // clones both Values. Eight header/payload passes cover those two clones'
    // reads/writes, the length/lexical reads and the temporary owners' teardown.
    // This is logical byte work; the full original scratch admission still
    // covers their simultaneous allocated storage during the standard sort.
    header
        .checked_add(longest_text)
        .and_then(|n| n.checked_mul(8))
        .ok_or(Error::ResourceLimited("text key comparison work overflow"))
}

// Admission and accounting depend on the engine and retained rows, not the
// serialized map source. Keep this work shared across source types as well.
#[inline(never)]
fn prepare_sorted_rows<A: CanonicalCborAllocator, C: FnMut() -> bool>(
    engine: &mut Engine<'_, A, C>,
    rows: &mut [(ciborium::Value, ciborium::Value)],
    max_encoded_key_bytes: usize,
) -> Result<(), Error> {
    // The retained key models come from these exact encoded bytes.
    // Keep the same conservative bound without charging unused key capacity.
    let scratch = max_encoded_key_bytes
        .checked_mul(512)
        .and_then(|n| n.checked_add(4096))
        .ok_or(Error::ResourceLimited("key sort scratch"))?;
    // Admit a deliberately conservative quadratic comparison allowance
    // before entering the existing bounded standard stable sort. Never
    // change the comparator when cancellation/work admission fails.
    engine.step()?;
    let n = rows.len() as u64;
    let allowance = n
        .checked_mul(n)
        .and_then(|v| v.checked_mul(16))
        .and_then(|v| v.checked_add(64))
        .ok_or(Error::ResourceLimited("sort work overflow"))?;
    // Text/Text comparisons have no re-encoding work. Every other model retains
    // the original conservative byte bound, including compound and tagged keys.
    let bytes_per_comparison = comparison_byte_bound(engine, rows, scratch)?;
    let comparison_bytes = allowance
        .checked_mul(
            u64::try_from(bytes_per_comparison)
                .map_err(|_| Error::ResourceLimited("sort byte work overflow"))?,
        )
        .ok_or(Error::ResourceLimited("sort byte work overflow"))?;
    engine.allocator.charge_work(allowance, comparison_bytes)?;
    engine.work.visits_remaining = engine
        .work
        .visits_remaining
        .checked_sub(allowance)
        .ok_or(Error::ResourceLimited("map sort visits"))?;
    let _scratch = engine.allocator.reserve(scratch)?;
    let comparisons = sort_rows(rows);
    engine.metrics.map_sorts = engine
        .metrics
        .map_sorts
        .checked_add(1)
        .ok_or(Error::ResourceLimited("sort metric overflow"))?;
    engine.metrics.map_comparisons = engine
        .metrics
        .map_comparisons
        .checked_add(comparisons)
        .ok_or(Error::ResourceLimited("sort metric overflow"))?;
    if comparisons > allowance {
        return Err(Error::ResourceLimited(
            "standard sort exceeded admitted comparisons",
        ));
    }
    engine.step()?;
    Ok(())
}

impl<A: CanonicalCborAllocator, C: FnMut() -> bool> Engine<'_, A, C> {
    pub(super) fn key<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<Encoded<A::Charge>, Error> {
        let cap = self.work.max_key_bytes;
        let charge = self.allocator.reserve(cap)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(cap)
            .map_err(|_| Error::ResourceLimited("key allocation"))?;
        if bytes.capacity() > cap {
            return Err(Error::ResourceLimited("key capacity"));
        }
        struct Bounded<'a> {
            bytes: &'a mut Vec<u8>,
            cap: usize,
            limited: bool,
        }
        impl Write for Bounded<'_> {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                if b.len() > self.cap - self.bytes.len() {
                    self.limited = true;
                    return Err(io::ErrorKind::OutOfMemory.into());
                }
                self.bytes.extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut sink = Bounded {
            bytes: &mut bytes,
            cap,
            limited: false,
        };
        let result = self.encode(value, &mut sink);
        if sink.limited {
            return Err(Error::ResourceLimited("encoded key bytes"));
        }
        result?;
        Ok(Encoded {
            bytes,
            _charge: charge,
        })
    }
}
impl<'a, 'e, 'v, T: Serialize + ?Sized, A: CanonicalCborAllocator, C: FnMut() -> bool>
    Map<'a, 'e, 'v, T, A, C>
{
    pub fn new(
        engine: &'e mut Engine<'a, A, C>,
        out: &'e mut dyn Write,
        root: &'v T,
        kind: Kind,
    ) -> Result<Self, Error> {
        engine.step()?;
        let cap = engine.work.max_map_keys;
        let metadata = cap
            .checked_mul(
                std::mem::size_of::<Key<A::Charge>>()
                    + 2 * std::mem::size_of::<(ciborium::Value, ciborium::Value)>(),
            )
            .ok_or(Error::ResourceLimited("map metadata overflow"))?;
        let charge = engine.allocator.reserve(metadata)?;
        let mut keys = Vec::new();
        let mut rows = Vec::new();
        keys.try_reserve_exact(cap)
            .map_err(|_| Error::ResourceLimited("map key allocation"))?;
        rows.try_reserve_exact(cap)
            .map_err(|_| Error::ResourceLimited("map rows allocation"))?;
        if keys.capacity() > cap || rows.capacity() > cap {
            return Err(Error::ResourceLimited("map capacity"));
        }
        Ok(Self {
            engine,
            out,
            root,
            kind,
            keys,
            rows,
            pending: None,
            max_encoded_key_bytes: 0,
            _rows: charge,
        })
    }
    fn push_key<K: Serialize + ?Sized>(&mut self, key: &K) -> Result<(), Error> {
        self.engine.step()?;
        if self.pending.is_some() || self.keys.len() >= self.engine.work.max_map_keys {
            return Err(Error::ResourceLimited("map keys/pending key"));
        }
        let encoded = self.engine.key(key)?;
        // Only bounded KEY models are permitted. Charge a conservative bound for
        // Value tree containers/string copies before parsing the already encoded key without tag interpretation.
        let bound = encoded
            .bytes
            .len()
            .checked_mul(128)
            .and_then(|n| n.checked_add(4096))
            .ok_or(Error::ResourceLimited("key model overflow"))?;
        let model = self.engine.allocator.reserve(bound)?;
        let value = super::key_model::parse(&encoded.bytes, self.engine)?;
        let ordinal = ciborium::Value::from(
            u64::try_from(self.keys.len()).map_err(|_| Error::ResourceLimited("map ordinal"))?,
        );
        self.rows.push((value, ordinal));
        self.max_encoded_key_bytes = self.max_encoded_key_bytes.max(encoded.bytes.len());
        self.pending = Some(Key {
            encoded,
            _model: model,
        });
        Ok(())
    }
    fn push_value<V: Serialize + ?Sized>(&mut self, _: &V) -> Result<(), Error> {
        self.engine.step()?;
        self.keys.push(
            self.pending
                .take()
                .ok_or(Error::Invalid("map value without key"))?,
        );
        Ok(())
    }
    fn finish(mut self) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Invalid("map unfinished key"));
        }
        // Exact old comparator and (Value,Value) sort element layout. This also
        // preserves its Tag/Tag semantics and stable duplicate-key order; it is
        // not a wire migration to a different interpretation of RFC ordering.
        prepare_sorted_rows(self.engine, &mut self.rows, self.max_encoded_key_bytes)?;
        self.engine.head(self.out, 5, self.rows.len() as u64)?;
        for (_, ordinal) in &self.rows {
            self.engine.step()?;
            let index = usize::try_from(i128::from(
                ordinal.as_integer().ok_or(Error::Invalid("key ordinal"))?,
            ))
            .map_err(|_| Error::Invalid("key ordinal"))?;
            let key = &self.keys[index].encoded.bytes;
            self.engine.emit(self.out, key)?;
            self.root.serialize(super::replay::Select {
                engine: self.engine,
                out: self.out,
                index,
                expected: key,
                total: self.keys.len(),
                kind: self.kind,
            })?;
        }
        self.engine.step()
    }
}
impl<T: Serialize + ?Sized, A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeMap
    for Map<'_, '_, '_, T, A, C>
{
    type Ok = ();
    type Error = Error;
    fn serialize_key<K: Serialize + ?Sized>(&mut self, key: &K) -> Result<(), Error> {
        self.push_key(key)
    }
    fn serialize_value<V: Serialize + ?Sized>(&mut self, value: &V) -> Result<(), Error> {
        self.push_value(value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}
impl<T: Serialize + ?Sized, A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeStruct
    for Map<'_, '_, '_, T, A, C>
{
    type Ok = ();
    type Error = Error;
    fn serialize_field<V: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &V,
    ) -> Result<(), Error> {
        self.push_key(key)?;
        self.push_value(value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}
impl<T: Serialize + ?Sized, A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeStructVariant
    for Map<'_, '_, '_, T, A, C>
{
    type Ok = ();
    type Error = Error;
    fn serialize_field<V: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &V,
    ) -> Result<(), Error> {
        self.push_key(key)?;
        self.push_value(value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}
