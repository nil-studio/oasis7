//! Exhaustive original DTO shapes; no arbitrary DTO serialization.
use super::*;
use crate::capability_authorization::*;
use crate::service_admission::participant::*;
use crate::service_admission::participant_policy::*;
use crate::service_admission::{AgentSignerDelegationChangeV1, SignedReadRequest};
use oasis7_agent_api::game::*;
macro_rules! dto {
($ty:ty,{$($name:ident),+$(,)?})=>{impl Codec for $ty {
fn encode<W:ValueNodeWriter+ValueNodeReader+StatePageWriter,C:FnMut()->bool>(&self,ctx:&mut MemberRecordWriteContext<'_,W,C>,work:&mut RevocationWriteWorkV1)->Result<Encoded<'_>,MemberRecordError>{let Self{$($name),+}=self;let mut root=new_tree(ctx,work)?;$(let value=$name.encode(ctx,work)?;put(&mut root,stringify!($name),value,ctx,work)?;)+Ok(Encoded::Tree(root))}
fn decode<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(cell:ColumnCellV1,d:&mut Decoder<'_, '_,M,R,E,C>)->Result<Self,ValueTreeError>{let mut c=d.read(cell)?;c.spend_output(std::mem::size_of::<Self>())?;let value=Self{$($name:Codec::decode(take_named(&mut c,stringify!($name))?,d)?),+};d.hold(c.finish_parts()?)?;Ok(value)}
}};}
dto!(CapabilityRevocationState,{participants,epoch,revoked_grant_ids,superseded_by,authority_records,authority_finality_proofs,finalized_receipt_id,agent_identities,manufactured_identity_acquisition_sequences,agent_signer_delegations,world_service_results,task_terminals_v2,safe_alternatives,system_identities});
dto!(CapabilityAuthorityRecord,{issuer_id,issuer_kind,key_id,public_key_hex,issuer_key_epoch,governance_epoch,finalized_receipt_id,authority_rotation_receipt_id,world_id,branch_id,finality_epoch,finality_block_hash,finality_status,revocation_epoch,revoked_grant_ids,superseded_by});
dto!(CapabilityAuthorityFinalityProof,{proof_version,certificate,binding,signatures});
dto!(CapabilityAuthorityFinalityBinding,{authority_record_hash,issuer_id,issuer_kind,key_id,issuer_public_key_hex,issuer_key_epoch,governance_epoch,finalized_receipt_id,authority_rotation_receipt_id,world_id,branch_id,finality_epoch,finality_block_hash,finality_status});
dto!(CapabilityAgentIdentity,{owner_binding,generation});
dto!(AgentSignerDelegationChangeV1,{world_id,branch_id,agent_id,owner_binding,agent_identity_generation,generation,delegate_public_key,revoked,nonce});
dto!(crate::governance::GovernanceFinalityCertificate,{proposal_id,manifest_hash,consensus_height,epoch_id,validator_set_hash,stake_root,threshold_bps,min_unique_signers,threshold,signatures});
dto!(ParticipantRegistryV1,{policy,policy_history,migrated_bindings,devlocal_policy_history,bindings,operations});
dto!(ParticipantEntryPolicyV1,{api_rev,world_id,genesis_digest,identity_namespace,issuer_public_key,generation,max_grant_ticks,max_participants,registry_complete,enabled});
dto!(ParticipantOperationV1,{request,receipt});
dto!(ParticipantPolicyAuthorizationV1,{policy,proof});
dto!(ParticipantPolicyProofV1,{manifest,certificate,signatures});
dto!(ParticipantMigrationManifestV1,{api_rev,policy_digest,world_id,genesis_digest,branch_id,reorg_epoch,identity_namespace,source_snapshot_epoch,source_watermark,source_snapshot_digest,complete,previous_registry_digest,legacy_owner_state_digest,entries});
dto!(ParticipantMigrationBindingV1,{registration,agent_id,owner_binding,owner_generation});
dto!(DevLocalParticipantPolicyAuthorizationV1,{policy,manifest});
dto!(GameIdentityNamespaceV1,{service_id,environment_id,instance_epoch});
dto!(GameParticipantRegisterV1,{api_rev,client_operation_id,identity_namespace,account_id,world_id,genesis_digest,owner_public_key,owner_key_version});
dto!(GameParticipantReceiptV1,{api_rev,identity_namespace,account_id,world_id,genesis_digest,agent_id,owner_public_key,owner_binding,owner_generation,client_operation_id,business_digest,registered_tick,imported});
dto!(crate::TaskTerminalIndexV2,{schema_revision,records});
dto!(GameTaskRevokeAndCloseV2,{api_revision,client_operation_id,custody_account_id,termination_plan_digest,permitted_branches,signer_public_key,signer_generation,signer_nonce,revoke_generation,revoke_nonce,cleanup_until_ms});
dto!(GameTaskCloseConditionsV2,{exact_initial_active,own_stop_intent});
dto!(GameOwnStopIntentV2,{operation_id,intent_digest});
dto!(GameOwnStopReferenceV2,{operation_id,original_digest,canonical_result_digest,post_fence});
dto!(GameTaskClosedV2,{schema_revision,close_operation_id,close_original_digest,termination_plan_digest,initial_fence,actual_before_fence,selected_predicate,revoked_signer_public_key,revoked_generation,permanent_nonce_high_water});
dto!(GameTaskFenceV1,{world_id,genesis_digest,branch_id,reorg_epoch,agent_id,owner_binding,owner_generation,owner_public_key,task_id,task_revision,task_status,latest_goal_revision,active_goal_revision,active_goal_reference,executor_epoch,executor_binding});
dto!(GameGoalReferenceV1,{goal_revision,goal_ref,prepare_operation_id,publish_operation_id,activation_committed_height});
dto!(GameExecutorBindingV1,{executor_id,executor_public_key,delegation_generation,executor_epoch});
impl<T: Codec> Codec for SignedReadRequest<T> {
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RevocationWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let Self {
            request,
            subject_public_key,
            signature_hex,
        } = self;
        let mut root = new_tree(ctx, work)?;
        let value = request.encode(ctx, work)?;
        put(&mut root, "request", value, ctx, work)?;
        let value = subject_public_key.encode(ctx, work)?;
        put(&mut root, "subject_public_key", value, ctx, work)?;
        let value = signature_hex.encode(ctx, work)?;
        put(&mut root, "signature_hex", value, ctx, work)?;
        Ok(Encoded::Tree(root))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let mut c = d.read(cell)?;
        c.spend_output(std::mem::size_of::<Self>())?;
        let value = Self {
            request: Codec::decode(take_named(&mut c, "request")?, d)?,
            subject_public_key: Codec::decode(take_named(&mut c, "subject_public_key")?, d)?,
            signature_hex: Codec::decode(take_named(&mut c, "signature_hex")?, d)?,
        };
        d.hold(c.finish_parts()?)?;
        Ok(value)
    }
}
impl Codec for DecimalU64 {
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RevocationWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        self.0.encode(ctx, work)
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        Ok(Self(u64::decode(cell, d)?))
    }
}
impl Codec for GameTaskStatusV1 {
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        _ctx: &mut MemberRecordWriteContext<'_, W, C>,
        _work: &mut RevocationWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        Ok(Encoded::Scalar(MemberPayloadRefV1::U64(match self {
            Self::Suspended => 0,
            Self::Active => 1,
        })))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        match u64::decode(cell, d)? {
            0 => Ok(Self::Suspended),
            1 => Ok(Self::Active),
            _ => Err(invalid()),
        }
    }
}
impl Codec for GameTaskClosePredicateV2 {
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RevocationWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let mut root = new_tree(ctx, work)?;
        let (tag, value) = match self {
            Self::InitialActive => (0, Encoded::Scalar(MemberPayloadRefV1::Unit)),
            Self::OwnStopSuspended(value) => (1, value.encode(ctx, work)?),
        };
        put(
            &mut root,
            "tag",
            Encoded::Scalar(MemberPayloadRefV1::U64(tag)),
            ctx,
            work,
        )?;
        put(&mut root, "value", value, ctx, work)?;
        Ok(Encoded::Tree(root))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let mut c = d.read(cell)?;
        c.spend_output(std::mem::size_of::<Self>())?;
        let tag = u64::decode(take_named(&mut c, "tag")?, d)?;
        let cell = take_named(&mut c, "value")?;
        let value = match (tag, cell) {
            (0, ColumnCellV1::Unit) => Self::InitialActive,
            (1, cell) => Self::OwnStopSuspended(Codec::decode(cell, d)?),
            _ => return Err(invalid()),
        };
        d.hold(c.finish_parts()?)?;
        Ok(value)
    }
}
impl Codec for crate::TaskTerminalRecordV2 {
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RevocationWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let Self {
            schema_revision,
            original,
            committed_height,
            committed_at_unix_ms,
            stop_predecessor_canonical_cbor,
            closed,
        } = self;
        let mut root = new_tree(ctx, work)?;
        macro_rules! col {($($v:ident),*)=>{$(let value=$v.encode(ctx,work)?;put(&mut root,stringify!($v),value,ctx,work)?;)*};}
        col!(
            schema_revision,
            original,
            committed_height,
            committed_at_unix_ms,
            closed
        );
        match stop_predecessor_canonical_cbor {
            None => put(
                &mut root,
                "stop_predecessor_canonical_cbor",
                Encoded::Scalar(MemberPayloadRefV1::Unit),
                ctx,
                work,
            )?,
            Some(value) => bytes::put_original(
                &mut root,
                "stop_predecessor_canonical_cbor",
                value,
                ctx,
                work,
            )?,
        }
        Ok(Encoded::Tree(root))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let mut c = d.read(cell)?;
        c.spend_output(std::mem::size_of::<Self>())?;
        let predecessor = match take_named(&mut c, "stop_predecessor_canonical_cbor")? {
            ColumnCellV1::Unit => None,
            ColumnCellV1::Root(root) => Some(bytes::recover(&root, d)?),
            _ => return Err(invalid()),
        };
        let value = Self {
            schema_revision: Codec::decode(take_named(&mut c, "schema_revision")?, d)?,
            original: Codec::decode(take_named(&mut c, "original")?, d)?,
            committed_height: Codec::decode(take_named(&mut c, "committed_height")?, d)?,
            committed_at_unix_ms: Codec::decode(take_named(&mut c, "committed_at_unix_ms")?, d)?,
            stop_predecessor_canonical_cbor: predecessor,
            closed: Codec::decode(take_named(&mut c, "closed")?, d)?,
        };
        d.hold(c.finish_parts()?)?;
        Ok(value)
    }
}
