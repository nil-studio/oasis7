//! Bounded validator for the existing dedicated scalar wire; no alternative encoder.
use super::{ModuleInstanceFieldV1 as Field, ModuleInstanceScalarRefV1 as Scalar, VALUE};
use crate::incremental_module_instance_index::{
    MODULE_INSTANCE_COUNT_KEY_BYTES, ModuleInstanceCountKeyHashV1,
};
use std::io::{self, Write};

pub struct ModuleInstanceScalarStreamV1 {
    field: Field,
    length: u64,
    received: u64,
    header: [u8; 64],
    used: usize,
    header_len: usize,
    text: Option<u64>,
    tail: [u8; 4],
    tail_len: usize,
    hash: Option<ModuleInstanceCountKeyHashV1>,
    failed: bool,
}
impl ModuleInstanceScalarStreamV1 {
    pub fn new(field: Field, length: u64) -> Self {
        let extra = match field {
            Field::Active => 1,
            Field::InstalledAt => 8,
            Field::Target => 1,
            _ => 8,
        };
        Self {
            field,
            length,
            received: 0,
            header: [0; 64],
            used: 0,
            header_len: VALUE.len() + 1 + extra,
            text: None,
            tail: [0; 4],
            tail_len: 0,
            hash: None,
            failed: false,
        }
    }
    fn header(&mut self) -> Result<(), &'static str> {
        if !self.header[..self.used].starts_with(VALUE)
            || self.header[VALUE.len()] != self.field as u8
        {
            return Err("module scalar stream domain/field");
        }
        if self.field == Field::Target
            && self.header[VALUE.len() + 1] == 1
            && self.header_len == VALUE.len() + 2
        {
            self.header_len += 8;
            return Ok(());
        }
        let text = match self.field {
            Field::Active | Field::InstalledAt => false,
            Field::Target => match self.header[VALUE.len() + 1] {
                0 => false,
                1 => true,
                _ => return Err("module target tag"),
            },
            _ => true,
        };
        if text {
            let n = u64::from_be_bytes(self.header[self.used - 8..self.used].try_into().unwrap());
            if n.checked_add(self.used as u64) != Some(self.length) {
                return Err("module scalar stream length");
            }
            self.text = Some(n);
            if self.field == Field::ModuleId {
                self.hash = Some(ModuleInstanceCountKeyHashV1::new(n));
            }
        } else {
            if self.length != self.used as u64 {
                return Err("module scalar stream fixed length");
            }
            Scalar::from_canonical_bytes(&self.header[..self.used], self.field)?;
        }
        Ok(())
    }
    fn utf8(&mut self, mut bytes: &[u8]) -> Result<(), &'static str> {
        while self.tail_len != 0 && !bytes.is_empty() {
            self.tail[self.tail_len] = bytes[0];
            self.tail_len += 1;
            bytes = &bytes[1..];
            match std::str::from_utf8(&self.tail[..self.tail_len]) {
                Ok(_) => self.tail_len = 0,
                Err(e) if e.error_len().is_none() && self.tail_len < 4 => {}
                Err(_) => return Err("module scalar UTF-8"),
            }
        }
        if bytes.is_empty() {
            return Ok(());
        }
        match std::str::from_utf8(bytes) {
            Ok(_) => Ok(()),
            Err(e) if e.error_len().is_none() => {
                let rest = &bytes[e.valid_up_to()..];
                if rest.len() > 3 {
                    return Err("module scalar UTF-8 tail");
                }
                self.tail[..rest.len()].copy_from_slice(rest);
                self.tail_len = rest.len();
                Ok(())
            }
            Err(_) => Err("module scalar UTF-8"),
        }
    }
    fn accept(&mut self, mut bytes: &[u8]) -> Result<(), &'static str> {
        self.received = self
            .received
            .checked_add(bytes.len() as u64)
            .ok_or("module scalar length overflow")?;
        if self.received > self.length {
            return Err("module scalar excess bytes");
        }
        while self.used < self.header_len && !bytes.is_empty() {
            let n = (self.header_len - self.used).min(bytes.len());
            self.header[self.used..self.used + n].copy_from_slice(&bytes[..n]);
            self.used += n;
            bytes = &bytes[n..];
            if self.used == self.header_len {
                self.header()?;
            }
        }
        if !bytes.is_empty() {
            if self.text.is_none() {
                return Err("module scalar unexpected payload");
            }
            self.utf8(bytes)?;
            if let Some(hash) = &mut self.hash {
                hash.update(bytes)?;
            }
        }
        Ok(())
    }
    pub fn finish(self) -> Result<Option<[u8; MODULE_INSTANCE_COUNT_KEY_BYTES]>, &'static str> {
        if self.failed
            || self.received != self.length
            || self.used != self.header_len
            || self.tail_len != 0
        {
            return Err("module scalar incomplete/invalid stream");
        }
        self.hash
            .map(ModuleInstanceCountKeyHashV1::finish)
            .transpose()
    }
}
impl Write for ModuleInstanceScalarStreamV1 {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("module scalar poisoned"));
        }
        if let Err(error) = self.accept(bytes) {
            self.failed = true;
            return Err(io::Error::other(error));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incremental_module_instance_fields::ModuleInstallTargetRefV1 as Target;
    use crate::incremental_module_instance_index::module_instance_count_key;
    #[test]
    fn stream_matches_original_scalar_decoder_and_count_identity_at_every_boundary() {
        let text = "\0边界🙂e\u{301}".repeat(7);
        for (field, value) in [
            (Field::InstanceId, Scalar::Text("")),
            (Field::ModuleId, Scalar::Text(&text)),
            (Field::Version, Scalar::Text(&text)),
            (Field::WasmHash, Scalar::Text("not normalized")),
            (Field::Owner, Scalar::Text(" owner ")),
            (Field::Target, Scalar::Target(Target::SelfAgent)),
            (
                Field::Target,
                Scalar::Target(Target::LocationInfrastructure(&text)),
            ),
            (Field::Active, Scalar::Bool(false)),
            (Field::Active, Scalar::Bool(true)),
            (Field::InstalledAt, Scalar::U64(u64::MAX)),
        ] {
            let mut original = Vec::new();
            value.write_to(field, &mut original).unwrap();
            assert_eq!(
                Scalar::from_canonical_bytes(&original, field).unwrap(),
                value
            );
            assert_eq!(value.encoded_len(field).unwrap(), original.len() as u64);
            for chunk in 1..=original.len() {
                let mut stream = ModuleInstanceScalarStreamV1::new(field, original.len() as u64);
                for bytes in original.chunks(chunk) {
                    stream.write_all(bytes).unwrap();
                }
                let key = stream.finish().unwrap();
                assert_eq!(
                    key,
                    (field == Field::ModuleId).then(|| module_instance_count_key(&text))
                );
                if let Some(key) = key {
                    // Independent original whole-string hash recipe; this is
                    // intentionally not the new incremental hash helper.
                    let mut hash = blake3::Hasher::new();
                    hash.update(b"oasis7.iws.module-instance-count-id.v1\0");
                    hash.update(&(text.len() as u64).to_be_bytes());
                    hash.update(text.as_bytes());
                    let mut expected = b"oasis7.iws.module-instance-count-key.v1\0".to_vec();
                    expected.extend_from_slice(hash.finalize().as_bytes());
                    assert_eq!(key.as_slice(), expected);
                }
            }
            for length in 0..original.len() {
                let mut stream = ModuleInstanceScalarStreamV1::new(field, original.len() as u64);
                let _ = stream.write_all(&original[..length]);
                assert!(stream.finish().is_err());
            }
        }
    }
    #[test]
    fn stream_rejects_original_invalid_types_utf8_lengths_and_poisoned_retry() {
        let mut original = Vec::new();
        Scalar::Text("é🙂")
            .write_to(Field::ModuleId, &mut original)
            .unwrap();
        let mut invalids = Vec::new();
        let mut bytes = original.clone();
        bytes[0] ^= 1;
        invalids.push(bytes);
        let mut bytes = original.clone();
        bytes[VALUE.len()] = Field::Version as u8;
        invalids.push(bytes);
        let mut bytes = original.clone();
        *bytes.last_mut().unwrap() = 0xff;
        invalids.push(bytes);
        let mut bytes = original.clone();
        bytes.push(0);
        invalids.push(bytes);
        let mut bytes = original.clone();
        bytes[VALUE.len() + 1] = 0xff;
        invalids.push(bytes);
        for bytes in invalids {
            assert!(Scalar::from_canonical_bytes(&bytes, Field::ModuleId).is_err());
            for chunk in [1, 2, 3, 17, 128] {
                let mut stream =
                    ModuleInstanceScalarStreamV1::new(Field::ModuleId, bytes.len() as u64);
                for part in bytes.chunks(chunk) {
                    if stream.write_all(part).is_err() {
                        break;
                    }
                }
                assert!(stream.finish().is_err());
            }
        }
        let mut bytes = Vec::new();
        Scalar::Bool(true)
            .write_to(Field::Active, &mut bytes)
            .unwrap();
        *bytes.last_mut().unwrap() = 2;
        let mut stream = ModuleInstanceScalarStreamV1::new(Field::Active, bytes.len() as u64);
        assert!(stream.write_all(&bytes).is_err());
        assert!(stream.write(&[]).is_err());
        assert!(stream.finish().is_err());
    }
}
