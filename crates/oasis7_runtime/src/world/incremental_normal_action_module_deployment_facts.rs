//! Real Current observations for the shared compile/deploy business kernel.
use super::super::deployment_rules::{self, ModuleDeploymentFactsV1};
use super::*;
impl<M, P, R, V, E, C, PE, PC> deployment_rules::sealed::Sealed
    for CurrentMarketFactsV1<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
{
}
impl<M, P, R, V, E, C, PE, PC> ModuleDeploymentFactsV1
    for CurrentMarketFactsV1<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    type Error = Error;
    type Compiled = ReservedValue<Vec<u8>>;
    fn reserve(&mut self, action: &crate::Action) -> Result<(), Error> {
        let fields: &[&str] = match action {
            crate::Action::CompileModuleArtifactFromSource {
                publisher_agent_id,
                module_id,
                ..
            } => &[publisher_agent_id, module_id],
            crate::Action::DeployModuleArtifact {
                publisher_agent_id,
                wasm_hash,
                ..
            } => &[publisher_agent_id, wasm_hash],
            _ => return Err(invalid().into()),
        };
        let bytes = fields
            .iter()
            .try_fold(0usize, |n, v| n.checked_add(v.len()).ok_or_else(invalid))?
            .checked_mul(16)
            .and_then(|n| n.checked_add(16384))
            .ok_or_else(invalid)?;
        self.pay_clone(bytes)
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Error> {
        ModuleMarketFactsV1::agent_exists(self, id)
    }
    fn balance(&mut self, id: &str, kind: ResourceKind) -> Result<i64, Error> {
        ModuleMarketFactsV1::balance(self, id, kind)
    }
    fn source_compile_allowed(&mut self) -> Result<bool, Error> {
        use oasis7_world_state::incremental_runtime_artifact_config::*;
        let sources = self.sources;
        let live = self.live;
        with_module_lifecycle_read_io_v1(
            sources.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, parent, factory, _| {
                let body = open_artifact_config_body_overlay_v1(
                    ArtifactConfigSelectedBodyRequestV1 {
                        field: ArtifactConfigFieldV1::Security,
                        key: ArtifactConfigRecordKeyV1::Global,
                        family: sources.families.config,
                        parent_roots: sources.families.roots,
                        current: None,
                    },
                    parent.state,
                    ctx,
                    &mut factory.config_scan,
                    &mut factory.config,
                )
                .map_err(overlay_error)?
                .ok_or_else(invalid)?;
                let guard = ctx.transaction.guard();
                let value = read_qualified_artifact_config_overlay_v1(
                    ArtifactConfigCellRequestV1 {
                        field: ArtifactConfigFieldV1::Security,
                        key: ArtifactConfigRecordKeyV1::Global,
                        address: ArtifactConfigAddressV1 {
                            section: "security",
                            ordinal: None,
                            inner_ordinal: None,
                            member: None,
                            column: "allow_runtime_source_compile",
                        },
                        family: sources.families.config,
                        qualification: &body,
                        max_payload_bytes: 1,
                    },
                    ctx,
                    &guard,
                    live,
                    &mut std::io::sink(),
                )
                .map_err(overlay_error)?;
                match &*value {
                    ArtifactConfigValueV1::Boolean(value) => Ok(*value),
                    _ => Err(invalid().into()),
                }
            },
        )
    }
    fn source_bytes(&mut self, p: &crate::ModuleSourcePackage) -> Result<usize, Error> {
        p.files.values().try_fold(0usize, |total, bytes| {
            self.common
                .charge(2, std::mem::size_of::<Vec<u8>>() as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            total
                .checked_add(bytes.len())
                .ok_or_else(|| invalid().into())
        })
    }
    fn compile(
        &mut self,
        id: &str,
        p: &crate::ModuleSourcePackage,
    ) -> Result<Result<Self::Compiled, crate::WorldError>, Error> {
        use crate::module_source_compiler::PaidSourceCompileErrorV1;
        match self.sources.compiler.compile_paid_output_v1(
            id,
            p,
            self.module.max_original_bytes,
            self.common,
        ) {
            Ok(bytes) => Ok(Ok(bytes)),
            Err(PaidSourceCompileErrorV1::Resource(error)) => Err(error.into()),
            Err(PaidSourceCompileErrorV1::Compiler(error)) => Ok(Err(error)),
        }
    }
    fn compiler_error(&mut self, error: &crate::WorldError) -> Result<(), Error> {
        let length = match error {
            crate::WorldError::ModuleChangeInvalid { reason } => reason.len(),
            crate::WorldError::Io(message) => message.len(),
            _ => return Err(invalid().into()),
        };
        let bytes = length
            .checked_mul(8)
            .and_then(|n| n.checked_add(2048))
            .ok_or_else(invalid)?;
        self.pay_clone(bytes)
    }
    fn compiled_bytes(bytes: &Self::Compiled) -> &[u8] {
        bytes.as_slice()
    }
    fn hash(&mut self, bytes: &[u8]) -> Result<String, Error> {
        use sha2::{Digest, Sha256};
        self.pay_clone(64 + std::mem::size_of::<String>())?;
        let mut hash = Sha256::new();
        for chunk in bytes.chunks(8192) {
            self.common
                .charge(1 + chunk.len() as u64, chunk.len() as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            hash.update(chunk);
        }
        self.common
            .charge(128, 64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        Ok(hex::encode(hash.finalize()))
    }
}
