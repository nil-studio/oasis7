use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootSetV1 {
    pub world_id: String,
    pub genesis_config: [u8; 32],
    pub height: u64,
    pub simulation_tick: u64,
    /// Fixed namespace order: business, allocator, scheduler, authorization,
    /// effects and artifact catalog. None can be omitted by a producer.
    pub state: [StateRootV1; 6],
}

impl RootSetV1 {
    pub fn empty(world_id: String, genesis_config: [u8; 32]) -> Result<Self, ExecutionError> {
        let mut roots = Vec::with_capacity(6);
        for namespace in NAMESPACES {
            roots.push(StateRootV1::empty(namespace).map_err(ExecutionError::Tree)?);
        }
        let value = Self {
            world_id,
            genesis_config,
            height: 0,
            simulation_tick: 0,
            state: roots
                .try_into()
                .map_err(|_| ExecutionError::Invalid("namespace count"))?,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ExecutionError> {
        if self.world_id.is_empty() || self.world_id.len() > 256 {
            return Err(ExecutionError::Invalid("world ID bound"));
        }
        for (namespace, root) in NAMESPACES.iter().zip(&self.state) {
            if *namespace != root.namespace {
                return Err(ExecutionError::Invalid("root namespace order"));
            }
            root.validate().map_err(ExecutionError::Tree)?;
        }
        Ok(())
    }

    pub fn state_root(&self, namespace: StateNamespaceV1) -> &StateRootV1 {
        &self.state[namespace_index(namespace)]
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        self.validate()?;
        let magic = b"oasis7.iws.root-set.v1\0";
        let payload_len =
            2 + self.world_id.len() + 48 + 6 * crate::incremental_state::STATE_ROOT_BYTES;
        let mut bytes = Vec::with_capacity(magic.len() + 4 + payload_len);
        bytes.extend_from_slice(magic);
        bytes.extend_from_slice(&(payload_len as u32).to_be_bytes());
        bytes.extend_from_slice(&(self.world_id.len() as u16).to_be_bytes());
        bytes.extend_from_slice(self.world_id.as_bytes());
        bytes.extend_from_slice(&self.genesis_config);
        bytes.extend_from_slice(&self.height.to_be_bytes());
        bytes.extend_from_slice(&self.simulation_tick.to_be_bytes());
        for root in &self.state {
            bytes.extend_from_slice(&root.canonical_bytes().map_err(ExecutionError::Tree)?);
        }
        Ok(bytes)
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        let mut cursor = codec::Cursor::envelope(bytes, b"oasis7.iws.root-set.v1\0", 1024)?;
        let world_len = usize::from(cursor.u16()?);
        if world_len == 0 || world_len > 256 {
            return Err(ExecutionError::Invalid("world ID length"));
        }
        let world_id = std::str::from_utf8(cursor.bytes(world_len)?)
            .map_err(|_| ExecutionError::Invalid("world ID UTF-8"))?
            .to_owned();
        let genesis_config = cursor.array()?;
        let height = cursor.u64()?;
        let simulation_tick = cursor.u64()?;
        let mut state = Vec::with_capacity(6);
        for _ in 0..6 {
            state.push(
                StateRootV1::from_canonical_bytes(
                    cursor.bytes(crate::incremental_state::STATE_ROOT_BYTES)?,
                )
                .map_err(ExecutionError::Tree)?,
            );
        }
        cursor.finish()?;
        let value = Self {
            world_id,
            genesis_config,
            height,
            simulation_tick,
            state: state
                .try_into()
                .map_err(|_| ExecutionError::Invalid("namespace count"))?,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn digest(&self) -> Result<[u8; 32], ExecutionError> {
        Ok(*blake3::hash(&self.canonical_bytes()?).as_bytes())
    }

    pub(super) fn heap_bytes(&self) -> usize {
        self.world_id.capacity()
            + self
                .state
                .iter()
                .map(|root| root.node_root.capacity())
                .sum::<usize>()
    }
}
