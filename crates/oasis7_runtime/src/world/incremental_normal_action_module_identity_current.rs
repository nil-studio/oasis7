//! The original identity predicates over sealed legacy or actual Current facts.
use crate::{ModuleArtifactIdentity, ModuleManifest, WorldError};
use crate::world::{World, incremental_body::BodyMutationError,
    incremental_normal_action_factory_schedule::{FactoryScheduleReadErrorV1, FactoryScheduleWorkV1, with_module_lifecycle_read_io_v1},
    incremental_normal_action_module_lifecycle::{ModuleLifecycleSourcesV1, ModuleLifecycleWorkV1}};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::ExecutionFence, incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

const HASH_SCHEME: &str = "identity_hash_v1";
trait IdentityFactsV1 {
    type Error: From<WorldError>;
    fn reserve(&mut self, module: &ModuleManifest, identity: Option<&ModuleArtifactIdentity>) -> Result<(), Self::Error>;
    fn hash_allowed(&mut self) -> Result<bool, Self::Error>;
    fn signer(&mut self, id: &str) -> Result<Option<String>, Self::Error>;
    fn with_payload<T>(&mut self, module: &ModuleManifest, identity: &ModuleArtifactIdentity,
        consume: impl FnOnce(&[u8]) -> Result<T, WorldError>) -> Result<T, Self::Error>;
}
fn bad(reason: String) -> WorldError { WorldError::ModuleChangeInvalid { reason } }
fn decode<const N: usize>(raw: &str, label: &str) -> Result<[u8; N], WorldError> {
    // Preserve the original valid-hex-before-length failure ordering.
    let bytes = hex::decode(raw).map_err(|_| bad(format!("{label} must be valid hex")))?;
    bytes.try_into().map_err(|_| bad(format!("{label} must be {N}-byte hex")))
}
fn validate<S: IdentityFactsV1>(source: &mut S, module: &ModuleManifest,
    supplied: Option<&ModuleArtifactIdentity>) -> Result<(), S::Error> {
    source.reserve(module, supplied)?;
    let identity = supplied.ok_or_else(||bad(format!("module artifact_identity is required for {}", module.module_id)))?;
    if !identity.is_complete() {
        return Err(bad(format!("module artifact_identity is incomplete for {}",module.module_id)).into());
    }
    if identity.signature_scheme == HASH_SCHEME {
        if !source.hash_allowed()? {
            return Err(bad(format!("module artifact_identity signature_scheme {} is disabled by release policy for {}",HASH_SCHEME,module.module_id)).into());
        }
        let expected_hash = crate::util::sha256_hex(format!("{}:{}:{}",module.module_id,identity.source_hash,identity.build_manifest_hash).as_bytes());
        let expected = format!("idhash:{expected_hash}");
        if identity.artifact_signature != expected {
            return Err(bad(format!("module artifact_identity identity_hash signature mismatch for {}: expected={} actual={}",module.module_id,expected,identity.artifact_signature)).into());
        }
        return Ok(());
    }
    if identity.has_unsigned_prefix() {
        return Err(bad(format!("module artifact_identity unsigned signature is forbidden for {}",module.module_id)).into());
    }
    let prefix = identity.expected_signature_prefix().ok_or_else(||bad(format!("module artifact_identity signature_scheme unsupported for {}: {}",module.module_id,identity.signature_scheme)))?;
    let signature_hex = identity.artifact_signature.strip_prefix(prefix)
        .ok_or_else(||bad(format!("module artifact_identity signature prefix mismatch for {}",module.module_id)))?;
    let key = source.signer(&identity.signer_node_id)?.ok_or_else(||bad(format!("module artifact_identity signer is not trusted for {}: {}",module.module_id,identity.signer_node_id)))?;
    let key = decode::<32>(&key,"module artifact signer public key")?;
    let signature = decode::<64>(signature_hex,"module artifact signature")?;
    let verifier = VerifyingKey::from_bytes(&key).map_err(|error|bad(format!("module artifact_identity signer public key parse failed for {}: {}",module.module_id,error)))?;
    source.with_payload(module,identity,|payload| {
        verifier.verify(payload,&Signature::from_bytes(&signature)).map_err(|error|bad(format!("module artifact_identity signature mismatch for {}: {}",module.module_id,error)))
    })
}

struct Legacy<'a>(&'a World);
impl IdentityFactsV1 for Legacy<'_> {
    type Error = WorldError;
    fn reserve(&mut self,_:&ModuleManifest,_:Option<&ModuleArtifactIdentity>)->Result<(),WorldError>{Ok(())}
    fn hash_allowed(&mut self)->Result<bool,WorldError>{Ok(self.0.release_security_policy.allow_identity_hash_signature)}
    fn signer(&mut self,id:&str)->Result<Option<String>,WorldError>{Ok(self.0.node_identity_public_key(id).map(str::to_owned))}
    fn with_payload<T>(&mut self,module:&ModuleManifest,identity:&ModuleArtifactIdentity,consume:impl FnOnce(&[u8])->Result<T,WorldError>)->Result<T,WorldError>{
        let bytes=ModuleArtifactIdentity::signing_payload_v1(&module.wasm_hash,&identity.source_hash,&identity.build_manifest_hash,&identity.signer_node_id);
        consume(&bytes)
    }
}
pub(in crate::world) fn validate_module_artifact_identity_from_legacy_v1(world:&World,module:&ModuleManifest)->Result<(),WorldError>{
    validate(&mut Legacy(world),module,module.artifact_identity.as_ref())
}
pub(in crate::world) fn validate_module_artifact_identity_fields_from_legacy_v1(world:&World,module:&ModuleManifest,identity:&ModuleArtifactIdentity)->Result<(),WorldError>{
    validate(&mut Legacy(world),module,Some(identity))
}

struct Current<'a,'s,'t,'v,'m,'p,'b,'mw,M,P,R,E,C,PE,PC> {
    sources:&'a ModuleLifecycleSourcesV1<'s>,
    ctx:&'a mut WorldFieldOverlayContextV1<'t,'v,'m,M,R,E,C>,
    parent:&'a mut WorldFieldReadContextV1<'p,P,R,PE,PC>,
    factory:&'a mut FactoryScheduleWorkV1,module:&'a mut ModuleLifecycleWorkV1<'mw>,
    common:&'a mut FormalBoundedContextV1<'b>,live:&'a ExecutionFence,
    fees:Vec<MemoryReservation>,
}
fn overlay(error:oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1)->BodyMutationError {
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1 as E;
    match error {E::Execution(e)=>e.into(),E::Value(e)=>e.into(),E::Member(e)=>BodyMutationError::Original(Box::new(e))}
}
impl<M,P,R,E,C,PE,PC> IdentityFactsV1 for Current<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,
    E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
    type Error=FactoryScheduleReadErrorV1;
    fn reserve(&mut self,module:&ModuleManifest,identity:Option<&ModuleArtifactIdentity>)->Result<(),Self::Error>{
        let mut bytes=module.module_id.len().checked_add(module.wasm_hash.len()).ok_or(BodyMutationError::Invalid("module identity size"))?;
        if let Some(i)=identity {for text in [&i.source_hash,&i.build_manifest_hash,&i.signer_node_id,&i.signature_scheme,&i.artifact_signature]{bytes=bytes.checked_add(text.len()).ok_or(BodyMutationError::Invalid("module identity size"))?;}}
        let bytes=bytes.checked_mul(16).and_then(|n|n.checked_add(16384)).ok_or(BodyMutationError::Invalid("module identity size"))?;
        self.common.charge(32,bytes as u64).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
        let fee=self.common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
        self.fees.push(fee);Ok(())
    }
    fn hash_allowed(&mut self)->Result<bool,Self::Error>{
        use oasis7_world_state::incremental_runtime_artifact_config::*;
        let sources=self.sources;let live=self.live;
        with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,_|{
            let body=open_artifact_config_body_overlay_v1(ArtifactConfigSelectedBodyRequestV1{
                field:ArtifactConfigFieldV1::Security,key:ArtifactConfigRecordKeyV1::Global,
                family:sources.families.config,parent_roots:sources.families.roots,current:None,
            },parent.state,ctx,&mut factory.config_scan,&mut factory.config).map_err(overlay)?
                .ok_or(BodyMutationError::Invalid("module identity Security missing"))?;
            let guard=ctx.transaction.guard();
            let value=read_qualified_artifact_config_overlay_v1(ArtifactConfigCellRequestV1{
                field:ArtifactConfigFieldV1::Security,key:ArtifactConfigRecordKeyV1::Global,
                address:ArtifactConfigAddressV1{section:"security",ordinal:None,inner_ordinal:None,member:None,column:"allow_identity_hash_signature"},
                family:sources.families.config,qualification:&body,max_payload_bytes:1,
            },ctx,&guard,live,&mut std::io::sink()).map_err(overlay)?;
            match &*value {ArtifactConfigValueV1::Boolean(value)=>Ok(*value),_=>Err(BodyMutationError::Invalid("module identity Security type").into())}
        })
    }
    fn signer(&mut self,id:&str)->Result<Option<String>,Self::Error>{
        use oasis7_world_state::incremental_world_text_maps::{read_world_text_map_overlay_v1,WorldTextMapFieldV1};
        let sources=self.sources;
        let selected=with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,_,_,module|{
            struct Count(usize);impl std::io::Write for Count{fn write(&mut self,b:&[u8])->std::io::Result<usize>{self.0=self.0.checked_add(b.len()).ok_or_else(||std::io::Error::other("signer key length"))?;Ok(b.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}
            let mut count=Count(0);
            let first=read_world_text_map_overlay_v1(WorldTextMapFieldV1::NodeIdentityBindings,id,ctx,module.max_selected_heap as u64,&mut count).map_err(overlay)?;
            if first.is_none(){return Ok(None)}
            let bytes=count.0.checked_mul(3).and_then(|n|n.checked_add(1024)).ok_or(BodyMutationError::Invalid("module signer size"))?;
            let fee=ctx.values.memory.reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
            let mut output=Vec::with_capacity(count.0);
            let second=read_world_text_map_overlay_v1(WorldTextMapFieldV1::NodeIdentityBindings,id,ctx,module.max_selected_heap as u64,&mut output).map_err(overlay)?;
            if first!=second||output.len()!=count.0{return Err(BodyMutationError::Invalid("module signer changed").into())}
            let key=String::from_utf8(output).map_err(|_|BodyMutationError::Invalid("module signer UTF8"))?;
            Ok(Some((key,fee)))
        })?;
        match selected {Some((key,fee))=>{self.fees.push(fee);Ok(Some(key))},None=>Ok(None)}
    }
    fn with_payload<T>(&mut self,module:&ModuleManifest,identity:&ModuleArtifactIdentity,consume:impl FnOnce(&[u8])->Result<T,WorldError>)->Result<T,Self::Error>{
        let bytes=crate::world::prepare_identity_signature_payload_v1(&module.wasm_hash,identity,usize::MAX,self.common)?;
        // Charge the actual SHA512/verification pass in addition to preimage
        // construction. The verifier is exactly the original non-strict one.
        self.common.charge(4096,bytes.len() as u64).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
        consume(bytes.as_slice()).map_err(Into::into)
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn validate_current_module_artifact_identity_v1<M,P,R,E,C,PE,PC>(
    module:&ModuleManifest,sources:&ModuleLifecycleSourcesV1<'_>,
    ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,
    parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,
    shared_factory:&mut FactoryScheduleWorkV1,module_work:&mut ModuleLifecycleWorkV1<'_>,
    common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,
)->Result<(),FactoryScheduleReadErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,
    E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
    ctx.transaction.validate_guard(&ctx.transaction.guard(),live).map_err(BodyMutationError::from)?;
    if !ctx.values.memory.same_pool(common.memory()) {return Err(BodyMutationError::Invalid("module identity pool").into())}
    let _frame=common.memory().reserve(std::mem::size_of::<Current<'_, '_, '_, '_, '_, '_, '_, '_,M,P,R,E,C,PE,PC>>()+4*std::mem::size_of::<MemoryReservation>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
    let mut current=Current{sources,ctx,parent,factory:shared_factory,module:module_work,common,live,fees:Vec::with_capacity(4)};
    validate(&mut current,module,module.artifact_identity.as_ref())
}
