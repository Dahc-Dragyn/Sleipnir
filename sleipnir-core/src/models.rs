use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum PayloadType<'a> {
    StreamUpdate {
        #[serde(borrow)]
        data: Cow<'a, str>,
    },
    ToolInvocation {
        #[serde(borrow)]
        tool_name: Cow<'a, str>,
        #[serde(borrow)]
        arguments: Cow<'a, str>, // JSON string to keep zero-copy over complex structures
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentActionFrame<'a> {
    #[serde(borrow)]
    pub transaction_id: Cow<'a, str>,
    #[serde(borrow)]
    pub agent_id: Cow<'a, str>,
    pub timestamp: u64,
    #[serde(borrow)]
    pub payload: PayloadType<'a>,
    #[serde(borrow)]
    pub context_monologue: Option<Cow<'a, str>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum ActionStatus {
    Approved,
    Denied,
    Mutated,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct HandshakeResolutionFrame<'a> {
    #[serde(borrow)]
    pub transaction_id: Cow<'a, str>,
    pub status: ActionStatus,
    #[serde(borrow)]
    pub mutated_payload: Option<PayloadType<'a>>,
}
