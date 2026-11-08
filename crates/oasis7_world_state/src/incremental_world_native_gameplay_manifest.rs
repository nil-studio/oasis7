//! Exhaustive original DTOs; each nested collection keeps its native order/key.
use super::*;
dto!(oasis7_wasm_abi::ModuleManifest, {
    module_id,
    name,
    version,
    kind,
    role,
    wasm_hash,
    interface_version,
    exports,
    subscriptions,
    required_caps,
    abi_contract,
    artifact_identity,
    limits,
});
dto!(oasis7_wasm_abi::ModuleLimits, {
    max_mem_bytes,
    max_gas,
    max_call_rate,
    max_output_bytes,
    max_effects,
    max_emits,
});
dto!(oasis7_wasm_abi::ModuleArtifactIdentity, {
    source_hash,
    build_manifest_hash,
    signer_node_id,
    signature_scheme,
    artifact_signature,
});
dto!(oasis7_wasm_abi::ModuleSubscription, {
    event_kinds,
    action_kinds,
    stage,
    filters,
});
dto!(oasis7_wasm_abi::ModuleAbiContract, {
    abi_version,
    input_schema,
    output_schema,
    cap_slots,
    policy_hooks,
    gameplay,
    declarations,
});
dto!(oasis7_wasm_abi::GameplayContract, {
    kind,
    game_modes,
    min_players,
    max_players,
});
// Keep the commands field visible to conservative workspace call-site analysis;
// an unknown DTO macro cannot distinguish this field from a CLI module reference.
impl Codec for oasis7_wasm_abi::ModuleSchemaDeclarations {
    fn accept_changed<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        profile: &TypedRecordProfileV1,
        before: Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
        after: Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
        d: &mut crate::incremental_typed_record_diff::ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError> {
        crate::incremental_typed_record_diff::accept_struct(
            profile,
            before,
            after,
            d,
            &["commands"],
            |name, before, after, d| match name {
                "commands" => {
                    crate::incremental_typed_record_diff::accept_projected::<Self, _, _, _, _, _>(
                        |owner| &owner.commands,
                        profile,
                        before,
                        after,
                        d,
                    )
                }
                _ => Err(ValueTreeError::Invalid("typed struct changed field")),
            },
        )
    }

    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        profile: &TypedRecordProfileV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let Self { commands } = self;
        let mut root = new_tree(profile, ctx, work)?;
        let value = commands.encode(profile, ctx, work)?;
        put(profile, &mut root, "commands", value, ctx, work)?;
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
            commands: Codec::decode(take_named(&mut c, "commands")?, d)?,
        };
        d.hold(c.finish_parts()?)?;
        Ok(value)
    }
}
dto!(oasis7_wasm_abi::ModuleCommandDeclaration, {
    namespace,
    name,
    schema_version,
    schema_hash,
    max_payload_bytes,
});
