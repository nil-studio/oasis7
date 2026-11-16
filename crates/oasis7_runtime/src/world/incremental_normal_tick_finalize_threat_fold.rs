//! Original threat arithmetic shared with the World wrapper. These facts are
//! DATA, not a source/permission constructor. Ordinary += profile overflow is
//! intentionally retained, along with original War-before-Crisis accumulation.
use std::collections::BTreeMap;
pub(in crate::world) fn fold_war_v1(next:&mut BTreeMap<String,i64>,active:bool,intensity:u32,aggressor:&str,defender:&str){
            if !active {
                return;
            }
            let war_risk = (intensity as i64).saturating_mul(10).max(10);
            *next
                .entry(format!("alliance:{}", aggressor))
                .or_insert(0) += war_risk;
            *next
                .entry(format!("alliance:{}", defender))
                .or_insert(0) += war_risk;
            *next.entry("global:war".to_string()).or_insert(0) += war_risk;

}
pub(in crate::world) fn fold_crisis_v1(next:&mut BTreeMap<String,i64>,status:crate::CrisisStatus,severity:u32,kind:&str){
            if !matches!(status, crate::CrisisStatus::Active) {
                return;
            }
            let crisis_risk = (severity as i64).saturating_mul(12).max(12);
            *next.entry(format!("crisis:{}", kind)).or_insert(0) += crisis_risk;
            *next.entry("global:crisis".to_string()).or_insert(0) += crisis_risk;

}
