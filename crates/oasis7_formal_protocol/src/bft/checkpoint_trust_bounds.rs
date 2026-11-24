//! Allocation-free framing preflight before decoding nested trust/policy DTOs.
const MAX: usize = 2 * 1024 * 1024;
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or("CBOR length overflow")?;
        let slice = self.bytes.get(self.at..end).ok_or("truncated CBOR input")?;
        self.at = end;
        Ok(slice)
    }
    fn head(&mut self) -> Result<(u8, u64), String> {
        let byte = self.take(1)?[0];
        let kind = byte >> 5;
        let additional = byte & 31;
        // Major type 7 uses these bits as a simple value or float marker,
        // not a length. Float payloads must never alias false/true/null.
        if kind == 7 && !matches!(additional, 20..=22) {
            return Err("unsupported checkpoint CBOR simple value or float".into());
        }
        let value = match additional {
            n @ 0..=23 => u64::from(n),
            24 => u64::from(self.take(1)?[0]),
            25 => u64::from(u16::from_be_bytes(self.take(2)?.try_into().unwrap())),
            26 => u64::from(u32::from_be_bytes(self.take(4)?.try_into().unwrap())),
            27 => u64::from_be_bytes(self.take(8)?.try_into().unwrap()),
            _ => return Err("indefinite or reserved CBOR framing".into()),
        };
        let minimum = match additional {
            24 => 24,
            25 => 1 << 8,
            26 => 1 << 16,
            27 => 1 << 32,
            _ => 0,
        };
        if value < minimum {
            return Err("noncanonical checkpoint CBOR head".into());
        }
        Ok((kind, value))
    }
    fn map_count(&mut self) -> Result<u64, String> {
        let (kind, count) = self.head()?;
        if kind != 5 || count > (self.bytes.len() - self.at) as u64 / 2 {
            return Err("invalid checkpoint CBOR map".into());
        }
        Ok(count)
    }
    fn text_key(&mut self) -> Result<&'a [u8], String> {
        let (kind, n) = self.head()?;
        if kind != 3 {
            return Err("checkpoint CBOR key is not text".into());
        }
        self.take(usize::try_from(n).map_err(|_| "CBOR key length overflow")?)
    }
    fn item(&mut self, depth: usize) -> Result<(), String> {
        if depth > 128 {
            return Err("CBOR nesting exceeds decoder bound".into());
        }
        let (kind, n) = self.head()?;
        match kind {
            0 => {}
            2 | 3 => {
                self.take(usize::try_from(n).map_err(|_| "CBOR byte length overflow")?)?;
            }
            4 | 5 => {
                let count = if kind == 5 {
                    n.checked_mul(2).ok_or("CBOR map length overflow")?
                } else {
                    n
                };
                if count > (self.bytes.len() - self.at) as u64 {
                    return Err("CBOR collection length exceeds remaining bytes".into());
                }
                for _ in 0..count {
                    self.item(depth + 1)?;
                }
            }
            6 if n == 55799 => self.item(depth + 1)?,
            7 if matches!(n, 20..=22) => {}
            _ => return Err("unsupported checkpoint CBOR value".into()),
        }
        Ok(())
    }
}

pub fn preflight(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > MAX {
        return Err("checkpoint configuration exceeds bound".into());
    }
    let mut c = Cursor { bytes, at: 0 };
    if c.head()? != (6, 55799) {
        return Err("checkpoint configuration requires self-described CBOR".into());
    }
    let (kind, count) = c.head()?;
    if kind != 5 || count > (bytes.len() - c.at) as u64 / 2 {
        return Err("invalid checkpoint config map".into());
    }
    for _ in 0..count {
        let (kind, n) = c.head()?;
        if kind != 3 {
            return Err("checkpoint config key is not text".into());
        }
        let key = c.take(usize::try_from(n).map_err(|_| "CBOR key length overflow")?)?;
        let start = c.at;
        c.item(0)?;
        if matches!(key, b"formal_read_trust" | b"policy") && c.at - start + 3 > 64 * 1024 {
            return Err("nested trust/policy exceeds 64 KiB".into());
        }
        if key == b"policy" {
            policy_issuer_bounds(&bytes[start..c.at])?;
        }
    }
    if c.at != bytes.len() {
        return Err("trailing checkpoint config bytes".into());
    }
    Ok(())
}

pub(super) fn policy_issuer_bounds(bytes: &[u8]) -> Result<(), String> {
    let mut c = Cursor { bytes, at: 0 };
    let (kind, count) = c.head()?;
    if kind != 5 || count > (bytes.len() - c.at) as u64 / 2 {
        return Err("invalid policy map".into());
    }
    let mut declared = None;
    let mut actual = None;
    for _ in 0..count {
        let (kind, n) = c.head()?;
        if kind != 3 {
            return Err("policy key is not text".into());
        }
        let key = c.take(usize::try_from(n).map_err(|_| "policy key length overflow")?)?;
        match key {
            b"max_issuer_keys" => {
                let (kind, n) = c.head()?;
                if kind != 0 || n == 0 || n > 256 || declared.replace(n).is_some() {
                    return Err("invalid declared issuer capacity".into());
                }
            }
            b"issuer_keys" => {
                let (kind, n) = c.head()?;
                if kind != 4 || n == 0 || n > 256 || actual.replace(n).is_some() {
                    return Err("invalid issuer array bound".into());
                }
                for _ in 0..n {
                    c.item(0)?;
                }
            }
            _ => c.item(0)?,
        }
    }
    match (actual, declared) {
        (Some(n), Some(max)) if n <= max => Ok(()),
        _ => Err("issuer array exceeds declared capacity".into()),
    }
}

pub fn time_observation_bounds(bytes: &[u8], maximum: usize) -> Result<(), String> {
    if bytes.len() > maximum || !bytes.starts_with(&[0xd9, 0xd9, 0xf7]) {
        return Err("invalid bounded checkpoint time observation framing".into());
    }
    let mut c = Cursor { bytes, at: 3 };
    c.item(0)?;
    if c.at != bytes.len() {
        return Err("trailing checkpoint time observation bytes".into());
    }
    Ok(())
}

pub fn issuer_status_bounds(
    bytes: &[u8],
    max_bytes: usize,
    issuer_count: usize,
) -> Result<(), String> {
    if bytes.len() > max_bytes || !bytes.starts_with(&[0xd9, 0xd9, 0xf7]) {
        return Err("invalid bounded checkpoint issuer status framing".into());
    }
    let mut c = Cursor { bytes, at: 3 };
    let mut found_body = false;
    let mut found_issuers = false;
    for _ in 0..c.map_count()? {
        if c.text_key()? != b"body" {
            c.item(0)?;
            continue;
        }
        if found_body {
            return Err("duplicate checkpoint issuer status body".into());
        }
        found_body = true;
        for _ in 0..c.map_count()? {
            if c.text_key()? != b"issuer_keys" {
                c.item(0)?;
                continue;
            }
            if found_issuers || c.head()? != (4, issuer_count as u64) {
                return Err("checkpoint issuer status exceeds exact issuer count".into());
            }
            for _ in 0..issuer_count {
                c.item(0)?;
            }
            found_issuers = true;
        }
    }
    if !found_issuers || c.at != bytes.len() {
        return Err("incomplete checkpoint issuer status framing".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn nested_trust(n: u16) -> Vec<u8> {
        let mut bytes = vec![0xd9, 0xd9, 0xf7, 0xa1, 0x71];
        bytes.extend_from_slice(b"formal_read_trust");
        bytes.push(0x59);
        bytes.extend_from_slice(&n.to_be_bytes());
        bytes.resize(bytes.len() + usize::from(n), 0);
        bytes
    }
    #[test]
    fn nested_bound_is_checked_before_any_dto_decode() {
        // Three bytes for the nested byte-string header and three for its
        // independently canonical self-describe tag count toward the 64 KiB.
        assert!(preflight(&nested_trust(65_530)).is_ok());
        assert!(
            preflight(&nested_trust(65_531))
                .unwrap_err()
                .contains("64 KiB")
        );
    }
    #[test]
    fn rejects_status_issuer_array_bound_before_reading_entries() {
        let mut bytes = vec![0xd9, 0xd9, 0xf7, 0xa1, 0x64];
        bytes.extend_from_slice(b"body");
        bytes.extend_from_slice(&[0xa1, 0x6b]);
        bytes.extend_from_slice(b"issuer_keys");
        bytes.extend_from_slice(&[0x9a, 0xff, 0xff, 0xff, 0xff]);
        assert!(
            issuer_status_bounds(&bytes, 64 * 1024, 1)
                .unwrap_err()
                .contains("exact issuer count")
        );
    }

    #[test]
    fn rejects_noncanonical_heads_and_floats_before_decode() {
        // Canonical false, true, null and unsigned 24 remain admissible.
        for value in [&[0xf4][..], &[0xf5], &[0xf6], &[0x18, 24]] {
            let mut bytes = vec![0xd9, 0xd9, 0xf7];
            bytes.extend_from_slice(value);
            assert!(time_observation_bounds(&bytes, 64).is_ok());
        }
        for value in [
            &[0xf9, 0x00, 0x14][..],
            &[0xf8, 20],
            &[0x18, 23],
            &[0x19, 0, 24],
            &[0x98, 0],
        ] {
            let mut bytes = vec![0xd9, 0xd9, 0xf7];
            bytes.extend_from_slice(value);
            assert!(time_observation_bounds(&bytes, 64).is_err());
        }
    }

    #[test]
    fn rejects_duplicate_status_envelope_body_before_decode() {
        let mut bytes = vec![0xd9, 0xd9, 0xf7, 0xa2, 0x64];
        bytes.extend_from_slice(b"body");
        bytes.extend_from_slice(&[0xa0, 0x64]);
        bytes.extend_from_slice(b"body");
        bytes.extend_from_slice(&[0xa1, 0x6b]);
        bytes.extend_from_slice(b"issuer_keys");
        bytes.extend_from_slice(&[0x81, 0xf6]);
        assert!(
            issuer_status_bounds(&bytes, 64, 1)
                .unwrap_err()
                .contains("duplicate")
        );

        let mut original = vec![0xd9, 0xd9, 0xf7, 0xa1, 0x64];
        original.extend_from_slice(b"body");
        original.extend_from_slice(&[0xa1, 0x6b]);
        original.extend_from_slice(b"issuer_keys");
        original.extend_from_slice(&[0x81, 0xf6]);
        assert!(issuer_status_bounds(&original, 64, 1).is_ok());
    }

    #[test]
    fn rejects_advertised_huge_collection_without_allocation() {
        let bytes = [
            0xd9, 0xd9, 0xf7, 0xbb, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ];
        assert!(preflight(&bytes).is_err());
    }
}
