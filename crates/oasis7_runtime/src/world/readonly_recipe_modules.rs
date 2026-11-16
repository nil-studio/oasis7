//! Original reviewed-product module selection, over actual ordered active IDs.
//! Selected strings remain DATA, never module admission or execution authority.
use std::{mem::size_of,ops::ControlFlow};
pub(in crate::world) fn select_product_module_v1<E>(
 kind:&str,builtin:Option<&'static str>,
 contains:&mut impl FnMut(&str)->Result<bool,E>,
 visit_keys:&mut impl FnMut(&mut dyn FnMut(&str)->Result<ControlFlow<()>,E>)->Result<(),E>,
 work:&mut impl FnMut(u64,usize,usize)->Result<(),E>,
)->Result<Option<String>,E>{
 if let Some(id)=builtin{if contains(id)?{work(1,id.len(),size_of::<String>()+id.len())?;return Ok(Some(id.to_owned()))}}
 let n=kind.len().checked_add(1).expect("resident product kind length");work(1,n,size_of::<String>()+n)?;
 let mut suffix=String::with_capacity(n);assert_eq!(suffix.capacity(),n,"product module suffix exact capacity");suffix.push('.');suffix.push_str(kind);
 let mut result=None;visit_keys(&mut|id|{work(1,id.len().saturating_mul(2)+suffix.len(),0)?;
  if id.contains(".product.")&&id.ends_with(&suffix){work(1,id.len(),size_of::<String>()+id.len())?;result=Some(id.to_owned());Ok(ControlFlow::Break(()))}else{Ok(ControlFlow::Continue(()))}
 })?;Ok(result)
}
