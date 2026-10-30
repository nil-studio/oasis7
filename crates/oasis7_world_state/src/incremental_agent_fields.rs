//! Fine-grained AgentCell fields. Existing body/mailbox formats remain owned
//! by their typed codecs. These keys and scalar values confer no authority.
use oasis7_formal_protocol::incremental_state::StateNamespaceV1 as Ns;
use serde::Serialize;
use std::io::Write;
#[path = "incremental_agent_scalar.rs"]
mod scalar;
pub use scalar::*;
#[path = "incremental_agent_visit.rs"]
mod visit;
pub use visit::*;
#[path = "incremental_agent_projection.rs"]
mod projection;
pub use projection::visit_agents_projection;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarKindV1 {
    U8,
    U16,
    U32,
    U64,
    I64,
    Bool,
    Text,
    OptionalText,
    OptionalU64,
}
macro_rules! fields {($($name:ident=$tag:literal,$ns:ident,$kind:ident,$indexed:literal;)*)=>{
    #[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize)]
    #[repr(u16)]
    pub enum AgentFieldV1{$($name=$tag,)*}
    impl AgentFieldV1 {
        pub const ALL:&'static[Self]=&[$(Self::$name,)*];
        pub fn from_tag(tag:u16)->Result<Self,&'static str>{match tag{$($tag=>Ok(Self::$name),)*_=>Err("agent field tag")}}
        pub fn name(self)->&'static str{match self{$(Self::$name=>concat!("agents.",stringify!($name)),)*}}
        pub fn namespace(self)->Ns{match self{$(Self::$name=>Ns::$ns,)*}}
        pub fn kind(self)->ScalarKindV1{match self{$(Self::$name=>ScalarKindV1::$kind,)*}}
        pub fn indexed(self)->bool{match self{$(Self::$name=>$indexed,)*}}
    }
}}
fields! {
    Identity=1,Business,Text,false;
    PositionX=2,Business,I64,false; PositionY=3,Business,I64,false; PositionZ=4,Business,I64,false;
    BodyKind=5,Business,Text,false; BodyHeight=6,Business,I64,false;
    ResourceAmount=7,Business,I64,true;
    FrameKind=8,Business,Text,false; SlotCapacity=9,Business,U16,false; ExpansionLevel=10,Business,U16,false;
    SlotCount=11,Business,U64,false; SlotNext=12,Business,U64,false;
    SlotId=13,Business,Text,true; SlotType=14,Business,U8,true; SlotInstalled=15,Business,OptionalText,true; SlotLocked=16,Business,Bool,true;
    CargoCount=17,Business,U64,false; CargoNext=18,Business,U64,false;
    CargoId=19,Business,Text,true; CargoKind=20,Business,U8,true; CargoQuantity=21,Business,I64,true; CargoUnitSize=22,Business,I64,true;
    ActivityPresent=23,Scheduler,Bool,false; ActivityStatus=24,Scheduler,U8,false; ActivityUpdated=25,Scheduler,U64,false;
    ActivityOperationKind=26,Scheduler,OptionalText,false; ActivityOperationId=27,Scheduler,OptionalU64,false;
    ActivityTarget=28,Scheduler,OptionalText,false; ActivityReasonCode=29,Scheduler,OptionalText,false; ActivityReasonSummary=30,Scheduler,OptionalText,false;
    IntentPresent=31,Authorization,Bool,false; IntentSchema=32,Authorization,U32,false; IntentAgent=33,Authorization,Text,false;
    IntentId=34,Authorization,Text,false; IntentKind=35,Authorization,Text,false; IntentSummary=36,Authorization,Text,false;
    IntentTarget=37,Authorization,OptionalText,false; IntentEffect=38,Authorization,OptionalText,false;
    IntentTick=39,Authorization,OptionalU64,false; IntentWorld=40,Authorization,OptionalText,false;
    IntentReorg=41,Authorization,OptionalU64,false; IntentAuthorityScope=42,Authorization,OptionalText,false;
    IntentStatus=43,Authorization,Text,false; IntentSource=44,Authorization,Text,false; IntentLogicalTime=45,Authorization,U64,false;
    IntentEventSeq=46,Authorization,U64,false; IntentUpdated=47,Authorization,U64,false;
    IntentReceipt=48,Authorization,OptionalText,false; IntentReasonCode=49,Authorization,OptionalText,false;
    IntentReasonSummary=50,Authorization,OptionalText,false; IntentReplacedBy=51,Authorization,OptionalText,false;
    IntentActor=52,Authorization,Text,false; IntentRequestDigest=53,Authorization,Text,false;
}
const KEY: &[u8] = b"oasis7.iws.agent-field-key.v1\0";
pub const AGENT_FIELD_KEY_BYTES: usize = KEY.len() + 42;
pub struct AgentFieldKeyV1([u8; AGENT_FIELD_KEY_BYTES]);
impl AgentFieldKeyV1 {
    pub fn new(agent: &str, field: AgentFieldV1, ordinal: u64) -> Result<Self, &'static str> {
        // Match the existing typed body/mailbox admission before hashing.
        crate::incremental_agent_body::AgentBodyKeyV1::new(agent)?;
        crate::incremental_event_fields::MailboxKeyV1::new(agent, Some(0))?;
        if field == AgentFieldV1::ResourceAmount && ordinal > 1 {
            return Err("agent resource kind ordinal");
        }
        if !field.indexed() && ordinal != 0 {
            return Err("scalar agent field ordinal");
        }
        let mut hash = blake3::Hasher::new();
        hash.update(b"oasis7.iws.agent-id.v1\0");
        hash.update(&(agent.len() as u64).to_be_bytes());
        hash.update(agent.as_bytes());
        let mut bytes = [0; AGENT_FIELD_KEY_BYTES];
        bytes[..KEY.len()].copy_from_slice(KEY);
        bytes[KEY.len()..KEY.len() + 32].copy_from_slice(hash.finalize().as_bytes());
        bytes[KEY.len() + 32..KEY.len() + 34].copy_from_slice(&(field as u16).to_be_bytes());
        bytes[KEY.len() + 34..].copy_from_slice(&ordinal.to_be_bytes());
        Ok(Self(bytes))
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<([u8; 32], AgentFieldV1, u64), &'static str> {
        if bytes.len() != AGENT_FIELD_KEY_BYTES || !bytes.starts_with(KEY) {
            return Err("agent field key domain/length");
        }
        let field = AgentFieldV1::from_tag(u16::from_be_bytes(
            bytes[KEY.len() + 32..KEY.len() + 34].try_into().unwrap(),
        ))?;
        let ordinal = u64::from_be_bytes(bytes[KEY.len() + 34..].try_into().unwrap());
        if (!field.indexed() && ordinal != 0)
            || (field == AgentFieldV1::ResourceAmount && ordinal > 1)
        {
            return Err("agent field key ordinal");
        }
        Ok((
            bytes[KEY.len()..KEY.len() + 32].try_into().unwrap(),
            field,
            ordinal,
        ))
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl AgentFieldV1 {
    pub fn validate_value(self, value: AgentScalarRefV1<'_>) -> Result<(), &'static str> {
        if value.kind() != self.kind() {
            return Err("agent field scalar kind");
        }
        match (self, value) {
            (Self::SlotType, AgentScalarRefV1::U8(v)) if v > 5 => Err("agent slot kind"),
            (Self::CargoKind, AgentScalarRefV1::U8(v)) if v > 2 => Err("agent cargo kind"),
            (Self::ActivityStatus, AgentScalarRefV1::U8(v)) if v > 4 => {
                Err("agent activity status")
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "incremental_agent_fields_tests.rs"]
mod tests;
