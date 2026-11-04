//! Typed collection keys and fixed-arity original tuples.
use super::*;
pub(crate) trait CollectionKey: Ord + Sized {
    type Semantic<'a>: SemanticKeyV1
    where
        Self: 'a;
    fn semantic_key(&self) -> Self::Semantic<'_>;
    fn from_column(key: ColumnKeyV1) -> Result<Self, ValueTreeError>;
    fn accepts_column(key: &ColumnKeyV1) -> bool;
}
impl CollectionKey for String {
    type Semantic<'a> = &'a str;
    fn semantic_key(&self) -> &str {
        self.as_str()
    }
    fn accepts_column(key: &ColumnKeyV1) -> bool {
        matches!(key, ColumnKeyV1::Text(_))
    }
    fn from_column(key: ColumnKeyV1) -> Result<Self, ValueTreeError> {
        match key {
            ColumnKeyV1::Text(v) => Ok(v),
            _ => Err(invalid()),
        }
    }
}
impl CollectionKey for u64 {
    type Semantic<'a> = u64;
    fn semantic_key(&self) -> u64 {
        *self
    }
    fn accepts_column(key: &ColumnKeyV1) -> bool {
        matches!(key, ColumnKeyV1::Ordinal(_))
    }
    fn from_column(key: ColumnKeyV1) -> Result<Self, ValueTreeError> {
        match key {
            ColumnKeyV1::Ordinal(v) => Ok(v),
            _ => Err(invalid()),
        }
    }
}
impl<A: CollectionKey, B: CollectionKey> CollectionKey for (A, B) {
    type Semantic<'a>
        = (A::Semantic<'a>, B::Semantic<'a>)
    where
        A: 'a,
        B: 'a;
    fn semantic_key(&self) -> Self::Semantic<'_> {
        (self.0.semantic_key(), self.1.semantic_key())
    }
    fn accepts_column(key: &ColumnKeyV1) -> bool {
        match key {
            ColumnKeyV1::Pair(a, b) => A::accepts_column(a) && B::accepts_column(b),
            _ => false,
        }
    }
    fn from_column(key: ColumnKeyV1) -> Result<Self, ValueTreeError> {
        let ColumnKeyV1::Pair(a, b) = key else {
            return Err(invalid());
        };
        Ok((A::from_column(*a)?, B::from_column(*b)?))
    }
}
macro_rules! tuple {
(@accept 2;$a:ident,$b:ident;$profile:expr,$before:expr,$after:expr,$d:expr)=>{crate::incremental_typed_record_diff::accept_tuple2::<$a,$b,_,_,_,_>($profile,$before,$after,$d)};
(@accept 3;$a:ident,$b:ident,$c:ident;$profile:expr,$before:expr,$after:expr,$d:expr)=>{crate::incremental_typed_record_diff::accept_tuple3::<$a,$b,$c,_,_,_,_>($profile,$before,$after,$d)};
($n:tt; $(($ty:ident,$name:ident,$index:literal)),+) => {impl<$($ty:Codec),+> Codec for ($($ty,)+) {
fn accept_changed<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(profile:&TypedRecordProfileV1,before:Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,after:Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,d:&mut crate::incremental_typed_record_diff::ChangeDecoder<'_, '_,M,R,E,C>)->Result<(),ValueTreeError>{
    tuple!(@accept $n; $($ty),+; profile,before,after,d)
}
fn encode<W:ValueNodeWriter+ValueNodeReader+StatePageWriter,C:FnMut()->bool>(&self,profile:&TypedRecordProfileV1,ctx:&mut MemberRecordWriteContext<'_,W,C>,work:&mut TypedRecordWriteWorkV1)->Result<Encoded<'_>,MemberRecordError>{let($($name,)+)=self;let mut root=new_tree(profile,ctx,work)?;put(profile,&mut root,"_length",Encoded::Scalar(MemberPayloadRefV1::U64($n)),ctx,work)?;$(let value=$name.encode(profile,ctx,work)?;put(profile,&mut root,&($index as u64),value,ctx,work)?;)+Ok(Encoded::Tree(root))}
fn decode<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(cell:ColumnCellV1,d:&mut Decoder<'_, '_,M,R,E,C>)->Result<Self,ValueTreeError>{let mut c=d.read(cell)?;c.spend_output(std::mem::size_of::<Self>())?;if u64::decode(take_named(&mut c,"_length")?,d)?!=$n{return Err(invalid());}let value=($($ty::decode(c.cells_mut().remove(&ColumnKeyV1::Ordinal($index)).ok_or_else(invalid)?,d)?,)+);d.hold(c.finish_parts()?)?;Ok(value)}
}};
}
tuple!(2;(A,a,0),(B,b,1));
tuple!(3;(A,a,0),(B,b,1),(D,d,2));
