//! V2 priorities retain checked i128 arithmetic but use canonical decimal text
//! on the wire. Serde's buffered tagged-enum decoder cannot decode i128, even
//! when a bare CBOR journal can. V1 encoding is intentionally independent.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{Error, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserializer, Serializer};

pub(super) fn serialize<S: Serializer>(
    values: &BTreeMap<String, i128>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(values.len()))?;
    for (key, value) in values {
        map.serialize_entry(key, &value.to_string())?;
    }
    map.end()
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, i128>, D::Error> {
    struct Priorities;
    impl<'de> Visitor<'de> for Priorities {
        type Value = BTreeMap<String, i128>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("unique priority keys with canonical decimal i128 strings")
        }

        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, encoded)) = map.next_entry::<String, String>()? {
                if result.len() >= super::BFT_MAX_VALIDATORS_PER_TRANSITION_V2 {
                    return Err(M::Error::custom("V2 priority map exceeds validator bound"));
                }
                if encoded.len() > 40 {
                    return Err(M::Error::custom("V2 priority exceeds i128 decimal bound"));
                }
                let value: i128 = encoded
                    .parse()
                    .map_err(|_| M::Error::custom("invalid V2 decimal priority"))?;
                if value.to_string() != encoded {
                    return Err(M::Error::custom("noncanonical V2 decimal priority"));
                }
                if result.insert(key, value).is_some() {
                    return Err(M::Error::custom("duplicate V2 priority key"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Priorities)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    #[serde(tag = "kind", content = "body")]
    enum Wire {
        Priority {
            #[serde(with = "super")]
            values: BTreeMap<String, i128>,
        },
    }

    #[test]
    fn full_i128_priorities_round_trip_through_tagged_canonical_cbor() {
        let values = BTreeMap::from([
            ("min".into(), i128::MIN),
            ("max".into(), i128::MAX),
            ("negative".into(), -1),
            ("zero".into(), 0),
            ("above_u64".into(), i128::from(u64::MAX) + 1),
        ]);
        let original = Wire::Priority { values };
        let bytes = oasis7_cas::to_canonical_cbor(&original).unwrap();
        let restored: Wire = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(restored, original);
        assert_eq!(oasis7_cas::to_canonical_cbor(&restored).unwrap(), bytes);
    }

    #[test]
    fn malformed_or_noncanonical_priorities_are_rejected() {
        for value in [
            "+1",
            "01",
            "-0",
            " 1",
            "1 ",
            "",
            "1.0",
            "1e1",
            "170141183460469231731687303715884105728",
            "-170141183460469231731687303715884105729",
        ] {
            let encoded = serde_json::json!({"kind":"Priority","body":{"values":{"key":value}}});
            assert!(
                serde_json::from_value::<Wire>(encoded).is_err(),
                "{value:?}"
            );
        }
        assert!(
            serde_json::from_str::<Wire>(r#"{"kind":"Priority","body":{"values":{"key":1}}}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<Wire>(
                r#"{"kind":"Priority","body":{"values":{"key":"1","key":"2"}}}"#
            )
            .is_err()
        );
    }
}
