use serde::{Deserialize, Serialize};
use std::fmt;
/// Material ledger identifier used by M4 economy state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(try_from = "String", into = "String")]
pub enum MaterialLedgerId {
    #[default]
    World,
    Agent(String),
    Site(String),
    Factory(String),
}

impl MaterialLedgerId {
    pub fn world() -> Self {
        Self::World
    }

    pub fn agent(agent_id: impl Into<String>) -> Self {
        Self::Agent(agent_id.into())
    }

    pub fn site(site_id: impl Into<String>) -> Self {
        Self::Site(site_id.into())
    }

    pub fn factory(factory_id: impl Into<String>) -> Self {
        Self::Factory(factory_id.into())
    }
}

impl fmt::Display for MaterialLedgerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MaterialLedgerId::World => write!(f, "world"),
            MaterialLedgerId::Agent(agent_id) => write!(f, "agent:{agent_id}"),
            MaterialLedgerId::Site(site_id) => write!(f, "site:{site_id}"),
            MaterialLedgerId::Factory(factory_id) => write!(f, "factory:{factory_id}"),
        }
    }
}

impl From<MaterialLedgerId> for String {
    fn from(value: MaterialLedgerId) -> Self {
        value.to_string()
    }
}

impl TryFrom<String> for MaterialLedgerId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value == "world" {
            return Ok(MaterialLedgerId::World);
        }
        if let Some(rest) = value.strip_prefix("agent:") {
            if rest.trim().is_empty() {
                return Err("agent ledger id cannot be empty".to_string());
            }
            return Ok(MaterialLedgerId::Agent(rest.to_string()));
        }
        if let Some(rest) = value.strip_prefix("site:") {
            if rest.trim().is_empty() {
                return Err("site ledger id cannot be empty".to_string());
            }
            return Ok(MaterialLedgerId::Site(rest.to_string()));
        }
        if let Some(rest) = value.strip_prefix("factory:") {
            if rest.trim().is_empty() {
                return Err("factory ledger id cannot be empty".to_string());
            }
            return Ok(MaterialLedgerId::Factory(rest.to_string()));
        }
        Err(format!("invalid material ledger id: {value}"))
    }
}
