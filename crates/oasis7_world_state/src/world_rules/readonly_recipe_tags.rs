//! Work-only form of the original recipe preferred-tag predicate.
use std::{collections::BTreeSet,mem::size_of};
pub fn compatible<E>(preferred:&[String],factory:&[String],work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<bool,E>{
    work(1,0,0)?;if preferred.is_empty(){return Ok(true);}
    let mut normalized_factory=BTreeSet::new();
    for tag in factory {
        work(1,tag.len().saturating_mul(3),tag.len()+size_of::<String>())?;
        let normalized=tag.trim().to_ascii_lowercase();
        if !normalized.is_empty(){let levels=(normalized_factory.len()+1usize).ilog2() as usize+2;work(1,(normalized.len()+1).saturating_mul(levels*2+4).saturating_add(512*levels),512*levels)?;normalized_factory.insert(normalized);}
    }
    for tag in preferred {
        work(1,tag.len().saturating_mul(3),tag.len()+size_of::<String>())?;
        let normalized=tag.trim().to_ascii_lowercase();
        if !normalized.is_empty(){work(1,(normalized.len()+1).saturating_mul((normalized_factory.len()+1).ilog2() as usize+2),0)?;if normalized_factory.contains(normalized.as_str()){return Ok(true);}}
    }Ok(false)
}
