//! Narrow host capability for development source compilation.
//! Implementations may perform IO; callers retain admission and deployment authority.
use crate::world_action::ModuleSourcePackage;
use std::{fmt, io};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleSourceCompileError {
    ModuleChangeInvalid { reason: String },
    Io(String),
    Unavailable(String),
}
impl From<io::Error> for ModuleSourceCompileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
impl fmt::Display for ModuleSourceCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(reason) => f.write_str(reason),
            _ => write!(f, "{self:?}"),
        }
    }
}
impl std::error::Error for ModuleSourceCompileError {}

pub trait ModuleSourceCompiler: Send + Sync {
    fn compile(
        &self,
        module_id: &str,
        package: &ModuleSourcePackage,
    ) -> Result<Vec<u8>, ModuleSourceCompileError>;
}
