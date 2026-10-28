//! First typed lowering of agents into independent IWS subfields.
//! The body leaf excludes mailbox, intent/receipt and all other agent state.
use crate::{BodyKernelView, WorldTime};

const KEY_DOMAIN: &[u8] = b"oasis7.iws.agent-body-key.v1\0";
const VALUE_DOMAIN: &[u8] = b"oasis7.iws.agent-body-value.v1\0";
pub const AGENT_BODY_VALUE_BYTES: usize = VALUE_DOMAIN.len() + 40;

/// Stack-bounded key. The length prefix distinguishes arbitrary UTF-8 IDs;
/// the domain fixes owner WorldState / agents / body, Business namespace.
pub struct AgentBodyKeyV1 {
    bytes: [u8; 1024],
    length: usize,
}
impl AgentBodyKeyV1 {
    pub fn new(agent_id: &str) -> Result<Self, &'static str> {
        let length = KEY_DOMAIN.len() + 2 + agent_id.len();
        if agent_id.is_empty() || length > 1024 {
            return Err("agent body key length");
        }
        let mut key = Self {
            bytes: [0; 1024],
            length,
        };
        key.bytes[..KEY_DOMAIN.len()].copy_from_slice(KEY_DOMAIN);
        key.bytes[KEY_DOMAIN.len()..KEY_DOMAIN.len() + 2]
            .copy_from_slice(&(agent_id.len() as u16).to_be_bytes());
        key.bytes[KEY_DOMAIN.len() + 2..length].copy_from_slice(agent_id.as_bytes());
        Ok(key)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentBodyValueV1 {
    pub body_view: BodyKernelView,
    pub last_active: WorldTime,
}
impl AgentBodyValueV1 {
    pub fn canonical_bytes(&self) -> [u8; AGENT_BODY_VALUE_BYTES] {
        // Exhaustive destructuring makes added body fields fail compilation.
        let BodyKernelView {
            mass_kg,
            radius_cm,
            thrust_limit,
            cross_section_cm2,
        } = self.body_view;
        let mut bytes = [0; AGENT_BODY_VALUE_BYTES];
        bytes[..VALUE_DOMAIN.len()].copy_from_slice(VALUE_DOMAIN);
        for (out, value) in bytes[VALUE_DOMAIN.len()..].chunks_exact_mut(8).zip([
            mass_kg,
            radius_cm,
            thrust_limit,
            cross_section_cm2,
            self.last_active,
        ]) {
            out.copy_from_slice(&value.to_be_bytes());
        }
        bytes
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != AGENT_BODY_VALUE_BYTES || !bytes.starts_with(VALUE_DOMAIN) {
            return Err("agent body value domain/length");
        }
        let mut values = [0; 5];
        for (value, encoded) in values
            .iter_mut()
            .zip(bytes[VALUE_DOMAIN.len()..].chunks_exact(8))
        {
            *value = u64::from_be_bytes(encoded.try_into().map_err(|_| "agent body scalar")?);
        }
        Ok(Self {
            body_view: BodyKernelView {
                mass_kg: values[0],
                radius_cm: values[1],
                thrust_limit: values[2],
                cross_section_cm2: values[3],
            },
            last_active: values[4],
        })
    }
}

/// Scalar clock lowering, separate from simulation tick and commit height.
pub const WORLD_TIME_KEY_V1: &[u8] = b"oasis7.iws.world-time-key.v1\0";
pub fn world_time_bytes_v1(time: WorldTime) -> [u8; b"oasis7.iws.world-time-value.v1\0".len() + 8] {
    const DOMAIN: &[u8] = b"oasis7.iws.world-time-value.v1\0";
    let mut bytes = [0; b"oasis7.iws.world-time-value.v1\0".len() + 8];
    bytes[..DOMAIN.len()].copy_from_slice(DOMAIN);
    bytes[DOMAIN.len()..].copy_from_slice(&time.to_be_bytes());
    bytes
}
