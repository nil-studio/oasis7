//! Exhaustive original DTO fields; no arbitrary serialization.
macro_rules! dto {
($ty:ty,{$($name:ident),+$(,)?})=>{impl Codec for $ty {
fn accept_changed<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(profile:&TypedRecordProfileV1,before:Option<&$crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,after:Option<&$crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,d:&mut $crate::incremental_typed_record_diff::ChangeDecoder<'_, '_,M,R,E,C>)->Result<(),ValueTreeError>{
$crate::incremental_typed_record_diff::accept_struct(profile,before,after,d,&[$(stringify!($name)),+],|name,before,after,d|match name{$(stringify!($name)=>$crate::incremental_typed_record_diff::accept_projected::<Self,_,_,_,_,_>(|owner|&owner.$name,profile,before,after,d),)+ _=>Err(ValueTreeError::Invalid("typed struct changed field"))})}
fn encode<W:ValueNodeWriter+ValueNodeReader+StatePageWriter,C:FnMut()->bool>(&self,profile:&TypedRecordProfileV1,ctx:&mut MemberRecordWriteContext<'_,W,C>,work:&mut TypedRecordWriteWorkV1)->Result<Encoded<'_>,MemberRecordError>{let Self{$($name),+}=self;let mut root=new_tree(profile,ctx,work)?;$(let value=$name.encode(profile,ctx,work)?;put(profile,&mut root,stringify!($name),value,ctx,work)?;)+Ok(Encoded::Tree(root))}
fn decode<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(cell:ColumnCellV1,d:&mut Decoder<'_, '_,M,R,E,C>)->Result<Self,ValueTreeError>{let mut c=d.read(cell)?;c.spend_output(std::mem::size_of::<Self>())?;let value=Self{$($name:Codec::decode(take_named(&mut c,stringify!($name))?,d)?),+};d.hold(c.finish_parts()?)?;Ok(value)}
}};}

pub(crate) use dto;
