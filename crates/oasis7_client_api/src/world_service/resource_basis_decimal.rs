use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceDecimalI64V1(pub i64);
impl Serialize for ResourceDecimalI64V1 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for ResourceDecimalI64V1 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        let value = raw
            .parse::<i64>()
            .map_err(|_| D::Error::custom("invalid decimal i64"))?;
        if value.to_string() != raw {
            return Err(D::Error::custom("noncanonical decimal i64"));
        }
        Ok(Self(value))
    }
}

pub(super) fn strict_map<'de, D, K, V>(d: D) -> Result<std::collections::BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct Visitor<K, V>(std::marker::PhantomData<(K, V)>);
    impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>> serde::de::Visitor<'de>
        for Visitor<K, V>
    {
        type Value = std::collections::BTreeMap<K, V>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a typed map without duplicate keys")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut a: A,
        ) -> Result<Self::Value, A::Error> {
            let mut m = std::collections::BTreeMap::new();
            while let Some((k, v)) = a.next_entry::<K, V>()? {
                if m.insert(k, v).is_some() {
                    return Err(A::Error::custom("duplicate resource fact key"));
                }
            }
            Ok(m)
        }
    }
    d.deserialize_map(Visitor(std::marker::PhantomData))
}
pub(super) fn strict_optional_map<'de, D, K, V>(
    d: D,
) -> Result<Option<std::collections::BTreeMap<K, V>>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct Map<K, V>(std::collections::BTreeMap<K, V>);
    impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>> Deserialize<'de> for Map<K, V> {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            strict_map(d).map(Self)
        }
    }
    Option::<Map<K, V>>::deserialize(d).map(|v| v.map(|m| m.0))
}
