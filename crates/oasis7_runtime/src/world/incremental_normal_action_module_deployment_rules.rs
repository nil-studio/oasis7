//! Original compile/deploy business order, shared by Legacy and Current owners.
use crate::world::World;
use crate::{Action, ModuleSourcePackage, RejectReason, WorldError};
use oasis7_agent_api::world_action::ResourceKind;
pub(in crate::world) mod sealed {
    pub trait Sealed {}
}
pub(in crate::world) trait ModuleDeploymentFactsV1: sealed::Sealed {
    type Error;
    type Compiled;
    fn reserve(&mut self, action: &Action) -> Result<(), Self::Error>;
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn source_compile_allowed(&mut self) -> Result<bool, Self::Error>;
    fn source_bytes(&mut self, package: &ModuleSourcePackage) -> Result<usize, Self::Error>;
    fn compile(
        &mut self,
        id: &str,
        package: &ModuleSourcePackage,
    ) -> Result<Result<Self::Compiled, WorldError>, Self::Error>;
    fn compiler_error(&mut self, error: &WorldError) -> Result<(), Self::Error>;
    fn compiled_bytes(output: &Self::Compiled) -> &[u8];
    fn hash(&mut self, bytes: &[u8]) -> Result<String, Self::Error>;
    fn balance(&mut self, id: &str, kind: ResourceKind) -> Result<i64, Self::Error>;
}
pub(in crate::world) enum DeploymentBytesV1<'a, B> {
    Borrowed(&'a [u8]),
    Compiled(B),
}
impl<B> DeploymentBytesV1<'_, B> {
    pub(in crate::world) fn bytes<S: ModuleDeploymentFactsV1<Compiled = B>>(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Compiled(bytes) => S::compiled_bytes(bytes),
        }
    }
}
pub(in crate::world) enum ModuleDeploymentDecisionV1<'a, B> {
    Rejected(RejectReason),
    Ready {
        publisher: &'a str,
        hash: String,
        bytes: DeploymentBytesV1<'a, B>,
        fee: i64,
    },
}
fn denied<B>(note: String) -> ModuleDeploymentDecisionV1<'static, B> {
    ModuleDeploymentDecisionV1::Rejected(RejectReason::RuleDenied { notes: vec![note] })
}
pub(in crate::world) fn evaluate_module_deployment_v1<'a, S: ModuleDeploymentFactsV1>(
    source: &mut S,
    action: &'a Action,
) -> Result<ModuleDeploymentDecisionV1<'a, S::Compiled>, S::Error> {
    source.reserve(action)?;
    let (publisher, bytes, hash, fee) = match action {
        Action::CompileModuleArtifactFromSource {
            publisher_agent_id,
            module_id,
            source_package,
        } => {
            if !source.agent_exists(publisher_agent_id)? {
                return Ok(ModuleDeploymentDecisionV1::Rejected(
                    RejectReason::AgentNotFound {
                        agent_id: publisher_agent_id.clone(),
                    },
                ));
            }
            if module_id.trim().is_empty() {
                return Ok(denied(
                    "compile module source rejected: module_id is empty".into(),
                ));
            }
            if !source.source_compile_allowed()? {
                return Ok(denied("compile module source rejected: runtime source compile is disabled by production release policy; use external Docker builder and deploy binary + receipt".into()));
            }
            let source_bytes = source.source_bytes(source_package)?;
            let compiled = match source.compile(module_id, source_package)? {
                Ok(bytes) => bytes,
                Err(error) => {
                    source.compiler_error(&error)?;
                    return Ok(denied(format!("compile module source rejected: {error:?}")));
                }
            };
            let hash = source.hash(S::compiled_bytes(&compiled))?;
            let fee =
                World::module_compile_fee_amount(source_bytes, S::compiled_bytes(&compiled).len());
            (
                publisher_agent_id.as_str(),
                DeploymentBytesV1::Compiled(compiled),
                hash,
                fee,
            )
        }
        Action::DeployModuleArtifact {
            publisher_agent_id,
            wasm_hash,
            wasm_bytes,
        } => {
            if !source.agent_exists(publisher_agent_id)? {
                return Ok(ModuleDeploymentDecisionV1::Rejected(
                    RejectReason::AgentNotFound {
                        agent_id: publisher_agent_id.clone(),
                    },
                ));
            }
            let hash = source.hash(wasm_bytes)?;
            if hash != *wasm_hash {
                return Ok(denied(format!(
                    "deploy module artifact rejected: artifact hash mismatch expected {} found {}",
                    wasm_hash, hash
                )));
            }
            (
                publisher_agent_id.as_str(),
                DeploymentBytesV1::Borrowed(wasm_bytes.as_slice()),
                hash,
                World::module_deploy_fee_amount(wasm_bytes.len()),
            )
        }
        _ => unreachable!("fixed deployment action caller"),
    };
    let available = source.balance(publisher, ResourceKind::Electricity)?;
    if available < fee {
        return Ok(ModuleDeploymentDecisionV1::Rejected(
            RejectReason::InsufficientResource {
                agent_id: publisher.to_owned(),
                kind: ResourceKind::Electricity,
                requested: fee,
                available,
            },
        ));
    }
    Ok(ModuleDeploymentDecisionV1::Ready {
        publisher,
        hash,
        bytes,
        fee,
    })
}
pub(in crate::world) struct LegacyModuleDeploymentFactsV1<'a>(pub &'a World);
impl sealed::Sealed for LegacyModuleDeploymentFactsV1<'_> {}
impl ModuleDeploymentFactsV1 for LegacyModuleDeploymentFactsV1<'_> {
    type Error = WorldError;
    type Compiled = Vec<u8>;
    fn reserve(&mut self, _: &Action) -> Result<(), Self::Error> {
        Ok(())
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.0.state.agents.contains_key(id))
    }
    fn source_compile_allowed(&mut self) -> Result<bool, Self::Error> {
        Ok(self.0.release_security_policy.allow_runtime_source_compile)
    }
    fn source_bytes(&mut self, p: &ModuleSourcePackage) -> Result<usize, Self::Error> {
        Ok(p.files.values().map(Vec::len).sum())
    }
    fn compile(
        &mut self,
        id: &str,
        p: &ModuleSourcePackage,
    ) -> Result<Result<Vec<u8>, WorldError>, Self::Error> {
        Ok(self.0.source_compiler.compile(id, p))
    }
    fn compiler_error(&mut self, _: &WorldError) -> Result<(), Self::Error> {
        Ok(())
    }
    fn compiled_bytes(bytes: &Vec<u8>) -> &[u8] {
        bytes
    }
    fn hash(&mut self, bytes: &[u8]) -> Result<String, Self::Error> {
        Ok(crate::util::sha256_hex(bytes))
    }
    fn balance(&mut self, id: &str, kind: ResourceKind) -> Result<i64, Self::Error> {
        Ok(self
            .0
            .state
            .agents
            .get(id)
            .map(|a| a.state.resources.get(kind))
            .unwrap_or(0))
    }
}
