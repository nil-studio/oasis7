//! Exact independent active-version and global-target fields. No authority.
use super::incremental_module_instance_fields::ModuleInstallTargetRefV1 as Target;
use std::io::{self, Write};
const KEY: &[u8] = b"oasis7.iws.module-selection-key.v1\0";
const VALUE: &[u8] = b"oasis7.iws.module-selection-value.v1\0";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ModuleSelectionFieldV1 {
    ActiveVersion = 1,
    Target = 2,
}
impl ModuleSelectionFieldV1 {
    fn from_tag(tag: u8) -> Result<Self, &'static str> {
        match tag {
            1 => Ok(Self::ActiveVersion),
            2 => Ok(Self::Target),
            _ => Err("module selection field"),
        }
    }
}
pub struct ModuleSelectionKeyV1 {
    bytes: [u8; 1024],
    length: usize,
}
impl ModuleSelectionKeyV1 {
    pub fn new(module_id: &str, field: ModuleSelectionFieldV1) -> Result<Self, &'static str> {
        if module_id.len() > 1024 - KEY.len() - 3 {
            return Err("module selection key length");
        }
        let mut bytes = [0; 1024];
        bytes[..KEY.len()].copy_from_slice(KEY);
        bytes[KEY.len()] = field as u8;
        bytes[KEY.len() + 1..KEY.len() + 3]
            .copy_from_slice(&(module_id.len() as u16).to_be_bytes());
        let length = KEY.len() + 3 + module_id.len();
        bytes[KEY.len() + 3..length].copy_from_slice(module_id.as_bytes());
        Ok(Self { bytes, length })
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<(&str, ModuleSelectionFieldV1), &'static str> {
        if bytes.len() > 1024 {
            return Err("module selection key length");
        }
        let bytes = bytes
            .strip_prefix(KEY)
            .ok_or("module selection key domain")?;
        if bytes.len() < 3 {
            return Err("module selection key framing");
        }
        let field = ModuleSelectionFieldV1::from_tag(bytes[0])?;
        let length = u16::from_be_bytes(bytes[1..3].try_into().unwrap()) as usize;
        if bytes.len() != length + 3 {
            return Err("module selection key framing");
        }
        Ok((
            std::str::from_utf8(&bytes[3..]).map_err(|_| "module selection key UTF-8")?,
            field,
        ))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleSelectionScalarRefV1<'a> {
    ActiveVersion(&'a str),
    Target(Target<'a>),
}
impl ModuleSelectionScalarRefV1<'_> {
    pub fn field(self) -> ModuleSelectionFieldV1 {
        match self {
            Self::ActiveVersion(_) => ModuleSelectionFieldV1::ActiveVersion,
            Self::Target(_) => ModuleSelectionFieldV1::Target,
        }
    }
    pub fn encoded_len(self) -> Result<u64, &'static str> {
        let payload = match self {
            Self::ActiveVersion(s) => (s.len() as u64).checked_add(8),
            Self::Target(Target::SelfAgent) => Some(1),
            Self::Target(Target::LocationInfrastructure(s)) => (s.len() as u64).checked_add(9),
        };
        payload
            .and_then(|n| n.checked_add(VALUE.len() as u64 + 1))
            .ok_or("module selection scalar length")
    }
    pub fn write_to(self, out: &mut impl Write) -> io::Result<()> {
        out.write_all(VALUE)?;
        out.write_all(&[self.field() as u8])?;
        let text = |out: &mut dyn Write, s: &str| {
            out.write_all(&(s.len() as u64).to_be_bytes())?;
            out.write_all(s.as_bytes())
        };
        match self {
            Self::ActiveVersion(s) => text(out, s),
            Self::Target(Target::SelfAgent) => out.write_all(&[0]),
            Self::Target(Target::LocationInfrastructure(s)) => {
                out.write_all(&[1])?;
                text(out, s)
            }
        }
    }
}
impl<'a> ModuleSelectionScalarRefV1<'a> {
    pub fn from_canonical_bytes(
        bytes: &'a [u8],
        field: ModuleSelectionFieldV1,
    ) -> Result<Self, &'static str> {
        let mut bytes = bytes
            .strip_prefix(VALUE)
            .ok_or("module selection value domain")?;
        if take(&mut bytes, 1)?[0] != field as u8 {
            return Err("module selection value field");
        }
        let value = match field {
            ModuleSelectionFieldV1::ActiveVersion => Self::ActiveVersion(text(&mut bytes)?),
            ModuleSelectionFieldV1::Target => Self::Target(match take(&mut bytes, 1)?[0] {
                0 => Target::SelfAgent,
                1 => Target::LocationInfrastructure(text(&mut bytes)?),
                _ => return Err("module selection target tag"),
            }),
        };
        if !bytes.is_empty() {
            return Err("module selection value trailing");
        }
        Ok(value)
    }
}
fn take<'a>(bytes: &mut &'a [u8], length: usize) -> Result<&'a [u8], &'static str> {
    let (head, tail) = bytes
        .split_at_checked(length)
        .ok_or("module selection value truncated")?;
    *bytes = tail;
    Ok(head)
}
fn text<'a>(bytes: &mut &'a [u8]) -> Result<&'a str, &'static str> {
    let length = usize::try_from(u64::from_be_bytes(take(bytes, 8)?.try_into().unwrap()))
        .map_err(|_| "module selection text length")?;
    std::str::from_utf8(take(bytes, length)?).map_err(|_| "module selection text UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_wire_preserves_empty_and_raw_text_and_rejects_malformed_fields() {
        assert_eq!(
            ModuleSelectionKeyV1::new("m", ModuleSelectionFieldV1::ActiveVersion)
                .unwrap()
                .as_bytes(),
            b"oasis7.iws.module-selection-key.v1\0\x01\x00\x01m"
        );
        let mut frozen = Vec::new();
        ModuleSelectionScalarRefV1::ActiveVersion("v")
            .write_to(&mut frozen)
            .unwrap();
        assert_eq!(
            frozen,
            b"oasis7.iws.module-selection-value.v1\0\x01\x00\x00\x00\x00\x00\x00\x00\x01v"
        );
        for id in ["", "m@x", " 雪\0 "] {
            for field in [
                ModuleSelectionFieldV1::ActiveVersion,
                ModuleSelectionFieldV1::Target,
            ] {
                let key = ModuleSelectionKeyV1::new(id, field).unwrap();
                assert_eq!(
                    ModuleSelectionKeyV1::from_bytes(key.as_bytes()).unwrap(),
                    (id, field)
                );
                let mut bad = key.as_bytes().to_vec();
                bad.push(0);
                assert!(ModuleSelectionKeyV1::from_bytes(&bad).is_err());
            }
        }
        let max = "x".repeat(1024 - KEY.len() - 3);
        assert_eq!(
            ModuleSelectionKeyV1::new(&max, ModuleSelectionFieldV1::Target)
                .unwrap()
                .as_bytes()
                .len(),
            1024
        );
        assert!(ModuleSelectionKeyV1::new(&(max + "x"), ModuleSelectionFieldV1::Target).is_err());
        for scalar in [
            ModuleSelectionScalarRefV1::ActiveVersion(""),
            ModuleSelectionScalarRefV1::ActiveVersion(" 2@雪\0 "),
            ModuleSelectionScalarRefV1::Target(Target::SelfAgent),
            ModuleSelectionScalarRefV1::Target(Target::LocationInfrastructure("")),
            ModuleSelectionScalarRefV1::Target(Target::LocationInfrastructure(" place 雪 ")),
        ] {
            let mut bytes = vec![];
            scalar.write_to(&mut bytes).unwrap();
            assert_eq!(bytes.len() as u64, scalar.encoded_len().unwrap());
            assert_eq!(
                ModuleSelectionScalarRefV1::from_canonical_bytes(&bytes, scalar.field()).unwrap(),
                scalar
            );
            let other = match scalar.field() {
                ModuleSelectionFieldV1::ActiveVersion => ModuleSelectionFieldV1::Target,
                _ => ModuleSelectionFieldV1::ActiveVersion,
            };
            assert!(ModuleSelectionScalarRefV1::from_canonical_bytes(&bytes, other).is_err());
            for length in 0..bytes.len() {
                assert!(
                    ModuleSelectionScalarRefV1::from_canonical_bytes(
                        &bytes[..length],
                        scalar.field()
                    )
                    .is_err()
                );
            }
            bytes.push(0);
            assert!(
                ModuleSelectionScalarRefV1::from_canonical_bytes(&bytes, scalar.field()).is_err()
            );
        }
        let mut bad = VALUE.to_vec();
        bad.extend([2, 9]);
        assert!(
            ModuleSelectionScalarRefV1::from_canonical_bytes(&bad, ModuleSelectionFieldV1::Target)
                .is_err()
        );
        let mut bad = VALUE.to_vec();
        bad.push(1);
        bad.extend(1u64.to_be_bytes());
        bad.push(255);
        assert!(
            ModuleSelectionScalarRefV1::from_canonical_bytes(
                &bad,
                ModuleSelectionFieldV1::ActiveVersion
            )
            .is_err()
        );
    }
}
