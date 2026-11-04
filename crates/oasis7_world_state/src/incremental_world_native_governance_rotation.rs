//! The original rotation history only admits a validated rotation action.
//! This is an internal record schema, never a subset Codec for global Action.
use super::*;
use oasis7_agent_api::world_action::{Action, GovernanceAuthorizedActionV1};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("native exact finality rotation")
}
fn action<'a, W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    source: &'a Action,
    profile: &TypedRecordProfileV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut TypedRecordWriteWorkV1,
) -> Result<Encoded<'a>, MemberRecordError> {
    let Action::RotateGovernanceValidatorFinalityKeyV1 {
        controller_account_id,
        body,
        old_key_proof,
        new_key_proof,
    } = source
    else {
        return Err(invalid().into());
    };
    let mut root = new_tree(profile, ctx, work)?;
    put(
        profile,
        &mut root,
        "tag",
        Encoded::Scalar(MemberPayloadRefV1::U64(0)),
        ctx,
        work,
    )?;
    macro_rules! field{($($name:ident),+)=>{$(let encoded=$name.encode(profile,ctx,work)?;put(profile,&mut root,stringify!($name),encoded,ctx,work)?;)+};}
    field!(controller_account_id, body, old_key_proof, new_key_proof);
    Ok(Encoded::Tree(root))
}
fn authorized<'a, W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    source: &'a GovernanceAuthorizedActionV1,
    profile: &TypedRecordProfileV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut TypedRecordWriteWorkV1,
) -> Result<Encoded<'a>, MemberRecordError> {
    let GovernanceAuthorizedActionV1 {
        action: source,
        authorization,
    } = source;
    let mut root = new_tree(profile, ctx, work)?;
    let encoded = action(source, profile, ctx, work)?;
    put(profile, &mut root, "action", encoded, ctx, work)?;
    let encoded = authorization.encode(profile, ctx, work)?;
    put(profile, &mut root, "authorization", encoded, ctx, work)?;
    Ok(Encoded::Tree(root))
}
fn decode_action<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    cell: ColumnCellV1,
    d: &mut Decoder<'_, '_, M, R, E, C>,
) -> Result<Box<Action>, ValueTreeError> {
    let mut c = d.read(cell)?;
    c.spend_output(std::mem::size_of::<Action>())?;
    if u64::decode(take_named(&mut c, "tag")?, d)? != 0 {
        return Err(invalid());
    }
    let value = Action::RotateGovernanceValidatorFinalityKeyV1 {
        controller_account_id: Codec::decode(take_named(&mut c, "controller_account_id")?, d)?,
        body: Codec::decode(take_named(&mut c, "body")?, d)?,
        old_key_proof: Codec::decode(take_named(&mut c, "old_key_proof")?, d)?,
        new_key_proof: Codec::decode(take_named(&mut c, "new_key_proof")?, d)?,
    };
    d.hold(c.finish_parts()?)?;
    d.ctx.check()?;
    Ok(Box::new(value))
}
fn decode_authorized<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    cell: ColumnCellV1,
    d: &mut Decoder<'_, '_, M, R, E, C>,
) -> Result<GovernanceAuthorizedActionV1, ValueTreeError> {
    let mut c = d.read(cell)?;
    c.spend_output(std::mem::size_of::<GovernanceAuthorizedActionV1>())?;
    let value = GovernanceAuthorizedActionV1 {
        action: decode_action(take_named(&mut c, "action")?, d)?,
        authorization: Codec::decode(take_named(&mut c, "authorization")?, d)?,
    };
    d.hold(c.finish_parts()?)?;
    Ok(value)
}
impl Codec for crate::governance::FinalityKeyRotationRecordV1 {
    fn encode<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        profile: &TypedRecordProfileV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        // validate_authorization clones only its already bounded <=32-signature
        // context. Admit that temporary work before calling the original code.
        let _validation = ctx
            .memory
            .reserve(
                128 << 10,
                oasis7_proto::memory_budget::MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        self.validate().map_err(|_| invalid())?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let Self {
            authorized_action,
            applied_at_tick,
        } = self;
        let mut root = new_tree(profile, ctx, work)?;
        let value = authorized(authorized_action, profile, ctx, work)?;
        put(profile, &mut root, "authorized_action", value, ctx, work)?;
        let value = applied_at_tick.encode(profile, ctx, work)?;
        put(profile, &mut root, "applied_at_tick", value, ctx, work)?;
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
            authorized_action: decode_authorized(take_named(&mut c, "authorized_action")?, d)?,
            applied_at_tick: Codec::decode(take_named(&mut c, "applied_at_tick")?, d)?,
        };
        d.hold(c.finish_parts()?)?;
        d.ctx.check()?;
        let _validation = d.ctx.values.memory.reserve(128 << 10, d.ctx.values.class)?;
        value.validate().map_err(|_| invalid())?;
        d.ctx.check()?;
        Ok(value)
    }
}
