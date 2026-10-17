//! Independently checksummed recipe descriptors preserve bounded range reads.
use super::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecipeEnvelope {
    #[serde(
        serialize_with = "super::span::serialize_body",
        deserialize_with = "super::span::deserialize_body"
    )]
    body: Vec<u8>,
    checksum: String,
}
pub(super) fn encode_recipe(recipe: &Recipe) -> Result<Vec<u8>, FormalError> {
    let body = serde_cbor::to_vec(recipe).map_err(|e| invalid(e.to_string()))?;
    let checksum = blake3::hash(&body).to_hex().to_string();
    serde_cbor::to_vec(&RecipeEnvelope { body, checksum }).map_err(|e| invalid(e.to_string()))
}
pub(super) fn decode_recipe(bytes: &[u8]) -> Result<Recipe, FormalError> {
    let envelope: RecipeEnvelope =
        serde_cbor::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
    if blake3::hash(&envelope.body).to_hex().as_str() != envelope.checksum
        || serde_cbor::to_vec(&envelope).map_err(|e| invalid(e.to_string()))? != bytes
    {
        return Err(invalid(
            "physical recipe descriptor checksum/canonical encoding",
        ));
    }
    let recipe: Recipe = serde_cbor::from_slice(&envelope.body)
        .map_err(|e| invalid(format!("physical recipe decode: {e}")))?;
    if serde_cbor::to_vec(&recipe).map_err(io)? != envelope.body {
        return Err(invalid("physical recipe inner canonical encoding"));
    }
    if recipe.version != 1
        || recipe.chunker != 1
        || recipe.length > LIMIT as u64
        || recipe.objects.is_empty()
        || recipe.objects.len() > MAX_OBJECTS
        || recipe.frames.len() > LIMIT / 4096 + 1
    {
        return Err(invalid("physical recipe version/budget"));
    }
    let mut length = 0u64;
    for f in &recipe.frames {
        hash_hex(&f.stored)?;
        hash_hex(&f.original)?;
        if f.length == 0 || f.length as usize > FRAME {
            return Err(invalid("physical frame length"));
        }
        length = length
            .checked_add(f.length as u64)
            .ok_or_else(|| invalid("physical frame overflow"))?;
    }
    if length != recipe.length {
        return Err(invalid("physical frame coverage"));
    }
    let mut previous = None;
    for object in &recipe.objects {
        hash_hex(&object.id)?;
        if previous.is_some_and(|old: &str| old >= object.id.as_str())
            || object
                .start
                .checked_add(object.length)
                .is_none_or(|end| end > length)
        {
            return Err(invalid("physical object index/order/span"));
        }
        previous = Some(object.id.as_str());
    }
    Ok(recipe)
}
const CACHE_BYTES: usize = 12 * 1024 * 1024;
const CACHE_COUNT: usize = 16;
#[derive(Default)]
struct ParsedCache {
    values: std::collections::BTreeMap<[u8; 32], (usize, std::sync::Arc<Recipe>)>,
    order: std::collections::VecDeque<[u8; 32]>,
    bytes: usize,
    #[cfg(test)]
    parses: usize,
}
static PARSED: std::sync::OnceLock<std::sync::Mutex<ParsedCache>> = std::sync::OnceLock::new();
impl ParsedCache {
    fn decode(&mut self, bytes: &[u8]) -> Result<std::sync::Arc<Recipe>, FormalError> {
        if bytes.len() > META_LIMIT {
            return Err(invalid("physical recipe cache input bound"));
        }
        let digest = *blake3::hash(bytes).as_bytes();
        if let Some((_, recipe)) = self.values.get(&digest) {
            return Ok(recipe.clone());
        }
        let recipe = std::sync::Arc::new(decode_recipe(bytes)?);
        let weight = bytes.len()
            + std::mem::size_of::<Recipe>()
            + 256
            + recipe.objects.capacity() * std::mem::size_of::<Span>()
            + recipe
                .objects
                .iter()
                .map(|object| object.id.capacity())
                .sum::<usize>()
            + recipe.frames.capacity() * std::mem::size_of::<Frame>()
            + recipe
                .frames
                .iter()
                .map(|frame| frame.stored.capacity() + frame.original.capacity())
                .sum::<usize>();
        #[cfg(test)]
        {
            self.parses += 1;
        }
        if weight <= CACHE_BYTES {
            while self.values.len() >= CACHE_COUNT || self.bytes + weight > CACHE_BYTES {
                let Some(old) = self.order.pop_front() else {
                    return Ok(recipe);
                };
                if let Some((size, _)) = self.values.remove(&old) {
                    self.bytes -= size;
                }
            }
            self.bytes += weight;
            self.order.push_back(digest);
            self.values.insert(digest, (weight, recipe.clone()));
        }
        Ok(recipe)
    }
}
/// Callers must first read the current no-follow file. Only that complete fresh
/// byte digest can reuse a prior canonical parse; no path/inode/mtime is trusted.
/// This process-wide cache has fixed decoded-allocation and entry budgets.
pub(super) fn decode_recipe_cached(bytes: &[u8]) -> Result<std::sync::Arc<Recipe>, FormalError> {
    PARSED
        .get_or_init(|| std::sync::Mutex::new(ParsedCache::default()))
        .lock()
        .map_err(|_| invalid("physical parsed recipe cache poisoned"))?
        .decode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_tuple_index_does_not_expand_hashes_or_metadata_into_integer_arrays() {
        let recipe = Recipe {
            version: 1,
            chunker: 1,
            length: 0,
            objects: (0..1024)
                .map(|i| Span {
                    id: format!("{i:064x}"),
                    start: 0,
                    length: 0,
                })
                .collect(),
            frames: vec![],
        };
        let encoded = encode_recipe(&recipe).unwrap();
        assert!(encoded.len() < 44 * 1024 + 2048, "{}", encoded.len());
        assert_eq!(decode_recipe(&encoded).unwrap().objects.len(), 1024);
    }
    #[test]
    fn hostile_declared_index_count_and_metadata_bitrot_are_rejected() {
        let body=hex::decode("a56776657273696f6e01676368756e6b657201666c656e67746801676f626a656374739bffffffffffffffff666672616d657380").unwrap();
        let checksum = blake3::hash(&body).to_hex().to_string();
        let bytes = serde_cbor::to_vec(&RecipeEnvelope { body, checksum }).unwrap();
        assert!(decode_recipe(&bytes).is_err());
        let recipe = Recipe {
            version: 1,
            chunker: 1,
            length: 0,
            objects: vec![Span {
                id: "0".repeat(64),
                start: 0,
                length: 0,
            }],
            frames: vec![],
        };
        let mut bytes = encode_recipe(&recipe).unwrap();
        let index = bytes.len() / 2;
        bytes[index] ^= 1;
        assert!(decode_recipe(&bytes).is_err());
    }
    #[test]
    fn checksummed_nonminimal_inner_integer_is_rejected() {
        let recipe = Recipe {
            version: 1,
            chunker: 1,
            length: 0,
            objects: vec![Span {
                id: "0".repeat(64),
                start: 0,
                length: 0,
            }],
            frames: vec![],
        };
        let mut body = serde_cbor::to_vec(&recipe).unwrap();
        let index = body.windows(7).position(|part| part == b"version").unwrap() + 7;
        assert_eq!(body[index], 1);
        body.splice(index..index + 1, [0x18, 1]);
        let checksum = blake3::hash(&body).to_hex().to_string();
        let bytes = serde_cbor::to_vec(&RecipeEnvelope { body, checksum }).unwrap();
        assert!(decode_recipe(&bytes).is_err());
    }
    #[test]
    fn parsed_recipe_cache_is_content_bound_and_fixed_capacity() {
        let mut cache = ParsedCache::default();
        for index in 0u32..40 {
            let recipe = Recipe {
                version: 1,
                chunker: 1,
                length: 0,
                objects: vec![Span {
                    id: format!("{index:064x}"),
                    start: 0,
                    length: 0,
                }],
                frames: vec![],
            };
            let bytes = encode_recipe(&recipe).unwrap();
            cache.decode(&bytes).unwrap();
            cache.decode(&bytes).unwrap();
        }
        assert!(cache.values.len() <= CACHE_COUNT);
        assert!(cache.bytes <= CACHE_BYTES);
        assert_eq!(cache.parses, 40);
    }
}
