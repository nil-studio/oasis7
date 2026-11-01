//! Preserve the exact encoded KEY model: the general CBOR deserializer
//! intentionally interprets bigint tags and can erase an explicit Tag identity.
//! This parser never decodes a value payload outside the admitted key buffer.
use super::*;
use ciborium::Value;
pub(super) fn parse<A: CanonicalCborAllocator, C: FnMut() -> bool>(
    bytes: &[u8],
    engine: &mut Engine<'_, A, C>,
) -> Result<Value, Error> {
    let mut parser = Parser {
        bytes,
        position: 0,
        engine,
    };
    let value = parser.value()?;
    if parser.position != bytes.len() {
        return Err(Error::Invalid("key model trailing bytes"));
    }
    Ok(value)
}
struct Parser<'b, 'a, 'e, A: CanonicalCborAllocator, C: FnMut() -> bool> {
    bytes: &'b [u8],
    position: usize,
    engine: &'e mut Engine<'a, A, C>,
}
impl<'b, A: CanonicalCborAllocator, C: FnMut() -> bool> Parser<'b, '_, '_, A, C> {
    fn take(&mut self, n: usize) -> Result<&'b [u8], Error> {
        let end = self
            .position
            .checked_add(n)
            .ok_or(Error::Invalid("key length overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or(Error::Invalid("key model truncated"))?;
        self.position = end;
        Ok(bytes)
    }
    fn header(&mut self) -> Result<(u8, u8, u64), Error> {
        let first = self.take(1)?[0];
        let ai = first & 31;
        let number = match ai {
            0..=23 => ai as u64,
            24 => self.take(1)?[0] as u64,
            25 => u16::from_be_bytes(
                self.take(2)?
                    .try_into()
                    .map_err(|_| Error::Invalid("key header"))?,
            ) as u64,
            26 => u32::from_be_bytes(
                self.take(4)?
                    .try_into()
                    .map_err(|_| Error::Invalid("key header"))?,
            ) as u64,
            27 => u64::from_be_bytes(
                self.take(8)?
                    .try_into()
                    .map_err(|_| Error::Invalid("key header"))?,
            ),
            _ => return Err(Error::Invalid("key model definite header")),
        };
        Ok((first >> 5, ai, number))
    }
    fn value(&mut self) -> Result<Value, Error> {
        self.engine.step()?;
        if self.engine.depth >= self.engine.work.max_depth {
            return Err(Error::ResourceLimited("key model depth"));
        }
        let _frame = self.engine.allocator.reserve(512)?;
        self.engine.depth += 1;
        let result = self.item();
        self.engine.depth -= 1;
        result
    }
    fn item(&mut self) -> Result<Value, Error> {
        let start = self.position;
        let (major, ai, number) = self.header()?;
        match major {
            0 => Ok(Value::from(number)),
            1 => Ok(Value::Integer(
                ciborium::value::Integer::try_from(-1i128 - i128::from(number))
                    .map_err(|_| Error::Invalid("key negative integer"))?,
            )),
            2 | 3 => {
                let n = usize::try_from(number).map_err(|_| Error::Invalid("key length"))?;
                let data = self.take(n)?;
                // The complete key model allocation bound is reserved by the caller
                // before entry; every copied byte belongs to this finite key slice.
                if major == 2 {
                    Ok(Value::Bytes(data.to_vec()))
                } else {
                    Ok(Value::Text(
                        std::str::from_utf8(data)
                            .map_err(|_| Error::Invalid("key UTF8"))?
                            .to_owned(),
                    ))
                }
            }
            4 => {
                let n = usize::try_from(number).map_err(|_| Error::Invalid("key array count"))?;
                if n > self.bytes.len() - self.position {
                    return Err(Error::Invalid("key array count bound"));
                }
                let mut values = Vec::new();
                values
                    .try_reserve_exact(n)
                    .map_err(|_| Error::ResourceLimited("key array allocation"))?;
                for _ in 0..n {
                    values.push(self.value()?);
                }
                Ok(Value::Array(values))
            }
            5 => {
                let n = usize::try_from(number).map_err(|_| Error::Invalid("key map count"))?;
                if n > (self.bytes.len() - self.position) / 2 {
                    return Err(Error::Invalid("key map count bound"));
                }
                let mut values = Vec::new();
                values
                    .try_reserve_exact(n)
                    .map_err(|_| Error::ResourceLimited("key map allocation"))?;
                for _ in 0..n {
                    let key = self.value()?;
                    let value = self.value()?;
                    values.push((key, value));
                }
                Ok(Value::Map(values))
            }
            6 => Ok(Value::Tag(number, Box::new(self.value()?))),
            7 => match ai {
                20 => Ok(Value::Bool(false)),
                21 => Ok(Value::Bool(true)),
                22 => Ok(Value::Null),
                25..=27 => {
                    // Only the fixed 3/5/9-byte float is delegated; no general Value or
                    // tag decoder is involved, and the existing float codec stays unique.
                    let value: f64 = ciborium::de::from_reader(&self.bytes[start..self.position])
                        .map_err(|_| Error::Invalid("key float"))?;
                    Ok(Value::Float(value))
                }
                _ => Err(Error::Invalid("key simple value")),
            },
            _ => Err(Error::Invalid("key major type")),
        }
    }
}
