//! Typed-first complete-tree decoder for signed Read Directory originals.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
fn rejected() -> String {
    "read directory JSON rejected".into()
}
pub(super) fn decode<T: DeserializeOwned + Serialize>(
    raw: &[u8],
    limit: usize,
) -> Result<T, String> {
    if raw.is_empty() || raw.len() > limit {
        return Err(rejected());
    }
    let typed: T = serde_json::from_slice(raw).map_err(|_| rejected())?;
    let original: UniqueJson = serde_json::from_slice(raw).map_err(|_| rejected())?;
    if serde_json::to_value(&typed).map_err(|_| rejected())? != original.0 {
        return Err(rejected());
    }
    Ok(typed)
}
struct UniqueJson(serde_json::Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique JSON object keys")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                self.visit_unit()
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| UniqueJson(serde_json::Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite JSON"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                let mut v = Vec::new();
                while let Some(n) = a.next_element::<UniqueJson>()? {
                    v.push(n.0);
                }
                Ok(UniqueJson(v.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                let mut m = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if m.contains_key(&k) {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                    m.insert(k, a.next_value::<UniqueJson>()?.0);
                }
                Ok(UniqueJson(serde_json::Value::Object(m)))
            }
        }
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        version: u8,
        original: serde_json::Value,
    }
    #[test]
    fn exact_tree_accepts_order_and_rejects_lossy_originals() {
        let good = br#"{"original":{"items":[{"x":1}]},"version":3}"#;
        let parsed = decode::<Envelope>(good, good.len()).unwrap();
        assert_eq!(parsed.version, 3);
        for raw in [
            br#"{"original":{"x":1,"x":2},"version":3}"#.as_slice(),
            br#"{"original":[{"x":1,"x":2}],"version":3}"#.as_slice(),
            br#"{"original":{},"version":3,"version":3}"#.as_slice(),
            br#"{"original":{},"version":3,"unknown":1}"#.as_slice(),
            br#"{"original":{},"version":3} null"#.as_slice(),
        ] {
            assert!(decode::<Envelope>(raw, 32768).is_err());
        }
        assert!(decode::<Envelope>(good, good.len() - 1).is_err());
        assert!(decode::<Envelope>(b"", 32768).is_err());
    }
}
