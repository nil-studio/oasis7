//! Header offsets derived through the existing canonical encoder, not a second
//! CBOR implementation. Only a canonical unsigned header's major type changes.
use super::*;

pub(super) struct Header {
    pub prefix: Vec<u8>,
    pub suffix: Vec<u8>,
}
pub(super) fn header(
    kind: CognitionHeadHashV1,
    count: u64,
    seq: u64,
) -> Result<Header, ValueTreeError> {
    let mut array = oasis7_wasm_abi::encode_canonical_cbor(&count).map_err(|_| invalid())?;
    if array.is_empty() || array[0] >> 5 != 0 {
        return Err(invalid());
    }
    array[0] |= 0x80;
    let (mut prefix, suffix) = if kind == CognitionHeadHashV1::Journal {
        let empty = serde_json::json!({"head_seq":seq,"events":[]});
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(kind.domain(), empty))
            .map_err(|_| invalid())?;
        let key = oasis7_wasm_abi::encode_canonical_cbor(&"events").map_err(|_| invalid())?;
        let mut matches = bytes
            .windows(key.len() + 1)
            .enumerate()
            .filter(|(_, part)| &part[..key.len()] == key.as_slice() && part[key.len()] == 0x80);
        let position = matches.next().ok_or_else(invalid)?.0 + key.len();
        if matches.next().is_some() {
            return Err(invalid());
        }
        (bytes[..position].to_vec(), bytes[position + 1..].to_vec())
    } else {
        let empty: &[serde_json::Value] = &[];
        let mut bytes = oasis7_wasm_abi::encode_canonical_cbor(&(kind.domain(), (seq, empty)))
            .map_err(|_| invalid())?;
        if bytes.pop() != Some(0x80) {
            return Err(invalid());
        }
        (bytes, Vec::new())
    };
    prefix.extend_from_slice(&array);
    if prefix.len() > HEADER_MAX || suffix.len() > HEADER_MAX {
        return Err(invalid());
    }
    Ok(Header { prefix, suffix })
}

pub(super) fn initialize(data: &mut Data) -> Result<(), ValueTreeError> {
    let representatives = [0, 24, 256, 65_536, 1_u64 << 32];
    let mut next = 0;
    for kind in CognitionHeadHashV1::ALL {
        let mut lengths = [0_usize; 25];
        let mut count = 0;
        for records in representatives {
            for seq in representatives {
                let length = header(kind, records, seq)?.prefix.len();
                if !lengths[..count].contains(&length) {
                    lengths[count] = length;
                    count += 1;
                }
            }
        }
        lengths[..count].sort_unstable();
        if count
            != if kind == CognitionHeadHashV1::Journal {
                5
            } else {
                12
            }
        {
            return Err(invalid());
        }
        for &length in &lengths[..count] {
            data.rows[next].kind = kind;
            data.rows[next].prefix_len = length as u16;
            next += 1;
        }
    }
    if next != ALIGNMENTS {
        return Err(invalid());
    }
    Ok(())
}
